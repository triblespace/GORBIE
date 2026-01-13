use crate::dataflow::ComputedState;
use crate::widgets::Button;
use eframe::egui;

pub fn load_button<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut ComputedState<T>,
    label: &str,
    action: impl FnOnce() -> T + Send + 'static,
) -> &'a mut T {
    value.poll();
    let clicked = ui
        .add_enabled(!value.is_running(), Button::new(label))
        .clicked();
    if clicked {
        value.spawn(action);
        ui.ctx().request_repaint();
    }
    if value.is_running() {
        ui.add(egui::widgets::Spinner::new());
        ui.ctx().request_repaint();
    }
    value.value_mut()
}

pub fn load_auto<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut ComputedState<T>,
    should_spawn: impl FnOnce(&T) -> bool,
    action: impl FnOnce() -> T + Send + 'static,
) -> &'a mut T {
    value.spawn_if(should_spawn, action);
    if value.is_running() {
        ui.add(egui::widgets::Spinner::new());
        ui.ctx().request_repaint();
    }
    value.value_mut()
}
