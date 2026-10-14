use rust_gui_core::{Button, Label};
use rust_gui_core::{Context, Dirty, Event, Key, Length, MonospaceMeasurer, Style, Ui};
use rust_gui_scene::{Color, Point, Primitive, Size};

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> rust_gui_core::FrameStats {
    context.run(Size::new(640.0, 480.0), &mut MonospaceMeasurer, view)
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

fn center(context: &Context, path: &[&str]) -> Point {
    let rect = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }
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

#[test]
fn click_is_delivered_once_and_release_outside_cancels_activation() {
    let mut context = Context::new();
    let mut count = 0;
    frame(&mut context, |ui| {
        ui.keyed("click", |ui| ui.button("Click"));
    });
    let position = center(&context, &["click"]);
    context.event(Event::PointerMoved(position));
    context.event(Event::PointerDown);
    assert_eq!(context.captured(), context.find(&["click"]));
    context.event(Event::PointerLeft);
    context.event(Event::PointerUp);
    frame(&mut context, |ui| {
        if ui.keyed("click", |ui| ui.button("Click")).clicked() {
            count += 1;
        }
    });
    assert_eq!(count, 0);
    context.event(Event::PointerMoved(position));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    for _ in 0..2 {
        frame(&mut context, |ui| {
            if ui.keyed("click", |ui| ui.button("Click")).clicked() {
                count += 1;
            }
        });
    }
    assert_eq!(count, 1);
}

#[test]
fn tab_skips_disabled_buttons_and_keyboard_repeat_does_not_double_activate() {
    let mut context = Context::new();
    frame(&mut context, buttons);
    let tab = |shift| Event::KeyDown {
        key: Key::Tab,
        shift,
        repeat: false,
    };
    context.event(tab(false));
    assert_eq!(context.focused(), context.find(&["actions", "first"]));
    context.event(tab(false));
    assert_eq!(context.focused(), context.find(&["actions", "last"]));
    context.event(tab(true));
    let first = context.focused().unwrap();
    context.event(Event::KeyDown {
        key: Key::Space,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyDown {
        key: Key::Space,
        shift: false,
        repeat: true,
    });
    context.event(Event::KeyUp(Key::Enter));
    context.event(Event::KeyUp(Key::Space));
    let mut clicks = 0;
    for _ in 0..2 {
        frame(&mut context, |ui| {
            ui.keyed("actions", |ui| {
                ui.row(Style::default(), |ui| {
                    if ui.keyed("first", |ui| ui.button("First")).clicked() {
                        clicks += 1;
                    }
                })
            });
        });
    }
    assert_eq!(clicks, 1);
    assert_eq!(context.focused(), Some(first));
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
