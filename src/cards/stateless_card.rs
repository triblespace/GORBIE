use crate::cards::Card;
use crate::Notebook;
use eframe::egui::Frame;
use eframe::egui::Stroke;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui)>,
    code: Option<String>,
    show_preview: bool,
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        (self.function)(ui);

        if let Some(code) = &mut self.code {
            ui.add_space(8.0);
            let header_h = 4.0;
            let frame_fill = ui.style().visuals.widgets.inactive.bg_fill;
            Frame::group(ui.style())
                .stroke(Stroke::NONE)
                .fill(frame_fill)
                .inner_margin(2.0)
                .corner_radius(10.0)
                .show(ui, |ui| {
                    let hdr_resp = crate::widgets::collapsing_divider(ui, header_h, |ui| {
                        if self.show_preview {
                            // Inner area with side margins so content aligns left
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                // Left margin
                                ui.add_space(8.0);

                                ui.vertical(|ui| {
                                    let _ = crate::widgets::code_view(ui, code, "rs");
                                });

                                // Right margin filler
                                ui.add_space(8.0);
                            });
                        }
                    });

                    if hdr_resp.clicked() {
                        self.show_preview = !self.show_preview;
                    }
                });
        }
    }
}

pub fn stateless_card(
    nb: &mut Notebook,
    function: impl FnMut(&mut egui::Ui) + 'static,
    code: Option<&str>,
) {
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
        show_preview: false,
    }));
}

#[macro_export]
macro_rules! view {
    ($nb:expr, ($($Dep:ident),*), $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::cards::stateless_card($nb, $code, Some(stringify!($code)))
        }
    };
}
