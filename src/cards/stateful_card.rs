use std::sync::Arc;

use egui::CollapsingHeader;
use parking_lot::RwLock;

use crate::Notebook;

use super::{Card, CardCtx, CardState};

pub struct StatefulCard<T> {
    current: CardState<T>,
    function: Box<dyn FnMut(&mut CardCtx, &mut T)>,
    code: Option<String>,
}

impl<T: std::fmt::Debug + std::default::Default> Card for StatefulCard<T> {
    fn update(&mut self, ctx: &mut CardCtx) -> () {
        let mut current = self.current.write();
        (self.function)(ctx, &mut current);

        CollapsingHeader::new("Current")
            .id_salt("__current")
            .show(ctx.ui(), |ui| {
                ui.monospace(format!("{:?}", current));
            });

        if let Some(code) = &mut self.code {
            CollapsingHeader::new("Code")
                .id_salt("__code")
                .show(ctx.ui(), |ui| {
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

pub fn stateful_card<T: std::fmt::Debug + std::default::Default + 'static>(
    nb: &mut Notebook,
    init: T,
    function: impl FnMut(&mut CardCtx, &mut T) + 'static,
    code: Option<&str>,
) -> CardState<T> {
    let current = Arc::new(RwLock::new(init));
    nb.push_card(Box::new(StatefulCard {
        current: current.clone(),
        function: Box::new(function),
        code: code.map(|s| s.to_owned()),
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
            $crate::stateful_card($nb, Default::default(), $code, Some(stringify!($code)))
        }
    };
    ($nb:expr, ($($Dep:ident),*), $init:expr, $code:expr) => {
        {
            // We capture the dependencies to ensure they are cloned.
            // Each clone gets assigned it's own let statement.
            // This makes type checking errors more readable.
            $(let $Dep = $Dep.clone();)*
            $crate::stateful_card($nb, $init, $code, Some(stringify!($code)))
        }
    };
}
