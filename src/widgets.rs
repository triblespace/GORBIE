use eframe::egui;

pub fn load_button<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut Computed<T>,
    label_init: &str,
    label_reinit: &str,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    *value = match std::mem::replace(value, Computed::Undefined) {
        Computed::Undefined if ui.button(label_init).clicked() => {
            Computed::Init(std::thread::spawn(action))
        }
        Computed::Undefined => Computed::Undefined,
        Computed::Init(handle) if handle.is_finished() => {
            Computed::Ready(handle.join().unwrap(), 0)
        }
        Computed::Init(handle) => {
            ui.add(egui::widgets::Spinner::new());
            Computed::Init(handle)
        }
        Computed::Ready(current, generation) if ui.button(label_reinit).clicked() => {
            ui.ctx().request_repaint();
            Computed::Stale(current, generation + 1, std::thread::spawn(action))
        }
        Computed::Ready(inner, generation) => Computed::Ready(inner, generation),
        Computed::Stale(_, generation, join_handle) if join_handle.is_finished() => {
            ui.ctx().request_repaint();
            Computed::Ready(join_handle.join().unwrap(), generation + 1)
        }
        Computed::Stale(inner, join_handle, generation) => {
            ui.add(egui::widgets::Spinner::new());
            Computed::Stale(inner, join_handle, generation)
        }
    };

    return value.ready_mut();
}

pub fn load_auto<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut Computed<T>,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    *value = match std::mem::replace(value, Computed::Undefined) {
        Computed::Undefined => Computed::Init(std::thread::spawn(action)),
        Computed::Init(handle) if handle.is_finished() => {
            Computed::Ready(handle.join().unwrap(), 0)
        }
        Computed::Init(handle) => {
            ui.add(egui::widgets::Spinner::new());
            Computed::Init(handle)
        }
        Computed::Ready(inner, generation) => Computed::Ready(inner, generation),
        Computed::Stale(_, _, _) => {
            unreachable!();
        }
    };

    return value.ready_mut();
}

use egui_extras::{Column, TableBuilder};
use polars::prelude::DataFrame;

use crate::Computed;

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
