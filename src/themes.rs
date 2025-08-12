use egui::{
    style::{Selection, WidgetVisuals, Widgets},
    Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style, TextStyle, Visuals,
};

pub fn cosmic_gel_light() -> Style {
    let mut style = Style::default();

    style.text_styles = cosmic_gel_text_styles().into_iter().collect();

    // Design tokens
    let ink = Color32::from_hex("#35243E").unwrap(); // richer, slightly saturated ink
    let parchment = Color32::from_hex("#FBF6F1").unwrap(); // warm parchment
    let purple = Color32::from_hex("#6B5AE6").unwrap(); // accent

    let visuals = Visuals {
        dark_mode: false,
        window_fill: parchment,
        panel_fill: Color32::from_hex("#FCF7F0").unwrap(),
        // Explicit text color to ensure markdown headings/bold/italic use readable ink
        override_text_color: Some(ink),
        faint_bg_color: Color32::from_hex("#EFEAF6").unwrap(),
        // Visible separator color on parchment
        extreme_bg_color: Color32::from_hex("#CFC4D6").unwrap(),
        selection: Selection {
            bg_fill: purple,
            stroke: Stroke::new(2.0, purple),
        },
        hyperlink_color: purple,
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: Color32::from_hex("#FCF7F0").unwrap(),
                weak_bg_fill: Color32::from_hex("#F2EEF9").unwrap(),
                bg_stroke: Stroke::NONE,
                // Make sure icons and inline text use ink explicitly
                fg_stroke: Stroke::new(1.0, ink),
                corner_radius: 10.0.into(),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: Color32::from_hex("#EEEAF6").unwrap(),
                weak_bg_fill: Color32::from_hex("#E6E2F4").unwrap(),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, ink),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            hovered: WidgetVisuals {
                bg_fill: Color32::from_rgba_premultiplied(107, 90, 230, 40),
                weak_bg_fill: purple,
                bg_stroke: Stroke::NONE,
                // stronger ink on hover so highlights remain visible
                fg_stroke: Stroke::new(1.4, ink),
                corner_radius: 10.0.into(),
                expansion: 3.0,
            },
            active: WidgetVisuals {
                bg_fill: purple,
                weak_bg_fill: Color32::from_rgba_premultiplied(53, 201, 190, 80),
                bg_stroke: Stroke::NONE,
                // Temporarily use `ink` for active icons to test visibility
                fg_stroke: Stroke::new(1.5, ink),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            open: WidgetVisuals {
                bg_fill: Color32::from_hex("#FCF7F0").unwrap(),
                weak_bg_fill: Color32::from_hex("#F2EEF9").unwrap(),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, ink),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
        },
        window_shadow: egui::epaint::Shadow {
            offset: [0, 6],
            blur: 14,
            spread: 0,
            color: Color32::from_rgba_premultiplied(0, 0, 0, 40),
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

    let ink = Color32::from_hex("#35243E").unwrap();
    let parchment = Color32::from_hex("#FBF6F1").unwrap();
    let purple = Color32::from_hex("#8E86FF").unwrap();

    let visuals = Visuals {
        dark_mode: true,
        window_fill: ink,
        panel_fill: Color32::from_hex("#1B1821").unwrap(),
        override_text_color: Some(parchment),
        faint_bg_color: Color32::from_hex("#252231").unwrap(),
        // lighter separator than background so divider is visible in dark
        extreme_bg_color: Color32::from_rgba_premultiplied(251, 246, 241, 100),
        selection: Selection {
            bg_fill: purple,
            stroke: Stroke::new(2.0, purple),
        },
        hyperlink_color: purple,
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: Color32::from_hex("#1B1821").unwrap(),
                weak_bg_fill: Color32::from_hex("#23202A").unwrap(),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, parchment),
                corner_radius: 10.0.into(),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: Color32::from_hex("#23202A").unwrap(),
                weak_bg_fill: Color32::from_hex("#2B2734").unwrap(),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, parchment),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            hovered: WidgetVisuals {
                bg_fill: Color32::from_rgba_premultiplied(142, 134, 255, 38),
                weak_bg_fill: purple,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.4, parchment),
                corner_radius: 10.0.into(),
                expansion: 3.0,
            },
            active: WidgetVisuals {
                bg_fill: purple,
                weak_bg_fill: Color32::from_rgba_premultiplied(53, 201, 190, 64),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.5, parchment),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
            open: WidgetVisuals {
                bg_fill: Color32::from_hex("#1B1821").unwrap(),
                weak_bg_fill: Color32::from_hex("#23202A").unwrap(),
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, parchment),
                corner_radius: 10.0.into(),
                expansion: 2.0,
            },
        },
        window_shadow: egui::epaint::Shadow {
            offset: [0, 10],
            blur: 20,
            spread: 0,
            color: Color32::from_rgba_premultiplied(0, 0, 0, 220),
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
