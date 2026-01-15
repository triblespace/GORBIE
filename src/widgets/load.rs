use crate::dataflow::ComputedState;
use crate::themes::GorbieToggleButtonStyle;
use crate::widgets::Button;
use eframe::egui;

pub fn load_button<'a, T: Send + 'static>(
    ui: &mut egui::Ui,
    value: &'a mut ComputedState<T>,
    label: &str,
    action: impl FnOnce() -> T + Send + 'static,
) -> &'a mut T {
    value.poll();
    let running = value.is_running();
    let style = GorbieToggleButtonStyle::from(ui.style().as_ref());
    let light_on = crate::themes::button_light_on();
    let off = style.rail_bg;
    let light = if running {
        let t = ui.input(|input| input.time) as f32;
        let wave = (t * std::f32::consts::TAU * 0.8).sin() * 0.5 + 0.5;
        let intensity = wave;
        crate::themes::blend(off, light_on, intensity)
    } else {
        off
    };
    let button = Button::new(label)
        .latched(running)
        .latch_on_click(true)
        .light(light);
    let clicked = ui.add(button).clicked();
    if clicked && !running {
        value.spawn(action);
        ui.ctx().request_repaint();
    }
    if running {
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
    value.poll();
    if should_spawn(value.value()) {
        value.spawn(action);
        ui.ctx().request_repaint();
    }
    if value.is_running() {
        ui.add(egui::widgets::Spinner::new());
        ui.ctx().request_repaint();
    }
    value.value_mut()
}
