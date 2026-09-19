//! Durable, queryable simulation observations for Erlkonig.
//!
//! The numerical engine and the semantic store deliberately have different
//! shapes. CubeCL owns device-resident state evolution; this module is the
//! narrow observation boundary that records selected frames in a native
//! TribleSpace pile. A frame is not a copied simulator state: it is a small
//! semantic event with run/scenario provenance and explicitly named quantity
//! values.
//!
//! The schema is intentionally domain-neutral. A fluid pressure, a thermal
//! temperature, and a robot displacement all use the same frame/value shape;
//! their meaning comes from `quantity`, `unit`, and the optional `subject`.

use std::fs::File;
use std::path::Path;

use ed25519_dalek::SigningKey;
use triblespace::core::blob::encodings::simplearchive::SimpleArchive;
use triblespace::core::collection::{
    AdmissionPolicy, Collection, CollectionPolicy, CollectionStoreExt,
};
use triblespace::core::id::{ExclusiveId, Id, fucid};
use triblespace::core::inline::encodings::shortstring::ShortString;
use triblespace::core::inline::{Inline, IntoInline, TryToInline};
use triblespace::core::repo::pile::Pile;
use triblespace::macros::entity;

/// Stable attributes for the generic Erlkonig simulation observation layer.
///
/// These IDs were minted with `trible genid` on 2026-09-19. The attributes are
/// deliberately small and compositional: the frame carries provenance and
/// time, while each value carries a quantity/unit and can point at a subject.
pub mod snapshot {
    use triblespace::macros::attributes;
    use triblespace::prelude::inlineencodings;

    attributes! {
        /// Run identity referenced by a sampled frame.
        "9137E4E3F8B25BD30620869E9831FE38" as pub run: inlineencodings::GenId;
        /// Scenario identity referenced by a sampled frame.
        "927ADBA1D639B22AD3F8A26C4F28968B" as pub scenario: inlineencodings::GenId;
        /// Frame identity referenced by an observation value.
        "A0477010570F9A37D1A5A646696D1CDE" as pub frame: inlineencodings::GenId;
        /// Monotonic sample index within a run/scenario.
        "5BBF873AB6FB9DEAB31555AF77EA8E1D" as pub frame_index: inlineencodings::U256LE;
        /// Simulation time in the model's declared time unit (normally seconds).
        "D4403CED9F03AD50A54120CB0259B729" as pub time: inlineencodings::F64;
        /// Physical or logical subject whose quantity was sampled.
        "42FBDDC8AA3A6C6C3B69E531D562712C" as pub subject: inlineencodings::GenId;
        /// Domain-neutral quantity name, for example `pressure` or `temperature`.
        "C8779FF36EC89D6CC5896E4098B4AA74" as pub quantity: inlineencodings::ShortString;
        /// Numeric sampled value.
        "15C2F693D36E9A02ECDF51ADA04A88BE" as pub value: inlineencodings::F64;
        /// Unit label, for example `Pa`, `K`, or `m`.
        "871D4787F404344A47FE8A1CB4322BA4" as pub unit: inlineencodings::ShortString;
        /// Human-readable name attached to a run or scenario identity.
        "8C4F7C639DC5C6219657EDA2895E5E4E" as pub name: inlineencodings::ShortString;
    }
}

/// Collection name used by the generic snapshot writer.
pub const COLLECTION_NAME: &str = "erlkonig-simulation-snapshots";

/// One semantic quantity emitted for a sampled frame.
#[derive(Clone, Copy, Debug)]
pub struct SnapshotValue<'a> {
    /// Optional physical/logical subject. `None` is useful for system totals.
    pub subject: Option<Id>,
    /// Stable, domain-neutral quantity label (at most 32 UTF-8 bytes).
    pub quantity: &'a str,
    /// Numeric value at the frame time.
    pub value: f64,
    /// Unit label (at most 32 UTF-8 bytes).
    pub unit: &'a str,
}

/// Result of one append, useful to connect GPU observation bookkeeping to the
/// semantic identity that was persisted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapshotFrame {
    pub id: Id,
    pub index: u64,
    pub time: f64,
    pub values: usize,
}

/// A synchronous append-only writer for the native TribleSpace pile format.
///
/// The writer intentionally accepts explicit frame indices. A caller that
/// resumes a run can derive its next index from its own observation plan (or a
/// query) instead of trusting hidden mutable catalogue state. Re-appending the
/// same canonical frame is harmless: TribleSpace collection commits are
/// content-addressed and duplicate canonical records are idempotent.
pub struct SnapshotWriter {
    pile: Pile,
    collection: Collection<SimpleArchive>,
    signing_key: SigningKey,
    run_id: Id,
    scenario_id: Id,
}

impl SnapshotWriter {
    /// Open or create a durable snapshot pile and publish run/scenario names.
    pub fn open(
        path: impl AsRef<Path>,
        run_id: Id,
        run_name: &str,
        scenario_id: Id,
        scenario_name: &str,
    ) -> Result<Self, String> {
        let path = path.as_ref();
        if !path.exists() {
            File::create(path)
                .map_err(|error| format!("create snapshot pile {}: {error}", path.display()))?;
        }
        let mut pile = Pile::open(path)
            .map_err(|error| format!("open snapshot pile {}: {error}", path.display()))?;
        pile.refresh()
            .map_err(|error| format!("refresh snapshot pile {}: {error}", path.display()))?;

        // This deterministic local authority makes a pile portable between
        // repeated invocations of the example. Real deployments should load a
        // durable signing key from their trust configuration.
        let signing_key = SigningKey::from_bytes(&[0xE1; 32]);
        let authority = signing_key.verifying_key();
        let policy = CollectionPolicy::new(
            AdmissionPolicy::direct(authority),
            AdmissionPolicy::direct(authority),
        );
        let collection = pile
            .collection(COLLECTION_NAME, policy)
            .map_err(|error| format!("register snapshot collection: {error}"))?;

        let run_name = short_string(run_name, "run name")?;
        let scenario_name = short_string(scenario_name, "scenario name")?;
        pile.commit(
            collection,
            &signing_key,
            entity! { ExclusiveId::force_ref(&run_id) @ snapshot::name: run_name },
        )
        .map_err(|error| format!("publish snapshot run identity: {error}"))?;
        pile.commit(
            collection,
            &signing_key,
            entity! { ExclusiveId::force_ref(&scenario_id) @ snapshot::name: scenario_name },
        )
        .map_err(|error| format!("publish snapshot scenario identity: {error}"))?;

        Ok(Self {
            pile,
            collection,
            signing_key,
            run_id,
            scenario_id,
        })
    }

    /// Append one frame and its values as one signed collection commit.
    pub fn append_frame(
        &mut self,
        index: u64,
        time: f64,
        values: &[SnapshotValue<'_>],
    ) -> Result<SnapshotFrame, String> {
        if !time.is_finite() {
            return Err("snapshot time must be finite".into());
        }
        let frame_index = index.to_inline();
        let frame_time = time.to_inline();
        let mut fragment = entity! {
            snapshot::run: &self.run_id,
            snapshot::scenario: &self.scenario_id,
            snapshot::frame_index: frame_index,
            snapshot::time: frame_time,
        };
        let frame_id = fragment
            .root()
            .ok_or_else(|| "snapshot frame entity has no root".to_owned())?;

        for value in values {
            if !value.value.is_finite() {
                return Err(format!(
                    "snapshot value for {:?} must be finite",
                    value.quantity
                ));
            }
            let quantity = short_string(value.quantity, "quantity")?;
            let unit = short_string(value.unit, "unit")?;
            let value_inline = value.value.to_inline();
            let observation = match value.subject {
                Some(subject) => entity! {
                    snapshot::frame: &frame_id,
                    snapshot::subject: &subject,
                    snapshot::quantity: quantity,
                    snapshot::value: value_inline,
                    snapshot::unit: unit,
                },
                None => entity! {
                    snapshot::frame: &frame_id,
                    snapshot::quantity: quantity,
                    snapshot::value: value_inline,
                    snapshot::unit: unit,
                },
            };
            fragment += observation;
        }

        self.pile
            .commit(self.collection, &self.signing_key, fragment)
            .map_err(|error| format!("publish snapshot frame {index}: {error}"))?;
        Ok(SnapshotFrame {
            id: frame_id,
            index,
            time,
            values: values.len(),
        })
    }

    /// Close the underlying file after all staged appends have been issued.
    pub fn close(self) -> Result<(), String> {
        self.pile
            .close()
            .map_err(|error| format!("close snapshot pile: {error}"))
    }
}

fn short_string(value: &str, label: &str) -> Result<Inline<ShortString>, String> {
    value
        .try_to_inline()
        .map_err(|error| format!("{label} is not a valid ShortString: {error:?}"))
}

/// Mint a caller-owned identity for a run/scenario/subject.
///
/// This is a convenience only; callers may use an existing domain identity
/// when one already exists in their twin.
pub fn new_identity() -> Id {
    *fucid()
}

#[cfg(test)]
mod tests {
    use super::*;

    use ed25519_dalek::SigningKey;
    use triblespace::core::collection::{
        AdmissionPolicy, CollectionPolicy, CollectionSnapshotExt, CollectionStoreExt,
    };
    use triblespace::core::inline::Inline;
    use triblespace::core::inline::encodings::{f64::F64, iu256::U256LE};
    use triblespace::core::repo::SnapshotSource;
    use triblespace::core::repo::pile::Pile;
    use triblespace::core::trible::TribleSet;
    use triblespace::macros::{find, pattern};

    #[test]
    fn frame_survives_reopen_and_declarative_query() {
        let directory = tempfile::tempdir().expect("temporary snapshot directory");
        let path = directory.path().join("frames.pile");
        let run_id = new_identity();
        let scenario_id = new_identity();
        {
            let mut writer =
                SnapshotWriter::open(&path, run_id, "test-run", scenario_id, "test-scenario")
                    .expect("open snapshot writer");
            writer
                .append_frame(
                    7,
                    1.25,
                    &[SnapshotValue {
                        subject: None,
                        quantity: "pressure",
                        value: 123_456.0,
                        unit: "Pa",
                    }],
                )
                .expect("append frame");
            writer.close().expect("close snapshot writer");
        }

        let mut pile = Pile::open(&path).expect("reopen snapshot pile");
        pile.refresh().expect("refresh snapshot pile");
        let signing_key = SigningKey::from_bytes(&[0xE1; 32]);
        let authority = signing_key.verifying_key();
        let collection = pile
            .collection(
                COLLECTION_NAME,
                CollectionPolicy::new(
                    AdmissionPolicy::direct(authority),
                    AdmissionPolicy::direct(authority),
                ),
            )
            .expect("resolve snapshot collection");
        let observed = pile.snapshot().expect("freeze snapshot pile");
        let facts: TribleSet = observed
            .collection(collection)
            .expect("observe snapshot collection")
            .view()
            .expect("materialize snapshot collection");
        let rows: Vec<(Id, Inline<U256LE>, Inline<F64>)> = find!(
            (frame: Id, index: Inline<U256LE>, time: Inline<F64>),
            pattern!(&facts, [{
                ?frame @
                    snapshot::run: &run_id,
                    snapshot::scenario: &scenario_id,
                    snapshot::frame_index: ?index,
                    snapshot::time: ?time
            }])
        )
        .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.raw[0], 7);
        let decoded_time =
            <f64 as triblespace::core::inline::TryFromInline<F64>>::try_from_inline(&rows[0].2)
                .expect("decode frame time");
        assert!((decoded_time - 1.25).abs() < f64::EPSILON);
        pile.close().expect("close reopened snapshot pile");
    }
}
