use eframe::egui;

use super::{CommitGraph, CommitGraphWidget, CommitHead, CommitSelection, CommitSelectionState};

/// High-level state for the commit history panel.
pub enum CommitHistoryState<'a> {
    Loading {
        message: &'a str,
    },
    Empty {
        message: &'a str,
    },
    Error {
        message: &'a str,
    },
    Ready {
        graph: &'a CommitGraph,
        heads: &'a [CommitHead],
    },
}

pub struct CommitHistoryResponse {
    pub response: egui::Response,
    pub selection: CommitSelection,
    pub selection_changed: bool,
}

#[must_use = "Use `CommitHistoryWidget::show(ui)` to render this widget."]
pub struct CommitHistoryWidget<'a> {
    state: CommitHistoryState<'a>,
    selection: &'a mut CommitSelectionState,
    card_width: f32,
    show_selection_label: bool,
}

impl<'a> CommitHistoryWidget<'a> {
    pub fn new(state: CommitHistoryState<'a>, selection: &'a mut CommitSelectionState) -> Self {
        Self {
            state,
            selection,
            card_width: 240.0,
            show_selection_label: true,
        }
    }

    pub fn card_width(mut self, card_width: f32) -> Self {
        self.card_width = card_width.max(120.0);
        self
    }

    pub fn show_selection_label(mut self, show_selection_label: bool) -> Self {
        self.show_selection_label = show_selection_label;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> CommitHistoryResponse {
        let selection_before = self.selection.selection();

        match self.state {
            CommitHistoryState::Loading { message } => CommitHistoryResponse {
                response: status_label(ui, message, ui.visuals().weak_text_color()),
                selection: selection_before,
                selection_changed: false,
            },
            CommitHistoryState::Empty { message } => CommitHistoryResponse {
                response: status_label(ui, message, ui.visuals().weak_text_color()),
                selection: selection_before,
                selection_changed: false,
            },
            CommitHistoryState::Error { message } => CommitHistoryResponse {
                response: status_label(ui, message, ui.visuals().error_fg_color),
                selection: selection_before,
                selection_changed: false,
            },
            CommitHistoryState::Ready { graph, heads } => {
                let response = CommitGraphWidget::new(graph, heads, self.selection)
                    .card_width(self.card_width)
                    .show(ui);
                if self.show_selection_label {
                    if let Some(label) = response.selection.label() {
                        ui.label(egui::RichText::new(label).small());
                    }
                }
                CommitHistoryResponse {
                    response: response.response,
                    selection: response.selection,
                    selection_changed: response.selection_changed,
                }
            }
        }
    }
}

fn status_label(ui: &mut egui::Ui, message: &str, color: egui::Color32) -> egui::Response {
    ui.label(egui::RichText::new(message).italics().color(color).small())
}
