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
    /// Background color of the slider rail — the milled groove, which sits on the
    /// *page*, so it inverts with the theme (it is the page ink).
    pub rail_bg: Color32,
    /// Fill color of the slider rail (trailing fill), and the slider's own ink:
    /// slot rim, knob rim, notches and tick marks all read this.
    pub rail_fill: Color32,
    /// Color of the slider knob.
    pub knob: Color32,
    /// Rim color for the button family, measured against the *keycap* face
    /// ([`GorbieSliderStyle::knob`]) rather than the page — hence theme-invariant.
    pub outline: Color32,
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
///
/// This is the base every widget-family style is built from, and it spans two
/// different surfaces, which is why some fields invert and others do not:
///
/// * `rail_bg` is the milled groove, drawn on the **page**, so it is the page ink
///   and inverts with the theme (16.03:1 against the page at both poles).
/// * `rail_fill` is the slider's ink — trailing fill, slot rim, knob rim, notches
///   and tick marks. RAL 2005 is the one value that clears 3:1 in all four roles:
///   page 3.60/4.46, groove 4.46/3.60, keycap 3.56.
/// * `outline` and `knob` are measured against the **keycap**, not the page, so
///   they are theme-invariant: RAL 9011 on RAL 9003 is 15.89:1 at both poles.
/// * `shadow` is `blend(background, foreground, 0.50)`, the fixed point of the
///   theme inversion — one hex (#8A8B86) at both poles, 3.23 light / 4.96 dark
///   against the page and 3.20 against the keycap it lifts.
pub fn slider_style(dark_mode: bool) -> GorbieSliderStyle {
    let foreground = if dark_mode { ral(9010) } else { ral(9011) };
    let background = if dark_mode { ral(9011) } else { ral(9010) };

    GorbieSliderStyle {
        rail_bg: foreground,
        rail_fill: ral(2005),
        knob: ral(9003),
        outline: ral(9011),
        shadow: blend(background, foreground, 0.50),
        shadow_offset: egui::vec2(2.0, 2.0),
        knob_extra_radius: 0.0,
    }
}

/// The "on" indicator color for button LEDs (RAL 2005, luminous orange).
pub fn button_light_on() -> Color32 {
    ral(2005)
}

/// Status: nominal (RAL 6010 "Grass green") — 5.18:1 light, 3.09:1 dark.
///
/// The four status colours are fixed rather than themed, and **must always be paired
/// with an icon and a label**; colour is the third channel here, never the first.
/// Two reasons, both measured. (1) Severity cannot be read from luminance: clearing
/// 3:1 on both poles pins every admissible colour into rel-lum 0.139..0.279, so a
/// monotone severity-by-lightness ladder does not exist — only the relief-band yellows
/// escape upward. (2) [`status_good`] against [`status_critical`] is a green/red pair,
/// which is exactly the distinction a red-green deficiency loses. RAL 6010/3014 is the
/// only pairing in the 202-entry table that clears the CVD target (ΔE 10.2) *and* keeps
/// a real lightness gap (OKLCH ΔL 0.158) *and* stays clear of the categorical ramp; the
/// conventional 6032/3018 pair scores ΔE 5.9 at ΔL 0.077, squarely inside the failure
/// mode. The recommended grammar is shape-first: nominal renders as an *outline* chip,
/// critical as a *solid fill* chip — a warning lamp is lit or it is not.
pub fn status_good() -> Color32 {
    ral(6010)
}

/// Status: warning (RAL 1018 "Zinc yellow") — 1.46:1 light, 11.00:1 dark.
///
/// Below 3:1 on the bone page by design: this is the light-surface relief band, and it
/// is legible only with the icon + label pairing described on [`status_good`].
pub fn status_warning() -> Color32 {
    ral(1018)
}

/// Status: serious (RAL 2003 "Pastel orange") — 2.35:1 light, 6.82:1 dark.
///
/// Also a relief-band value on bone; see [`status_warning`]. This is `warn_fg_color`.
/// It sits ΔE 6.8 from the RAL 2005 signature under CVD — unavoidable, since 2005
/// occupies the red-orange region "serious" wants — so never distinguish an accent from
/// a serious-status mark by colour alone.
pub fn status_serious() -> Color32 {
    ral(2003)
}

/// Status: critical (RAL 3014 "Antique pink") — 3.00:1 light, 5.33:1 dark.
///
/// This is `error_fg_color`. A pure red (`#FF0000`, egui's default) is never the right
/// answer here; see [`status_good`] for why this one was chosen and why it still may
/// not carry meaning on its own.
pub fn status_critical() -> Color32 {
    ral(3014)
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
            outline: base.outline,
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
            outline: base.outline,
            accent: style.visuals.selection.stroke.color,
            shadow: base.shadow,
            shadow_offset: base.shadow_offset,
            rounding: 2.0,
            // The LED rail sits on the keycap, not the page, so it does not invert.
            rail_bg: ral(9004),
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
            outline: base.outline,
            accent: style.visuals.selection.stroke.color,
            shadow: base.shadow,
            shadow_offset: base.shadow_offset,
            slot_rounding: 2.0,
            segment_rounding: 2,
            // The LED rail sits on the keycap, not the page, so it does not invert.
            rail_bg: ral(9004),
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
            outline: base.outline,
            accent: style.visuals.selection.stroke.color,
            shadow: base.shadow,
            shadow_offset: base.shadow_offset,
            rounding: 2.0,
            // The indicator rail sits on the keycap, not the page, so it does not invert.
            rail_bg: ral(9004),
            indicator_on: button_light_on(),
            indicator_off_towards_fill: 0.25,
        }
    }
}

impl From<&Style> for GorbieProgressBarStyle {
    fn from(style: &Style) -> Self {
        let base = GorbieSliderStyle::from(style);
        Self {
            // The progress track sits on the page, so it inverts with the theme.
            rail_bg: base.rail_bg,
            outline: base.outline,
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
            // The LCD well is the one place a `dark_mode` branch is honest: it is a lit
            // panel, not a page surface. RAL 6005 keeps the field *green* at both poles
            // (mint-on-graphite <-> mint-on-moss) where RAL 9004 measured 1.29:1 on the
            // graphite page. Ink is `lcd_ink_color` in `widgets/field.rs`: RAL 9011 on
            // 6027 = 8.27:1 light, RAL 6027 on 6005 = 5.43:1 dark.
            fill: if dark_mode { ral(6005) } else { ral(6027) },
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
            // Same LCD well as `GorbieTextFieldStyle`; see there for the measurements.
            fill: if dark_mode { ral(6005) } else { ral(6027) },
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
///
/// # Why the poles are RAL 9010 and RAL 9011
///
/// The two themes are a **true inversion**: the light page's ink is the dark page's
/// surface (both RAL 9011 "Graphite black"), so this is one decision rather than two,
/// and every token below is derived from `foreground`/`background` alone.
///
/// **Do not move the surfaces back toward the middle of the range.** They used to be
/// RAL 7047 (`#CFD0CF`, rel-lum 0.629) and RAL 7046 (`#82898E`, rel-lum 0.246) — both
/// mid-greys, only 2.29:1 apart. Contrast is a ratio, so from the middle you can go up
/// or down only a little: an exhaustive sRGB sweep found **zero** colours with usable
/// chroma clearing 3:1 on the old dark plot surface, against 96,589 on the light one,
/// and RAL 7046 against `#888888` measured **1.00:1** — a perfect luminance collision
/// that made grey text on the dark page mathematically invisible. On RAL 9010
/// (`#F7F9EF`, rel-lum 0.938) and RAL 9011 (`#1C1C1C`, rel-lum 0.012) the surfaces are
/// **16.03:1** apart and **58** RAL colours clear 3:1 on *both*, so a single palette
/// can finally serve both themes.
///
/// The price is real and is why some tokens look unambitious: because the surfaces are
/// 16.03:1 apart, no colour whatsoever clears 4.5:1 on both — the ceiling for any single
/// value is `sqrt(16.03) = 4.00:1`, attained only at rel-lum 0.197 (0 of 202 RAL entries
/// clear 4.5 on both). So all body text is `foreground`, and the hyperlink leans on its
/// underline for text-grade relief rather than on a second, per-theme blue.
pub fn industrial(
    foreground: Color32,
    background: Color32,
    surface: Color32,
    accent: Color32,
    mut base_visuals: Visuals,
) -> Visuals {
    // Table-zebra wash. Was `blend(surface, background, 0.2)`, which is a no-op now that
    // panels are the page (1.00:1, a dead stripe); re-derived from the ink instead so it
    // is a real, equal-weight stripe at both poles: 1.08 light / 1.11 dark.
    let faint_fill = blend(background, foreground, 0.04);
    // Hairlines are deliberately *lighter* than weak text (3.78/5.70 vs 4.83/7.03): a
    // rule should not outweigh the text it separates. This reverses the old ordering.
    let border = blend(foreground, background, 0.45);
    let weak_text = blend(foreground, background, 0.37);
    let control_radius = 2.0;
    let container_radius = 0.0;

    let control_fill = background;
    let control_fill_hover = blend(background, foreground, 0.08);
    // "Active" means *further from the page*, not *darker* — the only definition that
    // survives the inversion. hover:active is 1.18 light / 1.29 dark; the old
    // `blend(hover, ral(9011), 0.12)` collapsed to 1.01 on graphite.
    let control_fill_active = blend(background, foreground, 0.16);
    let selection_fill = blend(background, accent, 0.18);
    // RAL 5005 fails the graphite pole at 1.88:1. RAL 5012 is the same blue lineage one
    // lightness step up and sits at the invariant point: 3.97 light / 4.04 dark, i.e.
    // 99.1% / 100.9% of the 4.00:1 ceiling. Underlines are therefore not optional.
    let link = ral(5012);
    // t = 0.50 is the fixed point of the inversion: one hex (#8A8B86) at both poles.
    let popup_shadow_color = blend(background, foreground, 0.50);

    base_visuals.window_fill = background;
    base_visuals.panel_fill = background;
    base_visuals.override_text_color = None;
    base_visuals.weak_text_alpha = 1.0;
    base_visuals.weak_text_color = Some(weak_text);
    base_visuals.disabled_alpha = 1.0;
    base_visuals.faint_bg_color = faint_fill;
    // The plot/well surface is the page itself, not the hover nudge. Un-aliasing it is
    // what the pole move actually buys the plots: 58 of 202 RAL colours clear 3:1 on it
    // in *both* themes, against 37 on the old mid-grey pair and only 19 if it stayed
    // pinned to `control_fill_hover`. Cost: `egui::TextEdit` loses its tinted well and
    // is carried by its 1px border (3.78 light / 5.70 dark); GORBIE's own TextField and
    // NumberField do not go through this path.
    base_visuals.extreme_bg_color = background;
    // egui's own defaults leak through `Visuals::light()`/`dark()` for these three, and
    // the leaked values do not hold on the poles (`#404040` code wells at 1.64 on
    // graphite; `#FF6400` warnings at 2.79 on bone). Status colour is never the sole
    // carrier of meaning — pair `error_fg_color` with an icon and a label.
    base_visuals.code_bg_color = blend(background, foreground, 0.06);
    base_visuals.warn_fg_color = ral(2003);
    base_visuals.error_fg_color = ral(3014);
    base_visuals.slider_trailing_fill = true;
    base_visuals.selection = Selection {
        bg_fill: selection_fill,
        stroke: Stroke::new(2.0, accent),
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

    // Bone page, graphite ink. See `industrial` for why the poles are where they are.
    let foreground = ral(9011);
    let background = ral(9010);
    // Panels are the page; plane separation is carried by the hairline rule, which is
    // consistent with `window_shadow = Shadow::NONE` below.
    let surface = background;
    let accent = ral(2005);

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

    // The exact inversion of `industrial_light`: the light page's ink is this page's
    // surface. Ink was RAL 9003, which measured only 3.31:1 on the old mid-grey page.
    let foreground = ral(9010);
    let background = ral(9011);
    let surface = background;
    let accent = ral(2005);

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
/// | Heading   |    39px   |   48px     |   4     |
/// | Body      |    20px   |   24px     |   2     |
/// | Button    |    20px   |   24px     |   2     |
/// | Monospace |    19px   |   24px     |   2     |
/// | Small     |    13px   |   18px     |   1.5   |
///
/// The scale moved up by 4/3 (body 15 -> 20) so that a full-width, 12-column
/// measure lands near the readable band. IosevkaGorbie is a 0.5em advance, so
/// at 15px a 744px measure ran to ~99 characters per line; at 20px it is ~74.
/// The ratios are unchanged — two body lines still equal one heading line — and
/// body now sits on a whole 2-module row rather than a half-multiple, which
/// squares the vertical rhythm instead of splitting it.
pub fn industrial_text_styles() -> Vec<(TextStyle, FontId)> {
    vec![
        (
            TextStyle::Heading,
            FontId::new(39.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Body,
            FontId::new(20.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Name("LCD".into()),
            FontId::new(20.0, FontFamily::Name("LCD".into())),
        ),
        (
            TextStyle::Monospace,
            FontId::new(19.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Button,
            FontId::new(20.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
        (
            TextStyle::Small,
            FontId::new(13.0, FontFamily::Name("IosevkaGorbie".into())),
        ),
    ]
}
