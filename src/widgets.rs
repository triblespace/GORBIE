use eframe::egui;

pub fn load_button<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut ComputedState<T>,
    label_init: &str,
    label_reinit: &str,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    *value = match std::mem::replace(value, ComputedState::Undefined) {
        ComputedState::Undefined if ui.button(label_init).clicked() => {
            ComputedState::Init(std::thread::spawn(action))
        }
        ComputedState::Undefined => ComputedState::Undefined,
        ComputedState::Init(handle) if handle.is_finished() => {
            ComputedState::Ready(handle.join().unwrap(), 0)
        }
        ComputedState::Init(handle) => {
            ui.add(egui::widgets::Spinner::new());
            ComputedState::Init(handle)
        }
        ComputedState::Ready(current, generation) if ui.button(label_reinit).clicked() => {
            ui.ctx().request_repaint();
            ComputedState::Stale(current, generation + 1, std::thread::spawn(action))
        }
        ComputedState::Ready(inner, generation) => ComputedState::Ready(inner, generation),
        ComputedState::Stale(_, generation, join_handle) if join_handle.is_finished() => {
            ui.ctx().request_repaint();
            ComputedState::Ready(join_handle.join().unwrap(), generation + 1)
        }
        ComputedState::Stale(inner, join_handle, generation) => {
            ui.add(egui::widgets::Spinner::new());
            ComputedState::Stale(inner, join_handle, generation)
        }
    };

    return value.ready_mut();
}

pub fn load_auto<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut ComputedState<T>,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    *value = match std::mem::replace(value, ComputedState::Undefined) {
        ComputedState::Undefined => ComputedState::Init(std::thread::spawn(action)),
        ComputedState::Init(handle) if handle.is_finished() => {
            ComputedState::Ready(handle.join().unwrap(), 0)
        }
        ComputedState::Init(handle) => {
            ui.add(egui::widgets::Spinner::new());
            ComputedState::Init(handle)
        }
        ComputedState::Ready(inner, generation) => ComputedState::Ready(inner, generation),
        ComputedState::Stale(_, _, _) => {
            unreachable!();
        }
    };

    return value.ready_mut();
}

use egui_extras::{Column, TableBuilder};
use polars::prelude::DataFrame;

use crate::dataflow::ComputedState;

pub fn dataframe(ui: &mut egui::Ui, df: &DataFrame) {
    let nr_cols = df.width();
    let nr_rows = df.height();
    let cols = &df.get_column_names();

    TableBuilder::new(ui)
        .columns(Column::remainder(), nr_cols)
        .striped(true)
        .header(30.0, |mut header| {
            for head in cols {
                header.col(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{}", head))
                            .heading()
                            .size(16.0),
                    );
                });
            }
        })
        .body(|body| {
            body.rows(20.0, nr_rows, |mut row| {
                for col in cols {
                    let row_index = row.index();
                    row.col(|ui| {
                        if let Ok(column) = &df.column(col) {
                            if let Ok(value) = column.get(row_index) {
                                ui.label(format!("{}", value));
                            }
                        }
                    });
                }
            });
        });
}

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
        CommonMarkViewer::new().show(ui, &mut *cache, text);
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
pub fn collapsing_divider<R>(ui: &mut egui::Ui, height: f32, contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::Response {
    // Allocate a thin clickable header area and draw a pill-shaped divider centered in it.
    let (hdr_rect, hdr_resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );

    let visuals = ui.visuals();
    let pill_pad = 2.0f32;
    let pill_min = egui::pos2(hdr_rect.left() + pill_pad, hdr_rect.top() + 1.0);
    let pill_max = egui::pos2(hdr_rect.right() - pill_pad, hdr_rect.bottom() - 1.0);
    let pill_rect = egui::Rect::from_min_max(pill_min, pill_max);
    ui.painter()
        .rect_filled(pill_rect, pill_rect.height() / 2.0, visuals.widgets.active.bg_fill);

    // Let the caller render contents inside the parent ui. Return the header response so
    // the caller can inspect clicks or other interactions.
    let _ = contents(ui);

    hdr_resp
}

pub mod slider;
pub use slider::Slider;
