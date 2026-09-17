//! Shared graphical rendering for TribleSpace's immutable cluster report.

use std::sync::Arc;

use eframe::egui::{self, Response, RichText, Ui, Widget};
use triblespace_net::dashboard::{
    CountMetric, DashboardReport, Freshness, NativeSummary, ObservedCondition, ObserverReport,
    WantSummary,
};

use crate::cards::DEFAULT_CARD_PADDING;
use crate::widgets::{lcd_readout, Column, ProgressBar, TableBuilder};
use crate::NotebookCtx;

/// Render every cluster-health section in one embeddable egui widget.
///
/// The widget consumes an already frozen [`DashboardReport`]. It never opens,
/// maintains, fetches, or writes a pile, so notebooks and other GUI surfaces
/// share exactly the same observation as the terminal renderer.
pub struct ClusterHealthWidget<'a> {
    report: &'a DashboardReport,
}

impl<'a> ClusterHealthWidget<'a> {
    pub const fn new(report: &'a DashboardReport) -> Self {
        Self { report }
    }
}

impl Widget for ClusterHealthWidget<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.vertical(|ui| {
            render_summary(ui, self.report);
            ui.separator();
            render_native(ui, self.report);
            ui.separator();
            render_collections(ui, self.report);
            ui.separator();
            render_wants(ui, self.report);
            ui.separator();
            render_observers(ui, self.report);
            ui.separator();
            render_samples(ui, self.report);
        })
        .response
    }
}

/// Add a multi-card cluster-health notebook over one immutable report.
pub fn cluster_health_notebook(nb: &mut NotebookCtx, report: Arc<DashboardReport>) {
    let summary = Arc::clone(&report);
    nb.view(move |ctx| {
        ctx.with_padding(DEFAULT_CARD_PADDING, |ctx| {
            render_summary(ctx, &summary);
        });
    });

    let native = Arc::clone(&report);
    nb.view(move |ctx| {
        ctx.with_padding(DEFAULT_CARD_PADDING, |ctx| {
            render_native(ctx, &native);
        });
    });

    let collections = Arc::clone(&report);
    nb.view(move |ctx| {
        ctx.with_padding(DEFAULT_CARD_PADDING, |ctx| {
            render_collections(ctx, &collections);
        });
    });

    let wants = Arc::clone(&report);
    nb.view(move |ctx| {
        ctx.with_padding(DEFAULT_CARD_PADDING, |ctx| {
            render_wants(ctx, &wants);
        });
    });

    let observers = Arc::clone(&report);
    nb.view(move |ctx| {
        ctx.with_padding(DEFAULT_CARD_PADDING, |ctx| {
            render_observers(ctx, &observers);
        });
    });

    nb.view(move |ctx| {
        ctx.with_padding(DEFAULT_CARD_PADDING, |ctx| {
            render_samples(ctx, &report);
        });
    });
    nb.settled();
}

fn render_summary(ui: &mut Ui, report: &DashboardReport) {
    let local = &report.local;
    ui.heading("Cluster health");
    ui.label(
        "One immutable local observation plus the latest locally readable report from each observer.",
    );
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        metric(ui, "resident blobs", local.resident_blobs.to_string());
        metric(ui, "resident bytes", format_bytes(local.resident_bytes));
        metric(ui, "native records", local.stored_records.to_string());
        metric(ui, "pending WANTs", local.wants.pending.to_string());
        metric(ui, "observers", report.observers.len().to_string());
    });
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "Scope: exact local residency and stored evidence. Remote values are bounded reports; record/root convergence does not prove blob availability.",
        )
        .small(),
    );
}

fn metric(ui: &mut Ui, label: &str, value: String) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).small());
        lcd_readout(ui, &value, None);
    });
}

fn render_native(ui: &mut Ui, report: &DashboardReport) {
    ui.heading("Stored native work");
    ui.label(
        "A stored equation is known work; output residency and direct-reference readiness are separate facts.",
    );
    for (label, summary) in native_rows(report) {
        let fraction = ratio(summary.result_resident, summary.stored);
        ui.add(
            ProgressBar::new(fraction)
                .text(format!(
                    "{label}: {} / {} outputs resident",
                    summary.result_resident, summary.stored
                ))
                .segments(24),
        );
    }
    ui.add_space(6.0);
    TableBuilder::new(ui)
        .id_salt("cluster-health-native")
        .striped(true)
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::remainder())
        .header(24.0, |mut header| {
            for title in [
                "kind",
                "stored",
                "refs ready",
                "output here",
                "missing refs",
            ] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for (label, summary) in native_rows(report) {
                body.row(22.0, |mut row| {
                    text_col(&mut row, label, true);
                    number_col(&mut row, summary.stored);
                    number_col(&mut row, summary.all_references_resident);
                    number_col(&mut row, summary.result_resident);
                    number_col(&mut row, summary.missing_reference_occurrences);
                });
            }
        });
}

fn render_collections(ui: &mut Ui, report: &DashboardReport) {
    ui.heading("Collections with native evidence");
    if report.local.collections.is_empty() {
        ui.label("No native collection records are stored in this snapshot.");
        return;
    }
    TableBuilder::new(ui)
        .id_salt("cluster-health-collections")
        .striped(true)
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::remainder())
        .header(24.0, |mut header| {
            for title in ["collection", "commits", "merges", "derives", "missing refs"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for collection in &report.local.collections {
                let missing = collection
                    .commits
                    .missing_reference_occurrences
                    .saturating_add(collection.merges.missing_reference_occurrences)
                    .saturating_add(collection.derives.missing_reference_occurrences);
                body.row(22.0, |mut row| {
                    text_col(
                        &mut row,
                        &triblespace_net::dashboard::short_handle(&collection.collection),
                        true,
                    );
                    number_col(&mut row, collection.commits.stored);
                    number_col(&mut row, collection.merges.stored);
                    number_col(&mut row, collection.derives.stored);
                    number_col(&mut row, missing);
                });
            }
        });
}

fn render_wants(ui: &mut Ui, report: &DashboardReport) {
    ui.heading("Durable requested work");
    ui.label(
        "Only an unanswered durable WANT is called pending. An absent output without a WANT is not inferred work.",
    );
    TableBuilder::new(ui)
        .id_salt("cluster-health-wants")
        .striped(true)
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::remainder())
        .header(24.0, |mut header| {
            for title in ["kind", "total", "answered", "pending"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for (label, summary) in want_rows(report) {
                body.row(22.0, |mut row| {
                    text_col(&mut row, label, true);
                    number_col(&mut row, summary.total);
                    number_col(&mut row, summary.answered);
                    number_col(&mut row, summary.pending);
                });
            }
        });
}

fn render_observers(ui: &mut Ui, report: &DashboardReport) {
    ui.heading("Observer reports");
    if report.observers.is_empty() {
        ui.label("Not observed, or the health collection is not locally readable.");
        return;
    }

    for observer in &report.observers {
        let endpoint = observer
            .endpoints
            .first()
            .map(|endpoint| hex_prefix(endpoint, 6))
            .unwrap_or_else(|| format!("node {}", hex_prefix(observer.node.as_ref(), 6)));
        let age = observer
            .age_seconds
            .map(|seconds| format!("{seconds}s ago"))
            .unwrap_or_else(|| "future timestamp".to_owned());
        let resident = observer
            .conditions
            .iter()
            .find_map(|condition| condition.evidence.resident_blobs.value())
            .map(|count| format!(" · {count} blobs"))
            .unwrap_or_default();
        let alerts = observer
            .conditions
            .iter()
            .filter(|condition| condition.alert)
            .count();
        let label = format!(
            "{endpoint} · {} · {age}{resident} · {alerts} alerts",
            freshness_label(observer.freshness),
        );
        egui::CollapsingHeader::new(label)
            .id_salt(("cluster-health-observer", observer.report))
            .default_open(alerts != 0 || observer.freshness != Freshness::Fresh)
            .show(ui, |ui| render_observer_conditions(ui, observer));
    }
}

fn render_observer_conditions(ui: &mut Ui, observer: &ObserverReport) {
    TableBuilder::new(ui)
        .id_salt(("cluster-health-conditions", observer.report))
        .striped(true)
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::auto())
        .column(Column::remainder())
        .header(24.0, |mut header| {
            for title in ["component", "collection", "peer", "state", "evidence"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|mut body| {
            for condition in &observer.conditions {
                body.row(22.0, |mut row| {
                    text_col(
                        &mut row,
                        &join_debug(&condition.components),
                        condition.alert,
                    );
                    text_col(
                        &mut row,
                        &condition
                            .collections
                            .first()
                            .map(triblespace_net::dashboard::short_handle)
                            .unwrap_or_else(|| "—".to_owned()),
                        true,
                    );
                    text_col(
                        &mut row,
                        &condition
                            .peers
                            .first()
                            .map(|peer| hex_prefix(peer, 6))
                            .unwrap_or_else(|| "—".to_owned()),
                        true,
                    );
                    text_col(&mut row, &condition_state(condition), condition.alert);
                    text_col(&mut row, &condition_evidence(condition), false);
                });
            }
        });
}

fn render_samples(ui: &mut Ui, report: &DashboardReport) {
    ui.heading("Exact local samples");
    ui.label("Samples are lexicographically stable and bounded by the requested display limit.");

    egui::CollapsingHeader::new(format!(
        "Resident blobs{}",
        truncated_suffix(report.local.blob_sample_truncated)
    ))
    .show(ui, |ui| {
        for blob in &report.local.blob_sample {
            ui.horizontal(|ui| {
                ui.monospace(hex_full(&blob.handle));
                ui.label(format!("{} bytes", blob.bytes));
            });
        }
    });

    egui::CollapsingHeader::new(format!(
        "Missing direct references{}",
        truncated_suffix(report.local.missing_reference_sample_truncated)
    ))
    .show(ui, |ui| {
        for missing in &report.local.missing_reference_sample {
            ui.horizontal(|ui| {
                ui.monospace(triblespace_net::dashboard::short_handle(
                    &missing.collection,
                ));
                ui.label(missing.kind.label());
                ui.monospace(hex_full(&missing.handle));
            });
        }
    });

    egui::CollapsingHeader::new(format!(
        "Pending durable WANTs{}",
        truncated_suffix(report.local.pending_want_sample_truncated)
    ))
    .show(ui, |ui| {
        for pending in &report.local.pending_want_sample {
            ui.horizontal_wrapped(|ui| {
                ui.label(pending.kind.label());
                ui.monospace(format!("{:?}", pending.request));
            });
        }
    });
}

fn native_rows(report: &DashboardReport) -> [(&'static str, NativeSummary); 3] {
    [
        ("commit", report.local.commits),
        ("merge", report.local.merges),
        ("derive", report.local.derives),
    ]
}

fn want_rows(report: &DashboardReport) -> [(&'static str, WantSummary); 3] {
    [
        ("blob", report.local.blob_wants),
        ("merge", report.local.merge_wants),
        ("derive", report.local.derive_wants),
    ]
}

fn ratio(numerator: u64, denominator: u64) -> f32 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f32 / denominator as f32
    }
}

fn freshness_label(freshness: Freshness) -> &'static str {
    match freshness {
        Freshness::Fresh => "fresh",
        Freshness::Stale => "stale",
        Freshness::Future => "future",
    }
}

fn condition_state(condition: &ObservedCondition) -> String {
    let mut state = join_debug(&condition.states);
    if state.is_empty() {
        state.push('—');
    }
    if condition.recovered {
        state.push_str(" · recovered");
    }
    if condition.alert {
        state.push_str(" · alert");
    }
    state
}

fn condition_evidence(condition: &ObservedCondition) -> String {
    let evidence = &condition.evidence;
    if evidence.resident_blobs != CountMetric::Absent {
        return format!("{} resident blobs", format_metric(&evidence.resident_blobs));
    }
    if evidence.local_records != CountMetric::Absent
        || evidence.remote_records != CountMetric::Absent
    {
        return format!(
            "records {}/{} · auth {}/{}",
            format_metric(&evidence.local_records),
            format_metric(&evidence.remote_records),
            format_metric(&evidence.local_authorizations),
            format_metric(&evidence.remote_authorizations),
        );
    }
    if evidence.publication_keys != CountMetric::Absent {
        return format!(
            "keys {} · pending {}/{}/{} · active {}",
            format_metric(&evidence.publication_keys),
            format_metric(&evidence.publication_startup_pending),
            format_metric(&evidence.publication_incremental_pending),
            format_metric(&evidence.publication_retry_pending),
            format_metric(&evidence.publication_in_flight),
        );
    }
    "—".to_owned()
}

fn format_metric(metric: &CountMetric) -> String {
    match metric {
        CountMetric::Absent => "?".to_owned(),
        CountMetric::Value(value) => value.to_string(),
        CountMetric::Ambiguous(values) => format!("ambiguous {values:?}"),
    }
}

fn join_debug<T: std::fmt::Debug>(values: &[T]) -> String {
    values
        .iter()
        .map(|value| format!("{value:?}").to_lowercase())
        .collect::<Vec<_>>()
        .join("|")
}

fn text_col(row: &mut crate::widgets::table::TableRow<'_, '_>, text: &str, mono: bool) {
    row.col(|ui| {
        if mono {
            ui.monospace(text);
        } else {
            ui.label(text);
        }
    });
}

fn number_col(row: &mut crate::widgets::table::TableRow<'_, '_>, number: u64) {
    row.col(|ui| {
        ui.monospace(number.to_string());
    });
}

fn truncated_suffix(truncated: bool) -> &'static str {
    if truncated {
        " (sample truncated)"
    } else {
        ""
    }
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    hex_bytes(bytes.iter().take(count).copied())
}

fn hex_full(bytes: &[u8; 32]) -> String {
    hex_bytes(bytes.iter().copied())
}

fn hex_bytes(bytes: impl Iterator<Item = u8>) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn format_bytes(bytes: u128) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_ratio_handles_empty_and_partial_sets() {
        assert_eq!(ratio(0, 0), 0.0);
        assert_eq!(ratio(1, 4), 0.25);
    }

    #[test]
    fn count_metric_keeps_ambiguity_visible() {
        assert_eq!(format_metric(&CountMetric::Absent), "?");
        assert_eq!(format_metric(&CountMetric::Value(0)), "0");
        assert_eq!(
            format_metric(&CountMetric::Ambiguous(vec![1, 2])),
            "ambiguous [1, 2]"
        );
    }
}
