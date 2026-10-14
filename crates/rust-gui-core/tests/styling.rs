use rust_gui_core::{
    Action, Context, Event, FontFamily, Length, MonospaceMeasurer, Paint, Style, StyleSheet,
    StyleSheetError, Theme, Ui,
};
use rust_gui_core::{Button, Label};
use rust_gui_scene::{Color, Point, Primitive, Size};

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> rust_gui_core::FrameStats {
    context.run(Size::new(500.0, 400.0), &mut MonospaceMeasurer, view)
}

fn quads(context: &Context) -> Vec<Color> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Quad { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

fn text_colors(context: &Context) -> Vec<Color> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text { color, .. } | Primitive::StyledText { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

fn page(ui: &mut Ui<'_>, size: Option<f32>, family: Option<FontFamily>) {
    ui.keyed("page", |ui| {
        ui.column(
            Style {
                font_size: size,
                font_family: family,
                foreground: Some(Color::rgb(200, 10, 10)),
                ..Style::default()
            },
            |ui| {
                ui.keyed("inherits", |ui| ui.label("inherits"));
                ui.keyed("overrides", |ui| {
                    ui.label(Label::new("overrides").style(Style {
                        font_size: Some(30.0),
                        foreground: Some(Color::rgb(10, 200, 10)),
                        ..Style::default()
                    }))
                });
                ui.keyed("nested", |ui| {
                    ui.row(Style::default(), |ui| {
                        ui.keyed("deep", |ui| ui.label("deep"));
                    })
                });
            },
        )
    });
}

#[test]
fn text_properties_inherit_from_ancestors_then_the_theme() {
    let mut context = Context::new();
    frame(&mut context, |ui| page(ui, Some(20.0), None));
    let details = |context: &Context, path: &[&str]| {
        context
            .details(context.find(path).unwrap())
            .unwrap()
            .resolved
    };
    let inherits = details(&context, &["page", "inherits"]);
    assert_eq!(inherits.font_size, 20.0);
    assert_eq!(inherits.foreground, Color::rgb(200, 10, 10));
    assert_eq!(inherits.font_family, Theme::dark().font_family);
    let overrides = details(&context, &["page", "overrides"]);
    assert_eq!(overrides.font_size, 30.0);
    assert_eq!(overrides.foreground, Color::rgb(10, 200, 10));
    assert_eq!(
        details(&context, &["page", "nested", "deep"]).font_size,
        20.0
    );
    // Changing the parent font re-measures the inheriting labels only.
    let stats = frame(&mut context, |ui| page(ui, Some(24.0), None));
    assert_eq!(stats.text_measures, 2);
    assert_eq!(stats.layout_passes, 1);
    assert_eq!(
        details(&context, &["page", "nested", "deep"]).font_size,
        24.0
    );
    assert_eq!(details(&context, &["page", "overrides"]).font_size, 30.0);
    // A family change is a text change too.
    let stats = frame(&mut context, |ui| {
        page(ui, Some(24.0), Some(FontFamily::Monospace))
    });
    assert!(stats.text_measures >= 2);
    assert_eq!(
        details(&context, &["page", "inherits"]).font_family,
        FontFamily::Monospace
    );
    // Unchanged declaration: no work.
    let stats = frame(&mut context, |ui| {
        page(ui, Some(24.0), Some(FontFamily::Monospace))
    });
    assert_eq!((stats.text_measures, stats.layout_passes), (0, 0));
    assert!(!stats.scene_rebuilt);
}

#[test]
fn theme_switch_repaints_without_layout_and_controls_follow_tokens() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("save", |ui| ui.button("Save"));
                ui.keyed("note", |ui| ui.label("note"));
                ui.enabled(false, |ui| {
                    ui.keyed("off", |ui| ui.button("Off"));
                });
            })
        });
    };
    frame(&mut context, view);
    let dark = Theme::dark();
    assert!(quads(&context).contains(&dark.control));
    assert!(quads(&context).contains(&dark.control_disabled));
    assert_eq!(text_colors(&context)[1], dark.text);
    assert_eq!(text_colors(&context)[2], dark.text_disabled);
    context.set_theme(Theme::light());
    let stats = frame(&mut context, view);
    let light = Theme::light();
    assert!(stats.scene_rebuilt);
    assert_eq!(stats.layout_passes, 0, "same fonts: no layout");
    assert_eq!(stats.text_measures, 0);
    assert!(quads(&context).contains(&light.control));
    assert!(!quads(&context).contains(&dark.control));
    assert_eq!(text_colors(&context)[1], light.text);
    assert_eq!(context.theme().name, "light");
    // Hover and focus use the theme's state tokens.
    let save = context.find(&["page", "save"]).unwrap();
    let bounds = context.node(save).unwrap().bounds;
    context.event(Event::PointerMoved(Point {
        x: bounds.x + 2.0,
        y: bounds.y + 2.0,
    }));
    frame(&mut context, view);
    assert!(quads(&context).contains(&light.control_hover));
    context.action(save, Action::Focus);
    frame(&mut context, view);
    assert!(quads(&context).contains(&light.focus));
    // A theme with a different font size relayouts once.
    context.set_theme(Theme {
        font_size: 20.0,
        ..Theme::light()
    });
    let stats = frame(&mut context, view);
    assert_eq!(stats.layout_passes, 1);
    assert!(stats.text_measures > 0);
    // Setting the same theme again is a no-op.
    context.set_theme(Theme {
        font_size: 20.0,
        ..Theme::light()
    });
    assert!(!frame(&mut context, view).scene_rebuilt);
}

#[test]
fn state_overrides_apply_per_interaction_state_and_stay_repaint_only() {
    let mut context = Context::new();
    let hover = Color::rgb(1, 2, 3);
    let pressed = Color::rgb(4, 5, 6);
    let focused = Color::rgb(7, 8, 9);
    let disabled = Color::rgb(10, 11, 12);
    let view = move |ui: &mut Ui<'_>| {
        let style = Style::button()
            .background(Color::rgb(90, 90, 90))
            .on_hover(Paint::background(hover))
            .on_pressed(Paint::background(pressed))
            .on_focused(Paint {
                foreground: Some(focused),
                radius: Some(0.0),
                ..Paint::default()
            })
            .on_disabled(Paint::background(disabled));
        ui.keyed("bar", |ui| {
            ui.row(Style::default(), |ui| {
                ui.keyed("go", |ui| {
                    ui.button(Button::new("Go").enabled(true).style(style.clone()))
                });
                ui.keyed("stop", |ui| {
                    ui.button(Button::new("Stop").enabled(false).style(style))
                });
            })
        });
    };
    frame(&mut context, view);
    assert!(quads(&context).contains(&Color::rgb(90, 90, 90)));
    assert!(quads(&context).contains(&disabled));
    let go = context.find(&["bar", "go"]).unwrap();
    let bounds = context.node(go).unwrap().bounds;
    let inside = Point {
        x: bounds.x + 3.0,
        y: bounds.y + 3.0,
    };
    context.event(Event::PointerMoved(inside));
    let stats = frame(&mut context, view);
    assert_eq!(stats.layout_passes, 0);
    assert!(quads(&context).contains(&hover));
    context.event(Event::PointerDown);
    frame(&mut context, view);
    assert!(quads(&context).contains(&pressed));
    assert!(!quads(&context).contains(&hover), "pressed wins over hover");
    context.event(Event::PointerUp);
    frame(&mut context, view);
    // The click focused the button: focused text color and radius override.
    assert!(text_colors(&context).contains(&focused));
    assert!(quads(&context).contains(&Theme::dark().focus));
    context.event(Event::PointerLeft);
    frame(&mut context, view);
    assert!(!quads(&context).contains(&hover));
}

#[test]
fn stylesheets_parse_themes_tokens_classes_and_report_errors_with_lines() {
    let sheet = StyleSheet::parse(
        r#"
        // comment
        theme light
        token accent #ff00ff
        token focus #00ff0080

        class primary {
            background: #1a9c8f; foreground: #ffffff
            padding: 12
            radius: 8
            font-size: 16
            font-weight: 700
            font-family: monospace
            italic: true
            width: 50%
            height: auto
            wrap: word
            hover.background: #22b0a0
            disabled.foreground: #808080
        }
        class danger extends primary { background: #c0392b; font-family: "Segoe UI" }
        "#,
    )
    .unwrap();
    assert_eq!(sheet.theme.as_ref().unwrap().name, "light");
    assert_eq!(sheet.tokens.len(), 2);
    let theme = sheet.apply_to(&Theme::dark());
    assert_eq!(theme.name, "light");
    assert_eq!(theme.accent, Color::rgb(255, 0, 255));
    assert_eq!(theme.focus, Color(0, 255, 0, 128));
    assert_eq!(theme.token("accent"), Some(Color::rgb(255, 0, 255)));
    let primary = sheet.class("primary").unwrap();
    assert_eq!(primary.background, Some(Color::rgb(26, 156, 143)));
    assert_eq!(primary.padding, 12.0);
    assert_eq!(primary.width, Length::Percent(0.5));
    assert_eq!(primary.font_family, Some(FontFamily::Monospace));
    assert_eq!(primary.italic, Some(true));
    assert_eq!(primary.wrap, rust_gui_core::TextWrap::Word);
    let states = primary.states.as_ref().unwrap();
    assert_eq!(states.hover.background, Some(Color::rgb(34, 176, 160)));
    assert_eq!(states.disabled.foreground, Some(Color::rgb(128, 128, 128)));
    let danger = sheet.class("danger").unwrap();
    assert_eq!(danger.background, Some(Color::rgb(192, 57, 43)));
    assert_eq!(danger.padding, 12.0, "extends copies the base");
    assert_eq!(
        danger.font_family,
        Some(FontFamily::Named("Segoe UI".into()))
    );
    assert_eq!(sheet.classes().count(), 2);

    let error = |text: &str| StyleSheet::parse(text).unwrap_err();
    assert_eq!(
        error("theme dark\ntoken nope #ffffff"),
        StyleSheetError {
            line: 2,
            message: "unknown token `nope`".into()
        }
    );
    // Tokens without a base theme overlay whichever theme is active.
    let overlay = StyleSheet::parse("token accent #ff00ff").unwrap();
    assert!(overlay.theme.is_none());
    assert_eq!(
        overlay.apply_to(&Theme::light()).accent,
        Color::rgb(255, 0, 255)
    );
    assert_eq!(overlay.apply_to(&Theme::light()).name, "light");
    assert!(
        error("class a {\n  colour: #fff\n}")
            .message
            .contains("unknown property")
    );
    assert!(
        error("class a {\n  background: red\n}")
            .message
            .contains("#rrggbb")
    );
    assert!(
        error("class a { font-weight: 950 }")
            .message
            .contains("100..=900")
    );
    assert!(
        error("class a extends missing { }")
            .message
            .contains("unknown base class")
    );
    assert!(
        error("class a {\n padding: 1")
            .message
            .contains("missing `}`")
    );
    assert!(
        error("class a { hover.padding: 3 }")
            .message
            .contains("cannot set")
    );
    assert!(error("bogus line").message.contains("unknown directive"));
}

#[test]
fn installed_classes_style_widgets_and_unknown_classes_warn_once() {
    let mut context = Context::new();
    let sheet = StyleSheet::parse(
        "theme light\ntoken control #123456\nclass primary { background: #ff0000; foreground: #00ff00 }",
    )
    .unwrap();
    context.set_stylesheet(sheet);
    assert_eq!(context.theme().control, Color::rgb(0x12, 0x34, 0x56));
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("bar", |ui| {
            ui.row(Style::default(), |ui| {
                let primary = ui.class("primary");
                ui.keyed("go", |ui| {
                    ui.button(Button::new("Go").enabled(true).style(primary))
                });
                let missing = ui.class("missing");
                ui.keyed("plain", |ui| {
                    ui.button(Button::new("Plain").enabled(true).style(missing.clone()))
                });
                ui.keyed("plain2", |ui| {
                    ui.button(Button::new("Plain").enabled(true).style(missing))
                });
            })
        });
    };
    frame(&mut context, view);
    assert!(quads(&context).contains(&Color::rgb(255, 0, 0)));
    assert!(text_colors(&context).contains(&Color::rgb(0, 255, 0)));
    assert!(
        quads(&context).contains(&Color::rgb(0x12, 0x34, 0x56)),
        "theme default control"
    );
    assert_eq!(context.style_warnings(), ["unknown style class `missing`"]);
    frame(&mut context, view);
    assert_eq!(
        context.style_warnings().len(),
        1,
        "warnings are deduplicated"
    );
    // Replacing the sheet re-styles on the next declaration and clears warnings.
    let mut sheet = StyleSheet::default();
    sheet.insert(
        "primary",
        Style::default().background(Color::rgb(0, 0, 255)),
    );
    sheet.insert("missing", Style::default());
    context.set_stylesheet(sheet);
    let stats = frame(&mut context, view);
    assert!(stats.scene_rebuilt);
    assert!(quads(&context).contains(&Color::rgb(0, 0, 255)));
    assert!(context.style_warnings().is_empty());
    assert_eq!(
        context.theme().name,
        "light",
        "a sheet without a theme keeps the current one"
    );
}

#[test]
fn views_switch_themes_and_sheet_tokens_survive_the_switch() {
    let mut context = Context::new();
    context.set_stylesheet(StyleSheet::parse("token accent #ff00ff").unwrap());
    assert_eq!(context.theme().accent, Color::rgb(255, 0, 255));
    let mut wanted = Some(Theme::light());
    let mut view = |ui: &mut Ui<'_>| {
        if let Some(theme) = wanted.take() {
            ui.set_theme(theme);
        }
        ui.keyed("name", |ui| ui.label(ui.theme().name.clone()));
    };
    let stats = frame(&mut context, &mut view);
    assert_eq!(
        stats.activations, 1,
        "hosts declare again after a theme request"
    );
    assert_eq!(context.theme().name, "light");
    assert_eq!(
        context.theme().accent,
        Color::rgb(255, 0, 255),
        "sheet tokens overlay"
    );
    frame(&mut context, &mut view);
    assert_eq!(
        context
            .details(context.find(&["name"]).unwrap())
            .unwrap()
            .text,
        "light"
    );
    assert_eq!(
        context
            .details(context.find(&["name"]).unwrap())
            .unwrap()
            .resolved
            .foreground,
        Theme::light().text
    );
}
