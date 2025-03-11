use eframe::egui;

pub enum LoadState<T> {
    Undefined,
    Loading(std::thread::JoinHandle<T>),
    Ready(T),
}

impl<T> LoadState<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            LoadState::Ready(inner) => Some(inner),
            _ => None,
        }
    }
}

impl<T> std::default::Default for LoadState<T> {
    fn default() -> Self {
        LoadState::Undefined
    }
}

impl<T> std::fmt::Debug for LoadState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadState::Undefined => write!(f, "Undefined"),
            LoadState::Loading(_) => write!(f, "Loading"),
            LoadState::Ready(_) => write!(f, "Ready"),
        }
    }
}

pub fn load_button<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut LoadState<T>,
    label_init: &str,
    label_reinit: &str,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    match value {
        LoadState::Undefined if ui.button(label_init).clicked() => {
            *value = LoadState::Loading(std::thread::spawn(action));
            None
        }
        LoadState::Undefined => None,
        LoadState::Loading(handle) => {
            ui.add(egui::widgets::Spinner::new());
            if handle.is_finished() {
                let old_value = std::mem::replace(value, LoadState::Undefined);
                if let LoadState::Loading(handle) = old_value {
                    *value = LoadState::Ready(handle.join().unwrap());
                }
            }
            None
        }
        LoadState::Ready(_) if ui.button(label_reinit).clicked() => {
            *value = LoadState::Loading(std::thread::spawn(action));
            None
        }
        LoadState::Ready(inner) => Some(inner),
    }
}

pub fn load_auto<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut LoadState<T>,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<&'a mut T> {
    match value {
        LoadState::Undefined => {
            *value = LoadState::Loading(std::thread::spawn(action));
        }
        LoadState::Loading(handle) => {
            ui.add(egui::widgets::Spinner::new());

            if handle.is_finished() {
                let LoadState::Loading(handle) = std::mem::replace(value, LoadState::Undefined)
                else {
                    unreachable!();
                };
                *value = LoadState::Ready(handle.join().unwrap());

                let LoadState::Ready(ref mut inner) = value else {
                    unreachable!()
                };
                return Some(inner);
            }
        }
        LoadState::Ready(inner) => return Some(inner),
    }
    None
}


use egui_extras::{TableBuilder, Column};
use polars::prelude::DataFrame;

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
                    ui.label(egui::RichText::new(format!("{}", head)).heading().size(16.0));
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