mod support;

use seaglass_core::app::Action as Message;
use seaglass_core::{Button, Label};
use seaglass_core::{Context, Dirty, Event, Key, Length, MonospaceMeasurer, Style, Ui};
use seaglass_scene::{Color, Point, Primitive, Size};
use support::{Driven, act, center};

const ROOT: Size = Size::new(640.0, 480.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> seaglass_core::FrameStats {
    context.run(ROOT, &mut MonospaceMeasurer, view)
}

fn buttons(ui: &mut Ui<'_>) {
    ui.keyed("actions", |ui| {
        ui.row(Style::default(), |ui| {
            ui.keyed("first", |ui| ui.button("First"));
            ui.keyed("disabled", |ui| {
                ui.button(
                    Button::new("Disabled")
                        .enabled(false)
                        .style(Style::button()),
                )
            });
            ui.keyed("last", |ui| ui.button("Last"));
        })
    });
}

#[test]
fn unchanged_frames_skip_layout_measurement_and_paint() {
    let mut context = Context::new();
    let first = frame(&mut context, buttons);
    assert_eq!(first.created, 4);
    assert_eq!(first.layout_passes, 1);
    let scene = context.scene().clone();
    let next = frame(&mut context, buttons);
    assert_eq!(next.created, 0);
    assert_eq!(next.reused, 4);
    assert_eq!(next.layout_passes, 0);
    assert_eq!(next.text_measures, 0);
    assert!(!next.scene_rebuilt);
    assert!(next.dirty.is_empty());
    assert_eq!(*context.scene(), scene);
}

#[test]
fn explicit_keys_survive_reorder_and_removal_does_not_reuse_ids() {
    let mut context = Context::new();
    let declare = |keys: &[&str], ui: &mut Ui<'_>| {
        for key in keys {
            ui.keyed(*key, |ui| ui.button(*key));
        }
    };
    frame(&mut context, |ui| declare(&["a", "b", "c"], ui));
    let a = context.find(&["a"]).unwrap();
    let b = context.find(&["b"]).unwrap();
    frame(&mut context, |ui| declare(&["c", "a"], ui));
    assert_eq!(context.find(&["a"]), Some(a));
    assert!(context.node(b).is_none());
    assert_eq!(context.stats().removed, 1);
    frame(&mut context, |ui| declare(&["a", "b"], ui));
    assert_ne!(context.find(&["b"]), Some(b));
}

#[test]
fn keys_are_scoped_to_parent_and_type_changes_replace_nodes() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("a", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("child", |ui| ui.label("a"));
            })
        });
        ui.keyed("b", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("child", |ui| ui.label("b"));
            })
        });
    });
    assert_ne!(context.find(&["a", "child"]), context.find(&["b", "child"]));
    let old = context.find(&["a"]).unwrap();
    frame(&mut context, |ui| {
        ui.keyed("a", |ui| ui.button("a"));
    });
    assert_ne!(context.find(&["a"]), Some(old));
    assert_eq!(context.node_count(), 2);
}

#[test]
#[should_panic(expected = "duplicate widget key")]
fn duplicate_sibling_keys_are_rejected() {
    frame(&mut Context::new(), |ui| {
        ui.keyed("x", |ui| ui.label("one"));
        ui.keyed("x", |ui| ui.label("two"));
    });
}

/// One view that shows a notice only sometimes, ahead of a button.
fn notice_then_button(ui: &mut Ui<'_>, notice: bool) {
    if notice {
        ui.label("Saved");
    }
    ui.button("Send");
}

fn send(context: &Context) -> Option<seaglass_core::NodeId> {
    context
        .snapshots()
        .into_iter()
        .find(|node| node.semantics.label == "Send")
        .map(|node| node.id)
}

#[test]
fn a_widget_after_a_conditional_one_keeps_its_identity_and_focus() {
    let mut context = Context::new();
    frame(&mut context, |ui| notice_then_button(ui, false));
    let before = send(&context);
    context.event(Event::KeyDown {
        key: Key::Tab,
        shift: false,
        repeat: false,
    });
    assert_eq!(context.focused(), before);
    // The notice appears ahead of it: the button is still the same widget.
    frame(&mut context, |ui| notice_then_button(ui, true));
    assert_eq!(send(&context), before);
    assert_eq!(context.focused(), before);
    frame(&mut context, |ui| notice_then_button(ui, false));
    assert_eq!(send(&context), before);
}

#[test]
fn repeats_of_one_call_site_are_told_apart_by_order() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        for name in ["a", "b", "c"] {
            ui.button(name);
        }
    };
    frame(&mut context, view);
    let ids: Vec<_> = context
        .snapshots()
        .into_iter()
        .filter(|node| ["a", "b", "c"].contains(&node.semantics.label.as_str()))
        .map(|node| node.id)
        .collect();
    assert_eq!(ids.len(), 3);
    frame(&mut context, view);
    let again: Vec<_> = context
        .snapshots()
        .into_iter()
        .filter(|node| ["a", "b", "c"].contains(&node.semantics.label.as_str()))
        .map(|node| node.id)
        .collect();
    assert_eq!(again, ids);
}

#[test]
fn every_widget_is_identified_by_the_application_line_that_declared_it() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.label("label");
        ui.button("button");
        ui.text_input("text", "", |_| ());
        ui.checkbox("checkbox", false, |_| ());
        ui.radio("radio", 0, 0, |_| ());
        ui.slider("slider", 1.0, 0.0..=2.0, 1.0, |_| ());
        ui.combo_box("combo", 0, &["one", "two"], |_| ());
        ui.number_input(
            "number",
            1.0,
            seaglass_core::NumberOptions::default(),
            |_| (),
        );
        ui.column(Style::default(), |_| {});
        ui.row(Style::default(), |_| {});
        ui.grid(Style::default(), |_| {});
        ui.container(Style::default(), |_| {});
        ui.scroll(Style::default(), |_| {});
    });
    let root = context.snapshots()[0].id;
    let keys: Vec<String> = context
        .snapshots()
        .into_iter()
        .filter(|node| node.parent == Some(root))
        .map(|node| context.details(node.id).unwrap().key)
        .collect();
    assert_eq!(keys.len(), 13);
    for key in keys {
        // A key at a line of the framework's own means a method in between
        // lost `#[track_caller]`, and a conditional sibling would shift it.
        assert!(key.starts_with("kernel.rs:"), "{key}");
    }
}

#[test]
fn paint_changes_skip_layout_and_text_changes_update_geometry() {
    let mut context = Context::new();
    let declare = |ui: &mut Ui<'_>, text: &str, color| {
        ui.keyed("text", |ui| {
            ui.label(Label::new(text).style(Style {
                foreground: Some(color),
                ..Style::default()
            }))
        });
    };
    frame(&mut context, |ui| declare(ui, "Hello", Color::WHITE));
    let id = context.find(&["text"]).unwrap();
    let initial = context.node(id).unwrap().bounds;
    let stats = frame(&mut context, |ui| {
        declare(ui, "Hello", Color::rgb(255, 0, 0))
    });
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(stats.text_measures, 0);
    assert!(stats.dirty.contains(Dirty::PAINT));
    let stats = frame(&mut context, |ui| declare(ui, "Hello, world", Color::WHITE));
    assert_eq!(stats.text_measures, 1);
    assert_eq!(stats.layout_passes, 1);
    assert!(context.node(id).unwrap().bounds.width > initial.width);
}

#[test]
fn equal_intrinsic_text_size_updates_paint_and_semantics_without_layout() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("text", |ui| ui.label("one"));
    });
    let id = context.find(&["text"]).unwrap();
    let bounds = context.node(id).unwrap().bounds;
    let stats = frame(&mut context, |ui| {
        ui.keyed("text", |ui| ui.label("two"));
    });
    assert_eq!(stats.text_measures, 1);
    assert_eq!(stats.layout_passes, 0);
    assert!(stats.scene_rebuilt);
    assert!(
        stats
            .dirty
            .contains(Dirty::TEXT | Dirty::PAINT | Dirty::ACCESSIBILITY)
    );
    assert!(!stats.dirty.contains(Dirty::LAYOUT));
    let node = context.node(id).unwrap();
    assert_eq!(node.bounds, bounds);
    assert_eq!(node.semantics.label, "two");
    assert!(
        context.scene().primitives.iter().any(|primitive| {
            matches!(primitive, Primitive::Text { text, .. } if text == "two")
        })
    );
}

/// What the buttons sent: clicks of the one under test.
#[derive(Default)]
struct Clicks {
    count: usize,
}

fn click_view(_: &Clicks, ui: &mut Ui<'_, Message<Clicks>>) {
    ui.keyed("click", |ui| {
        ui.button(Button::new("Click").on_click(act(|s: &mut Clicks| s.count += 1)))
    });
}

#[test]
fn click_is_delivered_once_and_release_outside_cancels_activation() {
    let mut app = Driven::new(ROOT, Clicks::default(), click_view);
    let position = center(&app.context, &["click"]);
    app.context.event(Event::PointerMoved(position));
    app.context.event(Event::PointerDown);
    assert_eq!(app.context.captured(), app.context.find(&["click"]));
    app.context.event(Event::PointerLeft);
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().count, 0);
    app.context.event(Event::PointerMoved(position));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerUp);
    app.frame();
    app.frame();
    assert_eq!(app.state().count, 1);
}

/// The three buttons of `buttons`, the first one sending its clicks.
#[derive(Default)]
struct Presses {
    first: usize,
}

fn presses_view(_: &Presses, ui: &mut Ui<'_, Message<Presses>>) {
    ui.keyed("actions", |ui| {
        ui.row(Style::default(), |ui| {
            ui.keyed("first", |ui| {
                ui.button(Button::new("First").on_click(act(|s: &mut Presses| s.first += 1)))
            });
            ui.keyed("disabled", |ui| {
                ui.button(
                    Button::new("Disabled")
                        .enabled(false)
                        .style(Style::button()),
                )
            });
            ui.keyed("last", |ui| ui.button("Last"));
        })
    });
}

#[test]
fn tab_skips_disabled_buttons_and_keyboard_repeat_does_not_double_activate() {
    let mut app = Driven::new(ROOT, Presses::default(), presses_view);
    let tab = |shift| Event::KeyDown {
        key: Key::Tab,
        shift,
        repeat: false,
    };
    app.context.event(tab(false));
    assert_eq!(
        app.context.focused(),
        app.context.find(&["actions", "first"])
    );
    app.context.event(tab(false));
    assert_eq!(
        app.context.focused(),
        app.context.find(&["actions", "last"])
    );
    app.context.event(tab(true));
    let first = app.context.focused().unwrap();
    app.context.event(Event::KeyDown {
        key: Key::Space,
        shift: false,
        repeat: false,
    });
    app.context.event(Event::KeyDown {
        key: Key::Space,
        shift: false,
        repeat: true,
    });
    app.context.event(Event::KeyUp(Key::Enter));
    app.context.event(Event::KeyUp(Key::Space));
    app.frame();
    app.frame();
    assert_eq!(app.state().first, 1);
    assert_eq!(app.context.focused(), Some(first));
}

#[test]
fn removing_or_disabling_pressed_nodes_clears_focus_and_capture() {
    let mut context = Context::new();
    frame(&mut context, buttons);
    context.event(Event::PointerMoved(center(&context, &["actions", "first"])));
    context.event(Event::PointerDown);
    frame(&mut context, |ui| {
        ui.keyed("actions", |ui| {
            ui.row(Style::default(), |ui| {
                ui.keyed("first", |ui| {
                    ui.button(Button::new("First").enabled(false).style(Style::button()))
                });
            })
        });
    });
    assert!(context.focused().is_none());
    assert!(context.captured().is_none());
    frame(&mut context, |_| {});
    assert_eq!(context.node_count(), 1);
    assert!(context.hovered().is_none());
}

#[test]
fn clipping_applies_to_hit_testing_and_resize_updates_layout() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("clip", |ui| {
            ui.container(
                Style {
                    width: Length::Px(40.0),
                    height: Length::Px(40.0),
                    ..Style::default()
                },
                |ui| {
                    ui.keyed("wide", |ui| ui.button("A very wide button"));
                },
            )
        });
    };
    frame(&mut context, view);
    assert!(context.hit_test(Point { x: 20.0, y: 20.0 }).is_some());
    assert!(context.hit_test(Point { x: 80.0, y: 20.0 }).is_none());
    let stats = context.run(Size::new(20.0, 20.0), &mut MonospaceMeasurer, view);
    assert_eq!(stats.layout_passes, 1);
    assert!(context.hit_test(Point { x: 25.0, y: 15.0 }).is_none());
}

#[test]
fn focus_loss_cancels_pending_activation() {
    let mut context = Context::new();
    frame(&mut context, buttons);
    context.event(Event::PointerMoved(center(&context, &["actions", "first"])));
    context.event(Event::PointerDown);
    context.event(Event::FocusLost);
    assert!(!context.event(Event::PointerUp));
    assert!(context.focused().is_none());
    assert!(context.captured().is_none());
}
