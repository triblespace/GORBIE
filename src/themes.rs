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
    egui::hex_color!("#4d2bb0")
}
pub fn base_teal() -> Color32 {
    egui::hex_color!("#35C9BE")
}

/// Generic palette-to-visuals transformer for the Cosmic Gel theme.
/// Computes derived tones from four base colors and overrides the provided
/// `base_visuals` with the same visual fields previously hard-coded for
/// the light variant.
pub fn cosmic_gel(
    foreground: Color32,
    background: Color32,
    accent_foreground: Color32,
    accent_background: Color32,
    mut base_visuals: Visuals,
) -> Visuals {
    // Derived tokens
    let accent_background_tint = blend(background, accent_background, 0.10);
    let accent_background_subtle = blend(background, accent_background, 0.03);
    let background_darker = blend(background, foreground, 0.01);

    base_visuals.window_fill = background;
    base_visuals.panel_fill = background;
    base_visuals.override_text_color = None;
    base_visuals.faint_bg_color = background_darker;
    base_visuals.extreme_bg_color = accent_background_tint;
    base_visuals.slider_trailing_fill = true;
    base_visuals.selection = Selection {
        // selection should pair with the background (use bg_accent) to avoid purple+ink
        bg_fill: accent_background,
        stroke: Stroke::new(2.0, foreground),
    };
    base_visuals.hyperlink_color = accent_foreground;

    base_visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: accent_background_subtle,
            weak_bg_fill: accent_background_subtle,
            bg_stroke: Stroke::NONE,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 10.0.into(),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: accent_background_tint,
            weak_bg_fill: accent_background_tint,
            bg_stroke: Stroke::NONE,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 10.0.into(),
            expansion: 2.0,
        },
        hovered: WidgetVisuals {
            bg_fill: accent_background_tint,
            // use the background-paired accent for hovered weak fill (teal), not the foreground-paired one
            weak_bg_fill: accent_background_tint,
            bg_stroke: Stroke::NONE,
            fg_stroke: Stroke::new(1.4, foreground),
            corner_radius: 10.0.into(),
            expansion: 3.0,
        },
        active: WidgetVisuals {
            // active background uses the foreground-paired accent
            bg_fill: accent_background,
            weak_bg_fill: accent_background,
            bg_stroke: Stroke::NONE,
            fg_stroke: Stroke::new(1.5, foreground),
            corner_radius: 10.0.into(),
            expansion: 2.0,
        },
        open: WidgetVisuals {
            bg_fill: background,
            weak_bg_fill: background,
            bg_stroke: Stroke::NONE,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 10.0.into(),
            expansion: 2.0,
        },
    };

    // Shadow: derive from base tokens to respect palette (slightly darker than foreground)
    base_visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 14,
        spread: 0,
        color: blend(foreground, background, 0.18),
    };

    base_visuals
}

pub fn cosmic_gel_light() -> Style {
    let mut style = Style::default();

    style.text_styles = cosmic_gel_text_styles().into_iter().collect();

    // Base tokens (physical colors)
    let foreground = base_ink();
    let background = base_parchment();
    // Semantic roles for light theme
    let accent_foreground = base_purple(); // brand primary
    let accent_background = base_teal(); // supporting accent

    // Build visuals by delegating to the shared transformer
    let visuals = cosmic_gel(foreground, background, accent_foreground, accent_background, Visuals::light());

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

    // For dark theme keep primary = purple and secondary = teal so background blends
    // produced by the shared generator use teal (secondary) and foreground accents use purple
    // Base tokens (physical colors)
    let foreground = base_parchment();
    let background = base_ink();
    let accent_foreground = base_teal();
    let accent_background = base_purple();

    // Delegate to the shared generator using Visuals::dark() as the base
    let visuals = cosmic_gel(foreground, background, accent_foreground, accent_background, Visuals::dark());

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
