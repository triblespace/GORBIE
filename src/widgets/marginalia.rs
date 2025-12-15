use eframe::egui;

pub fn pinned_note<R>(
    ui: &mut egui::Ui,
    anchor: &egui::Response,
    open: &mut bool,
    align: egui::RectAlign,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let frame = egui::Frame::popup(ui.style())
        .fill(ui.visuals().window_fill)
        .stroke(stroke)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(10));

    egui::Popup::from_response(anchor)
        .open_bool(open)
        .align(align)
        .gap(8.0)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .frame(frame)
        .width(width)
        .show(add_contents)
}
