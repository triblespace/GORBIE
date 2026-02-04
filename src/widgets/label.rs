use eframe::egui::{vec2, Align2, Response, Sense, TextStyle, TextWrapMode, Ui, WidgetText};

/// Label sized to the control height, with vertically centered text.
pub fn row_label(ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
    let text = text.into();
    let wrap_mode = Some(TextWrapMode::Extend);
    let max_text_width = ui.available_width().max(0.0);
    let galley = text.into_galley(
        ui,
        wrap_mode,
        max_text_width,
        TextStyle::Name("LCD".into()),
    );

    let height = ui.spacing().interact_size.y.max(galley.size().y);
    let desired_size = vec2(galley.size().x, height);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let placement = Align2::LEFT_CENTER.align_size_within_rect(galley.size(), rect);
        let text_color = if ui.visuals().dark_mode {
            crate::themes::ral(6027)
        } else {
            crate::themes::ral(9011)
        };
        let galley_pos = placement.min - galley.rect.min.to_vec2();
        ui.painter()
            .with_clip_rect(rect)
            .galley(galley_pos, galley, text_color);
    }

    response
}
