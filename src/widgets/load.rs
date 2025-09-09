use crate::dataflow::ComputedState;
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
