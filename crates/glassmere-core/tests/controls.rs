mod support;

use glassmere_core::Label;
use glassmere_core::app::Action as Msg;
use glassmere_core::{
    Action, Context, Event, FontFamily, Key, Length, MenuItem, MonospaceMeasurer, Style, TextEdit,
    TextInput, TextMove, TextWrap, Ui,
};
use glassmere_scene::{Point, Primitive, Size};
use support::{Driven, act, on};

const ROOT: Size = Size::new(500.0, 500.0);

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

struct Input {
    value: String,
    enabled: bool,
    submits: u32,
}

impl Input {
    fn new(value: &str) -> Self {
        Self {
            value: value.into(),
            enabled: true,
            submits: 0,
        }
    }
}

fn input_view(state: &Input, ui: &mut Ui<'_, Msg<Input>>) {
    ui.enabled(state.enabled, |ui| {
        ui.keyed("input", |ui| {
            ui.text_input(
                TextInput::new("Name").on_submit(act(|s: &mut Input| s.submits += 1)),
                &state.value,
                on(|s: &mut Input, text: String| s.value = text),
            );
        });
    });
}

fn input(value: &str) -> Driven<Input> {
    Driven::new(ROOT, Input::new(value), input_view)
}

#[test]
fn input_binding_selection_graphemes_undo_and_submit_are_delivered_once() {
    let mut app = input("a👩‍💻e\u{301}");
    let id = app.context.find(&["input"]).unwrap();
    app.context.action(id, Action::Focus);
    key(&mut app.context, Key::Backspace);
    key(&mut app.context, Key::Backspace);
    assert!(app.frame().messages > 0);
    assert_eq!(app.state().value, "a");
    assert_eq!(app.frame().messages, 0);
    app.context.event(Event::Text(TextEdit::Undo));
    app.frame();
    assert_eq!(app.state().value, "a👩‍💻");
    app.context.event(Event::Text(TextEdit::SelectAll));
    assert_eq!(app.context.selected_text(), Some("a👩‍💻"));
    app.context
        .event(Event::Text(TextEdit::Insert("日本語\nname".into())));
    key(&mut app.context, Key::Enter);
    app.frame();
    assert_eq!(app.state().value, "日本語 name");
    assert_eq!(app.state().submits, 1, "Enter submits once, in its batch");
    app.frame();
    assert_eq!(app.state().submits, 1);
    let stats = app.frame();
    assert!(
        !stats.scene_rebuilt,
        "idle editor must preserve cached paint"
    );
}

#[test]
fn ime_preedit_is_uncommitted_cancels_on_disable_and_external_edits_reset_history() {
    let mut app = input("initial");
    let id = app.context.find(&["input"]).unwrap();
    app.context.action(id, Action::Focus);
    app.context.event(Event::Text(TextEdit::SelectAll));
    app.context.event(Event::Text(TextEdit::Preedit {
        text: "にほん".into(),
        selection: Some((9, 9)),
    }));
    app.frame();
    assert_eq!(app.state().value, "initial");
    assert!(app.context.composing_text());
    assert!(app.context.scene().primitives.iter().any(
        |primitive| matches!(primitive, Primitive::StyledText { text, .. } if text == "にほん")
    ));
    app.context
        .event(Event::Text(TextEdit::Commit("日本".into())));
    app.frame();
    assert_eq!(app.state().value, "日本");
    app.context.event(Event::Text(TextEdit::Undo));
    app.frame();
    assert_eq!(app.state().value, "initial");
    app.context.event(Event::Text(TextEdit::Preedit {
        text: "test".into(),
        selection: None,
    }));
    app.state_mut().enabled = false;
    app.frame();
    assert!(!app.context.accepts_text());
    assert!(!app.context.composing_text());
    assert!(!app.context.action(id, Action::SetText("forbidden".into())));
    app.state_mut().enabled = true;
    app.frame();
    app.context.action(id, Action::Focus);
    assert!(!app.context.composing_text());
    app.state_mut().value = "external".into();
    app.frame();
    assert!(!app.context.event(Event::Text(TextEdit::Undo)));
    assert_eq!(app.state().value, "external");
    app.context.event(Event::Text(TextEdit::Preedit {
        text: "temp".into(),
        selection: None,
    }));
    app.context.event(Event::FocusLost);
    app.frame();
    assert_eq!(app.state().value, "external");
}

#[test]
fn pointer_selection_uses_shaped_grapheme_boundaries_and_horizontal_reveal() {
    let mut app = input("a👩‍💻bc");
    app.context
        .event(Event::PointerMoved(Point { x: 11.0, y: 18.0 }));
    app.context.event(Event::PointerDown);
    app.context
        .event(Event::PointerMoved(Point { x: 30.0, y: 18.0 }));
    app.context.event(Event::PointerUp);
    assert_eq!(app.context.selected_text(), Some("a👩‍💻"));
    app.context.event(Event::Text(TextEdit::Move {
        direction: TextMove::End,
        select: false,
        word: false,
    }));
    app.context
        .event(Event::Text(TextEdit::Insert("long text ".repeat(20))));
    app.frame();
    let caret = app.context.ime_cursor_rect().unwrap();
    assert!(caret.x > 200.0 && caret.x < 280.0 && caret.width > 0.0);
    key(&mut app.context, Key::Home);
    app.frame();
    assert_eq!(app.context.ime_cursor_rect().unwrap().x, 10.0);
}

struct Choices {
    checked: bool,
    selected: usize,
}

fn choices_view(state: &Choices, ui: &mut Ui<'_, Msg<Choices>>) {
    ui.keyed("check", |ui| {
        ui.checkbox(
            "Enabled",
            state.checked,
            on(|s: &mut Choices, checked: bool| s.checked = checked),
        )
    });
    ui.keyed("radios", |ui| {
        ui.column(Style::default(), |ui| {
            ui.enabled(false, |ui| {
                ui.keyed("disabled", |ui| {
                    ui.radio(
                        "Disabled",
                        state.selected,
                        0,
                        on(|s: &mut Choices, i: usize| s.selected = i),
                    )
                });
            });
            ui.keyed("one", |ui| {
                ui.radio(
                    "One",
                    state.selected,
                    1,
                    on(|s: &mut Choices, i: usize| s.selected = i),
                )
            });
            ui.keyed("two", |ui| {
                ui.radio(
                    "Two",
                    state.selected,
                    2,
                    on(|s: &mut Choices, i: usize| s.selected = i),
                )
            });
        })
    });
}

#[test]
fn checkbox_radio_keyboard_and_disabled_scope_share_one_activation_path() {
    let mut app = Driven::new(
        ROOT,
        Choices {
            checked: false,
            selected: 0,
        },
        choices_view,
    );
    key(&mut app.context, Key::Tab);
    key(&mut app.context, Key::Space);
    app.context.event(Event::KeyUp(Key::Space));
    app.frame();
    assert!(app.state().checked);
    key(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&["radios", "one"]));
    key(&mut app.context, Key::ArrowRight);
    app.frame();
    assert_eq!(app.state().selected, 2);
    let disabled = app.context.find(&["radios", "disabled"]).unwrap();
    assert!(!app.context.action(disabled, Action::Activate));
    let one = app.context.find(&["radios", "one"]).unwrap();
    let two = app.context.find(&["radios", "two"]).unwrap();
    app.context.action(one, Action::Activate);
    app.context.action(two, Action::Activate);
    app.frame();
    assert_eq!(app.state().selected, 2);
}

struct Volume {
    value: f64,
}

fn volume_view(state: &Volume, ui: &mut Ui<'_, Msg<Volume>>) {
    ui.keyed("slider", |ui| {
        ui.slider(
            "Volume",
            state.value,
            -5.0..=6.0,
            2.0,
            on(|s: &mut Volume, value: f64| s.value = value),
        );
    });
}

#[test]
fn slider_drag_keys_and_accessibility_snap_and_clamp() {
    let mut app = Driven::new(ROOT, Volume { value: 0.0 }, volume_view);
    let id = app.context.find(&["slider"]).unwrap();
    app.context.action(id, Action::SetNumber(4.1));
    app.frame();
    assert_eq!(app.state().value, 5.0);
    app.context.action(id, Action::Increment);
    app.frame();
    assert_eq!(app.state().value, 6.0);
    assert!(!app.context.action(id, Action::SetNumber(f64::NAN)));
    app.context.action(id, Action::Focus);
    key(&mut app.context, Key::Home);
    app.frame();
    assert_eq!(app.state().value, -5.0);
    app.context
        .event(Event::PointerMoved(Point { x: 140.0, y: 50.0 }));
    app.context.event(Event::PointerDown);
    app.context
        .event(Event::PointerMoved(Point { x: 600.0, y: 50.0 }));
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().value, 6.0);
    assert_eq!(app.context.captured(), None);
}

struct Pick {
    selected: usize,
    entries: Vec<String>,
}

fn pick_view(state: &Pick, ui: &mut Ui<'_, Msg<Pick>>) {
    let items: Vec<&str> = state.entries.iter().map(String::as_str).collect();
    ui.keyed("combo", |ui| {
        ui.combo_box(
            "Choice",
            state.selected,
            &items,
            on(|s: &mut Pick, i: usize| s.selected = i),
        )
    });
    ui.keyed("behind", |ui| ui.button("Behind popup"));
}

#[test]
fn combo_popup_overlays_later_siblings_scrolls_highlight_and_restores_focus() {
    let mut app = Driven::new(
        ROOT,
        Pick {
            selected: 0,
            entries: (0..20).map(|index| format!("Choice {index}")).collect(),
        },
        pick_view,
    );
    let owner = app.context.find(&["combo"]).unwrap();
    app.context.action(owner, Action::Expand);
    app.frame();
    let panel = app.context.find(&["combo", "$popup"]).unwrap();
    let first = app.context.find(&["combo", "$popup", "0"]).unwrap();
    let bounds = app.context.node(first).unwrap().bounds;
    assert_eq!(
        app.context.hit_test(Point {
            x: bounds.x + 10.0,
            y: bounds.y + 10.0
        }),
        Some(first)
    );
    app.context.event(Event::PointerMoved(Point {
        x: bounds.x + 10.0,
        y: bounds.y + 10.0,
    }));
    app.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 80.0,
    });
    app.frame();
    assert_eq!(
        app.context.node(panel).unwrap().scroll.unwrap().offset,
        80.0,
        "wheel scrolling must not snap back to the highlighted item"
    );
    key(&mut app.context, Key::End);
    app.frame();
    assert!(app.context.node(panel).unwrap().scroll.unwrap().offset > 0.0);
    key(&mut app.context, Key::Enter);
    app.frame();
    assert_eq!(app.state().selected, 19);
    assert_eq!(app.context.focused(), Some(owner));
    assert!(app.context.find(&["combo", "$popup"]).is_none());
    app.context.action(owner, Action::Expand);
    app.frame();
    key(&mut app.context, Key::Home);
    key(&mut app.context, Key::Escape);
    app.frame();
    assert_eq!(app.state().selected, 19);
    app.context.action(owner, Action::Expand);
    app.frame();
    key(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&["behind"]));
}

struct Picked {
    picks: Vec<usize>,
}

fn menu_view(_: &Picked, ui: &mut Ui<'_, Msg<Picked>>) {
    let items = [
        MenuItem::disabled("Unavailable"),
        MenuItem::new("Open"),
        MenuItem::new("Save"),
    ];
    ui.keyed("menu", |ui| {
        ui.menu(
            "File",
            &items,
            on(|s: &mut Picked, i: usize| s.picks.push(i)),
        )
    });
}

#[test]
fn menus_skip_disabled_items_emit_once_and_close_on_outside_click() {
    let mut app = Driven::new(ROOT, Picked { picks: Vec::new() }, menu_view);
    let owner = app.context.find(&["menu"]).unwrap();
    app.context.action(owner, Action::Expand);
    app.frame();
    let disabled = app.context.find(&["menu", "$popup", "0"]).unwrap();
    assert!(!app.context.action(disabled, Action::Activate));
    key(&mut app.context, Key::Enter);
    app.frame();
    assert_eq!(app.state().picks, [1]);
    app.frame();
    assert_eq!(app.state().picks, [1], "one pick, one message");
    app.context.action(owner, Action::Expand);
    app.frame();
    app.context
        .event(Event::PointerMoved(Point { x: 450.0, y: 450.0 }));
    app.context.event(Event::PointerDown);
    app.frame();
    assert_eq!(app.context.node(owner).unwrap().expanded, Some(false));
}

struct Wheel {
    selected: usize,
    labels: Vec<String>,
}

fn wheel_view(state: &Wheel, ui: &mut Ui<'_, Msg<Wheel>>) {
    let labels: Vec<&str> = state.labels.iter().map(String::as_str).collect();
    ui.keyed("combo", |ui| {
        ui.combo_box(
            "Choices",
            state.selected,
            &labels,
            on(|s: &mut Wheel, i: usize| s.selected = i),
        )
    });
    ui.keyed("background", |ui| {
        ui.scroll(
            Style {
                width: Length::Px(400.0),
                height: Length::Px(100.0),
                ..Style::default()
            },
            |ui| {
                for index in 0..20 {
                    ui.keyed(index.to_string(), |ui| ui.button("Background row"));
                }
            },
        )
    });
}

#[test]
fn popup_wheel_targets_overlay_and_short_viewports_keep_last_choice_reachable() {
    let mut app = Driven::new(
        Size::new(220.0, 180.0),
        Wheel {
            selected: 0,
            labels: (0..20).map(|index| format!("{index}")).collect(),
        },
        wheel_view,
    );
    let owner = app.context.find(&["combo"]).unwrap();
    app.context.action(owner, Action::Expand);
    app.frame();
    let panel = app.context.find(&["combo", "$popup"]).unwrap();
    let background = app.context.find(&["background"]).unwrap();
    let bounds = app.context.node(panel).unwrap().bounds;
    assert!(bounds.x >= 0.0 && bounds.y >= 0.0 && bounds.width <= 220.0 && bounds.height <= 180.0);
    app.context
        .event(Event::PointerMoved(Point { x: 20.0, y: 80.0 }));
    app.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 80.0,
    });
    app.frame();
    assert_eq!(
        app.context.node(background).unwrap().scroll.unwrap().offset,
        0.0
    );
    assert_eq!(
        app.context.node(panel).unwrap().scroll.unwrap().offset,
        80.0
    );
    key(&mut app.context, Key::End);
    app.frame();
    let last = app
        .context
        .node(app.context.find(&["combo", "$popup", "19"]).unwrap())
        .unwrap();
    assert!(last.clip.height > 30.0 && last.bounds.y + last.bounds.height <= 180.0);
}

#[test]
fn wrapped_text_reflows_with_parent_width_and_retains_font_style() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("text", |ui| {
            ui.label(
                Label::new("A paragraph of text wraps and resizes with its parent.").style(Style {
                    width: Length::Percent(1.0),
                    wrap: TextWrap::Word,
                    font_family: Some(FontFamily::Serif),
                    font_weight: Some(700),
                    italic: Some(true),
                    padding: 10.0,
                    ..Style::default()
                }),
            )
        })
    };
    context.run(Size::new(140.0, 600.0), &mut MonospaceMeasurer, view);
    let id = context.find(&["text"]).unwrap();
    let narrow = context.node(id).unwrap().bounds;
    context.run(Size::new(320.0, 600.0), &mut MonospaceMeasurer, view);
    let wide = context.node(id).unwrap().bounds;
    assert!(narrow.height > wide.height);
    assert_eq!(wide.width, 320.0);
    assert!(context.scene().primitives.iter().any(
        |primitive| matches!(primitive, Primitive::StyledText { style, rect, .. }
        if style.weight == 700 && style.italic && rect.width == 300.0)
    ));
}
