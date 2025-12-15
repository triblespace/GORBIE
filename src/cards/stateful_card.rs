use std::sync::Arc;

use parking_lot::RwLock;

use crate::cards::Card;
use crate::Notebook;

use super::CardState;

pub struct StatefulCard<T> {
    current: CardState<T>,
    function: Box<dyn FnMut(&mut egui::Ui, &mut T)>,
    code: Option<String>,
}

impl<T: std::fmt::Debug + std::default::Default> Card for StatefulCard<T> {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let mut current = self.current.write();
        (self.function)(ui, &mut current);
    }

    fn code(&self) -> Option<&str> {
        self.code.as_deref()
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
