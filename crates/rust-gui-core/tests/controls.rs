use rust_gui_core::{
    Action, Context, Event, FontFamily, Key, Length, MenuItem, MonospaceMeasurer, Style, TextEdit,
    TextMove, TextWrap, Ui,
};
use rust_gui_scene::{Point, Primitive, Size};

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, view);
}
fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}
fn input(context: &mut Context, value: &mut String, enabled: bool) {
    frame(context, |ui| {
        ui.enabled(enabled, |ui| {
            ui.text_input("input", "Name", value);
        })
    });
}

#[test]
fn input_binding_selection_graphemes_undo_and_submit_are_delivered_once() {
    let mut context = Context::new();
    let mut value = String::from("a👩‍💻e\u{301}");
    input(&mut context, &mut value, true);
    let id = context.find(&["input"]).unwrap();
    context.action(id, Action::Focus);
    key(&mut context, Key::Backspace);
    key(&mut context, Key::Backspace);
    frame(&mut context, |ui| {
        assert!(ui.text_input("input", "Name", &mut value).changed)
    });
    assert_eq!(value, "a");
    frame(&mut context, |ui| {
        assert!(!ui.text_input("input", "Name", &mut value).changed)
    });
    context.event(Event::Text(TextEdit::Undo));
    input(&mut context, &mut value, true);
    assert_eq!(value, "a👩‍💻");
    context.event(Event::Text(TextEdit::SelectAll));
    assert_eq!(context.selected_text(), Some("a👩‍💻"));
    context.event(Event::Text(TextEdit::Insert("日本語\nname".into())));
    key(&mut context, Key::Enter);
    frame(&mut context, |ui| {
        let response = ui.text_input("input", "Name", &mut value);
        assert!(response.changed && response.submitted);
    });
    assert_eq!(value, "日本語 name");
    frame(&mut context, |ui| {
        assert!(!ui.text_input("input", "Name", &mut value).submitted)
    });
    let stats = context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, |ui| {
        ui.text_input("input", "Name", &mut value);
    });
    assert!(
        !stats.scene_rebuilt,
        "idle editor must preserve cached paint"
    );
}

#[test]
fn ime_preedit_is_uncommitted_cancels_on_disable_and_external_edits_reset_history() {
    let mut context = Context::new();
    let mut value = "initial".into();
    input(&mut context, &mut value, true);
    let id = context.find(&["input"]).unwrap();
    context.action(id, Action::Focus);
    context.event(Event::Text(TextEdit::SelectAll));
    context.event(Event::Text(TextEdit::Preedit {
        text: "にほん".into(),
        selection: Some((9, 9)),
    }));
    input(&mut context, &mut value, true);
    assert_eq!(value, "initial");
    assert!(context.composing_text());
    assert!(context.scene().primitives.iter().any(
        |primitive| matches!(primitive, Primitive::StyledText { text, .. } if text == "にほん")
    ));
    context.event(Event::Text(TextEdit::Commit("日本".into())));
    input(&mut context, &mut value, true);
    assert_eq!(value, "日本");
    context.event(Event::Text(TextEdit::Undo));
    input(&mut context, &mut value, true);
    assert_eq!(value, "initial");
    context.event(Event::Text(TextEdit::Preedit {
        text: "test".into(),
        selection: None,
    }));
    input(&mut context, &mut value, false);
    assert!(!context.accepts_text());
    assert!(!context.composing_text());
    assert!(!context.action(id, Action::SetText("forbidden".into())));
    input(&mut context, &mut value, true);
    context.action(id, Action::Focus);
    assert!(!context.composing_text());
    value = "external".into();
    input(&mut context, &mut value, true);
    assert!(!context.event(Event::Text(TextEdit::Undo)));
    assert_eq!(value, "external");
    context.event(Event::Text(TextEdit::Preedit {
        text: "temp".into(),
        selection: None,
    }));
    context.event(Event::FocusLost);
    input(&mut context, &mut value, true);
    assert_eq!(value, "external");
}

#[test]
fn pointer_selection_uses_shaped_grapheme_boundaries_and_horizontal_reveal() {
    let mut context = Context::new();
    let mut value = "a👩‍💻bc".into();
    input(&mut context, &mut value, true);
    context.event(Event::PointerMoved(Point { x: 11.0, y: 18.0 }));
    context.event(Event::PointerDown);
    context.event(Event::PointerMoved(Point { x: 30.0, y: 18.0 }));
    context.event(Event::PointerUp);
    assert_eq!(context.selected_text(), Some("a👩‍💻"));
    context.event(Event::Text(TextEdit::Move {
        direction: TextMove::End,
        select: false,
        word: false,
    }));
    context.event(Event::Text(TextEdit::Insert("long text ".repeat(20))));
    input(&mut context, &mut value, true);
    let caret = context.ime_cursor_rect().unwrap();
    assert!(caret.x > 200.0 && caret.x < 280.0 && caret.width > 0.0);
    key(&mut context, Key::Home);
    input(&mut context, &mut value, true);
    assert_eq!(context.ime_cursor_rect().unwrap().x, 10.0);
}

#[test]
fn checkbox_radio_keyboard_and_disabled_scope_share_one_activation_path() {
    let mut context = Context::new();
    let mut checked = false;
    let mut selected = 0;
    let view = |ui: &mut Ui<'_>, checked: &mut bool, selected: &mut usize| {
        ui.checkbox("check", "Enabled", checked);
        ui.column("radios", Style::default(), |ui| {
            ui.enabled(false, |ui| {
                ui.radio("disabled", "Disabled", selected, 0);
            });
            ui.radio("one", "One", selected, 1);
            ui.radio("two", "Two", selected, 2);
        });
    };
    frame(&mut context, |ui| view(ui, &mut checked, &mut selected));
    key(&mut context, Key::Tab);
    key(&mut context, Key::Space);
    context.event(Event::KeyUp(Key::Space));
    frame(&mut context, |ui| view(ui, &mut checked, &mut selected));
    assert!(checked);
    key(&mut context, Key::Tab);
    assert_eq!(context.focused(), context.find(&["radios", "one"]));
    key(&mut context, Key::ArrowRight);
    frame(&mut context, |ui| view(ui, &mut checked, &mut selected));
    assert_eq!(selected, 2);
    let disabled = context.find(&["radios", "disabled"]).unwrap();
    assert!(!context.action(disabled, Action::Activate));
    let one = context.find(&["radios", "one"]).unwrap();
    let two = context.find(&["radios", "two"]).unwrap();
    context.action(one, Action::Activate);
    context.action(two, Action::Activate);
    frame(&mut context, |ui| view(ui, &mut checked, &mut selected));
    assert_eq!(selected, 2);
}

#[test]
fn slider_drag_keys_and_accessibility_snap_and_clamp() {
    let mut context = Context::new();
    let mut value = 0.0;
    let view = |ui: &mut Ui<'_>, value: &mut f64| {
        ui.slider("slider", "Volume", value, -5.0..=6.0, 2.0);
    };
    frame(&mut context, |ui| view(ui, &mut value));
    let id = context.find(&["slider"]).unwrap();
    context.action(id, Action::SetNumber(4.1));
    frame(&mut context, |ui| view(ui, &mut value));
    assert_eq!(value, 5.0);
    context.action(id, Action::Increment);
    frame(&mut context, |ui| view(ui, &mut value));
    assert_eq!(value, 6.0);
    assert!(!context.action(id, Action::SetNumber(f64::NAN)));
    context.action(id, Action::Focus);
    key(&mut context, Key::Home);
    frame(&mut context, |ui| view(ui, &mut value));
    assert_eq!(value, -5.0);
    context.event(Event::PointerMoved(Point { x: 140.0, y: 50.0 }));
    context.event(Event::PointerDown);
    context.event(Event::PointerMoved(Point { x: 600.0, y: 50.0 }));
    context.event(Event::PointerUp);
    frame(&mut context, |ui| view(ui, &mut value));
    assert_eq!(value, 6.0);
    assert_eq!(context.captured(), None);
}

#[test]
fn combo_popup_overlays_later_siblings_scrolls_highlight_and_restores_focus() {
    let mut context = Context::new();
    let mut selected = 0;
    let entries: Vec<_> = (0..20).map(|index| format!("Choice {index}")).collect();
    let items: Vec<_> = entries.iter().map(String::as_str).collect();
    let view = |ui: &mut Ui<'_>, selected: &mut usize| {
        ui.combo_box("combo", "Choice", selected, &items);
        ui.button_keyed("behind", "Behind popup");
    };
    frame(&mut context, |ui| view(ui, &mut selected));
    let owner = context.find(&["combo"]).unwrap();
    context.action(owner, Action::Expand);
    frame(&mut context, |ui| view(ui, &mut selected));
    let panel = context.find(&["combo", "$popup"]).unwrap();
    let first = context.find(&["combo", "$popup", "0"]).unwrap();
    let bounds = context.node(first).unwrap().bounds;
    assert_eq!(
        context.hit_test(Point {
            x: bounds.x + 10.0,
            y: bounds.y + 10.0
        }),
        Some(first)
    );
    context.event(Event::PointerMoved(Point {
        x: bounds.x + 10.0,
        y: bounds.y + 10.0,
    }));
    context.event(Event::Scroll { delta_y: 80.0 });
    frame(&mut context, |ui| view(ui, &mut selected));
    assert_eq!(
        context.node(panel).unwrap().scroll.unwrap().offset,
        80.0,
        "wheel scrolling must not snap back to the highlighted item"
    );
    key(&mut context, Key::End);
    frame(&mut context, |ui| view(ui, &mut selected));
    assert!(context.node(panel).unwrap().scroll.unwrap().offset > 0.0);
    key(&mut context, Key::Enter);
    frame(&mut context, |ui| view(ui, &mut selected));
    assert_eq!(selected, 19);
    assert_eq!(context.focused(), Some(owner));
    assert!(context.find(&["combo", "$popup"]).is_none());
    context.action(owner, Action::Expand);
    frame(&mut context, |ui| view(ui, &mut selected));
    key(&mut context, Key::Home);
    key(&mut context, Key::Escape);
    frame(&mut context, |ui| view(ui, &mut selected));
    assert_eq!(selected, 19);
    context.action(owner, Action::Expand);
    frame(&mut context, |ui| view(ui, &mut selected));
    key(&mut context, Key::Tab);
    assert_eq!(context.focused(), context.find(&["behind"]));
}

#[test]
fn menus_skip_disabled_items_emit_once_and_close_on_outside_click() {
    let mut context = Context::new();
    let mut result = None;
    let items = [
        MenuItem::disabled("Unavailable"),
        MenuItem::new("Open"),
        MenuItem::new("Save"),
    ];
    frame(&mut context, |ui| {
        ui.menu("menu", "File", &items);
    });
    let owner = context.find(&["menu"]).unwrap();
    context.action(owner, Action::Expand);
    frame(&mut context, |ui| {
        ui.menu("menu", "File", &items);
    });
    let disabled = context.find(&["menu", "$popup", "0"]).unwrap();
    assert!(!context.action(disabled, Action::Activate));
    key(&mut context, Key::Enter);
    frame(&mut context, |ui| result = ui.menu("menu", "File", &items));
    assert_eq!(result, Some(1));
    frame(&mut context, |ui| result = ui.menu("menu", "File", &items));
    assert_eq!(result, None);
    context.action(owner, Action::Expand);
    frame(&mut context, |ui| {
        ui.menu("menu", "File", &items);
    });
    context.event(Event::PointerMoved(Point { x: 450.0, y: 450.0 }));
    context.event(Event::PointerDown);
    frame(&mut context, |ui| {
        ui.menu("menu", "File", &items);
    });
    assert_eq!(context.node(owner).unwrap().expanded, Some(false));
}

#[test]
fn popup_wheel_targets_overlay_and_short_viewports_keep_last_choice_reachable() {
    let mut context = Context::new();
    let mut selected = 0;
    let items: Vec<_> = (0..20).map(|index| format!("{index}")).collect();
    let labels: Vec<_> = items.iter().map(String::as_str).collect();
    let mut view = |ui: &mut Ui<'_>| {
        ui.combo_box("combo", "Choices", &mut selected, &labels);
        ui.scroll(
            "background",
            Style {
                width: Length::Px(400.0),
                height: Length::Px(100.0),
                ..Style::default()
            },
            |ui| {
                for index in 0..20 {
                    ui.button_keyed(index.to_string(), "Background row");
                }
            },
        );
    };
    context.run(Size::new(220.0, 180.0), &mut MonospaceMeasurer, &mut view);
    let owner = context.find(&["combo"]).unwrap();
    context.action(owner, Action::Expand);
    context.run(Size::new(220.0, 180.0), &mut MonospaceMeasurer, &mut view);
    let panel = context.find(&["combo", "$popup"]).unwrap();
    let background = context.find(&["background"]).unwrap();
    let bounds = context.node(panel).unwrap().bounds;
    assert!(bounds.x >= 0.0 && bounds.y >= 0.0 && bounds.width <= 220.0 && bounds.height <= 180.0);
    context.event(Event::PointerMoved(Point { x: 20.0, y: 80.0 }));
    context.event(Event::Scroll { delta_y: 80.0 });
    context.run(Size::new(220.0, 180.0), &mut MonospaceMeasurer, &mut view);
    assert_eq!(
        context.node(background).unwrap().scroll.unwrap().offset,
        0.0
    );
    assert_eq!(context.node(panel).unwrap().scroll.unwrap().offset, 80.0);
    key(&mut context, Key::End);
    context.run(Size::new(220.0, 180.0), &mut MonospaceMeasurer, &mut view);
    let last = context
        .node(context.find(&["combo", "$popup", "19"]).unwrap())
        .unwrap();
    assert!(last.clip.height > 30.0 && last.bounds.y + last.bounds.height <= 180.0);
}

#[test]
fn wrapped_text_reflows_with_parent_width_and_retains_font_style() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.label_styled(
            "text",
            "A paragraph of text wraps and resizes with its parent.",
            Style {
                width: Length::Percent(1.0),
                wrap: TextWrap::Word,
                font_family: FontFamily::Serif,
                font_weight: 700,
                italic: true,
                padding: 10.0,
                ..Style::default()
            },
        )
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
