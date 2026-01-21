use eframe::egui;

#[cfg(feature = "code")]
pub fn code_view(ui: &mut egui::Ui, code: &str, language: &str) -> egui::Response {
    let bg = ui.visuals().code_bg_color;
    let stroke = ui.visuals().widgets.inactive.bg_stroke;

    let inner = egui::Frame::group(ui.style())
        .fill(bg)
        .stroke(stroke)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(10.0)
        .show(ui, |ui| {
            let theme =
                egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
            egui_extras::syntax_highlighting::code_view_ui(ui, &theme, code, language)
        });

    inner.response.union(inner.inner)
}

#[cfg(not(feature = "code"))]
pub fn code_view(ui: &mut egui::Ui, code: &str, _language: &str) -> egui::Response {
    let bg = ui.visuals().code_bg_color;
    let stroke = ui.visuals().widgets.inactive.bg_stroke;

    let inner = egui::Frame::group(ui.style())
        .fill(bg)
        .stroke(stroke)
        .inner_margin(egui::Margin::same(8))
        .corner_radius(10.0)
        .show(ui, |ui| ui.label(egui::RichText::new(code).monospace()));

    inner.response.union(inner.inner)
}
