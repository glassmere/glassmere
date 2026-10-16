mod support;

use rust_gui_core::Label;
use rust_gui_core::app::Action as Msg;
use rust_gui_core::{
    Align, Axis, CodeEditorOptions, Context, Direction, Event, Key, Length, MonospaceMeasurer,
    Role, SplitOptions, Style, StyleSheet, Tab, TextDocument, TextOp, TextWrap, Track, Ui,
};
use rust_gui_scene::{Point, Primitive, Rect, Size};
use support::{Driven, on};

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
    ui.keyed("row", |ui| {
        ui.row(
            Style::default().width(Length::Percent(1.0)).gap(10.0),
            |ui| {
                ui.keyed("a", |ui| ui.label(Label::new("a").style(boxed(100.0))));
                ui.keyed("b", |ui| ui.label(Label::new("b").style(boxed(50.0))));
            },
        )
    });
    ui.keyed("column", |ui| {
        ui.column(
            Style::default()
                .width(Length::Percent(1.0))
                .align_items(Align::Start),
            |ui| ui.keyed("start", |ui| ui.label(Label::new("s").style(boxed(80.0)))),
        )
    });
    ui.keyed("grid", |ui| {
        ui.grid(
            Style::default()
                .width(Length::Percent(1.0))
                .columns(vec![Track::Px(100.0), Track::Px(200.0)])
                .gap(0.0),
            |ui| {
                ui.keyed("one", |ui| {
                    ui.label(Label::new("1").style(Style::default().height(Length::Px(20.0))))
                });
                ui.keyed("two", |ui| {
                    ui.label(Label::new("2").style(Style::default().height(Length::Px(20.0))))
                });
            },
        )
    });
    // A subtree keeps its own direction.
    ui.keyed("ltr", |ui| {
        ui.row(
            Style::default()
                .width(Length::Percent(1.0))
                .direction(Direction::Ltr),
            |ui| ui.keyed("first", |ui| ui.label(Label::new("f").style(boxed(60.0)))),
        )
    });
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
        ui.keyed("single", |ui| {
            ui.label(Label::new("abc").style(Style::default().width(Length::Px(300.0))))
        });
        ui.keyed("wrapped", |ui| {
            ui.label(
                Label::new("several words that wrap").style(
                    Style::default()
                        .width(Length::Px(100.0))
                        .wrap(TextWrap::Word),
                ),
            )
        });
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

struct Panel {
    volume: f64,
    fraction: f32,
    selected: usize,
    choice: usize,
}

fn panel_view(state: &Panel, ui: &mut Ui<'_, Msg<Panel>>) {
    let tabs = [Tab::new("One"), Tab::new("Two"), Tab::new("Three")];
    ui.keyed("volume", |ui| {
        ui.slider(
            "Volume",
            state.volume,
            0.0..=100.0,
            1.0,
            on(|s: &mut Panel, volume: f64| s.volume = volume),
        );
    });
    ui.keyed("split", |ui| {
        ui.split(
            state.fraction,
            Axis::Horizontal,
            SplitOptions {
                style: Style::default()
                    .width(Length::Px(400.0))
                    .height(Length::Px(60.0)),
                min_first: 20.0,
                min_second: 20.0,
                ..SplitOptions::default()
            },
            on(|s: &mut Panel, fraction: f32| s.fraction = fraction),
            |ui| {
                ui.label("first");
            },
            |ui| {
                ui.label("second");
            },
        );
    });
    ui.keyed("tabs", |ui| {
        ui.tabs(
            state.selected,
            &tabs,
            on(|s: &mut Panel, index: usize| s.selected = index),
            |ui, index| {
                ui.label(format!("panel {index}"));
            },
        );
    });
    ui.keyed("combo", |ui| {
        ui.combo_box(
            "Size",
            state.choice,
            &["Small", "Large"],
            on(|s: &mut Panel, index: usize| s.choice = index),
        );
    });
}

#[test]
fn sliders_splits_tabs_and_popups_mirror_their_behaviour() {
    let mut app = Driven::new(
        ROOT,
        Panel {
            volume: 25.0,
            fraction: 0.25,
            selected: 0,
            choice: 0,
        },
        panel_view,
    );
    app.context.set_direction(Direction::Rtl);
    app.set_view(panel_view);
    app.frame();

    // Slider: the minimum is on the right, and Left increases.
    let slider = labelled(&app.context, Role::Slider, "Volume");
    app.context.event(Event::PointerMoved(Point {
        x: slider.x + slider.width - 13.0,
        y: slider.y + slider.height - 15.0,
    }));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerUp);
    app.frame();
    assert!(app.state().volume < 2.0, "{}", app.state().volume);
    let before = app.state().volume;
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert_eq!(app.state().volume, before + 1.0);

    // Split: the first pane is on the right; dragging the divider right shrinks it.
    let first = bounds(&app.context, &["split", "$first"]);
    let second = bounds(&app.context, &["split", "$second"]);
    assert!(first.x > second.x);
    let divider = bounds(&app.context, &["split", "$divider"]);
    app.context.event(Event::PointerMoved(Point {
        x: divider.x + 2.0,
        y: divider.y + 10.0,
    }));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerMoved(Point {
        x: divider.x + 42.0,
        y: divider.y + 10.0,
    }));
    app.context.event(Event::PointerUp);
    app.frame();
    assert!(app.state().fraction < 0.2, "{}", app.state().fraction);
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    let grown = app.state().fraction;
    assert!(grown > 0.15, "{grown}");

    // Tabs run right to left, and Left selects the next one.
    let one = labelled(&app.context, Role::Tab, "One");
    let two = labelled(&app.context, Role::Tab, "Two");
    assert!(one.x > two.x);
    app.context.event(Event::PointerMoved(Point {
        x: one.x + 5.0,
        y: one.y + 5.0,
    }));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerUp);
    app.frame();
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert_eq!(app.state().selected, 1);

    // A combo popup hangs from the right edge of its owner.
    let combo = labelled(&app.context, Role::ComboBox, "Size");
    app.context.event(Event::PointerMoved(Point {
        x: combo.x + 5.0,
        y: combo.y + 5.0,
    }));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerUp);
    app.frame();
    app.frame();
    let popup = bounds(&app.context, &["combo", "$popup"]);
    assert!((popup.x + popup.width - (combo.x + combo.width)).abs() < 0.5);
}

fn press(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

struct Form {
    name: String,
    document: TextDocument,
}

fn form_view(state: &Form, ui: &mut Ui<'_, Msg<Form>>) {
    ui.keyed("name", |ui| {
        ui.text_input(
            "Name",
            &state.name,
            on(|s: &mut Form, name: String| s.name = name),
        )
    });
    ui.keyed("code", |ui| {
        ui.code_editor(
            &state.document,
            CodeEditorOptions {
                line_numbers: false,
                style: Style::default()
                    .width(Length::Px(300.0))
                    .height(Length::Px(100.0))
                    .padding(8.0),
                ..CodeEditorOptions::default()
            },
            on(|s: &mut Form, op: TextOp| s.document.apply(op)),
        )
    });
}

#[test]
fn text_fields_align_right_and_the_code_editor_stays_left_to_right() {
    let mut app = Driven::new(
        ROOT,
        Form {
            name: String::from("abc"),
            document: TextDocument::from_text("let x = 1;\n"),
        },
        form_view,
    );
    app.context.set_direction(Direction::Rtl);
    app.set_view(form_view);
    app.frame();
    app.frame();
    let field = labelled(&app.context, Role::TextInput, "Name");
    // Clicking the right end of the field lands at the text's start or end,
    // inside the right-aligned text rather than in empty space on the left.
    app.context.event(Event::PointerMoved(Point {
        x: field.x + field.width - 12.0,
        y: field.y + 20.0,
    }));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerUp);
    app.frame();
    let caret = app.context.ime_cursor_rect().unwrap();
    assert!(
        caret.x > field.x + field.width - 20.0,
        "{caret:?} in {field:?}"
    );

    // The code editor sits at the right of the column but its text starts at
    // its own left edge.
    let editor = labelled(&app.context, Role::TextInput, "Editor");
    assert!((editor.x + editor.width - 600.0).abs() < 0.5);
    let line = app
        .context
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
