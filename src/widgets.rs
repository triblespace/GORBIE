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

use crate::ComputedState;

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

use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

/// Render CommonMark markdown inline inside the current UI using a provided cache.
/// Example:
/// let mut cache = CommonMarkCache::default();
/// md_inline(ui, &mut cache, "# Hello {}", name);
pub fn md_inline(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    CommonMarkViewer::new().show(ui, cache, text);
}

#[macro_export]
macro_rules! md_inline {
    ($ui:expr, $cache:expr, $fmt:expr $(, $args:expr)*) => {
        $crate::widgets::md_inline($ui, $cache, &format!($fmt $(, $args)*));
    };
}
