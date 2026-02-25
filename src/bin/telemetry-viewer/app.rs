// Notebook implementation for `telemetry-viewer`.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
use rand_core06::OsRng;
use triblespace::core::blob::schemas::longstring::LongString;
use triblespace::core::blob::schemas::simplearchive::SimpleArchive;
use triblespace::core::metadata;
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::{BlobStore, BlobStoreGet, BlobStoreMeta, BranchStore, Repository};
use triblespace::core::trible::TribleSet;
use triblespace::core::value::Value;
use triblespace::core::value::schemas::hash::{Blake3, Handle};
use triblespace::core::value::schemas::iu256::U256BE;
use triblespace::macros::{find, pattern};
use triblespace::prelude::View;

use GORBIE::NotebookCtx;
use GORBIE::cards::with_padding;
use GORBIE::dataflow::ComputedState;
use GORBIE::themes;
use GORBIE::widgets;
use GORBIE::widgets::triblespace::{PileRepoState, PileRepoWidget};

use GORBIE::telemetry::schema as t;

type CommitHandle = Value<Handle<Blake3, SimpleArchive>>;

struct RepoGuard {
    repo: Option<Repository<Pile<Blake3>>>,
}

impl RepoGuard {
    fn new(repo: Repository<Pile<Blake3>>) -> Self {
        Self { repo: Some(repo) }
    }

    fn as_mut(&mut self) -> Option<&mut Repository<Pile<Blake3>>> {
        self.repo.as_mut()
    }
}

impl std::fmt::Debug for RepoGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepoGuard")
            .field("repo_open", &self.repo.is_some())
            .finish()
    }
}

impl Drop for RepoGuard {
    fn drop(&mut self) {
        if let Some(repo) = self.repo.take() {
            // Avoid the "Pile dropped without calling close()" warning.
            let _ = repo.close();
        }
    }
}

struct RepoCache {
    open_path: Option<PathBuf>,
    repo: Option<RepoGuard>,
    signing_key: SigningKey,
}

impl Default for RepoCache {
    fn default() -> Self {
        Self {
            open_path: None,
            repo: None,
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }
}

impl std::fmt::Debug for RepoCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepoCache")
            .field("open_path", &self.open_path)
            .field("repo_open", &self.repo.is_some())
            .finish()
    }
}

impl RepoCache {
    fn ensure_open(&mut self, pile_path: &Path) -> Result<(), String> {
        let open_path = pile_path.to_path_buf();
        let path_changed = self
            .open_path
            .as_ref()
            .map_or(true, |existing| existing != &open_path);

        if path_changed || self.repo.is_none() {
            self.repo = None;
            let mut pile =
                Pile::<Blake3>::open(&open_path).map_err(|err| format!("open pile: {err:?}"))?;
            if let Err(err) = pile.restore() {
                let _ = pile.close();
                return Err(format!("restore pile: {err:?}"));
            }
            let repo = Repository::new(pile, self.signing_key.clone());
            self.repo = Some(RepoGuard::new(repo));
            self.open_path = Some(open_path);
        }

        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn fmt_duration_ns(ns: u64) -> String {
    const US: u64 = 1_000;
    const MS: u64 = 1_000_000;
    const S: u64 = 1_000_000_000;

    if ns < US {
        format!("{ns}ns")
    } else if ns < MS {
        format!("{:.1}us", ns as f64 / US as f64)
    } else if ns < S {
        format!("{:.3}ms", ns as f64 / MS as f64)
    } else {
        format!("{:.3}s", ns as f64 / S as f64)
    }
}

fn nice_time_step_ns(target_ns: u64) -> u64 {
    let target = target_ns.max(1) as f64;
    let exp = target.log10().floor();
    let base = 10f64.powf(exp);
    let mant = target / base;
    let nice = if mant <= 1.0 {
        1.0
    } else if mant <= 2.0 {
        2.0
    } else if mant <= 5.0 {
        5.0
    } else {
        10.0
    };
    (nice * base).round().max(1.0) as u64
}

fn contains_case_insensitive_ascii(haystack: &str, needle_lc: &[u8]) -> bool {
    if needle_lc.is_empty() {
        return true;
    }
    let hay = haystack.as_bytes();
    if needle_lc.len() > hay.len() {
        return false;
    }
    for start in 0..=hay.len().saturating_sub(needle_lc.len()) {
        let mut ok = true;
        for (offset, &needle) in needle_lc.iter().enumerate() {
            if hay[start + offset].to_ascii_lowercase() != needle {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
    }
    false
}

fn u256be_to_u64(value: Value<U256BE>) -> Option<u64> {
    let raw = value.raw;
    if raw[..24].iter().any(|byte| *byte != 0) {
        return None;
    }
    let bytes: [u8; 8] = raw[24..32].try_into().ok()?;
    Some(u64::from_be_bytes(bytes))
}

fn load_longstring(
    ws: &mut triblespace::core::repo::Workspace<Pile<Blake3>>,
    handle: Value<Handle<Blake3, LongString>>,
    cache: &mut HashMap<[u8; 32], String>,
) -> Result<String, String> {
    if let Some(value) = cache.get(&handle.raw) {
        return Ok(value.clone());
    }
    let view: View<str> = ws
        .get(handle)
        .map_err(|err| format!("load longstring: {err:?}"))?;
    let value = view.to_string();
    cache.insert(handle.raw, value.clone());
    Ok(value)
}

#[derive(Clone, Debug)]
struct BranchInfo {
    id: triblespace::core::id::Id,
    meta: CommitHandle,
    name: String,
}

fn scan_branches(
    repo: &mut Repository<Pile<Blake3>>,
    prefix: &str,
    prev: &[BranchInfo],
) -> Result<Vec<BranchInfo>, String> {
    let mut prev_by_id: HashMap<triblespace::core::id::Id, (CommitHandle, &str)> = HashMap::new();
    for info in prev {
        prev_by_id.insert(info.id, (info.meta, info.name.as_str()));
    }

    let iter = repo
        .storage_mut()
        .branches()
        .map_err(|err| format!("list branches: {err:?}"))?;

    let mut reader = None;
    let mut out = Vec::new();
    for item in iter {
        let branch_id = item.map_err(|err| format!("branch id: {err:?}"))?;
        let Some(meta) = repo
            .storage_mut()
            .head(branch_id)
            .map_err(|err| format!("branch head: {err:?}"))?
        else {
            continue;
        };

        let name = match prev_by_id.get(&branch_id) {
            Some((prev_meta, prev_name)) if *prev_meta == meta => (*prev_name).to_owned(),
            _ => {
                if reader.is_none() {
                    reader = Some(
                        repo.storage_mut()
                            .reader()
                            .map_err(|err| format!("open pile reader: {err:?}"))?,
                    );
                }
                let reader = reader.as_ref().expect("reader missing after init");
                let meta_set: TribleSet = reader
                    .get(meta)
                    .map_err(|err| format!("branch metadata blob: {err:?}"))?;
                let mut names = find!(
                    (handle: Value<Handle<Blake3, LongString>>),
                    pattern!(&meta_set, [{ metadata::name: ?handle }])
                )
                .into_iter();
                let Some((handle,)) = names.next() else {
                    continue;
                };
                let view: View<str> = reader
                    .get(handle)
                    .map_err(|err| format!("read branch name blob: {err:?}"))?;
                view.to_string()
            }
        };

        if !prefix.is_empty() && !name.starts_with(prefix) {
            continue;
        }

        out.push(BranchInfo {
            id: branch_id,
            meta,
            name,
        });
    }

    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

#[derive(Clone, Debug)]
struct SpanRecord {
    category: String,
    name: String,
    source: Option<String>,
    begin_ns: u64,
    duration_ns: Option<u64>,
}

#[derive(Clone, Debug)]
struct Hotspot {
    label: String,
    count: u64,
    total_ns: u64,
    max_ns: u64,
}

#[derive(Clone, Debug)]
struct FlameSpan {
    id: triblespace::core::id::Id,
    parent: Option<triblespace::core::id::Id>,
    category: String,
    name: String,
    source: Option<String>,
    begin_ns: u64,
    duration_ns: u64,
}

#[derive(Clone, Debug)]
struct CollapsedSpan {
    category: String,
    name: String,
    source: Option<String>,
    self_ns: u64,
    total_ns: u64,
    children: Vec<CollapsedSpan>,
}

#[derive(Clone, Debug)]
struct SessionSnapshot {
    head_timestamp_ms: Option<u64>,
    session_title: Option<String>,
    session_duration_ns: Option<u64>,
    spans_total: usize,
    flame: Vec<FlameSpan>,
    collapsed: Vec<CollapsedSpan>,
    spans_open: Vec<SpanRecord>,
    spans_slowest: Vec<SpanRecord>,
    hotspots: Vec<Hotspot>,
}

#[derive(Clone, Debug, Default)]
struct SpanState {
    category: Option<String>,
    name: Option<String>,
    source: Option<String>,
    parent: Option<triblespace::core::id::Id>,
    begin_ns: Option<u64>,
    duration_ns: Option<u64>,
}

#[derive(Clone, Debug)]
struct SlowEntry {
    duration_ns: u64,
    record: SpanRecord,
}

impl PartialEq for SlowEntry {
    fn eq(&self, other: &Self) -> bool {
        self.duration_ns == other.duration_ns && self.record.name == other.record.name
    }
}

impl Eq for SlowEntry {}

impl PartialOrd for SlowEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SlowEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering: we want a min-heap by duration so we can cheaply
        // keep the top-k slowest spans.
        other
            .duration_ns
            .cmp(&self.duration_ns)
            .then_with(|| other.record.name.cmp(&self.record.name))
    }
}

#[derive(Clone, Debug)]
struct SessionIndex {
    branch_id: triblespace::core::id::Id,
    head: Option<CommitHandle>,
    head_timestamp_ms: Option<u64>,
    session_title: Option<String>,
    session_duration_ns: Option<u64>,
    long_cache: HashMap<[u8; 32], String>,
    spans: HashMap<triblespace::core::id::Id, SpanState>,
    open: HashSet<triblespace::core::id::Id>,
    slowest: BinaryHeap<SlowEntry>,
    hotspots: HashMap<String, Hotspot>,
}

impl SessionIndex {
    fn new(branch_id: triblespace::core::id::Id) -> Self {
        Self {
            branch_id,
            head: None,
            head_timestamp_ms: None,
            session_title: None,
            session_duration_ns: None,
            long_cache: HashMap::new(),
            spans: HashMap::new(),
            open: HashSet::new(),
            slowest: BinaryHeap::new(),
            hotspots: HashMap::new(),
        }
    }

    fn snapshot(&self) -> SessionSnapshot {
        let mut flame = Vec::new();
        for (span_id, span) in &self.spans {
            let (Some(begin_ns), Some(duration_ns)) = (span.begin_ns, span.duration_ns) else {
                continue;
            };

            flame.push(FlameSpan {
                id: *span_id,
                parent: span.parent,
                category: span.category.clone().unwrap_or_default(),
                name: span.name.clone().unwrap_or_else(|| format!("{span_id:x}")),
                source: span.source.clone(),
                begin_ns,
                duration_ns,
            });
        }
        flame.sort_by(|a, b| {
            a.begin_ns
                .cmp(&b.begin_ns)
                .then_with(|| a.duration_ns.cmp(&b.duration_ns))
                .then_with(|| a.name.cmp(&b.name))
        });

        let collapsed = build_collapsed_flamegraph(&flame);

        let mut spans_open = Vec::new();
        for span_id in self.open.iter().copied() {
            let Some(span) = self.spans.get(&span_id) else {
                continue;
            };
            spans_open.push(SpanRecord {
                category: span.category.clone().unwrap_or_default(),
                name: span.name.clone().unwrap_or_else(|| format!("{span_id:x}")),
                source: span.source.clone(),
                begin_ns: span.begin_ns.unwrap_or(0),
                duration_ns: span.duration_ns,
            });
        }
        spans_open.sort_by(|a, b| {
            a.begin_ns
                .cmp(&b.begin_ns)
                .then_with(|| a.name.cmp(&b.name))
        });

        let mut spans_slowest: Vec<SpanRecord> =
            self.slowest.iter().map(|e| e.record.clone()).collect();
        spans_slowest.sort_by(|a, b| {
            b.duration_ns
                .unwrap_or(0)
                .cmp(&a.duration_ns.unwrap_or(0))
                .then_with(|| a.name.cmp(&b.name))
        });

        let mut hotspots: Vec<Hotspot> = self.hotspots.values().cloned().collect();
        hotspots.sort_by(|a, b| {
            b.total_ns
                .cmp(&a.total_ns)
                .then_with(|| b.max_ns.cmp(&a.max_ns))
        });

        SessionSnapshot {
            head_timestamp_ms: self.head_timestamp_ms,
            session_title: self.session_title.clone(),
            session_duration_ns: self.session_duration_ns,
            spans_total: self.spans.len(),
            flame,
            collapsed,
            spans_open: spans_open.into_iter().take(50).collect(),
            spans_slowest: spans_slowest.into_iter().take(60).collect(),
            hotspots: hotspots.into_iter().take(40).collect(),
        }
    }
}

fn build_collapsed_flamegraph(flame: &[FlameSpan]) -> Vec<CollapsedSpan> {
    if flame.is_empty() {
        return Vec::new();
    }

    #[derive(Default)]
    struct StrInterner {
        map: HashMap<Arc<str>, u32>,
        values: Vec<Arc<str>>,
    }

    impl StrInterner {
        fn intern(&mut self, value: &str) -> u32 {
            if let Some(&id) = self.map.get(value) {
                return id;
            }
            let arc: Arc<str> = Arc::from(value);
            let id = self.values.len() as u32;
            self.values.push(arc.clone());
            self.map.insert(arc, id);
            id
        }

        fn get(&self, id: u32) -> &str {
            self.values
                .get(id as usize)
                .map(|s| s.as_ref())
                .unwrap_or("")
        }
    }

    #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
    struct Key {
        category: u32,
        name: u32,
        source: Option<u32>,
    }

    #[derive(Debug)]
    struct Node {
        key: Key,
        self_ns: u64,
        total_ns: u64,
        children: Vec<Node>,
        index: HashMap<Key, usize>,
    }

    impl Node {
        fn new(key: Key) -> Self {
            Self {
                key,
                self_ns: 0,
                total_ns: 0,
                children: Vec::new(),
                index: HashMap::new(),
            }
        }

        fn child_mut(&mut self, key: Key) -> &mut Node {
            if let Some(&idx) = self.index.get(&key) {
                return &mut self.children[idx];
            }
            let idx = self.children.len();
            self.children.push(Node::new(key));
            self.index.insert(key, idx);
            &mut self.children[idx]
        }
    }

    #[derive(Default)]
    struct Root {
        children: Vec<Node>,
        index: HashMap<Key, usize>,
    }

    impl Root {
        fn child_mut(&mut self, key: Key) -> &mut Node {
            if let Some(&idx) = self.index.get(&key) {
                return &mut self.children[idx];
            }
            let idx = self.children.len();
            self.children.push(Node::new(key));
            self.index.insert(key, idx);
            &mut self.children[idx]
        }
    }

    let mut id_to_idx: HashMap<triblespace::core::id::Id, usize> = HashMap::new();
    id_to_idx.reserve(flame.len());
    for (idx, span) in flame.iter().enumerate() {
        id_to_idx.insert(span.id, idx);
    }

    let mut parent_idx: Vec<Option<usize>> = vec![None; flame.len()];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); flame.len()];
    for (idx, span) in flame.iter().enumerate() {
        let Some(parent_id) = span.parent else {
            continue;
        };
        let Some(parent) = id_to_idx.get(&parent_id).copied() else {
            continue;
        };
        parent_idx[idx] = Some(parent);
        children[parent].push(idx);
    }

    // Compute per-span self time by subtracting union(children) from the span's duration.
    let mut self_time: Vec<u64> = vec![0; flame.len()];
    for idx in 0..flame.len() {
        let begin = flame[idx].begin_ns;
        let end = begin.saturating_add(flame[idx].duration_ns);

        if children[idx].is_empty() {
            self_time[idx] = flame[idx].duration_ns;
            continue;
        }

        let mut intervals: Vec<(u64, u64)> = Vec::with_capacity(children[idx].len());
        for &child in &children[idx] {
            let child_begin = flame[child].begin_ns;
            let child_end = child_begin.saturating_add(flame[child].duration_ns);
            let start = child_begin.max(begin);
            let finish = child_end.min(end);
            if start < finish {
                intervals.push((start, finish));
            }
        }

        if intervals.is_empty() {
            self_time[idx] = flame[idx].duration_ns;
            continue;
        }

        intervals.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut covered = 0u64;
        let mut cur = intervals[0];
        for (start, finish) in intervals.into_iter().skip(1) {
            if start <= cur.1 {
                cur.1 = cur.1.max(finish);
            } else {
                covered = covered.saturating_add(cur.1.saturating_sub(cur.0));
                cur = (start, finish);
            }
        }
        covered = covered.saturating_add(cur.1.saturating_sub(cur.0));
        self_time[idx] = flame[idx].duration_ns.saturating_sub(covered);
    }

    let mut interner = StrInterner::default();
    let mut keys: Vec<Key> = Vec::with_capacity(flame.len());
    for span in flame {
        let key = Key {
            category: interner.intern(&span.category),
            name: interner.intern(&span.name),
            source: span.source.as_deref().map(|src| interner.intern(src)),
        };
        keys.push(key);
    }

    let mut root = Root::default();
    let mut path: Vec<usize> = Vec::new();
    for idx in 0..flame.len() {
        let self_ns = self_time[idx];
        if self_ns == 0 {
            continue;
        }

        path.clear();
        let mut cur = Some(idx);
        while let Some(pos) = cur {
            if path.contains(&pos) {
                break;
            }
            path.push(pos);
            cur = parent_idx[pos];
        }
        path.reverse();

        let first = path[0];
        let mut node = root.child_mut(keys[first]);
        node.total_ns = node.total_ns.saturating_add(self_ns);
        for &pos in path.iter().skip(1) {
            let child = node.child_mut(keys[pos]);
            node = child;
            node.total_ns = node.total_ns.saturating_add(self_ns);
        }
        node.self_ns = node.self_ns.saturating_add(self_ns);
    }

    fn into_span(node: Node, interner: &StrInterner) -> CollapsedSpan {
        let mut children: Vec<CollapsedSpan> = node
            .children
            .into_iter()
            .map(|c| into_span(c, interner))
            .collect();
        children.sort_by(|a, b| {
            b.total_ns
                .cmp(&a.total_ns)
                .then_with(|| a.name.cmp(&b.name))
        });

        CollapsedSpan {
            category: interner.get(node.key.category).to_string(),
            name: interner.get(node.key.name).to_string(),
            source: node.key.source.map(|id| interner.get(id).to_string()),
            self_ns: node.self_ns,
            total_ns: node.total_ns,
            children,
        }
    }

    let mut out: Vec<CollapsedSpan> = root
        .children
        .into_iter()
        .map(|n| into_span(n, &interner))
        .collect();
    out.sort_by(|a, b| {
        b.total_ns
            .cmp(&a.total_ns)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

fn load_session(
    cache: &mut RepoCache,
    pile_path: PathBuf,
    branch_id: triblespace::core::id::Id,
    prev: Option<SessionIndex>,
) -> Result<SessionIndex, String> {
    cache.ensure_open(&pile_path)?;
    let repo = cache
        .repo
        .as_mut()
        .and_then(|repo| repo.as_mut())
        .ok_or_else(|| "repo missing after open".to_owned())?;

    let reader = repo
        .storage_mut()
        .reader()
        .map_err(|err| format!("open pile reader: {err:?}"))?;

    let mut ws = repo
        .pull(branch_id)
        .map_err(|err| format!("pull branch {branch_id:x}: {err:?}"))?;

    let head = ws.head();
    let head_timestamp_ms = ws.head().and_then(|head| {
        reader
            .metadata(head)
            .ok()
            .flatten()
            .map(|meta| meta.timestamp)
    });

    let mut index = prev
        .filter(|prev| prev.branch_id == branch_id)
        .unwrap_or_else(|| SessionIndex::new(branch_id));

    index.head_timestamp_ms = head_timestamp_ms;

    // Load only the delta since the last head we observed, and incrementally
    // update the derived index structures. This keeps refresh cheap for long
    // sessions.
    let space = match (index.head, head) {
        (Some(prev_head), Some(new_head)) if prev_head == new_head => TribleSet::new(),
        (Some(prev_head), Some(_)) => ws
            .checkout(prev_head..)
            .map_err(|err| format!("checkout delta: {err}"))?,
        (None, Some(_)) => ws
            .checkout(..)
            .map_err(|err| format!("checkout branch: {err}"))?,
        (_, None) => TribleSet::new(),
    };
    index.head = head;

    if index.session_title.is_none() {
        let session_title = find!(
            (title: Value<Handle<Blake3, LongString>>),
            pattern!(&space, [{
                t::kind: t::kind_session,
                t::name: ?title,
            }])
        )
        .into_iter()
        .next()
        .map(|(h,)| h);
        if let Some(h) = session_title {
            index.session_title = Some(load_longstring(&mut ws, h, &mut index.long_cache)?);
        }
    }

    // Session summary (optional until shutdown).
    let session_dur = find!(
        (dur: Value<U256BE>),
        pattern!(&space, [{
            t::kind: t::kind_session,
            t::duration_ns: ?dur,
        }])
    )
    .into_iter()
    .next()
    .and_then(|(v,)| u256be_to_u64(v));
    if session_dur.is_some() {
        index.session_duration_ns = session_dur;
    }

    // Span begin facts.
    for (span_id, category, name_handle, begin_raw) in find!(
        (
            span: triblespace::core::id::Id,
            category: String,
            name: Value<Handle<Blake3, LongString>>,
            begin: Value<U256BE>
        ),
        pattern!(&space, [{
            ?span @
                t::kind: t::kind_span,
                t::category: ?category,
                t::name: ?name,
                t::begin_ns: ?begin,
        }])
    ) {
        let begin_ns = u256be_to_u64(begin_raw).unwrap_or(0);
        let name = load_longstring(&mut ws, name_handle, &mut index.long_cache)?;

        let span = index.spans.entry(span_id).or_default();
        span.category = Some(category);
        span.name = Some(name);
        span.begin_ns = Some(begin_ns);

        if span.duration_ns.is_none() {
            index.open.insert(span_id);
        }
    }

    for (span_id, parent_id) in find!(
        (span: triblespace::core::id::Id, parent: triblespace::core::id::Id),
        pattern!(&space, [{ ?span @ t::parent: ?parent }])
    ) {
        let span = index.spans.entry(span_id).or_default();
        span.parent = Some(parent_id);
    }

    for (span_id, src_handle) in find!(
        (
            span: triblespace::core::id::Id,
            src: Value<Handle<Blake3, LongString>>
        ),
        pattern!(&space, [{ ?span @ t::source: ?src }])
    ) {
        let source = load_longstring(&mut ws, src_handle, &mut index.long_cache)?;
        let span = index.spans.entry(span_id).or_default();
        span.source = Some(source);
    }

    // Span completion facts.
    for (span_id, dur_raw) in find!(
        (span: triblespace::core::id::Id, dur: Value<U256BE>),
        pattern!(&space, [{ ?span @ t::duration_ns: ?dur }])
    ) {
        let Some(dur) = u256be_to_u64(dur_raw) else {
            continue;
        };

        let span = index.spans.entry(span_id).or_default();
        if span.duration_ns.replace(dur).is_some() {
            continue;
        }

        index.open.remove(&span_id);

        let record = SpanRecord {
            category: span.category.clone().unwrap_or_default(),
            name: span.name.clone().unwrap_or_else(|| format!("{span_id:x}")),
            source: span.source.clone(),
            begin_ns: span.begin_ns.unwrap_or(0),
            duration_ns: Some(dur),
        };

        // Keep slowest spans (top-k).
        const SLOWEST_K: usize = 60;
        let entry = SlowEntry {
            duration_ns: dur,
            record: record.clone(),
        };
        if index.slowest.len() < SLOWEST_K {
            index.slowest.push(entry);
        } else if let Some(keep) = index.slowest.peek() {
            if dur > keep.duration_ns {
                let _ = index.slowest.pop();
                index.slowest.push(entry);
            }
        }

        // Aggregate hotspots by label.
        let label = match &record.source {
            Some(src) => format!("{} ({src})", record.name),
            None => record.name.clone(),
        };
        let entry = index.hotspots.entry(label.clone()).or_insert(Hotspot {
            label,
            count: 0,
            total_ns: 0,
            max_ns: 0,
        });
        entry.count += 1;
        entry.total_ns = entry.total_ns.saturating_add(dur);
        entry.max_ns = entry.max_ns.max(dur);
    }

    Ok(index)
}

#[derive(Default)]
struct SessionLoader {
    cache: RepoCache,
    result: Option<Result<SessionIndex, String>>,
}

impl std::fmt::Debug for SessionLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("SessionLoader");
        debug.field("cache", &self.cache);
        debug.field("loaded", &self.result.is_some());
        debug.finish()
    }
}

impl SessionLoader {
    fn refresh(&mut self, pile_path: PathBuf, branch_id: triblespace::core::id::Id) {
        let prev = self
            .result
            .take()
            .and_then(|res| res.ok())
            .filter(|prev| prev.branch_id == branch_id);

        self.result = Some(load_session(&mut self.cache, pile_path, branch_id, prev));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlameMode {
    Timeline,
    Collapsed,
}

impl Default for FlameMode {
    fn default() -> Self {
        Self::Timeline
    }
}

#[derive(Debug)]
struct ViewerState {
    branch_prefix: String,
    branches: Vec<BranchInfo>,
    selected: Option<usize>,
    filter_text: String,
    min_duration_ms: u64,
    last_snapshot: Option<SessionSnapshot>,
    last_snapshot_head: Option<CommitHandle>,
    session: ComputedState<SessionLoader>,
    last_repo_path: Option<PathBuf>,
    last_repo_open: bool,
    last_loaded_branch: Option<triblespace::core::id::Id>,
    last_loaded_meta: Option<CommitHandle>,
    flame_mode: FlameMode,
    flame_zoom: f32,
    selected_span: Option<triblespace::core::id::Id>,
    selected_collapsed: Option<u64>,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            branch_prefix: "telemetry-".to_owned(),
            branches: Vec::new(),
            selected: None,
            filter_text: String::new(),
            min_duration_ms: 0,
            last_snapshot: None,
            last_snapshot_head: None,
            session: ComputedState::default(),
            last_repo_path: None,
            last_repo_open: false,
            last_loaded_branch: None,
            last_loaded_meta: None,
            flame_mode: FlameMode::default(),
            flame_zoom: 1.0,
            selected_span: None,
            selected_collapsed: None,
        }
    }
}

pub fn notebook(nb: &mut NotebookCtx) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;

    nb.view(|ui| {
        widgets::markdown(
            ui,
            "# Tracing telemetry viewer\n\nThis reads span telemetry emitted by any process using `triblespace::telemetry` with `TELEMETRY_PILE` set.\n\nTip: pass the pile path as the first CLI arg to override the environment variable.",
        );
    });

    let pile_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("TELEMETRY_PILE").ok())
        .unwrap_or_else(|| "./telemetry.pile".to_owned());

    let repo_state = nb.state("repo", PileRepoState::new(pile_path), move |ui, repo| {
        with_padding(ui, padding, |ui| {
            ui.heading("Pile");
            PileRepoWidget::new(repo).show(ui);
        });
    });

    nb.state("viewer", ViewerState::default(), move |ui, state| {
        let mut repo_state_guard = repo_state.read_mut(ui);
        with_padding(ui, padding, |ui| {
            state.session.poll();

            {
                let open_path = repo_state_guard.open_path().map(|p| p.to_path_buf());
                let is_open = repo_state_guard.is_open();
                if open_path != state.last_repo_path || is_open != state.last_repo_open {
                    state.last_repo_path = open_path;
                    state.last_repo_open = is_open;
                    state.branches.clear();
                    state.selected = None;
                    state.last_snapshot = None;
                    state.last_snapshot_head = None;
                    state.last_loaded_branch = None;
                    state.last_loaded_meta = None;
                    state.selected_span = None;
                    state.selected_collapsed = None;
                    state.session.set(SessionLoader::default());
                }
            }

            ui.horizontal(|ui| {
                widgets::row_label(ui, "Branch prefix:");
                ui.add_sized(
                    [ui.available_width(), 0.0],
                    widgets::TextField::singleline(&mut state.branch_prefix),
                );
            });

            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                let repo_open = repo_state_guard.is_open();

                if repo_open {
                    let prev_selected = state
                        .selected
                        .and_then(|idx| state.branches.get(idx))
                        .map(|b| b.id);
                    if let Some(repo) = repo_state_guard.repo_mut() {
                        let scanned =
                            scan_branches(repo, state.branch_prefix.trim(), &state.branches);
                        match scanned {
                            Ok(branches) => {
                                state.branches = branches;
                                state.selected = prev_selected
                                    .and_then(|id| state.branches.iter().position(|b| b.id == id));
                                if state.selected.is_none() && !state.branches.is_empty() {
                                    state.selected = Some(0);
                                }
                            }
                            Err(err) => {
                                state.session.value_mut().result = Some(Err(err));
                            }
                        }
                    }
                } else {
                    state.branches.clear();
                    state.selected = None;
                }

                ui.add_space(12.0);
                widgets::row_label(ui, "Session:");
                egui::ComboBox::from_id_salt("telemetry_branch")
                    .selected_text(
                        state
                            .selected
                            .and_then(|idx| state.branches.get(idx))
                            .map(|b| b.name.as_str())
                            .unwrap_or("<none>"),
                    )
                    .show_ui(ui, |ui| {
                        for (idx, branch) in state.branches.iter().enumerate() {
                            let selected = state.selected == Some(idx);
                            if ui.selectable_label(selected, &branch.name).clicked() {
                                state.selected = Some(idx);
                            }
                        }
                    });

                if state.session.is_running() {
                    ui.add(egui::Spinner::new());
                    ui.label("Loading…");
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                widgets::row_label(ui, "Filter:");
                ui.add_sized(
                    [ui.available_width(), 0.0],
                    widgets::TextField::singleline(&mut state.filter_text),
                )
                .on_hover_text("Case-insensitive substring filter (matches name/source/category).");
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                widgets::row_label(ui, "Min duration(ms):");
                ui.add(
                    widgets::NumberField::new(&mut state.min_duration_ms)
                        .constrain_value(&|_, next| next.min(60_000))
                        .speed(25.0),
                )
                .on_hover_text("Filters slowest spans + hotspots (by max span duration).");
            });

            if repo_state_guard.is_open() && !state.session.is_running() {
                let selected = state
                    .selected
                    .and_then(|idx| state.branches.get(idx))
                    .map(|b| (b.id, b.meta));

                if let Some((branch_id, branch_meta)) = selected {
                    let needs_refresh = match state.session.value().result.as_ref() {
                        Some(Ok(_)) => {
                            state.last_loaded_branch != Some(branch_id)
                                || state.last_loaded_meta != Some(branch_meta)
                        }
                        Some(Err(_)) | None => true,
                    };

                    if needs_refresh {
                        if let Some(Ok(index)) = state.session.value().result.as_ref() {
                            let head = index.head;
                            if state.last_snapshot_head != head || state.last_snapshot.is_none() {
                                state.last_snapshot = Some(index.snapshot());
                                state.last_snapshot_head = head;
                            }
                        } else if state.last_loaded_branch != Some(branch_id) {
                            // Avoid showing an unrelated snapshot while switching sessions.
                            state.last_snapshot = None;
                            state.last_snapshot_head = None;
                            state.selected_span = None;
                            state.selected_collapsed = None;
                        }

                        state.last_loaded_branch = Some(branch_id);
                        state.last_loaded_meta = Some(branch_meta);

                        let pile_path = PathBuf::from(repo_state_guard.pile_path().trim());
                        let mut loader = std::mem::take(state.session.value_mut());
                        state.session.spawn(move || {
                            loader.refresh(pile_path, branch_id);
                            loader
                        });
                        ui.ctx().request_repaint_after(Duration::from_millis(50));
                    }
                }
            }

            ui.add_space(10.0);

            match state.session.value().result.as_ref() {
                None => {
                    if state.session.is_running() {
                        if let Some(snapshot) = state.last_snapshot.as_ref() {
                            let ViewerState {
                                flame_mode,
                                flame_zoom,
                                selected_span,
                                selected_collapsed,
                                filter_text,
                                min_duration_ms,
                                ..
                            } = state;
                            show_snapshot(
                                ui,
                                flame_mode,
                                flame_zoom,
                                selected_span,
                                selected_collapsed,
                                snapshot,
                                filter_text,
                                *min_duration_ms,
                            );
                        } else {
                            ui.label(egui::RichText::new("Loading…").italics().small());
                        }
                    } else {
                        ui.label(
                            egui::RichText::new(
                                "No session loaded (open pile and select a session).",
                            )
                            .italics()
                            .small(),
                        );
                    }
                }
                Some(Err(err)) => {
                    ui.label(
                        egui::RichText::new(err)
                            .color(ui.visuals().error_fg_color)
                            .monospace(),
                    );
                }
                Some(Ok(index)) => {
                    let head = index.head;
                    if state.last_snapshot_head != head || state.last_snapshot.is_none() {
                        state.last_snapshot = Some(index.snapshot());
                        state.last_snapshot_head = head;
                    }
                    if let Some(snapshot) = state.last_snapshot.as_ref() {
                        let ViewerState {
                            flame_mode,
                            flame_zoom,
                            selected_span,
                            selected_collapsed,
                            filter_text,
                            min_duration_ms,
                            ..
                        } = state;
                        show_snapshot(
                            ui,
                            flame_mode,
                            flame_zoom,
                            selected_span,
                            selected_collapsed,
                            snapshot,
                            filter_text,
                            *min_duration_ms,
                        );
                    }
                }
            }

            // Poll for new branch heads even without input.
            if repo_state_guard.is_open() {
                let poll_ms = std::env::var("TELEMETRY_FLUSH_MS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(250)
                    .max(10);
                ui.ctx()
                    .request_repaint_after(Duration::from_millis(poll_ms));
            }
        });
    });
}

fn show_snapshot(
    ui: &mut egui::Ui,
    flame_mode: &mut FlameMode,
    flame_zoom: &mut f32,
    selected_span: &mut Option<triblespace::core::id::Id>,
    selected_collapsed: &mut Option<u64>,
    snapshot: &SessionSnapshot,
    filter: &str,
    min_duration_ms: u64,
) {
    let filter = filter.trim();
    let filter_lc = filter.to_ascii_lowercase();
    let min_duration_ns = min_duration_ms.saturating_mul(1_000_000);

    let now = now_ms();

    ui.heading("Summary");
    ui.add_space(4.0);
    if let Some(title) = snapshot.session_title.as_deref() {
        ui.label(egui::RichText::new(title).monospace());
    }
    ui.label(format!(
        "Spans: {} total, {} open",
        snapshot.spans_total,
        snapshot.spans_open.len()
    ));
    if let Some(dur) = snapshot.session_duration_ns {
        ui.label(format!("Session duration: {}", fmt_duration_ns(dur)));
    }
    if let Some(ts) = snapshot.head_timestamp_ms {
        let age_ms = now.saturating_sub(ts);
        ui.label(format!("Last commit: {} ms ago ({}).", age_ms, ts));
    }

    ui.add_space(10.0);
    ui.heading("Flamegraph");
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label("View:");
        ui.add(
            widgets::ChoiceToggle::new(flame_mode)
                .choice(FlameMode::Timeline, "Timeline")
                .choice(FlameMode::Collapsed, "Collapsed")
                .small(),
        );
    });
    ui.add_space(4.0);

    match *flame_mode {
        FlameMode::Timeline => show_flamegraph_timeline(
            ui,
            flame_zoom,
            selected_span,
            snapshot,
            filter,
            &filter_lc,
            min_duration_ns,
        ),
        FlameMode::Collapsed => show_flamegraph_collapsed(
            ui,
            flame_zoom,
            selected_collapsed,
            snapshot,
            filter,
            &filter_lc,
            min_duration_ns,
        ),
    }

    ui.add_space(10.0);
    ui.heading("Open Spans");
    ui.add_space(4.0);
    if snapshot.spans_open.is_empty() {
        ui.label(egui::RichText::new("<none>").italics().small());
    } else {
        egui::ScrollArea::vertical()
            .id_salt("telemetry_open_spans")
            .max_height(220.0)
            .show(ui, |ui| {
                for span in snapshot.spans_open.iter().take(50) {
                    if !filter.is_empty() {
                        let category = span.category.to_ascii_lowercase();
                        let name = span.name.to_ascii_lowercase();
                        let source = span.source.as_deref().unwrap_or("").to_ascii_lowercase();
                        if !category.contains(&filter_lc)
                            && !name.contains(&filter_lc)
                            && !source.contains(&filter_lc)
                        {
                            continue;
                        }
                    }

                    let mut line = format!("{}  {}", span.category, span.name);
                    if let Some(src) = span.source.as_deref() {
                        line.push_str(&format!("  ({src})"));
                    }
                    ui.label(egui::RichText::new(line).monospace().small());
                }
            });
    }

    ui.add_space(10.0);
    ui.heading("Slowest Spans");
    ui.add_space(4.0);
    if snapshot.spans_slowest.is_empty() {
        ui.label(egui::RichText::new("<none>").italics().small());
    } else {
        egui::ScrollArea::vertical()
            .id_salt("telemetry_slowest_spans")
            .max_height(260.0)
            .show(ui, |ui| {
                for span in &snapshot.spans_slowest {
                    let dur = span.duration_ns.unwrap_or(0);
                    if dur < min_duration_ns {
                        continue;
                    }
                    if !filter.is_empty() {
                        let category = span.category.to_ascii_lowercase();
                        let name = span.name.to_ascii_lowercase();
                        let source = span.source.as_deref().unwrap_or("").to_ascii_lowercase();
                        if !category.contains(&filter_lc)
                            && !name.contains(&filter_lc)
                            && !source.contains(&filter_lc)
                        {
                            continue;
                        }
                    }

                    let mut line = format!("{:>10}  {}", fmt_duration_ns(dur), span.name);
                    if let Some(src) = span.source.as_deref() {
                        line.push_str(&format!("  ({src})"));
                    }
                    ui.label(egui::RichText::new(line).monospace().small());
                }
            });
    }

    ui.add_space(10.0);
    ui.heading("Hotspots (Total)");
    ui.add_space(4.0);
    if snapshot.hotspots.is_empty() {
        ui.label(egui::RichText::new("<none>").italics().small());
    } else {
        egui::ScrollArea::vertical()
            .id_salt("telemetry_hotspots")
            .max_height(280.0)
            .show(ui, |ui| {
                for hot in &snapshot.hotspots {
                    if hot.max_ns < min_duration_ns {
                        continue;
                    }
                    if !filter.is_empty() && !hot.label.to_ascii_lowercase().contains(&filter_lc) {
                        continue;
                    }

                    let avg = if hot.count == 0 {
                        0
                    } else {
                        (hot.total_ns / hot.count).min(hot.max_ns)
                    };
                    ui.label(
                        egui::RichText::new(format!(
                            "{:>10} total  {:>10} max  {:>10} avg  x{:>4}  {}",
                            fmt_duration_ns(hot.total_ns),
                            fmt_duration_ns(hot.max_ns),
                            fmt_duration_ns(avg),
                            hot.count,
                            hot.label
                        ))
                        .monospace()
                        .small(),
                    );
                }
            });
    }
}

fn show_flamegraph_timeline(
    ui: &mut egui::Ui,
    flame_zoom: &mut f32,
    selected_span: &mut Option<triblespace::core::id::Id>,
    snapshot: &SessionSnapshot,
    filter: &str,
    filter_lc: &str,
    min_duration_ns: u64,
) {
    if snapshot.flame.is_empty() {
        ui.label(egui::RichText::new("<none>").italics().small());
        return;
    }

    let needle = filter_lc.as_bytes();
    let mut visible = Vec::new();
    visible.reserve(snapshot.flame.len());
    for (idx, span) in snapshot.flame.iter().enumerate() {
        if span.duration_ns < min_duration_ns {
            continue;
        }
        if !filter.is_empty() {
            let source_matches = span
                .source
                .as_deref()
                .is_some_and(|src| contains_case_insensitive_ascii(src, needle));
            if !contains_case_insensitive_ascii(&span.category, needle)
                && !contains_case_insensitive_ascii(&span.name, needle)
                && !source_matches
            {
                continue;
            }
        }
        visible.push(idx);
    }

    if visible.is_empty() {
        ui.label(egui::RichText::new("<none>").italics().small());
        return;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Zoom: {:.2}x", *flame_zoom));
        ui.label(
            egui::RichText::new("(pinch or Ctrl+scroll)")
                .italics()
                .small(),
        );
        if ui
            .add(widgets::Button::new("Reset"))
            .on_hover_text("Reset flamegraph zoom to 1.0x.")
            .clicked()
        {
            *flame_zoom = 1.0;
        }
    });

    let mut id_to_pos: HashMap<triblespace::core::id::Id, usize> = HashMap::new();
    id_to_pos.reserve(visible.len());
    for (pos, &idx) in visible.iter().enumerate() {
        id_to_pos.insert(snapshot.flame[idx].id, pos);
    }

    let mut depth_cache: Vec<Option<usize>> = vec![None; visible.len()];

    fn depth_for(
        pos: usize,
        visible: &[usize],
        spans: &[FlameSpan],
        id_to_pos: &HashMap<triblespace::core::id::Id, usize>,
        depth_cache: &mut [Option<usize>],
        visiting: &mut HashSet<usize>,
    ) -> usize {
        if let Some(depth) = depth_cache[pos] {
            return depth;
        }
        if !visiting.insert(pos) {
            depth_cache[pos] = Some(0);
            return 0;
        }
        let idx = visible[pos];
        let depth = spans[idx]
            .parent
            .and_then(|parent| id_to_pos.get(&parent).copied())
            .map(|parent_pos| {
                depth_for(parent_pos, visible, spans, id_to_pos, depth_cache, visiting) + 1
            })
            .unwrap_or(0);
        visiting.remove(&pos);
        depth_cache[pos] = Some(depth);
        depth
    }

    let mut max_depth = 0usize;
    let mut visiting = HashSet::new();
    for pos in 0..visible.len() {
        let depth = depth_for(
            pos,
            &visible,
            &snapshot.flame,
            &id_to_pos,
            &mut depth_cache,
            &mut visiting,
        );
        max_depth = max_depth.max(depth);
    }

    let min_begin = visible
        .iter()
        .map(|&idx| snapshot.flame[idx].begin_ns)
        .min()
        .unwrap_or(0);
    let max_end = visible
        .iter()
        .map(|&idx| {
            snapshot.flame[idx]
                .begin_ns
                .saturating_add(snapshot.flame[idx].duration_ns)
        })
        .max()
        .unwrap_or(0);
    let total_ns = max_end.saturating_sub(min_begin).max(1);

    let row_h = 18.0;
    let axis_h = 18.0;
    let content_h = axis_h + (max_depth as f32 + 1.0) * row_h + 6.0;

    egui::ScrollArea::both()
        .id_salt("telemetry_flamegraph")
        .auto_shrink([false; 2])
        .max_height(320.0)
        .show(ui, |ui| {
            let available_w = ui.available_width().max(1.0);
            let px_per_ns_fit = available_w / total_ns as f32;
            let px_per_ns = (px_per_ns_fit * *flame_zoom).max(0.0000001);
            let content_w = (total_ns as f32 * px_per_ns).max(available_w);

            let (rect, _response) =
                ui.allocate_exact_size(egui::vec2(content_w, content_h), egui::Sense::hover());

            let pointer_inside = ui
                .input(|i| i.pointer.hover_pos())
                .is_some_and(|pos| rect.contains(pos));
            if pointer_inside {
                let zoom_delta = ui.input(|i| i.zoom_delta());
                if zoom_delta != 1.0 {
                    *flame_zoom = (*flame_zoom * zoom_delta).clamp(0.05, 200.0);
                    ui.ctx().request_repaint();
                }
            }

            let painter = ui.painter_at(rect);
            let clip = ui.clip_rect();
            let x0 = rect.min.x;
            let y0 = rect.min.y;

            let visible_left = (clip.min.x - x0).max(0.0);
            let visible_right = (clip.max.x - x0).max(0.0);

            // Time axis ticks/grid.
            let axis_clip = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y),
                egui::pos2(rect.max.x, rect.min.y + axis_h),
            );
            let axis_painter = painter.with_clip_rect(axis_clip);

            let ink = ui.visuals().text_color();
            let grid_major = themes::blend(ui.visuals().window_fill, ink, 0.16);
            let grid_minor = themes::blend(ui.visuals().window_fill, ink, 0.10);
            let baseline_stroke = egui::Stroke::new(1.0, grid_major);

            axis_painter.line_segment(
                [
                    egui::pos2(rect.min.x, rect.min.y + axis_h - 1.0),
                    egui::pos2(rect.max.x, rect.min.y + axis_h - 1.0),
                ],
                baseline_stroke,
            );

            let px_per_ns_f64 = px_per_ns as f64;
            let target_px = 110.0f64;
            let target_ns = (target_px / px_per_ns_f64).ceil().max(1.0) as u64;
            let step_ns = nice_time_step_ns(target_ns);

            let visible_ns_min = (visible_left as f64 / px_per_ns_f64).floor().max(0.0) as u64;
            let visible_ns_max = (visible_right as f64 / px_per_ns_f64).ceil().max(0.0) as u64;
            let first_tick = (visible_ns_min / step_ns) * step_ns;
            let last_tick = visible_ns_max.saturating_add(step_ns).min(total_ns);

            let label_font = egui::FontId::monospace(10.0);
            let label_color = themes::blend(ui.visuals().window_fill, ink, 0.7);

            let mut tick = first_tick;
            while tick <= last_tick {
                let x = rect.min.x + (tick as f64 * px_per_ns_f64) as f32;
                // Minor tick in the axis header.
                axis_painter.line_segment(
                    [
                        egui::pos2(x, rect.min.y + axis_h - 6.0),
                        egui::pos2(x, rect.min.y + axis_h - 1.0),
                    ],
                    egui::Stroke::new(1.0, grid_major),
                );

                // Faint grid line across the spans area.
                painter.line_segment(
                    [
                        egui::pos2(x, rect.min.y + axis_h),
                        egui::pos2(x, rect.max.y),
                    ],
                    egui::Stroke::new(1.0, grid_minor),
                );

                let label = fmt_duration_ns(tick);
                let x_rel = x - rect.min.x;
                let align = if x_rel < 30.0 {
                    egui::Align2::LEFT_TOP
                } else if x_rel > rect.width() - 30.0 {
                    egui::Align2::RIGHT_TOP
                } else {
                    egui::Align2::CENTER_TOP
                };
                axis_painter.text(
                    egui::pos2(x, rect.min.y + 2.0),
                    align,
                    label,
                    label_font.clone(),
                    label_color,
                );

                let next = tick.saturating_add(step_ns);
                if next == tick {
                    break;
                }
                tick = next;
            }

            for (pos, &idx) in visible.iter().enumerate() {
                let span = &snapshot.flame[idx];
                let depth = depth_cache[pos].unwrap_or(0);

                let x = (span.begin_ns.saturating_sub(min_begin) as f32) * px_per_ns;
                let w = span.duration_ns as f32 * px_per_ns;

                if w < 0.5 {
                    continue;
                }
                if x + w < visible_left || x > visible_right {
                    continue;
                }

                let y = axis_h + depth as f32 * row_h;
                let span_rect = egui::Rect::from_min_size(
                    egui::pos2(x0 + x, y0 + y),
                    egui::vec2(w, row_h - 2.0),
                );

                let fill = themes::colorhash::ral_categorical_key(&span.category, &span.name);

                let id = ui.id().with(("flame", span.id));
                let resp = ui.interact(span_rect, id, egui::Sense::click());
                if resp.clicked() {
                    *selected_span = Some(span.id);
                }
                if resp.hovered() {
                    resp.on_hover_ui(|ui| {
                        ui.label(egui::RichText::new(&span.name).strong());
                        ui.label(format!(
                            "{}  {}",
                            span.category,
                            fmt_duration_ns(span.duration_ns)
                        ));
                        if let Some(src) = span.source.as_deref() {
                            ui.label(egui::RichText::new(src).monospace().small());
                        }
                    });
                }

                let selected = *selected_span == Some(span.id);
                painter.rect_filled(span_rect, 2.0, fill);
                if selected {
                    let outline = themes::colorhash::highlight_stroke(fill);
                    painter.rect_stroke(span_rect, 2.0, outline, egui::StrokeKind::Inside);
                }

                if w > 32.0 {
                    let text_painter = painter.with_clip_rect(span_rect.shrink(2.0));
                    text_painter.text(
                        egui::pos2(span_rect.min.x + 4.0, span_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &span.name,
                        egui::FontId::monospace(10.0),
                        themes::colorhash::text_color_on(fill),
                    );
                }
            }
        });

    if let Some(sel) = *selected_span {
        if let Some(span) = snapshot.flame.iter().find(|s| s.id == sel) {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!(
                    "Selected: {}  {}",
                    span.name,
                    fmt_duration_ns(span.duration_ns)
                ))
                .monospace(),
            );
            if let Some(src) = span.source.as_deref() {
                ui.label(egui::RichText::new(src).monospace().small());
            }
        }
    }
}

fn show_flamegraph_collapsed(
    ui: &mut egui::Ui,
    flame_zoom: &mut f32,
    selected: &mut Option<u64>,
    snapshot: &SessionSnapshot,
    filter: &str,
    filter_lc: &str,
    min_duration_ns: u64,
) {
    if snapshot.collapsed.is_empty() {
        ui.label(egui::RichText::new("<none>").italics().small());
        return;
    }

    #[derive(Clone)]
    struct RenderNode<'a> {
        node: &'a CollapsedSpan,
        hash: u64,
        total_ns: u64,
        children: Vec<RenderNode<'a>>,
    }

    fn hash_with(mut hash: u64, bytes: &[u8]) -> u64 {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        hash
    }

    fn hash_node(parent: u64, node: &CollapsedSpan) -> u64 {
        let mut hash = parent ^ 1469598103934665603u64;
        hash = hash_with(hash, node.category.as_bytes());
        hash = hash_with(hash, &[0]);
        hash = hash_with(hash, node.name.as_bytes());
        hash = hash_with(hash, &[0]);
        if let Some(src) = node.source.as_deref() {
            hash = hash_with(hash, src.as_bytes());
            hash = hash_with(hash, &[0]);
        }
        hash
    }

    fn matches_filter(node: &CollapsedSpan, needle: &[u8]) -> bool {
        contains_case_insensitive_ascii(&node.category, needle)
            || contains_case_insensitive_ascii(&node.name, needle)
            || node
                .source
                .as_deref()
                .is_some_and(|src| contains_case_insensitive_ascii(src, needle))
    }

    fn build_render<'a>(
        node: &'a CollapsedSpan,
        parent_hash: u64,
        needle: &[u8],
        filter_empty: bool,
        min_duration_ns: u64,
    ) -> Option<RenderNode<'a>> {
        let hash = hash_node(parent_hash, node);

        let mut rendered_children = Vec::new();
        let mut child_total = 0u64;
        for child in &node.children {
            if let Some(rendered) = build_render(child, hash, needle, filter_empty, min_duration_ns)
            {
                child_total = child_total.saturating_add(rendered.total_ns);
                rendered_children.push(rendered);
            }
        }

        let total_ns = node.self_ns.saturating_add(child_total);
        if total_ns < min_duration_ns {
            return None;
        }

        if !filter_empty && !matches_filter(node, needle) && rendered_children.is_empty() {
            return None;
        }

        rendered_children.sort_by(|a, b| {
            b.total_ns
                .cmp(&a.total_ns)
                .then_with(|| a.node.name.cmp(&b.node.name))
        });

        Some(RenderNode {
            node,
            hash,
            total_ns,
            children: rendered_children,
        })
    }

    let needle = filter_lc.as_bytes();
    let filter_empty = filter.trim().is_empty();

    let mut roots: Vec<RenderNode<'_>> = Vec::new();
    for node in &snapshot.collapsed {
        if let Some(rendered) = build_render(node, 0, needle, filter_empty, min_duration_ns) {
            roots.push(rendered);
        }
    }

    if roots.is_empty() {
        ui.label(egui::RichText::new("<none>").italics().small());
        return;
    }

    roots.sort_by(|a, b| {
        b.total_ns
            .cmp(&a.total_ns)
            .then_with(|| a.node.name.cmp(&b.node.name))
    });

    let total_ns = roots
        .iter()
        .fold(0u64, |acc, n| acc.saturating_add(n.total_ns))
        .max(1);

    fn max_depth(node: &RenderNode<'_>) -> usize {
        let mut best = 1usize;
        for child in &node.children {
            best = best.max(1 + max_depth(child));
        }
        best
    }
    let depth = roots.iter().map(max_depth).max().unwrap_or(1);

    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Zoom: {:.2}x", *flame_zoom));
        ui.label(
            egui::RichText::new("(pinch or Ctrl+scroll)")
                .italics()
                .small(),
        );
        if ui
            .add(widgets::Button::new("Reset"))
            .on_hover_text("Reset flamegraph zoom to 1.0x.")
            .clicked()
        {
            *flame_zoom = 1.0;
        }
    });

    let row_h = 18.0;
    let axis_h = 18.0;
    let content_h = axis_h + depth as f32 * row_h + 6.0;

    egui::ScrollArea::both()
        .id_salt("telemetry_flamegraph_collapsed")
        .auto_shrink([false; 2])
        .max_height(320.0)
        .show(ui, |ui| {
            let available_w = ui.available_width().max(1.0);
            let content_w = (available_w * *flame_zoom).max(available_w);
            let (rect, _response) =
                ui.allocate_exact_size(egui::vec2(content_w, content_h), egui::Sense::hover());

            let pointer_inside = ui
                .input(|i| i.pointer.hover_pos())
                .is_some_and(|pos| rect.contains(pos));
            if pointer_inside {
                let zoom_delta = ui.input(|i| i.zoom_delta());
                if zoom_delta != 1.0 {
                    *flame_zoom = (*flame_zoom * zoom_delta).clamp(0.05, 200.0);
                    ui.ctx().request_repaint();
                }
            }

            let painter = ui.painter_at(rect);
            let clip = ui.clip_rect();
            let x0 = rect.min.x;
            let y0 = rect.min.y;

            let visible_left = (clip.min.x - x0).max(0.0);
            let visible_right = (clip.max.x - x0).max(0.0);

            // Time axis ticks/grid (collapsed view uses the aggregated total time).
            let axis_clip = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y),
                egui::pos2(rect.max.x, rect.min.y + axis_h),
            );
            let axis_painter = painter.with_clip_rect(axis_clip);

            let ink = ui.visuals().text_color();
            let grid_major = themes::blend(ui.visuals().window_fill, ink, 0.16);
            let grid_minor = themes::blend(ui.visuals().window_fill, ink, 0.10);
            let baseline_stroke = egui::Stroke::new(1.0, grid_major);

            axis_painter.line_segment(
                [
                    egui::pos2(rect.min.x, rect.min.y + axis_h - 1.0),
                    egui::pos2(rect.max.x, rect.min.y + axis_h - 1.0),
                ],
                baseline_stroke,
            );

            let px_per_ns = (content_w as f64 / total_ns.max(1) as f64).max(0.0000001);
            let target_px = 110.0f64;
            let target_ns = (target_px / px_per_ns).ceil().max(1.0) as u64;
            let step_ns = nice_time_step_ns(target_ns);

            let visible_ns_min = (visible_left as f64 / px_per_ns).floor().max(0.0) as u64;
            let visible_ns_max = (visible_right as f64 / px_per_ns).ceil().max(0.0) as u64;
            let first_tick = (visible_ns_min / step_ns) * step_ns;
            let last_tick = visible_ns_max.saturating_add(step_ns).min(total_ns);

            let label_font = egui::FontId::monospace(10.0);
            let label_color = themes::blend(ui.visuals().window_fill, ink, 0.7);

            let mut tick = first_tick;
            while tick <= last_tick {
                let x = rect.min.x + (tick as f64 * px_per_ns) as f32;
                axis_painter.line_segment(
                    [
                        egui::pos2(x, rect.min.y + axis_h - 6.0),
                        egui::pos2(x, rect.min.y + axis_h - 1.0),
                    ],
                    egui::Stroke::new(1.0, grid_major),
                );
                painter.line_segment(
                    [
                        egui::pos2(x, rect.min.y + axis_h),
                        egui::pos2(x, rect.max.y),
                    ],
                    egui::Stroke::new(1.0, grid_minor),
                );

                let label = fmt_duration_ns(tick);
                let x_rel = x - rect.min.x;
                let align = if x_rel < 30.0 {
                    egui::Align2::LEFT_TOP
                } else if x_rel > rect.width() - 30.0 {
                    egui::Align2::RIGHT_TOP
                } else {
                    egui::Align2::CENTER_TOP
                };
                axis_painter.text(
                    egui::pos2(x, rect.min.y + 2.0),
                    align,
                    label,
                    label_font.clone(),
                    label_color,
                );

                let next = tick.saturating_add(step_ns);
                if next == tick {
                    break;
                }
                tick = next;
            }

            fn draw_node(
                ui: &mut egui::Ui,
                painter: &egui::Painter,
                x0: f32,
                y0: f32,
                axis_h: f32,
                x: f32,
                depth: usize,
                w: f32,
                node: &RenderNode<'_>,
                visible_left: f32,
                visible_right: f32,
                selected: &mut Option<u64>,
            ) {
                if w < 0.5 {
                    return;
                }
                if x + w < visible_left || x > visible_right {
                    return;
                }

                let row_h = 18.0;
                let y = axis_h + depth as f32 * row_h;
                let rect = egui::Rect::from_min_size(
                    egui::pos2(x0 + x, y0 + y),
                    egui::vec2(w, row_h - 2.0),
                );

                let id = ui.id().with(("collapsed", node.hash));
                let resp = ui.interact(rect, id, egui::Sense::click());
                if resp.clicked() {
                    *selected = Some(node.hash);
                }
                if resp.hovered() {
                    resp.on_hover_ui(|ui| {
                        ui.label(egui::RichText::new(&node.node.name).strong());
                        ui.label(format!(
                            "{}  {} total  {} self",
                            node.node.category,
                            fmt_duration_ns(node.total_ns),
                            fmt_duration_ns(node.node.self_ns),
                        ));
                        if let Some(src) = node.node.source.as_deref() {
                            ui.label(egui::RichText::new(src).monospace().small());
                        }
                    });
                }

                let fill =
                    themes::colorhash::ral_categorical_key(&node.node.category, &node.node.name);

                let selected_now = *selected == Some(node.hash);
                painter.rect_filled(rect, 2.0, fill);
                if selected_now {
                    let outline = themes::colorhash::highlight_stroke(fill);
                    painter.rect_stroke(rect, 2.0, outline, egui::StrokeKind::Inside);
                }

                if w > 42.0 {
                    let text_painter = painter.with_clip_rect(rect.shrink(2.0));
                    text_painter.text(
                        egui::pos2(rect.min.x + 4.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &node.node.name,
                        egui::FontId::monospace(10.0),
                        themes::colorhash::text_color_on(fill),
                    );
                }

                let mut child_x = x;
                for child in &node.children {
                    let child_w = w * (child.total_ns as f32 / node.total_ns.max(1) as f32);
                    draw_node(
                        ui,
                        painter,
                        x0,
                        y0,
                        axis_h,
                        child_x,
                        depth + 1,
                        child_w,
                        child,
                        visible_left,
                        visible_right,
                        selected,
                    );
                    child_x += child_w;
                }
            }

            let mut x = 0.0f32;
            for node in &roots {
                let w = content_w * (node.total_ns as f32 / total_ns as f32);
                draw_node(
                    ui,
                    &painter,
                    x0,
                    y0,
                    axis_h,
                    x,
                    0,
                    w,
                    node,
                    visible_left,
                    visible_right,
                    selected,
                );
                x += w;
            }
        });

    if let Some(sel) = *selected {
        fn find<'a>(nodes: &'a [RenderNode<'a>], hash: u64) -> Option<&'a RenderNode<'a>> {
            for node in nodes {
                if node.hash == hash {
                    return Some(node);
                }
                if let Some(found) = find(&node.children, hash) {
                    return Some(found);
                }
            }
            None
        }

        if let Some(node) = find(&roots, sel) {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!(
                    "Selected: {}  {} total  {} self",
                    node.node.name,
                    fmt_duration_ns(node.total_ns),
                    fmt_duration_ns(node.node.self_ns)
                ))
                .monospace(),
            );
            if let Some(src) = node.node.source.as_deref() {
                ui.label(egui::RichText::new(src).monospace().small());
            }
        }
    }
}
