use std::sync::Arc;

use egui::{Frame, Stroke};
use parking_lot::RwLock;

use crate::{cards::Card, Notebook};

use super::CardState;

pub struct StatefulCard<T> {
    current: CardState<T>,
    function: Box<dyn FnMut(&mut egui::Ui, &mut T)>,
    code: Option<String>,
    // UI state for the card so we don't rely on global memory.
    show_preview: bool,
    // 0 = value, 1 = code
    preview_tab: usize,
}

impl<T: std::fmt::Debug + std::default::Default> Card for StatefulCard<T> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let mut current = self.current.write();
        (self.function)(ui, &mut current);

        // Unified preview panel (value + optional code) ------------------------
        ui.add_space(8.0);
        let header_h = 4.0; // thin divider-like header
        let frame_fill = ui.style().visuals.widgets.inactive.bg_fill;
        Frame::group(ui.style())
            .stroke(Stroke::NONE)
            .fill(frame_fill)
            .inner_margin(2.0)
            .corner_radius(4.0)
            .show(ui, |ui| {
                // thin clickable header area that doesn't take much space
                let hdr_resp = crate::widgets::collapsing_divider(ui, header_h, |ui| {
                    // Only show tabs and copy controls when expanded
                    if self.show_preview {
                        // Inner area with side margins so content and tabs align left
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            // Left margin
                            ui.add_space(8.0);

                            ui.vertical(|ui| {
                                // Controls row: tabs on the left, copy on the right
                                ui.horizontal(|ui| {
                                    ui.selectable_value(&mut self.preview_tab, 0, "Value");
                                    if self.code.is_some() {
                                        ui.selectable_value(&mut self.preview_tab, 1, "Code");
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let copy_btn = egui::Button::new("Copy").frame(false);
                                            if ui
                                                .add(copy_btn)
                                                .on_hover_text("Copy current preview to clipboard")
                                                .clicked()
                                            {
                                                if self.preview_tab == 0 {
                                                    ui.ctx().copy_text(format!("{:?}", &*current));
                                                } else if let Some(code) = &self.code {
                                                    ui.ctx().copy_text(code.clone());
                                                }
                                            }
                                        },
                                    );
                                });

                                // Content (left-aligned within the vertical column)
                                ui.add_space(6.0);
                                if self.preview_tab == 0 {
                                    ui.monospace(format!("{:?}", &*current));
                                } else if let Some(code) = &mut self.code {
                                    let language = "rs";
                                    let theme =
                                        egui_extras::syntax_highlighting::CodeTheme::from_memory(
                                            ui.ctx(),
                                            ui.style(),
                                        );
                                    egui_extras::syntax_highlighting::code_view_ui(
                                        ui, &theme, code, language,
                                    );
                                }
                            });

                            // Right margin filler
                            ui.add_space(8.0);
                        });
                    }
                });

                // Click handling is performed by the caller using the returned response
                if hdr_resp.clicked() {
                    self.show_preview = !self.show_preview;
                }
            });
    }
}

pub fn stateful_card<T: std::fmt::Debug + std::default::Default + 'static>(
    nb: &mut Notebook,
    init: T,
    function: impl FnMut(&mut egui::Ui, &mut T) + 'static,
    code: Option<&str>,
) -> CardState<T> {
    let current = Arc::new(RwLock::new(init));
    nb.push(Box::new(StatefulCard {
        current: current.clone(),
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
        show_preview: false,
        preview_tab: 0,
    }));

    current
}

#[macro_export]
macro_rules! state {
    ($nb:expr, ($($Dep:ident),*), $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::cards::stateful_card($nb, Default::default(), $code, Some(stringify!($code)))
        }
    };
    ($nb:expr, ($($Dep:ident),*), $init:expr, $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::cards::stateful_card($nb, $init, $code, Some(stringify!($code)))
        }
    };
}
