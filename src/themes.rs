use egui::{
    style::{Selection, WidgetVisuals, Widgets},
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Visuals,
};

// Color utilities: simple sRGB linear interpolation for quick palette derivation
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let r = (a.r() as f32 * (1.0 - t) + b.r() as f32 * t).round() as u8;
    let g = (a.g() as f32 * (1.0 - t) + b.g() as f32 * t).round() as u8;
    let bch = (a.b() as f32 * (1.0 - t) + b.b() as f32 * t).round() as u8;
    Color32::from_rgb(r, g, bch)
}

// Accessor functions for base tokens (use instead of direct consts in functions)
pub fn base_ink() -> Color32 {
    // Midpoint between the warm ink (#35243E) and the old panel (#1B1821)
    egui::hex_color!("#241C2B")
}
pub fn base_parchment() -> Color32 {
    egui::hex_color!("#FBF6F1")
}
pub fn base_purple() -> Color32 {
    egui::hex_color!("#5d2fe3")
}
pub fn base_teal() -> Color32 {
    egui::hex_color!("#35C9BE")
}

pub fn cosmic_gel_light() -> Style {
    let mut style = Style::default();

    style.text_styles = cosmic_gel_text_styles().into_iter().collect();

    // Design tokens (named colors)
    let ink = base_ink();
    let parchment = base_parchment();
    // semantic names for intent: brand primary and supporting contrast accent
    let purple = base_purple();
    let teal = base_teal();

    // hover blend tokens (25% purple over base)
    let hover_light = blend(parchment, purple, 0.25);

    // additional named tones derived from base tokens
    let panel = parchment;
    let panel_alt = blend(panel, purple, 0.10); // 10% brand_primary over panel
                                                       // Keep blends bounded between parchment and ink (no pure white/black)
    let panel_weak = blend(panel, parchment, 0.02); // slight tint toward purple (still <= parchment)
    let panel_alt_weak = blend(panel_alt, parchment, 0.02); // nudge back toward parchment
    let faint_bg = blend(panel, ink, 0.01); // slightly darker toward ink
    let extreme_bg = blend(panel, purple, 0.08);
    let active_weak = teal;

    let visuals = Visuals {
        dark_mode: false,
        window_fill: parchment,
        panel_fill: panel,
        override_text_color: None,
        faint_bg_color: faint_bg,
        // Visible separator color on parchment
        extreme_bg_color: extreme_bg,
        slider_trailing_fill: true,
        selection: Selection {
            bg_fill: purple,
            stroke: Stroke::new(2.0, parchment),
        },
        hyperlink_color: purple,
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: panel,
                weak_bg_fill: panel_weak,
                bg_stroke: Stroke::NONE,
                // Make sure icons and inline text use ink explicitly
                fg_stroke: Stroke::new(1.0, ink),
                corner_radius: 10.0.into(),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: panel_alt,
                weak_bg_fill: panel_alt_weak,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, ink),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            hovered: WidgetVisuals {
                bg_fill: hover_light,
                weak_bg_fill: purple,
                bg_stroke: Stroke::NONE,
                // stronger ink on hover so highlights remain visible
                fg_stroke: Stroke::new(1.4, ink),
                corner_radius: 10.0.into(),
                expansion: 3.0,
            },
            active: WidgetVisuals {
                bg_fill: purple,
                weak_bg_fill: active_weak,
                bg_stroke: Stroke::NONE,
                // use `ink` for active icons in light theme
                fg_stroke: Stroke::new(1.5, parchment),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            open: WidgetVisuals {
                bg_fill: panel,
                weak_bg_fill: panel_weak,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, ink),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
        },
        // Shadow: derive from base tokens to respect palette (slightly darker than ink)
        window_shadow: egui::epaint::Shadow {
            offset: [0, 6],
            blur: 14,
            spread: 0,
            color: blend(ink, panel, 0.18),
        },
        ..Visuals::light()
    };

    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(10.0, 7.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 240.0;
    style.spacing.interact_size = egui::vec2(32.0, 24.0);
    style.animation_time = 0.14;

    style.visuals = visuals;
    style
}

pub fn cosmic_gel_dark() -> Style {
    let mut style = Style::default();

    style.text_styles = cosmic_gel_text_styles().into_iter().collect();

    // Base tokens
    let ink = base_ink();
    let parchment = base_parchment();
    // in dark theme we swap roles: use contrast_accent as brand primary here
    let teal = base_teal(); // TEAL
    let purple = base_purple(); // PURPLE

    // derived dark tones
    let hover_dark = blend(ink, teal, 0.25);
    // Use the same "ink" color for both the window and panel so the dark theme
    // background matches the ink tone (consistent with light theme where both
    // window_fill and panel_fill use parchment).
    let panel = ink;
    let panel_alt = blend(panel, teal, 0.10);
    // Keep dark blends bounded toward ink rather than pure black
    let panel_weak = blend(panel, ink, 0.08);
    let faint_bg = blend(panel, ink, 0.15);
    let extreme_bg = blend(parchment, ink, 0.10);
    let active_weak = purple;

    let visuals = Visuals {
        dark_mode: true,
        window_fill: ink,
        panel_fill: panel,
        override_text_color: None,
        faint_bg_color: faint_bg,
        extreme_bg_color: extreme_bg,
        slider_trailing_fill: true,
        selection: Selection {
            bg_fill: teal,
            stroke: Stroke::new(2.0, ink),
        },
        hyperlink_color: teal,
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: panel,
                weak_bg_fill: panel_weak,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, parchment),
                corner_radius: 10.0.into(),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: panel_alt,
                weak_bg_fill: panel_weak,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, ink),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            hovered: WidgetVisuals {
                bg_fill: hover_dark,
                weak_bg_fill: teal,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.4, parchment),
                corner_radius: 10.0.into(),
                expansion: 3.0,
            },
            active: WidgetVisuals {
                bg_fill: teal,
                weak_bg_fill: active_weak,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.5, parchment),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            open: WidgetVisuals {
                bg_fill: panel,
                weak_bg_fill: panel_weak,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, parchment),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
        },
        // Shadow: derive from base tokens (slightly darker than panel)
        window_shadow: egui::epaint::Shadow {
            offset: [0, 10],
            blur: 20,
            spread: 0,
            color: blend(panel, ink, 0.22),
        },
        ..Visuals::dark()
    };

    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(10.0, 7.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 240.0;
    style.spacing.interact_size = egui::vec2(32.0, 24.0);
    style.animation_time = 0.14;

    style.visuals = visuals;
    style
}

pub fn cosmic_gel_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "Lora".to_owned(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Lora/Lora-VariableFont_wght.ttf"
        ))),
    );

    fonts.font_data.insert(
        "Caprasimo".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Caprasimo/Caprasimo-Regular.ttf"
        ))
        .into(),
    );

    fonts.font_data.insert(
        "JetBrainsMono".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrains_Mono/static/JetBrainsMono-Regular.ttf"
        ))
        .into(),
    );

    // Set up font families
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "Lora".to_owned());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, "JetBrainsMono".to_owned());

    fonts
        .families
        .insert(FontFamily::Name("Lora".into()), vec!["Lora".to_owned()]);
    fonts.families.insert(
        FontFamily::Name("Caprasimo".into()),
        vec!["Caprasimo".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("JetBrainsMono".into()),
        vec!["JetBrainsMono".to_owned()],
    );

    fonts
}

pub fn cosmic_gel_text_styles() -> Vec<(TextStyle, FontId)> {
    vec![
        (
            TextStyle::Heading,
            // Restore the warm, old-book charm
            FontId::new(30.0, FontFamily::Name("Caprasimo".into())),
        ),
        (
            TextStyle::Body,
            FontId::new(16.0, FontFamily::Name("Lora".into())),
        ),
        (
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Name("JetBrainsMono".into())),
        ),
        (
            TextStyle::Button,
            FontId::new(16.0, FontFamily::Name("Lora".into())),
        ),
        (
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Name("Lora".into())),
        ),
    ]
}
