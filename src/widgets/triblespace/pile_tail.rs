use std::path::Path;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use rand_core06::OsRng;
use triblespace::core::blob::encodings::simplearchive::SimpleArchive;
use triblespace::core::id::Id;
use triblespace::core::inline::encodings::hash::Handle;
use triblespace::core::inline::Inline;
use triblespace::core::repo::pile::{Pile, PileReader};
use triblespace::core::repo::{BlobStore, PullError, Repository};
use triblespace::core::trible::TribleSet;

type CommitHandle = Inline<Handle<SimpleArchive>>;

/// Which branch a [`PileTail`] follows.
enum BranchRef {
    /// A concrete branch id.
    Id(Id),
    /// A branch name, resolved lazily on [`PileTail::poll`] so tailing
    /// can start before the writer has created the branch.
    Name(String),
}

/// A live tail over one branch of a `.pile` file: repeated [`poll`]
/// calls return only the [`TribleSet`] delta committed since the
/// previous poll.
///
/// `PileTail` owns a **read-only** handle to the pile:
/// - *open-loud*: [`open`] fails with an error if the pile has a torn
///   tail; it never auto-truncates (repair is an explicit operator
///   decision, `trible pile amputate`);
/// - *never pushes*: the tail only ever reads — the underlying
///   repository handle is not exposed.
///
/// [`poll`] tracks the last seen branch head and checks out only the
/// commit range since it (the delta-tail pattern), returning an empty
/// set when the head is unchanged. A built-in wall-clock throttle
/// (default 250 ms, see [`min_interval`]) makes it safe to call every
/// frame: throttled polls return an empty set *without* advancing the
/// head, so no data is ever skipped — the next unthrottled poll
/// returns the accumulated delta.
///
/// ```ignore
/// let mut tail = PileTail::open_by_name("./drive.pile", "telemetry")?
///     .min_interval(Duration::from_millis(100));
/// // Every frame:
/// let delta = tail.poll()?;
/// if !delta.is_empty() {
///     // query `delta` with find!/pattern! and update derived state
/// }
/// ```
///
/// [`open`]: Self::open
/// [`poll`]: Self::poll
/// [`min_interval`]: Self::min_interval
pub struct PileTail {
    repo: Option<Repository<Pile>>,
    branch: BranchRef,
    resolved: Option<Id>,
    head: Option<CommitHandle>,
    min_interval: Duration,
    last_poll: Option<Instant>,
}

impl Drop for PileTail {
    fn drop(&mut self) {
        if let Some(repo) = self.repo.take() {
            // Avoid the "Pile dropped without calling close()" warning.
            let _ = repo.close();
        }
    }
}

impl std::fmt::Debug for PileTail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PileTail")
            .field("branch_id", &self.branch_id())
            .field("has_head", &self.head.is_some())
            .field("min_interval", &self.min_interval)
            .finish()
    }
}

impl PileTail {
    /// Open a tail over the branch with the given id.
    ///
    /// Fails loud when the pile cannot be opened or has a torn tail —
    /// never auto-truncates. The branch itself may not exist yet;
    /// [`poll`](Self::poll) returns empty sets until it appears.
    pub fn open(path: impl AsRef<Path>, branch_id: Id) -> Result<Self, String> {
        Self::open_ref(path.as_ref(), BranchRef::Id(branch_id))
    }

    /// Open a tail over the branch with the given name.
    ///
    /// The name is resolved lazily on [`poll`](Self::poll), so tailing
    /// can start before the writer has created the branch.
    pub fn open_by_name(path: impl AsRef<Path>, name: impl Into<String>) -> Result<Self, String> {
        Self::open_ref(path.as_ref(), BranchRef::Name(name.into()))
    }

    fn open_ref(path: &Path, branch: BranchRef) -> Result<Self, String> {
        let mut pile = Pile::open(path).map_err(|err| format!("open pile: {err:?}"))?;
        if let Err(err) = pile.refresh() {
            let _ = pile.close();
            return Err(format!(
                "pile failed to load ({err:?}); refusing to auto-truncate — repair \
                 explicitly with `trible pile amputate` if the tail is genuinely torn"
            ));
        }
        // The signing key is required by `Repository` but never used:
        // a tail only reads.
        let repo = Repository::new(pile, SigningKey::generate(&mut OsRng), TribleSet::new())
            .map_err(|err| format!("create repository: {err:?}"))?;
        Ok(Self {
            repo: Some(repo),
            branch,
            resolved: None,
            head: None,
            min_interval: Duration::from_millis(250),
            last_poll: None,
        })
    }

    /// Set the minimum wall-clock interval between pile reads. Polls
    /// arriving sooner return an empty set without touching the pile
    /// (and without advancing the head — no data is skipped). Default
    /// is 250 ms; use [`Duration::ZERO`] to poll on every call.
    pub fn min_interval(mut self, interval: Duration) -> Self {
        self.min_interval = interval;
        self
    }

    /// Adjust the throttle interval on a live tail (see [`Self::min_interval`]).
    pub fn set_min_interval(&mut self, interval: Duration) {
        self.min_interval = interval;
    }

    /// The id of the tailed branch, once known. `None` until a
    /// name-referenced branch has been resolved by a poll.
    pub fn branch_id(&self) -> Option<Id> {
        match &self.branch {
            BranchRef::Id(id) => Some(*id),
            BranchRef::Name(_) => self.resolved,
        }
    }

    /// The last branch head observed by [`poll`](Self::poll).
    pub fn head(&self) -> Option<CommitHandle> {
        self.head
    }

    /// Forget the tracked head so the next poll returns the branch's
    /// full history again.
    pub fn rewind(&mut self) {
        self.head = None;
    }

    /// A blob reader for the underlying pile, for resolving handles
    /// (e.g. `LongString` payloads) referenced by polled deltas.
    pub fn reader(&mut self) -> Result<PileReader, String> {
        self.repo
            .as_mut()
            .expect("repo present until drop")
            .storage_mut()
            .reader()
            .map_err(|err| format!("open pile reader: {err:?}"))
    }

    /// Return the delta committed to the tailed branch since the
    /// previous poll, or an empty set when nothing changed (or the
    /// call was throttled — see [`Self::min_interval`]).
    pub fn poll(&mut self) -> Result<TribleSet, String> {
        if let Some(last) = self.last_poll {
            if last.elapsed() < self.min_interval {
                return Ok(TribleSet::new());
            }
        }
        self.last_poll = Some(Instant::now());

        let repo = self.repo.as_mut().expect("repo present until drop");

        // Resolve a name-referenced branch; absent branch = empty delta.
        let branch_id = match &self.branch {
            BranchRef::Id(id) => *id,
            BranchRef::Name(name) => match self.resolved {
                Some(id) => id,
                None => match repo.lookup_branch(name) {
                    Ok(Some(id)) => {
                        self.resolved = Some(id);
                        id
                    }
                    Ok(None) => return Ok(TribleSet::new()),
                    Err(err) => return Err(format!("lookup branch {name:?}: {err:?}")),
                },
            },
        };

        let mut ws = match repo.pull(branch_id) {
            Ok(ws) => ws,
            // The branch may appear later (or was deleted); treat as empty.
            Err(PullError::BranchNotFound(_)) => return Ok(TribleSet::new()),
            Err(err) => return Err(format!("pull branch {branch_id:x}: {err:?}")),
        };

        let head = ws.head();
        let delta = match (self.head, head) {
            (Some(prev), Some(new)) if prev == new => TribleSet::new(),
            (Some(prev), Some(_)) => ws
                .checkout(prev..)
                .map_err(|err| format!("checkout delta: {err:?}"))?
                .into_facts(),
            (None, Some(_)) => ws
                .checkout(..)
                .map_err(|err| format!("checkout branch: {err:?}"))?
                .into_facts(),
            (_, None) => TribleSet::new(),
        };
        self.head = head;
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use triblespace::core::id::rngid;
    use triblespace::macros::entity;

    mod tail_test {
        use triblespace::macros::attributes;
        attributes! {
            // Minted with `trible genid`.
            "E79B015C562E875E21D18639BC1B9099" as pub sample:
                triblespace::core::inline::encodings::shortstring::ShortString;
        }
    }

    fn temp_pile(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "gorbie_pile_tail_{tag}_{pid}.pile",
            pid = std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::fs::File::create(&path).expect("create pile file");
        path
    }

    fn open_writer(path: &Path) -> Repository<Pile> {
        let mut pile = Pile::open(path).expect("open pile");
        pile.refresh().expect("refresh pile");
        Repository::new(pile, SigningKey::generate(&mut OsRng), TribleSet::new())
            .expect("create repository")
    }

    fn sample_set(labels: &[&str]) -> TribleSet {
        let mut set = TribleSet::new();
        for label in labels {
            let e = rngid();
            set += entity! { &e @ tail_test::sample: *label };
        }
        set
    }

    #[test]
    fn poll_returns_only_deltas() {
        let path = temp_pile("delta");
        let mut writer = open_writer(&path);
        let branch_id = *writer.create_branch("tail", None).expect("create branch");

        let mut ws = writer.pull(branch_id).expect("pull");
        let first = sample_set(&["alpha", "beta", "gamma"]);
        ws.commit(first.clone(), "first");
        writer.push(&mut ws).expect("push first");

        let mut tail = PileTail::open(&path, branch_id)
            .expect("open tail")
            .min_interval(Duration::ZERO);

        let delta = tail.poll().expect("poll first");
        assert_eq!(delta.len(), first.len(), "first poll returns full history");

        let delta = tail.poll().expect("poll unchanged");
        assert!(delta.is_empty(), "unchanged head yields empty delta");

        let mut ws = writer.pull(branch_id).expect("pull again");
        let second = sample_set(&["delta", "epsilon"]);
        ws.commit(second.clone(), "second");
        writer.push(&mut ws).expect("push second");

        let delta = tail.poll().expect("poll delta");
        assert_eq!(delta.len(), second.len(), "second poll returns only the delta");
        assert!(
            second.iter().all(|t| delta.contains(t)),
            "delta contains exactly the new facts"
        );

        drop(tail);
        let _ = writer.close();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn throttled_polls_do_not_skip_data() {
        let path = temp_pile("throttle");
        let mut writer = open_writer(&path);
        let branch_id = *writer.create_branch("tail", None).expect("create branch");

        let mut tail = PileTail::open(&path, branch_id)
            .expect("open tail")
            .min_interval(Duration::ZERO);
        assert!(tail.poll().expect("initial poll").is_empty());

        let mut ws = writer.pull(branch_id).expect("pull");
        let facts = sample_set(&["zeta"]);
        ws.commit(facts.clone(), "tick");
        writer.push(&mut ws).expect("push");

        // Throttled: returns empty without advancing the head.
        tail.set_min_interval(Duration::from_secs(3600));
        assert!(tail.poll().expect("throttled poll").is_empty());

        // Unthrottled: the accumulated delta arrives in full.
        tail.set_min_interval(Duration::ZERO);
        let delta = tail.poll().expect("unthrottled poll");
        assert_eq!(delta.len(), facts.len(), "no data skipped by throttling");

        drop(tail);
        let _ = writer.close();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_by_name_resolves_lazily() {
        let path = temp_pile("byname");

        // Tail starts before the branch exists.
        let mut tail = PileTail::open_by_name(&path, "late")
            .expect("open tail")
            .min_interval(Duration::ZERO);
        assert!(tail.poll().expect("poll before branch").is_empty());
        assert_eq!(tail.branch_id(), None);

        let mut writer = open_writer(&path);
        let branch_id = *writer.create_branch("late", None).expect("create branch");
        let mut ws = writer.pull(branch_id).expect("pull");
        let facts = sample_set(&["eta", "theta"]);
        ws.commit(facts.clone(), "late data");
        writer.push(&mut ws).expect("push");

        let delta = tail.poll().expect("poll after branch");
        assert_eq!(delta.len(), facts.len());
        assert_eq!(tail.branch_id(), Some(branch_id));

        drop(tail);
        let _ = writer.close();
        let _ = std::fs::remove_file(&path);
    }
}
