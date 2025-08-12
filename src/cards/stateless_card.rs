use crate::{Card, Notebook};
use eframe::egui::CollapsingHeader;

pub struct StatelessCard {
    function: Box<dyn FnMut(&mut egui::Ui) -> ()>,
    code: Option<String>,
}

impl Card for StatelessCard {
    fn draw(&mut self, ui: &mut egui::Ui) {
        (self.function)(ui);

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt("code")
                .show(ui, |ui| {
                    let language = "rs";
                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(
                        ui.ctx(),
                        ui.style(),
                    );
                    egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language);
                });
        }
    }
}

pub fn stateless_card(
    nb: &mut Notebook,
    function: impl FnMut(&mut egui::Ui) -> () + 'static,
    code: Option<&str>,
) {
    nb.push(Box::new(StatelessCard {
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
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
            $crate::stateless_card($nb, $code, Some(stringify!($code)))
        }
    };
}
