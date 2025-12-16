use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Vec2, Visuals,
};

mod style;
pub use style::Styled;
pub mod ral;
use ral::RAL_COLORS;

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
    let outline = blend(ral(9011), ral(7047), 0.4);

    if dark_mode {
        GorbieSliderStyle {
            rail_bg: ral(9004),
            rail_fill: outline,
            knob: ral(9003),
            shadow: ral(9004),
            shadow_offset: egui::vec2(2.0, 2.0),
            knob_extra_radius: 0.0,
        }
    } else {
        GorbieSliderStyle {
            rail_bg: ral(9004),
            rail_fill: outline,
            knob: ral(9003),
            shadow: ral(9004),
            shadow_offset: egui::vec2(2.0, 2.0),
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

pub fn ral(num: u16) -> Color32 {
    RAL_COLORS
        .iter()
        .find(|(code, _, _)| *code == num)
        .map(|(_, _, c)| *c)
        .unwrap_or(Color32::from_rgb(0, 0, 0))
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
    let weak_text = blend(foreground, background, 0.55);
    let link = ral(5005);
    let popup_shadow_color = ral(9004);

    base_visuals.window_fill = background;
    base_visuals.panel_fill = background;
    base_visuals.override_text_color = None;
    base_visuals.weak_text_alpha = 1.0;
    base_visuals.weak_text_color = Some(weak_text);
    base_visuals.disabled_alpha = 1.0;
    base_visuals.faint_bg_color = surface_muted;
    base_visuals.extreme_bg_color = surface_hover;
    base_visuals.slider_trailing_fill = true;
    base_visuals.selection = Selection {
        bg_fill: accent,
        stroke: Stroke::new(1.5, foreground),
    };
    base_visuals.hyperlink_color = link;
    base_visuals.window_stroke = Stroke::new(1.0, border);
    base_visuals.menu_corner_radius = 0.0.into();

    base_visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: surface,
            weak_bg_fill: surface,
            bg_stroke: Stroke::new(1.0, border),
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 10.0.into(),
            expansion: 1.0,
        },
        inactive: WidgetVisuals {
            bg_fill: surface,
            weak_bg_fill: surface,
            bg_stroke: Stroke::new(1.0, border),
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 10.0.into(),
            expansion: 1.0,
        },
        hovered: WidgetVisuals {
            bg_fill: surface_hover,
            weak_bg_fill: surface_hover,
            bg_stroke: Stroke::new(1.0, accent),
            fg_stroke: Stroke::new(1.2, foreground),
            corner_radius: 10.0.into(),
            expansion: 2.0,
        },
        active: WidgetVisuals {
            bg_fill: accent,
            weak_bg_fill: accent,
            bg_stroke: Stroke::new(1.0, accent),
            fg_stroke: Stroke::new(1.4, foreground),
            corner_radius: 10.0.into(),
            expansion: 2.0,
        },
        open: WidgetVisuals {
            bg_fill: background,
            weak_bg_fill: background,
            bg_stroke: Stroke::new(1.0, border),
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: 10.0.into(),
            expansion: 1.0,
        },
    };

    base_visuals.window_shadow = egui::epaint::Shadow::NONE;
    base_visuals.popup_shadow = egui::epaint::Shadow {
        offset: [4, 4],
        blur: 0,
        spread: 0,
        color: popup_shadow_color,
    };

    base_visuals
}

pub fn industrial_light() -> Style {
    let mut style = Style::default();

    style.text_styles = industrial_text_styles().into_iter().collect();

    let foreground = ral(9011);
    let background = ral(7047);
    let surface = ral(7047);
    let accent = ral(2009);

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

    let foreground = ral(9003);
    let background = ral(7046);
    let surface = ral(7047);
    let accent = ral(2009);

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
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["Inconsolata".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["Inconsolata".to_owned()]);
    fonts.families.insert(
        FontFamily::Name("Inconsolata".into()),
        vec!["Inconsolata".to_owned()],
    );

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
