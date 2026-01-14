#!/usr/bin/env -S watchexec -r rust-script
//! ```cargo
//! [dependencies]
//! GORBIE = { path = ".." }
//! egui = "0.32"
//! triblespace = { path = "../../triblespace-rs" }
//! ```

use triblespace::core::blob::schemas::longstring::LongString;
use triblespace::core::blob::schemas::simplearchive::SimpleArchive;
use triblespace::core::blob::schemas::succinctarchive::SuccinctArchiveBlob;
use triblespace::core::blob::schemas::wasmcode::WasmCode;
use triblespace::core::blob::MemoryBlobStore;
use triblespace::core::id::Id;
use triblespace::core::id::RawId;
use triblespace::core::metadata;
use triblespace::core::metadata::ConstMetadata;
use triblespace::core::repo::BlobStore;
use triblespace::core::repo::BlobStoreGet;
use triblespace::core::trible::TribleSet;
use triblespace::core::value::schemas::boolean::Boolean;
use triblespace::core::value::schemas::ed25519::{
    ED25519PublicKey, ED25519RComponent, ED25519SComponent,
};
use triblespace::core::value::schemas::f256::{F256BE, F256LE};
use triblespace::core::value::schemas::f64::F64;
use triblespace::core::value::schemas::genid::GenId;
use triblespace::core::value::schemas::hash::{Blake2b, Blake3, Handle};
use triblespace::core::value::schemas::iu256::{I256BE, I256LE, U256BE, U256LE};
use triblespace::core::value::schemas::linelocation::LineLocation;
use triblespace::core::value::schemas::r256::{R256BE, R256LE};
use triblespace::core::value::schemas::range::{RangeInclusiveU128, RangeU128};
use triblespace::core::value::schemas::shortstring::ShortString;
use triblespace::core::value::schemas::time::NsTAIInterval;
use triblespace::core::value::Value;
use triblespace::macros::{find, pattern};
use triblespace::prelude::View;

use GORBIE::prelude::*;

fn build_schema_metadata(blobs: &mut MemoryBlobStore<Blake3>) -> TribleSet {
    let mut metadata_set = TribleSet::new();

    metadata_set += Boolean::describe(blobs).expect("boolean metadata");
    metadata_set += ShortString::describe(blobs).expect("shortstring metadata");
    metadata_set += GenId::describe(blobs).expect("genid metadata");
    metadata_set += F64::describe(blobs).expect("f64 metadata");
    metadata_set += F256LE::describe(blobs).expect("f256le metadata");
    metadata_set += F256BE::describe(blobs).expect("f256be metadata");
    metadata_set += U256LE::describe(blobs).expect("u256le metadata");
    metadata_set += U256BE::describe(blobs).expect("u256be metadata");
    metadata_set += I256LE::describe(blobs).expect("i256le metadata");
    metadata_set += I256BE::describe(blobs).expect("i256be metadata");
    metadata_set += R256LE::describe(blobs).expect("r256le metadata");
    metadata_set += R256BE::describe(blobs).expect("r256be metadata");
    metadata_set += RangeU128::describe(blobs).expect("range_u128 metadata");
    metadata_set += RangeInclusiveU128::describe(blobs).expect("range_u128_inc metadata");
    metadata_set += LineLocation::describe(blobs).expect("line_location metadata");
    metadata_set += NsTAIInterval::describe(blobs).expect("nstai_interval metadata");
    metadata_set += ED25519RComponent::describe(blobs).expect("ed25519_r metadata");
    metadata_set += ED25519SComponent::describe(blobs).expect("ed25519_s metadata");
    metadata_set += ED25519PublicKey::describe(blobs).expect("ed25519_pubkey metadata");
    metadata_set += Blake2b::describe(blobs).expect("blake2 metadata");
    metadata_set += Blake3::describe(blobs).expect("blake3 metadata");
    metadata_set += Handle::<Blake3, LongString>::describe(blobs).expect("handle longstring");
    metadata_set += Handle::<Blake3, SimpleArchive>::describe(blobs)
        .expect("handle simplearchive");
    metadata_set += Handle::<Blake3, SuccinctArchiveBlob>::describe(blobs)
        .expect("handle succinctarchive");
    metadata_set += Handle::<Blake3, WasmCode>::describe(blobs).expect("handle wasmcode");

    metadata_set += LongString::describe(blobs).expect("longstring metadata");
    metadata_set += SimpleArchive::describe(blobs).expect("simplearchive metadata");
    metadata_set += SuccinctArchiveBlob::describe(blobs).expect("succinctarchive metadata");
    metadata_set += WasmCode::describe(blobs).expect("wasmcode metadata");

    metadata_set
}

fn render_schema_sections(
    ui: &mut egui::Ui,
    title: &str,
    metadata_set: &TribleSet,
    blobs: &impl BlobStoreGet<Blake3>,
    kind: Id,
) {
    let id_color = ui.visuals().weak_text_color();
    let id_size = ui.text_style_height(&egui::TextStyle::Small);
    let body_size = ui.text_style_height(&egui::TextStyle::Body);
    let desc_size = ui.text_style_height(&egui::TextStyle::Small);
    let separator_stroke = egui::Stroke::new(1.0, ui.visuals().weak_text_color());

    let mut rows: Vec<(Id, Value<ShortString>, View<str>)> = find!(
        (
            id: Id,
            shortname: Value<ShortString>,
            description: Value<Handle<Blake3, LongString>>
        ),
        pattern!(metadata_set, [{
            ?id @
                metadata::tag: kind,
                metadata::shortname: ?shortname,
                metadata::description: ?description
        }])
    )
    .into_iter()
    .filter_map(|(id, shortname, description)| {
        blobs
            .get::<View<str>, LongString>(description)
            .ok()
            .map(|view| (id, shortname, view))
    })
    .collect();

    rows.sort_by(|left, right| {
        let left_name = left.1.from_value::<&str>();
        let right_name = right.1.from_value::<&str>();
        let left_id: &RawId = left.0.as_ref();
        let right_id: &RawId = right.0.as_ref();
        left_name
            .cmp(right_name)
            .then_with(|| left_id.cmp(right_id))
    });

    ui.label(egui::RichText::new(title).heading());
    ui.add_space(6.0);

    for (idx, row) in rows.iter().enumerate() {
        let shortname = row.1.from_value::<&str>();
        let description = row.2.as_ref();

        let row_height = body_size.max(id_size);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_height),
            egui::Layout::left_to_right(egui::Align::BOTTOM),
            |ui| {
                ui.label(
                    egui::RichText::new(shortname)
                        .monospace()
                        .size(body_size)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("{:X}", row.0))
                        .monospace()
                        .size(id_size)
                        .color(id_color),
                );
            },
        );
        ui.label(
            egui::RichText::new(description)
                .size(desc_size)
                .color(ui.visuals().text_color()),
        );
        if idx + 1 < rows.len() {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 10.0),
                egui::Sense::hover(),
            );
            ui.painter()
                .hline(rect.x_range(), rect.center().y, separator_stroke);
        } else {
            ui.add_space(6.0);
        }
    }
}

#[notebook]
fn main(nb: &mut Notebook) {
    let padding = GORBIE::cards::DEFAULT_CARD_PADDING;
    let mut blobs = MemoryBlobStore::<Blake3>::new();
    let metadata_set = build_schema_metadata(&mut blobs);
    let reader = blobs.reader().expect("metadata blob reader");

    stateless_card(nb, move |ui| {
        ui.with_padding(padding, |ui| {
            ui.label(egui::RichText::new("Schema metadata").heading());
            ui.label("Built-in value and blob schemas with their discovery metadata.");
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(12.0);
            render_schema_sections(
                ui,
                "Value schemas",
                &metadata_set,
                &reader,
                metadata::KIND_VALUE_SCHEMA,
            );
            ui.add_space(8.0);
            render_schema_sections(
                ui,
                "Blob schemas",
                &metadata_set,
                &reader,
                metadata::KIND_BLOB_SCHEMA,
            );
        });
    });
}
