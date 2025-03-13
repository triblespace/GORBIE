use eframe::egui;

impl<T> std::default::Default for Computed<T> {
    fn default() -> Self {
        Computed::Undefined
    }
}

impl<T> std::fmt::Debug for Computed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Computed::Undefined => write!(f, "Undefined"),
            Computed::Running(_) => write!(f, "Loading"),
            Computed::Ready(_) => write!(f, "Ready"),
        }
    }
}

pub fn load_button<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut Computed<T>,
    label_init: &str,
    label_reinit: &str,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    match value {
        Computed::Undefined if ui.button(label_init).clicked() => {
            *value = Computed::Running(std::thread::spawn(action));
            None
        }
        Computed::Undefined => None,
        Computed::Running(handle) => {
            ui.add(egui::widgets::Spinner::new());
            if handle.is_finished() {
                let old_value = std::mem::replace(value, Computed::Undefined);
                if let Computed::Running(handle) = old_value {
                    *value = Computed::Ready(handle.join().unwrap());
                }
            }
            None
        }
        Computed::Ready(_) if ui.button(label_reinit).clicked() => {
            *value = Computed::Running(std::thread::spawn(action));
            None
        }
        Computed::Ready(inner) => Some(inner),
    }
}

pub fn load_auto<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut Computed<T>,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    match value {
        Computed::Undefined => {
            *value = Computed::Running(std::thread::spawn(action));
        }
        Computed::Running(handle) => {
            ui.add(egui::widgets::Spinner::new());

            if handle.is_finished() {
                let Computed::Running(handle) = std::mem::replace(value, Computed::Undefined)
                else {
                    unreachable!();
                };
                *value = Computed::Ready(handle.join().unwrap());

                let Computed::Ready(ref mut inner) = value else {
                    unreachable!()
                };
                return Some(inner);
            }
        }
        Computed::Ready(inner) => return Some(inner),
    }
    None
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
