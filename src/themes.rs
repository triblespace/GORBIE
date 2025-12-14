use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Vec2,
    Visuals,
};

mod style;
pub use style::Styled;

/// Gorbie-specific semantic style for the custom slider widget.
#[derive(Clone, Debug)]
pub struct GorbieSliderStyle {
    pub rail_bg: Color32,
    pub rail_fill: Color32,
    pub knob: Color32,
    pub shadow: Color32,
    pub shadow_offset: Vec2,
    pub knob_extra_radius: f32,
}

/// Return a `GorbieSliderStyle` preset for light/dark mode based on our base tokens.
pub fn slider_style(dark_mode: bool) -> GorbieSliderStyle {
    if dark_mode {
        let background = ral_telegrey();
        let accent_foreground = ral_orange();
        let accent_background = ral_orange();

        GorbieSliderStyle {
            rail_bg: blend(background, accent_background, 0.10),
            rail_fill: accent_foreground,
            knob: accent_foreground,
            shadow: accent_background,
            shadow_offset: egui::vec2(-3.0, 0.0),
            knob_extra_radius: 0.0,
        }
    } else {
        let background = ral_signal_white();
        let accent_foreground = ral_orange();
        let accent_background = ral_telegrey();

        GorbieSliderStyle {
            rail_bg: blend(background, accent_background, 0.10),
            rail_fill: accent_foreground,
            knob: accent_foreground,
            shadow: accent_background,
            shadow_offset: egui::vec2(-3.0, 0.0),
            knob_extra_radius: 0.0,
        }
    }
}

// Color utilities: simple sRGB linear interpolation for quick palette derivation
pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let r = (a.r() as f32 * (1.0 - t) + b.r() as f32 * t).round() as u8;
    let g = (a.g() as f32 * (1.0 - t) + b.g() as f32 * t).round() as u8;
    let bch = (a.b() as f32 * (1.0 - t) + b.b() as f32 * t).round() as u8;
    Color32::from_rgb(r, g, bch)
}

// RAL palette
pub fn ral_orange() -> Color32 {
    Color32::from_rgb(0xF4, 0x46, 0x11) // Traffic Orange (RAL 2009)
}
pub fn ral_signal_white() -> Color32 {
    Color32::from_rgb(0xF4, 0xF4, 0xF4) // Signal White (RAL 9003)
}
pub fn ral_telegrey() -> Color32 {
    Color32::from_rgb(0xCF, 0xD3, 0xD5) // Telegrey (RAL 7047)
}
pub fn ral_ink() -> Color32 {
    Color32::from_rgb(0x1C, 0x1C, 0x1C)
}

/// Build visuals from the RAL palette for a clean, industrial feel.
pub fn industrial(
    foreground: Color32,
    background: Color32,
    surface: Color32,
    accent: Color32,
    mut base_visuals: Visuals,
) -> Visuals {
    let surface_muted = blend(surface, background, 0.2);
    let surface_hover = blend(surface, accent, 0.08);
    let border = blend(foreground, background, 0.4);

    base_visuals.window_fill = background;
    base_visuals.panel_fill = surface;
    base_visuals.override_text_color = None;
    base_visuals.faint_bg_color = surface_muted;
    base_visuals.extreme_bg_color = surface_hover;
    base_visuals.slider_trailing_fill = true;
    base_visuals.selection = Selection {
        bg_fill: accent,
        stroke: Stroke::new(1.5, foreground),
    };
    base_visuals.hyperlink_color = accent;

    base_visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: surface,
            weak_bg_fill: surface,
            bg_stroke: Stroke::new(1.0, border),
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 8.0.into(),
            expansion: 1.0,
        },
        inactive: WidgetVisuals {
            bg_fill: surface,
            weak_bg_fill: surface,
            bg_stroke: Stroke::new(1.0, border),
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 8.0.into(),
            expansion: 1.0,
        },
        hovered: WidgetVisuals {
            bg_fill: surface_hover,
            weak_bg_fill: surface_hover,
            bg_stroke: Stroke::new(1.0, accent),
            fg_stroke: Stroke::new(1.2, foreground),
            corner_radius: 8.0.into(),
            expansion: 2.0,
        },
        active: WidgetVisuals {
            bg_fill: accent,
            weak_bg_fill: accent,
            bg_stroke: Stroke::new(1.0, accent),
            fg_stroke: Stroke::new(1.4, foreground),
            corner_radius: 8.0.into(),
            expansion: 2.0,
        },
        open: WidgetVisuals {
            bg_fill: background,
            weak_bg_fill: background,
            bg_stroke: Stroke::new(1.0, border),
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 8.0.into(),
            expansion: 1.0,
        },
    };

    base_visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 22,
        spread: 0,
        color: blend(foreground, background, 0.25),
    };

    base_visuals
}

pub fn industrial_light() -> Style {
    let mut style = Style::default();

    style.text_styles = industrial_text_styles().into_iter().collect();

    let foreground = ral_ink();
    let background = ral_signal_white();
    let surface = ral_telegrey();
    let accent = ral_orange();

    let visuals = industrial(foreground, background, surface, accent, Visuals::light());

    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 240.0;
    style.spacing.interact_size = egui::vec2(34.0, 26.0);
    style.animation_time = 0.12;

    style.visuals = visuals;
    style
}

pub fn industrial_dark() -> Style {
    let mut style = Style::default();

    style.text_styles = industrial_text_styles().into_iter().collect();

    let foreground = ral_signal_white();
    let background = Color32::from_rgb(0x20, 0x23, 0x24);
    let surface = ral_telegrey();
    let accent = ral_orange();

    let visuals = industrial(foreground, background, surface, accent, Visuals::dark());

    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 240.0;
    style.spacing.interact_size = egui::vec2(34.0, 26.0);
    style.animation_time = 0.12;

    style.visuals = visuals;
    style
}

pub fn industrial_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    // Remove defaults to avoid fallback to built-in fonts.
    fonts.font_data.clear();

    fonts.font_data.insert(
        "Inconsolata".to_owned(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inconsolata/Inconsolata-VariableFont_wdth,wght.ttf"
        ))),
    );

    fonts.families.clear();
    fonts.families.insert(
        FontFamily::Proportional,
        vec!["Inconsolata".to_owned()],
    );
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["Inconsolata".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Name("Inconsolata".into()), vec!["Inconsolata".to_owned()]);

    fonts
}

pub fn industrial_text_styles() -> Vec<(TextStyle, FontId)> {
    vec![
        (
            TextStyle::Heading,
            FontId::new(30.0, FontFamily::Name("Inconsolata".into())),
        ),
        (
            TextStyle::Body,
            FontId::new(16.0, FontFamily::Name("Inconsolata".into())),
        ),
        (
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Name("Inconsolata".into())),
        ),
        (
            TextStyle::Button,
            FontId::new(16.0, FontFamily::Name("Inconsolata".into())),
        ),
        (
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Name("Inconsolata".into())),
        ),
    ]
}
