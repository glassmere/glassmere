mod support;

use glassmere_core::{
    Action, Context, Event, FontFamily, FrameStats, Length, MonospaceMeasurer, Paint, Style,
    StyleSheet, StyleSheetError, TextWrap, Theme, Track, Ui, WindowId, WindowRequest,
};
use glassmere_core::{Button, Label};
use glassmere_scene::{Color, Point, Primitive, Size};
use support::Driven;

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> FrameStats {
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
    assert_eq!(primary.wrap, TextWrap::Word);
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
    let mut app = Driven::new(Size::new(500.0, 400.0), (), |_, ui| {
        ui.keyed("name", |ui| ui.label(ui.theme().name.clone()));
    });
    app.context
        .set_stylesheet(StyleSheet::parse("token accent #ff00ff").unwrap());
    assert_eq!(app.context.theme().accent, Color::rgb(255, 0, 255));
    app.effects(|_, effects| effects.set_theme(Theme::light()));
    let stats = app.frame();
    assert!(stats.declared, "the view declares with the new theme");
    let context = &app.context;
    assert_eq!(context.theme().name, "light");
    assert_eq!(
        context.theme().accent,
        Color::rgb(255, 0, 255),
        "sheet tokens overlay"
    );
    app.frame();
    let context = &app.context;
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

/// What a window's batch asked for reaches every window, not only the one
/// whose batch ran the update.
#[test]
fn a_theme_asked_for_in_one_window_reaches_every_window() {
    let mut app = Driven::new(Size::new(500.0, 400.0), None::<WindowId>, |_, ui| {
        ui.keyed("name", |ui| ui.label(ui.theme().name.clone()));
    });
    app.set_window_view(|_, _, ui| {
        ui.keyed("switch", |ui| {
            ui.button(Button::new("Light").on_click(support::act_fx(|_, effects| {
                effects.set_theme(Theme::light());
            })))
        });
    });
    app.effects(|window, effects| {
        *window = Some(effects.open_window(WindowRequest::default()));
    });
    app.frame();
    let second = app.state().expect("the app opened a window");
    assert_ne!(app.window(second).context.theme().name, "light");

    let context = &mut app.window(second).context;
    let button = context
        .node(context.find(&["switch"]).unwrap())
        .unwrap()
        .bounds;
    let at = Point {
        x: button.x + button.width / 2.0,
        y: button.y + button.height / 2.0,
    };
    context.event(Event::PointerMoved(at));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    app.frame();
    // The first window applies it in its own next batch, which the host wakes.
    app.frame();
    assert_eq!(app.window(second).context.theme().name, "light");
    assert_eq!(app.context.theme().name, "light");
}

fn error_of(text: &str) -> StyleSheetError {
    StyleSheet::parse(text).unwrap_err()
}

#[test]
fn variables_are_declared_once_and_stand_in_for_any_value() {
    let sheet = StyleSheet::parse(
        r#"
        --brand: #1a9c8f   // a comment after the value
        --pad: 12; --corner: 8
        --font: "Segoe UI"
        --glow: #00000080
        --cols: repeat(2, 1fr)
        token accent var(--brand)
        class primary {
            background: var(--brand)
            padding: var(--pad); radius: var(--corner)
            font-family: var(--font)
            shadow: 0 4 12 var(--glow)
            columns: var(--cols) 50
            hover.background: var(--brand)
            pressed.radius: var(--corner)
            transition: 150ms ease-out
        }
        class quoted { font-family: "var(--font)" }
        "#,
    )
    .unwrap();
    assert_eq!(
        sheet.apply_to(&Theme::dark()).accent,
        Color::rgb(26, 156, 143)
    );
    let primary = sheet.class("primary").unwrap();
    assert_eq!(primary.background, Some(Color::rgb(26, 156, 143)));
    assert_eq!(primary.padding, 12.0);
    assert_eq!(primary.radius, 8.0);
    assert_eq!(
        primary.font_family,
        Some(FontFamily::Named("Segoe UI".into()))
    );
    assert_eq!(primary.shadow.as_ref().unwrap().color, Color(0, 0, 0, 128));
    assert_eq!(
        primary.columns,
        vec![Track::Repeat(2, vec![Track::Fr(1.0)]), Track::Px(50.0)]
    );
    let states = primary.states.as_ref().unwrap();
    assert_eq!(states.hover.background, Some(Color::rgb(26, 156, 143)));
    assert_eq!(states.pressed.radius, Some(8.0));
    // Text inside quotes is text, not a reference.
    assert_eq!(
        sheet.class("quoted").unwrap().font_family,
        Some(FontFamily::Named("var(--font)".into()))
    );
    // A declaration nobody uses changes nothing.
    assert_eq!(
        StyleSheet::parse("class a { padding: 3 }").unwrap(),
        StyleSheet::parse("--unused: 1\nclass a { padding: 3 }").unwrap()
    );
}

#[test]
fn a_variable_may_refer_to_earlier_variables_and_a_later_one_replaces_it() {
    let sheet = StyleSheet::parse(
        "--base: #112233\n--alias: var(--base)\n--again: var(--alias)\n\
         class one { background: var(--again) }\n\
         --base: #445566\n\
         class two { background: var(--alias); foreground: var(--base) }\n\
         --alias: var(--base)\n\
         class three { background: var(--alias) }",
    )
    .unwrap();
    let background = |name: &str| sheet.class(name).unwrap().background;
    assert_eq!(background("one"), Some(Color::rgb(0x11, 0x22, 0x33)));
    // `--alias` kept the value it had when it was declared.
    assert_eq!(background("two"), Some(Color::rgb(0x11, 0x22, 0x33)));
    assert_eq!(
        sheet.class("two").unwrap().foreground,
        Some(Color::rgb(0x44, 0x55, 0x66))
    );
    assert_eq!(background("three"), Some(Color::rgb(0x44, 0x55, 0x66)));
}

#[test]
fn a_fallback_stands_in_for_a_variable_that_is_not_declared() {
    let sheet = StyleSheet::parse(
        "--set: #112233\n\
         class a {\n\
             background: var(--missing, #ff0000)\n\
             foreground: var(--set, #ff0000)\n\
             padding: var(--nope, var(--set, 7))\n\
             columns: var(--none, repeat(2, 1fr))\n\
             radius: var( --missing , 5 )\n\
         }",
    );
    // `--set` is a colour, so `padding` cannot take it: the error is at its line.
    assert_eq!(
        sheet.unwrap_err(),
        StyleSheetError {
            line: 5,
            message: "invalid number `#112233`".into()
        }
    );
    let sheet = StyleSheet::parse(
        "--set: #112233\n\
         class a {\n\
             background: var(--missing, #ff0000)\n\
             foreground: var(--set, #ff0000)\n\
             padding: var(--nope, var(--missing, 7))\n\
             columns: var(--none, repeat(2, 1fr))\n\
             radius: var( --missing , 5 )\n\
         }",
    )
    .unwrap();
    let a = sheet.class("a").unwrap();
    assert_eq!(a.background, Some(Color::rgb(255, 0, 0)));
    assert_eq!(a.foreground, Some(Color::rgb(0x11, 0x22, 0x33)));
    assert_eq!(a.padding, 7.0);
    assert_eq!(a.radius, 5.0);
    assert_eq!(a.columns, vec![Track::Repeat(2, vec![Track::Fr(1.0)])]);
    // The fallback is only read when it is needed.
    StyleSheet::parse("--a: 1\nclass b { padding: var(--a, var(--never)) }").unwrap();
    assert!(
        error_of("class b { padding: var(--a, var(--never)) }")
            .message
            .contains("unknown variable `--never`")
    );
}

#[test]
fn variable_errors_name_the_line_they_were_found_on() {
    assert_eq!(
        error_of("class a {\n  padding: 1\n  background: var(--accent)\n}"),
        StyleSheetError {
            line: 3,
            message: "unknown variable `--accent`; variables must be declared before use".into()
        }
    );
    // Variables are resolved in order, so a later declaration is too late.
    let late = error_of("--a: var(--b)\n--b: 1");
    assert_eq!(late.line, 1);
    assert!(late.message.contains("unknown variable `--b`"));
    assert_eq!(
        error_of("token accent var(--brand)").message,
        "unknown variable `--brand`; variables must be declared before use"
    );
    // A variable cannot be its own value, with or without a fallback.
    for text in ["--a: var(--a)", "--a: 1\n--a: var(--a)", "--a: var(--a, 2)"] {
        let error = error_of(&format!("\n{text}"));
        assert_eq!(error.line, text.lines().count() + 1, "{text}");
        assert_eq!(error.message, "variable `--a` refers to itself", "{text}");
    }
    // A value is read where it is used, so a bad one is reported there.
    assert_eq!(
        error_of("--size: wide\n\nclass a {\n  padding: 1\n  gap: var(--size)\n}"),
        StyleSheetError {
            line: 5,
            message: "invalid number `wide`".into()
        }
    );
    assert_eq!(
        error_of("--c: red\nclass a { background: var(--c) }").message,
        "color `red` must be #rrggbb or #rrggbbaa"
    );
    // Malformed declarations and calls.
    assert_eq!(
        error_of("class x {}\n--a:").message,
        "variable `--a` needs a value"
    );
    assert_eq!(error_of("\n--a 1").message, "expected `--name: value`");
    assert_eq!(error_of("--a: 1; b: 2").message, "expected `--name: value`");
    assert_eq!(
        error_of("class a { padding: var(brand) }").message,
        "var() needs a variable name like `--accent`, got `brand`"
    );
    assert!(
        error_of("--a: 1\nclass a { padding: var(--a }")
            .message
            .contains("unclosed `var(`")
    );
    assert_eq!(
        error_of("class a {\n  --a: 1\n}"),
        StyleSheetError {
            line: 2,
            message: "variables are declared at the top level of the sheet, not inside class `a`"
                .into()
        }
    );
}

#[test]
fn nested_rules_flatten_into_states_and_extending_classes() {
    let sheet = StyleSheet::parse(
        r#"
        --fill: #112233
        class card {
            background: var(--fill)
            padding: 10
            hover.radius: 3
            &:hover { background: #445566 }
            &:pressed {
                background: #778899
                opacity: 0.5
            }
            &.selected {
                background: #ff0000
                &:hover { background: #ee0000 }
                &.large { padding: 20 }
            }
            padding: 14
        }
        "#,
    )
    .unwrap();
    // The states are on `card`; only `&.name` makes a class of its own.
    assert_eq!(sheet.classes().count(), 3);
    let card = sheet.class("card").unwrap();
    assert_eq!(card.background, Some(Color::rgb(0x11, 0x22, 0x33)));
    assert_eq!(card.padding, 14.0, "a line after a nested rule counts");
    let states = card.states.as_ref().unwrap();
    assert_eq!(states.hover.background, Some(Color::rgb(0x44, 0x55, 0x66)));
    assert_eq!(states.hover.radius, Some(3.0));
    assert_eq!(
        states.pressed.background,
        Some(Color::rgb(0x77, 0x88, 0x99))
    );
    assert_eq!(states.pressed.opacity, Some(0.5));
    // `&.selected` is the class `card.selected`: `card` plus its own lines.
    let selected = sheet.class("card.selected").unwrap();
    assert_eq!(selected.background, Some(Color::rgb(255, 0, 0)));
    assert_eq!(selected.padding, 14.0);
    let states = selected.states.as_ref().unwrap();
    assert_eq!(states.hover.background, Some(Color::rgb(0xee, 0, 0)));
    assert_eq!(states.hover.radius, Some(3.0));
    assert_eq!(states.pressed.opacity, Some(0.5));
    let large = sheet.class("card.selected.large").unwrap();
    assert_eq!(large.padding, 20.0);
    assert_eq!(large.background, Some(Color::rgb(255, 0, 0)));
    assert_eq!(
        large.states.as_ref().unwrap().hover.background,
        Some(Color::rgb(0xee, 0, 0))
    );
    // Nesting is only a way of writing what the flat form already says.
    let flat = StyleSheet::parse(
        "class card {
            background: #112233; padding: 14; hover.radius: 3
            hover.background: #445566
            pressed.background: #778899; pressed.opacity: 0.5
        }
        class card.selected extends card { background: #ff0000; hover.background: #ee0000 }
        class card.selected.large extends card.selected { padding: 20 }",
    )
    .unwrap();
    assert_eq!(sheet, flat);
}

#[test]
fn nested_rules_keep_the_order_the_lines_were_written_in() {
    let hover = |text: &str| {
        let sheet = StyleSheet::parse(text).unwrap();
        let card = sheet.class("card").unwrap();
        card.states.as_ref().unwrap().hover.background
    };
    // The later line wins, nested or not.
    assert_eq!(
        hover("class card {\n hover.background: #111111\n &:hover { background: #222222 }\n}"),
        Some(Color::rgb(0x22, 0x22, 0x22))
    );
    assert_eq!(
        hover("class card {\n &:hover { background: #222222 }\n hover.background: #333333\n}"),
        Some(Color::rgb(0x33, 0x33, 0x33))
    );
    assert_eq!(
        hover(
            "class card {\n &:hover { background: #222222 }\n &:hover { background: #444444 }\n}"
        ),
        Some(Color::rgb(0x44, 0x44, 0x44))
    );
    // A nested rule beats what the class it belongs to inherited.
    let sheet = StyleSheet::parse(
        "class base { background: #111111 }\n\
         class card extends base { &.on { background: #222222 } padding: 1 }",
    )
    .unwrap();
    assert_eq!(
        sheet.class("card.on").unwrap().background,
        Some(Color::rgb(0x22, 0x22, 0x22))
    );
    // `extends` copies the class and not the variants nested in it.
    let sheet =
        StyleSheet::parse("class a { &.b { padding: 2 } }\nclass c extends a { padding: 3 }")
            .unwrap();
    assert!(sheet.class("a.b").is_some());
    assert!(sheet.class("c.b").is_none());
}

#[test]
fn nested_rules_may_share_a_line_with_the_class_and_with_each_other() {
    let nested = StyleSheet::parse(
        "--c: #010203\n\
         class a { padding: 1; &:hover { background: var(--c) } &.b { padding: 2; &:focused { radius: 4 } } }\n\
         class d {\n  &.e {\n  padding: 5 } }\n\
         class f { &:hover {\n background: #040506\n} padding: 6 }\n\
         class g { &:hover { background: #070809 }; &.h { padding: 7 }\n }",
    )
    .unwrap();
    let a = nested.class("a").unwrap();
    assert_eq!(a.padding, 1.0);
    assert_eq!(
        a.states.as_ref().unwrap().hover.background,
        Some(Color::rgb(1, 2, 3))
    );
    let b = nested.class("a.b").unwrap();
    assert_eq!(b.padding, 2.0);
    assert_eq!(b.states.as_ref().unwrap().focused.radius, Some(4.0));
    assert_eq!(nested.class("d.e").unwrap().padding, 5.0);
    assert_eq!(nested.class("f").unwrap().padding, 6.0);
    assert_eq!(
        nested
            .class("f")
            .unwrap()
            .states
            .as_ref()
            .unwrap()
            .hover
            .background,
        Some(Color::rgb(4, 5, 6))
    );
    assert_eq!(nested.class("g.h").unwrap().padding, 7.0);
    // Braces inside quotes and `url(...)` are not rules.
    let plain = StyleSheet::parse(
        "class a { font-family: \"A {B}\" }\nclass b { background-image: url(x{1}.png) }",
    )
    .unwrap();
    assert_eq!(
        plain.class("a").unwrap().font_family,
        Some(FontFamily::Named("A {B}".into()))
    );
    assert!(plain.class("b").unwrap().background_image.is_some());
}

#[test]
fn nesting_without_an_ampersand_or_beyond_the_model_is_an_error() {
    let message = |text: &str| {
        let error = error_of(text);
        (error.line, error.message)
    };
    let unsupported = |header: &str| {
        format!(
            "nested rule `{header}` is not supported: use `&:state` or `&.name`; \
             there are no descendant selectors"
        )
    };
    for header in [
        ".title",
        "title",
        "class title",
        "& .title",
        "&>title",
        "&.a:hover",
        "&.",
        "&",
        "&hover",
    ] {
        assert_eq!(
            message(&format!(
                "class a {{\n  padding: 1\n  {header} {{ padding: 2 }}\n}}"
            )),
            (3, unsupported(header)),
            "{header}"
        );
    }
    assert_eq!(
        message("class a { .title { padding: 1 } }"),
        (1, unsupported(".title"))
    );
    assert_eq!(
        message("class a {\n  &:nope { padding: 1 }\n}"),
        (2, "unknown state `nope`".into())
    );
    assert_eq!(
        message("class a { &:hover:focused { opacity: 0.5 } }"),
        (1, "unknown state `hover:focused`".into())
    );
    assert_eq!(
        message("class a {\n  &:hover {\n    &:pressed { opacity: 0.5 }\n  }\n}"),
        (3, "`&:hover` cannot contain nested rules".into())
    );
    assert_eq!(
        message("class a {\n  &:hover {\n    &.b { opacity: 0.5 }\n  }\n}"),
        (3, "`&:hover` cannot contain nested rules".into())
    );
    // A state block can set what a `hover.` line can set, and no more.
    assert_eq!(
        message("class a {\n  &:hover {\n    padding: 3\n  }\n}"),
        (3, "state `hover` cannot set `padding`".into())
    );
    assert_eq!(
        message("class a {\n  &:hover {\n    hover.background: #fff\n  }\n}"),
        (3, "state `hover` cannot set `hover.background`".into())
    );
    // A variant reports a bad line where it is written.
    assert_eq!(
        message("class a {\n  &.b {\n    padding: 1\n    colour: #fff\n  }\n  padding: 2\n}"),
        (4, "unknown property `colour`".into())
    );
    assert_eq!(
        message("class a {\n  &.b {\n    padding: var(--nope)\n  }\n}").0,
        3
    );
    // Rules that never close, close twice or are followed by text.
    assert_eq!(
        message("class a {\n  &:hover {\n    opacity: 0.5\n  }"),
        (4, "class `a` is missing `}`".into())
    );
    assert_eq!(
        message("class a {\n  &.b {\n    padding: 1\n"),
        (3, "class `a` is missing `}`".into())
    );
    assert_eq!(
        message("class a { &:hover { opacity: 0.5 } } x"),
        (1, "unexpected text after `}`".into())
    );
    assert_eq!(
        message("class a {\n  padding: 1\n}\n}"),
        (4, "unknown directive `}`".into())
    );
    assert_eq!(
        message("&:hover { opacity: 0.5 }"),
        (1, "unknown directive `&:hover`".into())
    );
}

#[test]
fn a_reloaded_sheet_with_variables_and_nesting_replaces_the_installed_one() {
    let text = |accent: &str| {
        format!(
            "--accent: {accent}\ntoken accent var(--accent)\n\
             class go {{ background: var(--accent); &:hover {{ background: #00ff00 }} }}"
        )
    };
    let mut context = Context::new();
    context.set_stylesheet(StyleSheet::parse(&text("#ff0000")).unwrap());
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("go", |ui| {
            let go = ui.class("go");
            ui.button(Button::new("Go").enabled(true).style(go))
        });
    };
    frame(&mut context, view);
    assert!(quads(&context).contains(&Color::rgb(255, 0, 0)));
    assert_eq!(context.theme().accent, Color::rgb(255, 0, 0));
    // Parsing is a pure function of the text: reading it again changes nothing.
    assert_eq!(
        StyleSheet::parse(&text("#ff0000")).unwrap(),
        StyleSheet::parse(&text("#ff0000")).unwrap()
    );
    // The file changes on disk and the host installs what it parses.
    context.set_stylesheet(StyleSheet::parse(&text("#0000ff")).unwrap());
    let stats = frame(&mut context, view);
    assert!(stats.scene_rebuilt);
    assert!(quads(&context).contains(&Color::rgb(0, 0, 255)));
    assert_eq!(context.theme().accent, Color::rgb(0, 0, 255));
    // A reload that does not parse never reaches the context: the host
    // installs only a sheet that parsed, so the last good one stays.
    assert!(StyleSheet::parse(&format!("{}\nclass bad {{ &.x", text("#123456"))).is_err());
    assert_eq!(context.theme().accent, Color::rgb(0, 0, 255));
}
