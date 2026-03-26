use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Vec2, Visuals,
};

/// Widget style extraction trait.
mod style;
pub use style::Styled;
/// Deterministic color hashing for categorical UI coloring.
pub mod colorhash;
/// RAL Classic colour table (272 colours).
pub mod ral;
use ral::RAL_COLORS;

/// Gorbie-specific semantic style for the custom slider widget.
#[derive(Clone, Debug)]
pub struct GorbieSliderStyle {
    /// Background color of the slider rail.
    pub rail_bg: Color32,
    /// Fill color of the slider rail (trailing fill).
    pub rail_fill: Color32,
    /// Color of the slider knob.
    pub knob: Color32,
    /// Drop shadow color.
    pub shadow: Color32,
    /// Drop shadow offset in pixels.
    pub shadow_offset: Vec2,
    /// Extra radius added to the knob beyond its default size.
    pub knob_extra_radius: f32,
}

/// Gorbie-specific semantic style for the `Button` widget.
#[derive(Clone, Debug)]
pub struct GorbieButtonStyle {
    /// Button face fill color.
    pub fill: Color32,
    /// Border/outline color.
    pub outline: Color32,
    /// Accent color for focus/hover highlights.
    pub accent: Color32,
    /// Drop shadow color.
    pub shadow: Color32,
    /// Drop shadow offset in pixels.
    pub shadow_offset: Vec2,
    /// Corner rounding radius.
    pub rounding: f32,
}

/// Gorbie-specific semantic style for the `ToggleButton` widget.
#[derive(Clone, Debug)]
pub struct GorbieToggleButtonStyle {
    /// Button face fill color.
    pub fill: Color32,
    /// Border/outline color.
    pub outline: Color32,
    /// Accent color for focus/hover highlights.
    pub accent: Color32,
    /// Drop shadow color.
    pub shadow: Color32,
    /// Drop shadow offset in pixels.
    pub shadow_offset: Vec2,
    /// Corner rounding radius.
    pub rounding: f32,
    /// Background color of the LED rail.
    pub rail_bg: Color32,
    /// LED color when the toggle is on.
    pub led_on: Color32,
    /// Blend factor toward fill when the LED is off (0.0 = rail_bg, 1.0 = fill).
    pub led_off_towards_fill: f32,
}

/// Gorbie-specific semantic style for the `ChoiceToggle` widget.
#[derive(Clone, Debug)]
pub struct GorbieChoiceToggleStyle {
    /// Button face fill color.
    pub fill: Color32,
    /// Border/outline color.
    pub outline: Color32,
    /// Accent color for focus/hover highlights.
    pub accent: Color32,
    /// Drop shadow color.
    pub shadow: Color32,
    /// Drop shadow offset in pixels.
    pub shadow_offset: Vec2,
    /// Corner rounding of the outer slot.
    pub slot_rounding: f32,
    /// Corner rounding of individual segments.
    pub segment_rounding: u8,
    /// Background color of the rail behind segments.
    pub rail_bg: Color32,
    /// Gap between adjacent segments in pixels.
    pub segment_gap: f32,
    /// LED color when the segment is selected.
    pub led_on: Color32,
    /// Blend factor toward fill when the LED is off.
    pub led_off_towards_fill: f32,
}

/// Gorbie-specific semantic style for the `RadioButton` widget.
#[derive(Clone, Debug)]
pub struct GorbieRadioStyle {
    /// Button face fill color.
    pub fill: Color32,
    /// Border/outline color.
    pub outline: Color32,
    /// Accent color for focus/hover highlights.
    pub accent: Color32,
    /// Drop shadow color.
    pub shadow: Color32,
    /// Drop shadow offset in pixels.
    pub shadow_offset: Vec2,
    /// Corner rounding radius.
    pub rounding: f32,
    /// Background color of the radio indicator rail.
    pub rail_bg: Color32,
    /// Indicator color when selected.
    pub indicator_on: Color32,
    /// Blend factor toward fill when the indicator is off.
    pub indicator_off_towards_fill: f32,
}

/// Gorbie-specific semantic style for the `ProgressBar` widget.
#[derive(Clone, Debug)]
pub struct GorbieProgressBarStyle {
    /// Background color of the progress rail.
    pub rail_bg: Color32,
    /// Border/outline color.
    pub outline: Color32,
    /// Accent color for focus/hover highlights.
    pub accent: Color32,
    /// Blend factor toward outline for unlit segments.
    pub off_towards_outline: f32,
    /// Inset in pixels between the slot border and the fill area.
    pub fill_inset: f32,
}

/// Gorbie-specific semantic style for the `Histogram` widget.
#[derive(Clone, Debug)]
pub struct GorbieHistogramStyle {
    /// Border/outline color.
    pub outline: Color32,
    /// Foreground ink color for bars and labels.
    pub ink: Color32,
    /// Grid line color.
    pub grid: Color32,
    /// Accent color for highlighted bars.
    pub accent: Color32,
}

/// Gorbie-specific semantic style for the `TextField` widget.
#[derive(Clone, Debug)]
pub struct GorbieTextFieldStyle {
    /// Background fill color of the text field.
    pub fill: Color32,
    /// Border/outline color.
    pub outline: Color32,
    /// Accent color for the cursor and selection.
    pub accent: Color32,
    /// Corner rounding radius.
    pub rounding: f32,
    /// Height of the LCD scanline overlay effect.
    pub scanline_height: f32,
}

/// Gorbie-specific semantic style for the `NumberField` widget.
#[derive(Clone, Debug)]
pub struct GorbieNumberFieldStyle {
    /// Background fill color of the number field.
    pub fill: Color32,
    /// Border/outline color.
    pub outline: Color32,
    /// Accent color for the cursor and selection.
    pub accent: Color32,
    /// Corner rounding radius.
    pub rounding: f32,
    /// Height of the LCD scanline overlay effect.
    pub scanline_height: f32,
}

/// Return a `GorbieSliderStyle` preset for light/dark mode based on our base tokens.
pub fn slider_style(_dark_mode: bool) -> GorbieSliderStyle {
    let outline = blend(ral(9011), ral(7047), 0.4);

    GorbieSliderStyle {
        rail_bg: ral(9004),
        rail_fill: outline,
        knob: ral(9003),
        shadow: ral(9004),
        shadow_offset: egui::vec2(2.0, 2.0),
        knob_extra_radius: 0.0,
    }
}

/// The "on" indicator color for button LEDs (RAL 2005, luminous orange).
pub fn button_light_on() -> Color32 {
    ral(2005)
}

impl From<&Style> for GorbieSliderStyle {
    fn from(style: &Style) -> Self {
        slider_style(style.visuals.dark_mode)
    }
}

impl From<&Style> for GorbieButtonStyle {
    fn from(style: &Style) -> Self {
        let base = GorbieSliderStyle::from(style);
        Self {
            fill: base.knob,
            outline: base.rail_fill,
            accent: style.visuals.selection.stroke.color,
            shadow: base.shadow,
            shadow_offset: base.shadow_offset,
            rounding: 2.0,
        }
    }
}

impl From<&Style> for GorbieToggleButtonStyle {
    fn from(style: &Style) -> Self {
        let base = GorbieSliderStyle::from(style);
        Self {
            fill: base.knob,
            outline: base.rail_fill,
            accent: style.visuals.selection.stroke.color,
            shadow: base.shadow,
            shadow_offset: base.shadow_offset,
            rounding: 2.0,
            rail_bg: base.rail_bg,
            led_on: button_light_on(),
            led_off_towards_fill: 0.25,
        }
    }
}

impl From<&Style> for GorbieChoiceToggleStyle {
    fn from(style: &Style) -> Self {
        let base = GorbieSliderStyle::from(style);
        Self {
            fill: base.knob,
            outline: base.rail_fill,
            accent: style.visuals.selection.stroke.color,
            shadow: base.shadow,
            shadow_offset: base.shadow_offset,
            slot_rounding: 2.0,
            segment_rounding: 2,
            rail_bg: base.rail_bg,
            segment_gap: 2.0,
            led_on: button_light_on(),
            led_off_towards_fill: 0.25,
        }
    }
}

impl From<&Style> for GorbieRadioStyle {
    fn from(style: &Style) -> Self {
        let base = GorbieSliderStyle::from(style);
        Self {
            fill: base.knob,
            outline: base.rail_fill,
            accent: style.visuals.selection.stroke.color,
            shadow: base.shadow,
            shadow_offset: base.shadow_offset,
            rounding: 2.0,
            rail_bg: base.rail_bg,
            indicator_on: button_light_on(),
            indicator_off_towards_fill: 0.25,
        }
    }
}

impl From<&Style> for GorbieProgressBarStyle {
    fn from(style: &Style) -> Self {
        let base = GorbieSliderStyle::from(style);
        Self {
            rail_bg: base.rail_bg,
            outline: base.rail_fill,
            accent: style.visuals.selection.stroke.color,
            off_towards_outline: 0.18,
            fill_inset: 2.0,
        }
    }
}

impl From<&Style> for GorbieHistogramStyle {
    fn from(style: &Style) -> Self {
        let background = style.visuals.window_fill;
        let ink = style.visuals.widgets.noninteractive.fg_stroke.color;
        Self {
            outline: style.visuals.widgets.noninteractive.bg_stroke.color,
            ink,
            grid: blend(background, ink, 0.22),
            accent: style.visuals.selection.stroke.color,
        }
    }
}

impl From<&Style> for GorbieTextFieldStyle {
    fn from(style: &Style) -> Self {
        let dark_mode = style.visuals.dark_mode;
        Self {
            fill: if dark_mode { ral(9004) } else { ral(6027) },
            outline: if dark_mode { ral(6027) } else { ral(9011) },
            accent: style.visuals.selection.stroke.color,
            rounding: 0.0,
            scanline_height: 3.0,
        }
    }
}

impl From<&Style> for GorbieNumberFieldStyle {
    fn from(style: &Style) -> Self {
        let dark_mode = style.visuals.dark_mode;
        Self {
            fill: if dark_mode { ral(9004) } else { ral(6027) },
            outline: if dark_mode { ral(6027) } else { ral(9011) },
            accent: style.visuals.selection.stroke.color,
            rounding: 0.0,
            scanline_height: 3.0,
        }
    }
}

/// Linearly interpolate between two colors in sRGB space.
///
/// `t = 0.0` returns `a`, `t = 1.0` returns `b`.
pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let r = (a.r() as f32 * (1.0 - t) + b.r() as f32 * t).round() as u8;
    let g = (a.g() as f32 * (1.0 - t) + b.g() as f32 * t).round() as u8;
    let bch = (a.b() as f32 * (1.0 - t) + b.b() as f32 * t).round() as u8;
    Color32::from_rgb(r, g, bch)
}

/// Look up a RAL Classic color by its number, returning black if not found.
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
    _accent: Color32,
    mut base_visuals: Visuals,
) -> Visuals {
    let surface_muted = blend(surface, background, 0.2);
    let border = blend(foreground, background, 0.4);
    let weak_text = blend(foreground, background, 0.55);
    let control_radius = 2.0;
    let container_radius = 0.0;

    let control_fill = background;
    let control_fill_hover = blend(background, foreground, 0.05);
    let control_fill_active = blend(control_fill_hover, ral(9011), 0.12);
    let selection_fill = blend(background, foreground, 0.12);
    let link = ral(5005);
    let popup_shadow_color = ral(9004);

    base_visuals.window_fill = background;
    base_visuals.panel_fill = background;
    base_visuals.override_text_color = None;
    base_visuals.weak_text_alpha = 1.0;
    base_visuals.weak_text_color = Some(weak_text);
    base_visuals.disabled_alpha = 1.0;
    base_visuals.faint_bg_color = surface_muted;
    base_visuals.extreme_bg_color = control_fill_hover;
    base_visuals.slider_trailing_fill = true;
    base_visuals.selection = Selection {
        bg_fill: selection_fill,
        stroke: Stroke::new(2.0, foreground),
    };
    base_visuals.hyperlink_color = link;
    base_visuals.window_stroke = Stroke::new(1.0, border);
    base_visuals.menu_corner_radius = 0.0.into();

    let border_stroke = Stroke::new(1.0, border);
    let hover_stroke = Stroke::new(1.4, border);
    let active_stroke = Stroke::new(2.0, foreground);

    base_visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: surface,
            weak_bg_fill: surface,
            bg_stroke: border_stroke,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: container_radius.into(),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: control_fill,
            weak_bg_fill: control_fill,
            bg_stroke: border_stroke,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: control_radius.into(),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: control_fill_hover,
            weak_bg_fill: control_fill_hover,
            bg_stroke: hover_stroke,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: control_radius.into(),
            expansion: 0.0,
        },
        active: WidgetVisuals {
            bg_fill: control_fill_active,
            weak_bg_fill: control_fill_active,
            bg_stroke: active_stroke,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: control_radius.into(),
            expansion: 0.0,
        },
        open: WidgetVisuals {
            bg_fill: control_fill_hover,
            weak_bg_fill: control_fill_hover,
            bg_stroke: active_stroke,
            fg_stroke: Stroke::new(1.0, foreground),
            corner_radius: control_radius.into(),
            expansion: 0.0,
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

/// Complete light-mode egui `Style` using the industrial RAL palette.
pub fn industrial_light() -> Style {
    let mut style = Style {
        text_styles: industrial_text_styles().into_iter().collect(),
        ..Default::default()
    };

    let foreground = ral(9011);
    let background = ral(7047);
    let surface = ral(7047);
    let accent = ral(2009);

    let visuals = industrial(foreground, background, surface, accent, Visuals::light());

    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 240.0;
    // 34px matches our button + LCD field visual height (with the current padding/font sizes).
    style.spacing.interact_size = egui::vec2(34.0, 34.0);
    style.animation_time = 0.12;

    style.visuals = visuals;
    style
}

/// Complete dark-mode egui `Style` using the industrial RAL palette.
pub fn industrial_dark() -> Style {
    let mut style = Style {
        text_styles: industrial_text_styles().into_iter().collect(),
        ..Default::default()
    };

    let foreground = ral(9003);
    let background = ral(7046);
    let surface = ral(7047);
    let accent = ral(2009);

    let visuals = industrial(foreground, background, surface, accent, Visuals::dark());

    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 240.0;
    // 34px matches our button + LCD field visual height (with the current padding/font sizes).
    style.spacing.interact_size = egui::vec2(34.0, 34.0);
    style.animation_time = 0.12;

    style.visuals = visuals;
    style
}

/// Font definitions for the industrial theme (IosevkaGorbie + LCD).
pub fn industrial_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    // Remove defaults to avoid fallback to built-in fonts.
    fonts.font_data.clear();

    fonts.font_data.insert(
        "IosevkaGorbie".to_owned(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/IosevkaGorbie/IosevkaGorbie-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "IosevkaGorbieBold".to_owned(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/IosevkaGorbie/IosevkaGorbie-Bold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "LCD".to_owned(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Jersey_15/Jersey15-Regular.ttf"
        ))),
    );

    fonts.families.clear();
    fonts
        .families
        .insert(FontFamily::Proportional, vec!["IosevkaGorbie".to_owned()]);
    fonts
        .families
        .insert(FontFamily::Monospace, vec!["IosevkaGorbie".to_owned()]);
    fonts.families.insert(
        FontFamily::Name("IosevkaGorbie".into()),
        vec!["IosevkaGorbie".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("IosevkaGorbieBold".into()),
        vec!["IosevkaGorbieBold".to_owned()],
    );
    fonts
        .families
        .insert(FontFamily::Name("LCD".into()), vec!["LCD".to_owned()]);

    fonts
}

/// Grid-aligned text styles for IosevkaGorbie.
///
/// Every style produces a row height that is a multiple (or half-multiple)
/// of `GRID_ROW_MODULE` (12px):
///
/// | Style     | Font size | Row height | Modules |
/// |-----------|-----------|------------|---------|
/// | Heading   |    29px   |   36px     |   3     |
/// | Body      |    15px   |   18px     |   1.5   |
/// | Button    |    15px   |   18px     |   1.5   |
/// | Monospace |    14px   |   18px     |   1.5   |
/// | Small     |   9.5px   |   12px     |   1     |
pub fn industrial_text_styles() -> Vec<(TextStyle, FontId)> {
    vec![
        (
            TextStyle::Heading,
            FontId::new(29.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Body,
            FontId::new(15.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Name("LCD".into()),
            FontId::new(15.0, FontFamily::Name("LCD".into())),
        ),
        (
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Small,
            FontId::new(9.5, FontFamily::Name("IosevkaGorbie".into())),
        ),
    ]
}
