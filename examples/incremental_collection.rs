#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = "..", features = ["triblespace"] }
//! ed25519-dalek = "2.1"
//! tempfile = "3.26"
//! triblespace = { path = "../../triblespace-rs" }
//! ```
//!
//! A native, stateful notebook card which observes collection commits appended
//! through a second handle to the same Pile. The card polls a cheap store
//! revision at an absolute deadline; it does not own a worker thread.

use std::fs::File;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use eframe::egui;
use tempfile::TempDir;
use triblespace::core::blob::encodings::simplearchive::SimpleArchive;
use triblespace::core::collection::succinctarchive_union::{
    SuccinctArchiveCollection, SuccinctArchiveView,
};
use triblespace::core::collection::{
    reach, simplearchive_union, Collection, CollectionStoreExt, Cover, SimpleArchiveCollection,
};
use triblespace::core::examples::literature;
use triblespace::core::repo::pile::{Pile, PileRevision};
use triblespace::core::repo::{StoreRevision, StoreRevisionChanges};
use triblespace::prelude::*;

use GORBIE::cards::DEFAULT_CARD_PADDING;
use GORBIE::prelude::*;

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const MORE_TITLES: [&str; 4] = [
    "Dune Messiah",
    "Children of Dune",
    "God Emperor of Dune",
    "Heretics of Dune",
];

#[derive(Debug, Eq, PartialEq)]
enum Observation {
    NoCollectionChange,
    Folded(usize),
}

enum DemoError {
    Publication(String),
    Observation(String),
}

impl DemoError {
    fn message(&self) -> &str {
        match self {
            Self::Publication(message) | Self::Observation(message) => message,
        }
    }
}

struct Demo {
    observer: Pile,
    writer: Pile,
    collection: Collection<SimpleArchive>,
    signing_key: SigningKey,
    simple: SimpleArchiveCollection,
    full_view: SuccinctArchiveView,
    author: Id,
    checkpoint: Option<Cover<SimpleArchive>>,
    acknowledged_revision: Option<PileRevision>,
    observed_titles: Vec<String>,
    next_title: usize,
    next_probe: Option<Instant>,
    last_status: String,
    error: Option<DemoError>,
    // Fields drop in declaration order. Keep the directory last so both Pile
    // handles release the file before TempDir removes its parent directory.
    _directory: TempDir,
}

impl Demo {
    fn new() -> Result<Self, String> {
        let directory = tempfile::tempdir()
            .map_err(|error| format!("could not create a temporary directory: {error}"))?;
        let pile_path = directory.path().join("incremental-collection.pile");
        File::create(&pile_path)
            .map_err(|error| format!("could not create {}: {error}", pile_path.display()))?;

        // A deterministic demo key makes reruns legible; the temporary pile is
        // isolated per process and this key must not be reused as real authority.
        let signing_key = SigningKey::from_bytes(&[0x47; 32]);
        let authority = signing_key.verifying_key();
        let name = "gorbie-incremental-literature";
        let source_reach = reach::private();

        // Open the observer before publishing the initial commits. Seeing them
        // therefore exercises Pile's external-append refresh path immediately.
        let observer = Pile::open(&pile_path)
            .map_err(|error| format!("could not open observer pile: {error}"))?;
        let mut writer = Pile::open(&pile_path)
            .map_err(|error| format!("could not open writer pile: {error}"))?;
        let collection = writer
            .collection::<SimpleArchive>(simplearchive_union::descriptor(
                name,
                authority,
                source_reach.clone(),
            ))
            .map_err(|error| format!("could not register the collection: {error}"))?;

        let author = entity! {
            literature::firstname: "Frank",
            literature::lastname: "Herbert",
        };
        let author_id = author
            .root()
            .ok_or_else(|| "the intrinsic author fragment has no root".to_owned())?;
        writer
            .commit(collection, &signing_key, author)
            .map_err(|error| format!("could not publish the author: {error}"))?;
        writer
            .commit(
                collection,
                &signing_key,
                entity! {
                    literature::title: "Dune",
                    literature::author: &author_id,
                },
            )
            .map_err(|error| format!("could not publish the initial book: {error}"))?;

        let simple = SimpleArchiveCollection::new(name, authority, source_reach.clone());
        let succinct = SuccinctArchiveCollection::new(
            name,
            authority,
            source_reach,
            authority,
            reach::private(),
        );

        Ok(Self {
            observer,
            writer,
            collection,
            signing_key,
            simple,
            full_view: succinct.exact_view(),
            author: author_id,
            checkpoint: None,
            acknowledged_revision: None,
            observed_titles: Vec::new(),
            next_title: 0,
            next_probe: Some(Instant::now()),
            last_status: "Waiting for the initial observation".to_owned(),
            error: None,
            _directory: directory,
        })
    }

    fn next_title(&self) -> Option<&'static str> {
        MORE_TITLES.get(self.next_title).copied()
    }

    fn publish_next(&mut self) -> Result<&'static str, String> {
        let title = self
            .next_title()
            .ok_or_else(|| "all example commits have already been published".to_owned())?;
        self.writer
            .commit(
                self.collection,
                &self.signing_key,
                entity! {
                    literature::title: title,
                    literature::author: &self.author,
                },
            )
            .map_err(|error| format!("could not publish {title:?}: {error}"))?;
        self.next_title += 1;
        Ok(title)
    }

    fn observe(
        &mut self,
        mut consume: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<Observation, String> {
        // This is the observation boundary. If another handle appends after it,
        // acknowledging exactly `sampled` leaves that later write for the next
        // probe rather than accidentally swallowing it.
        let sampled = self
            .observer
            .store_revision()
            .map_err(|error| format!("could not sample the Pile revision: {error}"))?;

        if let Some(previous) = self.acknowledged_revision.as_ref() {
            let changes = <Pile as StoreRevision>::revision_changes(previous, &sampled);
            if !changes.contains(StoreRevisionChanges::COLLECTION_RECORDS) {
                // No collection cover could have changed, so there is no fold to retry.
                self.acknowledged_revision = Some(sampled);
                return Ok(Observation::NoCollectionChange);
            }
        }

        let current = self
            .observer
            .cover(self.collection)
            .map_err(|error| format!("could not discover the collection cover: {error}"))?;
        let added = match self.checkpoint.as_ref() {
            Some(previous) => current
                .additions_since(previous)
                .map_err(|error| format!("cover is not an additions-only advance: {error}"))?,
            None => current.clone(),
        };

        // Other collections' MERGE/DERIVE records share the same Pile-level
        // revision component. Once the exact source cover says there is no
        // semantic delta, acknowledge the sampled false positive without
        // touching either archive representation.
        if added.is_empty() {
            self.acknowledged_revision = Some(sampled);
            return Ok(Observation::NoCollectionChange);
        }

        let full = self
            .full_view
            .ensure(&mut self.observer, &current)
            .map_err(|error| format!("could not ensure the Succinct full view: {error}"))?;
        let changed = self
            .simple
            .attach_exact(&mut self.observer, &added)
            .map_err(|error| format!("could not attach the SimpleArchive delta: {error}"))?;

        let titles: Vec<String> = find!(
            title: String,
            pattern_changes!(&full, &changed, [
                { _?author @ literature::firstname: "Frank" },
                { _?book @
                    literature::author: _?author,
                    literature::title: ?title
                }
            ])
        )
        .collect();

        for title in &titles {
            consume(title)?;
        }

        // The consumer continuation and sampled revision advance only after
        // every fallible step and the complete fold succeeded. A failure
        // remains retryable; advancing the immutable full-view cache is safe.
        let count = titles.len();
        self.observed_titles.extend(titles);
        self.checkpoint = Some(current);
        self.acknowledged_revision = Some(sampled);
        Ok(Observation::Folded(count))
    }

    fn advance_probe_deadline(&mut self, previous_deadline: Instant) {
        let finished = Instant::now();
        let elapsed = finished.saturating_duration_since(previous_deadline);
        let steps = elapsed.as_nanos() / POLL_INTERVAL.as_nanos() + 1;
        let advanced = u32::try_from(steps)
            .ok()
            .and_then(|steps| previous_deadline.checked_add(POLL_INTERVAL * steps))
            .filter(|deadline| *deadline > finished)
            .unwrap_or(finished + POLL_INTERVAL);
        self.next_probe = Some(advanced);
    }
}

#[notebook]
fn main(nb: &mut NotebookCtx) {
    nb.state_with("incremental-collection", Demo::new, |ctx, state| {
        ctx.with_padding(DEFAULT_CARD_PADDING, |ctx| match state {
            Err(error) => {
                ctx.heading("Incremental collection");
                ctx.label(
                    egui::RichText::new(format!("Initialization failed: {error}"))
                        .color(egui::Color32::RED),
                );
                if ctx.button("Retry initialization").clicked() {
                    *state = Demo::new();
                }
            }
            Ok(demo) => {
                ctx.heading("Incremental collection");
                ctx.label("The observer and writer are separate handles to one append-only Pile.");
                ctx.label(
                    egui::RichText::new(
                        "The demo writer is the collection authority. The 100 ms gate does not \
                         model delegated-capability expiry; authorization clocks require their own \
                         wake-up policy even when no collection record changes.",
                    )
                    .weak(),
                );
                if let Some(title) = demo.next_title() {
                    if ctx
                        .button(format!("Publish {title:?} through writer"))
                        .clicked()
                    {
                        match demo.publish_next() {
                            Ok(title) => {
                                demo.last_status =
                                    format!("Published {title:?}; observation pending");
                                if matches!(demo.error.as_ref(), Some(DemoError::Publication(_))) {
                                    demo.error = None;
                                }
                                demo.next_probe = Some(Instant::now());
                            }
                            Err(error) => demo.error = Some(DemoError::Publication(error)),
                        }
                    }
                } else {
                    ctx.label("All example commits have been published.");
                }

                let now = Instant::now();
                if let Some(deadline) = demo.next_probe.filter(|deadline| now >= *deadline) {
                    match demo.observe(|_| Ok(())) {
                        Ok(Observation::Folded(count)) => {
                            demo.last_status = format!("Folded {count} newly supported title(s)");
                            if matches!(demo.error.as_ref(), Some(DemoError::Observation(_))) {
                                demo.error = None;
                            }
                            demo.advance_probe_deadline(deadline);
                        }
                        Ok(Observation::NoCollectionChange) => {
                            demo.last_status = "Collection is up to date".to_owned();
                            if matches!(demo.error.as_ref(), Some(DemoError::Observation(_))) {
                                demo.error = None;
                            }
                            demo.advance_probe_deadline(deadline);
                        }
                        Err(error) => {
                            demo.last_status =
                                "Observation failed; retry will fold the same delta".to_owned();
                            demo.error = Some(DemoError::Observation(error));
                            demo.next_probe = None;
                        }
                    }
                }

                ctx.label(&demo.last_status);
                if let Some(error) = &demo.error {
                    ctx.label(
                        egui::RichText::new(error.message())
                            .color(egui::Color32::RED)
                            .monospace(),
                    );
                    if matches!(error, DemoError::Observation(_))
                        && ctx.button("Retry observation now").clicked()
                    {
                        demo.next_probe = Some(Instant::now());
                    }
                }

                ctx.separator();
                ctx.label(format!(
                    "Checkpoint: {} payload member(s); observed incremental rows:",
                    demo.checkpoint.as_ref().map_or(0, Cover::len)
                ));
                for title in &demo.observed_titles {
                    ctx.label(format!("• {title}"));
                }

                // The deadline is retained in state. Unrelated repaints do not
                // postpone the probe by resetting a relative 100 ms timer.
                if let Some(deadline) = demo.next_probe {
                    ctx.ctx()
                        .request_repaint_after(deadline.saturating_duration_since(Instant::now()));
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observes_external_appends_and_retries_a_failed_fold() {
        let mut demo = Demo::new().expect("demo initialization");

        assert_eq!(demo.observe(|_| Ok(())).unwrap(), Observation::Folded(1));
        assert_eq!(demo.observed_titles, ["Dune"]);
        assert_eq!(
            demo.observe(|_| Ok(())).unwrap(),
            Observation::NoCollectionChange
        );
        assert_eq!(demo.observed_titles, ["Dune"]);

        demo.publish_next().expect("writer commit");
        let checkpoint_before_failure = demo.checkpoint.clone();
        let revision_before_failure = demo.acknowledged_revision.clone();
        let titles_before_failure = demo.observed_titles.clone();

        let error = demo
            .observe(|_| Err("simulated sink failure".to_owned()))
            .expect_err("the consumer must fail");
        assert_eq!(error, "simulated sink failure");
        assert_eq!(demo.checkpoint, checkpoint_before_failure);
        assert!(demo.acknowledged_revision == revision_before_failure);
        assert_eq!(demo.observed_titles, titles_before_failure);

        assert_eq!(demo.observe(|_| Ok(())).unwrap(), Observation::Folded(1));
        assert_eq!(demo.observed_titles, ["Dune", "Dune Messiah"]);
    }
}
