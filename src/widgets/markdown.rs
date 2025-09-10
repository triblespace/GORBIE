use eframe::egui;
use egui_commonmark::CommonMarkCache;
use egui_commonmark::CommonMarkViewer;
use std::cell::RefCell;

thread_local! {
    static GORBIE_MD_CACHE: RefCell<CommonMarkCache> = RefCell::new(CommonMarkCache::default());
}

pub fn markdown(ui: &mut egui::Ui, text: &str) {
    // Use a thread-local cache (no locking) and render the formatted markdown.
    GORBIE_MD_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        CommonMarkViewer::new().show(ui, &mut cache, text);
    });
}

#[macro_export]
macro_rules! md {
    ($ui:expr, $fmt:expr $(, $args:expr)*) => {
        {
            let text = format!($fmt $(, $args)*);
            $crate::widgets::markdown($ui, &text);
        }
    };
}
