use rust_gui_core::{
    Align, Axis, Context, Direction, Event, Key, Length, MonospaceMeasurer, Role, SplitOptions,
    Style, StyleSheet, Tab, TextWrap, Track, Ui,
};
use rust_gui_scene::{Point, Primitive, Rect, Size};

const ROOT: Size = Size::new(600.0, 400.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

fn labelled(context: &Context, role: Role, label: &str) -> Rect {
    let mut stack = vec![context.root()];
    while let Some(id) = stack.pop() {
        let node = context.node(id).unwrap();
        if node.semantics.role == role && node.semantics.label == label {
            return node.bounds;
        }
        stack.extend(node.children.iter().rev());
    }
    panic!("no {role:?} labelled {label}");
}

fn boxed(width: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(30.0))
}

fn layout(ui: &mut Ui<'_>) {
    ui.row(
        "row",
        Style::default().width(Length::Percent(1.0)).gap(10.0),
        |ui| {
            ui.label_styled("a", "a", boxed(100.0));
            ui.label_styled("b", "b", boxed(50.0));
        },
    );
    ui.column(
        "column",
        Style::default()
            .width(Length::Percent(1.0))
            .align_items(Align::Start),
        |ui| ui.label_styled("start", "s", boxed(80.0)),
    );
    ui.grid(
        "grid",
        Style::default()
            .width(Length::Percent(1.0))
            .columns(vec![Track::Px(100.0), Track::Px(200.0)])
            .gap(0.0),
        |ui| {
            ui.label_styled("one", "1", Style::default().height(Length::Px(20.0)));
            ui.label_styled("two", "2", Style::default().height(Length::Px(20.0)));
        },
    );
    // A subtree keeps its own direction.
    ui.row(
        "ltr",
        Style::default()
            .width(Length::Percent(1.0))
            .direction(Direction::Ltr),
        |ui| ui.label_styled("first", "f", boxed(60.0)),
    );
}

#[test]
fn rows_columns_and_grids_run_from_the_right_and_subtrees_can_opt_out() {
    let mut context = Context::new();
    frame(&mut context, layout);
    assert_eq!(bounds(&context, &["row", "a"]).x, 0.0);
    assert_eq!(bounds(&context, &["row", "b"]).x, 110.0);

    context.set_direction(Direction::Rtl);
    frame(&mut context, layout);
    assert_eq!(bounds(&context, &["row", "a"]).x, 500.0);
    assert_eq!(bounds(&context, &["row", "b"]).x, 440.0);
    assert_eq!(bounds(&context, &["column", "start"]).x, 520.0);
    assert_eq!(bounds(&context, &["grid", "one"]).x, 500.0);
    assert_eq!(bounds(&context, &["grid", "two"]).x, 300.0);
    assert_eq!(bounds(&context, &["ltr", "first"]).x, 0.0);

    // Back again at runtime.
    context.set_direction(Direction::Ltr);
    frame(&mut context, layout);
    assert_eq!(bounds(&context, &["row", "a"]).x, 0.0);

    // The stylesheet property does the same for a class.
    let sheet = StyleSheet::parse("class mirrored {\ndirection: rtl\n}").unwrap();
    assert_eq!(
        sheet.class("mirrored").unwrap().direction,
        Some(Direction::Rtl)
    );
    assert!(StyleSheet::parse("class bad {\ndirection: up\n}").is_err());
}

#[test]
fn text_aligns_to_the_right_in_stretched_and_wrapped_leaves() {
    let mut context = Context::new();
    context.set_direction(Direction::Rtl);
    let view = |ui: &mut Ui<'_>| {
        ui.label_styled("single", "abc", Style::default().width(Length::Px(300.0)));
        ui.label_styled(
            "wrapped",
            "several words that wrap",
            Style::default()
                .width(Length::Px(100.0))
                .wrap(TextWrap::Word),
        );
    };
    frame(&mut context, view);
    let advance = 16.0 * 0.6;
    let (mut single, mut wrapped_rtl) = (None, None);
    for primitive in &context.scene().primitives {
        match primitive {
            Primitive::Text { rect, text, .. } | Primitive::StyledText { rect, text, .. }
                if text == "abc" =>
            {
                single = Some(*rect);
            }
            Primitive::StyledText { text, style, .. } if text.starts_with("several") => {
                wrapped_rtl = Some(style.rtl);
            }
            _ => {}
        }
    }
    // The 300 px leaf is flush right in the 600 px column; its text ends there.
    let single = single.unwrap();
    assert!(
        (single.x - (600.0 - 3.0 * advance)).abs() < 0.01,
        "{single:?}"
    );
    assert_eq!(wrapped_rtl, Some(true));
}

#[test]
fn sliders_splits_tabs_and_popups_mirror_their_behaviour() {
    let mut context = Context::new();
    context.set_direction(Direction::Rtl);
    let mut value = 25.0;
    let mut fraction = 0.25f32;
    let mut selected = 0usize;
    let mut choice = 0usize;
    let tabs = [Tab::new("One"), Tab::new("Two"), Tab::new("Three")];
    let view = |context: &mut Context,
                value: &mut f64,
                fraction: &mut f32,
                selected: &mut usize,
                choice: &mut usize| {
        frame(context, |ui| {
            ui.slider("volume", "Volume", value, 0.0..=100.0, 1.0);
            ui.split(
                "split",
                fraction,
                Axis::Horizontal,
                SplitOptions {
                    style: Style::default()
                        .width(Length::Px(400.0))
                        .height(Length::Px(60.0)),
                    min_first: 20.0,
                    min_second: 20.0,
                    ..SplitOptions::default()
                },
                |ui| ui.label("first"),
                |ui| ui.label("second"),
            );
            ui.tabs("tabs", selected, &tabs, |ui, index| {
                ui.label(format!("panel {index}"));
            });
            ui.combo_box("combo", "Size", choice, &["Small", "Large"]);
        });
    };
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );

    // Slider: the minimum is on the right, and Left increases.
    let slider = labelled(&context, Role::Slider, "Volume");
    context.event(Event::PointerMoved(Point {
        x: slider.x + slider.width - 13.0,
        y: slider.y + slider.height - 15.0,
    }));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    assert!(value < 2.0, "{value}");
    let press = |context: &mut Context, key: Key| {
        context.event(Event::KeyDown {
            key,
            shift: false,
            repeat: false,
        });
        context.event(Event::KeyUp(key));
    };
    let before = value;
    press(&mut context, Key::ArrowLeft);
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    assert_eq!(value, before + 1.0);

    // Split: the first pane is on the right; dragging the divider right shrinks it.
    let first = bounds(&context, &["split", "$first"]);
    let second = bounds(&context, &["split", "$second"]);
    assert!(first.x > second.x);
    let divider = bounds(&context, &["split", "$divider"]);
    context.event(Event::PointerMoved(Point {
        x: divider.x + 2.0,
        y: divider.y + 10.0,
    }));
    context.event(Event::PointerDown);
    context.event(Event::PointerMoved(Point {
        x: divider.x + 42.0,
        y: divider.y + 10.0,
    }));
    context.event(Event::PointerUp);
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    assert!(fraction < 0.2, "{fraction}");
    press(&mut context, Key::ArrowLeft);
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    let grown = fraction;
    assert!(grown > 0.15, "{grown}");

    // Tabs run right to left, and Left selects the next one.
    let one = labelled(&context, Role::Tab, "One");
    let two = labelled(&context, Role::Tab, "Two");
    assert!(one.x > two.x);
    context.event(Event::PointerMoved(Point {
        x: one.x + 5.0,
        y: one.y + 5.0,
    }));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    press(&mut context, Key::ArrowLeft);
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    assert_eq!(selected, 1);

    // A combo popup hangs from the right edge of its owner.
    let combo = labelled(&context, Role::ComboBox, "Size");
    context.event(Event::PointerMoved(Point {
        x: combo.x + 5.0,
        y: combo.y + 5.0,
    }));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    view(
        &mut context,
        &mut value,
        &mut fraction,
        &mut selected,
        &mut choice,
    );
    let popup = bounds(&context, &["combo", "$popup"]);
    assert!((popup.x + popup.width - (combo.x + combo.width)).abs() < 0.5);
}

#[test]
fn text_fields_align_right_and_the_code_editor_stays_left_to_right() {
    let mut context = Context::new();
    context.set_direction(Direction::Rtl);
    let mut name = String::from("abc");
    let mut document = rust_gui_core::TextDocument::from_text("let x = 1;\n");
    let view =
        |context: &mut Context, name: &mut String, document: &mut rust_gui_core::TextDocument| {
            frame(context, |ui| {
                ui.text_input("name", "Name", name);
                ui.code_editor(
                    "code",
                    document,
                    rust_gui_core::CodeEditorOptions {
                        line_numbers: false,
                        style: Style::default()
                            .width(Length::Px(300.0))
                            .height(Length::Px(100.0))
                            .padding(8.0),
                        ..rust_gui_core::CodeEditorOptions::default()
                    },
                );
            });
        };
    view(&mut context, &mut name, &mut document);
    view(&mut context, &mut name, &mut document);
    let field = labelled(&context, Role::TextInput, "Name");
    // Clicking the right end of the field lands at the text's start or end,
    // inside the right-aligned text rather than in empty space on the left.
    context.event(Event::PointerMoved(Point {
        x: field.x + field.width - 12.0,
        y: field.y + 20.0,
    }));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    view(&mut context, &mut name, &mut document);
    let caret = context.ime_cursor_rect().unwrap();
    assert!(
        caret.x > field.x + field.width - 20.0,
        "{caret:?} in {field:?}"
    );

    // The code editor sits at the right of the column but its text starts at
    // its own left edge.
    let editor = labelled(&context, Role::TextInput, "Editor");
    assert!((editor.x + editor.width - 600.0).abs() < 0.5);
    let line = context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::RichText { rect, style, .. } => Some((*rect, style.rtl)),
            _ => None,
        })
        .unwrap();
    assert!((line.0.x - (editor.x + 8.0)).abs() < 0.5 && !line.1);
}
