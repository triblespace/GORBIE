use eframe::egui;

pub fn spawn_button<T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &mut Option<std::thread::JoinHandle<T>>,
    label: &str,
    action: impl FnMut() -> T + Send + 'static,
) -> Option<T> {
    if let Some(handle) = value {
        ui.add(egui::widgets::Spinner::new());

        if handle.is_finished() {
            return Some(value.take().unwrap().join().unwrap());
        }
    } else {
        if ui.button(label).clicked() {
            value.get_or_insert_with(|| std::thread::spawn(action));
        }
    }
    None
}