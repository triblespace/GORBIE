use eframe::egui;

pub fn collapsing_divider<R>(
    ui: &mut egui::Ui,
    height: f32,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::Response {
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
    ui.painter().rect_filled(
        pill_rect,
        pill_rect.height() / 2.0,
        visuals.widgets.active.bg_fill,
    );

    // Let the caller render contents inside the parent ui. Return the header response so
    // the caller can inspect clicks or other interactions.
    let _ = contents(ui);

    hdr_resp
}
