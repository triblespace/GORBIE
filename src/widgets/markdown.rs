use eframe::egui;
use egui_commonmark::CommonMarkCache;
use egui_commonmark::CommonMarkViewer;
use std::cell::Cell;
use std::cell::RefCell;

thread_local! {
    static GORBIE_MD_CACHE: RefCell<CommonMarkCache> = RefCell::new(CommonMarkCache::default());
    static GORBIE_MD_THEMES_INSTALLED: Cell<bool> = Cell::new(false);
}

const RAL_THEME_LIGHT: &str = "gorbie-ral-light";
const RAL_THEME_DARK: &str = "gorbie-ral-dark";
const RAL_THEME_LIGHT_BYTES: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/syntax/ral_light.tmTheme"));
const RAL_THEME_DARK_BYTES: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/syntax/ral_dark.tmTheme"));

fn ensure_ral_syntax_themes(cache: &mut CommonMarkCache) {
    GORBIE_MD_THEMES_INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
        if let Err(err) = cache.add_syntax_theme_from_bytes(RAL_THEME_LIGHT, RAL_THEME_LIGHT_BYTES)
        {
            log::warn!("failed to add RAL light syntax theme: {err}");
        }
        if let Err(err) = cache.add_syntax_theme_from_bytes(RAL_THEME_DARK, RAL_THEME_DARK_BYTES) {
            log::warn!("failed to add RAL dark syntax theme: {err}");
        }
        installed.set(true);
    });
}

pub fn markdown(ui: &mut egui::Ui, text: &str) {
    // Use a thread-local cache (no locking) and render the formatted markdown.
    GORBIE_MD_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        ensure_ral_syntax_themes(&mut cache);
        CommonMarkViewer::new()
            .syntax_theme_light(RAL_THEME_LIGHT)
            .syntax_theme_dark(RAL_THEME_DARK)
            .show(ui, &mut cache, text);
    });
}

#[macro_export]
macro_rules! md {
    ($ui:expr, $fmt:expr $(, $args:expr)*) => {
        {
            let text = format!($fmt $(, $args)*);
            $crate::cards::with_padding(
                $ui,
                $crate::cards::DEFAULT_CARD_PADDING,
                |ui| {
                    $crate::widgets::markdown(ui, &text);
                },
            );
        }
    };
}
