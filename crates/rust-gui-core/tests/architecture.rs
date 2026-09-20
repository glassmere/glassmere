use rust_gui_core::{
    Action, Component, Context, CustomWidget, Event, Key, Length, MonospaceMeasurer, Role,
    Semantics, Style, Ui,
};
use rust_gui_scene::{Color, Point, Primitive, Rect, Size};

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> rust_gui_core::FrameStats {
    context.run(Size::new(400.0, 400.0), &mut MonospaceMeasurer, view)
}

fn scroll_list(ui: &mut Ui<'_>, count: usize) {
    ui.scroll(
        "list",
        Style {
            width: Length::Px(160.0),
            height: Length::Px(100.0),
            gap: 0.0,
            ..Style::default()
        },
        |ui| {
            for index in 0..count {
                ui.button_styled(
                    index.to_string(),
                    format!("Item {index}"),
                    true,
                    Style {
                        width: Length::Px(120.0),
                        height: Length::Px(40.0),
                        padding: 0.0,
                        ..Style::button()
                    },
                );
            }
        },
    );
}

#[test]
fn scrolling_moves_paint_and_hit_testing_without_layout_and_clamps_after_removal() {
    let mut context = Context::new();
    frame(&mut context, |ui| scroll_list(ui, 10));
    let list = context.find(&["list"]).unwrap();
    let item = context.find(&["list", "2"]).unwrap();
    context.event(Event::PointerMoved(Point { x: 10.0, y: 10.0 }));
    assert!(context.event(Event::Scroll { delta_y: 80.0 }));
    let stats = frame(&mut context, |ui| scroll_list(ui, 10));
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(context.node(list).unwrap().scroll.unwrap().offset, 80.0);
    assert_eq!(context.node(item).unwrap().bounds.y, 0.0);
    assert_eq!(context.hit_test(Point { x: 10.0, y: 10.0 }), Some(item));
    assert!(context.scene().primitives.iter().any(|p| matches!(p,
        Primitive::Text { text, rect, .. } if text == "Item 2" && rect.y == 0.0)));
    assert_ne!(context.hit_test(Point { x: 10.0, y: 110.0 }), Some(item));
    assert!(!context.action(list, Action::ScrollTo(f32::NAN)));
    assert!(context.action(list, Action::ScrollTo(10000.0)));
    assert_eq!(context.node(list).unwrap().scroll.unwrap().offset, 300.0);
    assert!(!context.action(list, Action::ScrollBy(40.0)));
    frame(&mut context, |ui| scroll_list(ui, 1));
    assert_eq!(context.node(list).unwrap().scroll.unwrap().offset, 0.0);
    assert_eq!(context.node(list).unwrap().scroll.unwrap().maximum, 0.0);
}

#[test]
fn wheel_bubbles_from_an_inner_viewport_at_its_limit() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.scroll(
            "outer",
            Style {
                width: Length::Px(200.0),
                height: Length::Px(100.0),
                gap: 0.0,
                ..Style::default()
            },
            |ui| {
                ui.scroll(
                    "inner",
                    Style {
                        width: Length::Px(160.0),
                        height: Length::Px(80.0),
                        gap: 0.0,
                        ..Style::default()
                    },
                    |ui| {
                        for index in 0..5 {
                            ui.label_styled(
                                index.to_string(),
                                "line",
                                Style {
                                    height: Length::Px(40.0),
                                    ..Style::default()
                                },
                            );
                        }
                    },
                );
                ui.label_styled(
                    "tail",
                    "tail",
                    Style {
                        height: Length::Px(200.0),
                        ..Style::default()
                    },
                );
            },
        );
    });
    let inner = context.find(&["outer", "inner"]).unwrap();
    let outer = context.find(&["outer"]).unwrap();
    context.event(Event::PointerMoved(Point { x: 10.0, y: 10.0 }));
    context.action(inner, Action::ScrollTo(10000.0));
    assert!(context.event(Event::Scroll { delta_y: 20.0 }));
    assert_eq!(context.node(inner).unwrap().scroll.unwrap().offset, 120.0);
    assert_eq!(context.node(outer).unwrap().scroll.unwrap().offset, 20.0);
}

#[test]
fn focus_reveals_offscreen_buttons_and_keyboard_scroll_uses_the_ancestor() {
    let mut context = Context::new();
    frame(&mut context, |ui| scroll_list(ui, 10));
    let last = context.find(&["list", "9"]).unwrap();
    assert!(context.action(last, Action::Focus));
    assert!(context.node(last).unwrap().clip.height > 0.0);
    assert_eq!(context.focused(), Some(last));
    assert!(context.event(Event::KeyDown {
        key: Key::Home,
        shift: false,
        repeat: false
    }));
    let list = context.find(&["list"]).unwrap();
    assert_eq!(context.node(list).unwrap().scroll.unwrap().offset, 0.0);
    assert!(context.event(Event::KeyDown {
        key: Key::Tab,
        shift: true,
        repeat: false
    }));
    let focused = context.focused().unwrap();
    assert!(context.node(focused).unwrap().clip.height > 0.0);
}

#[test]
fn accessibility_activation_is_once_and_ignores_disabled_and_removed_ids() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.button_keyed("active", "Active");
        ui.button_styled("disabled", "Disabled", false, Style::button());
    });
    let active = context.find(&["active"]).unwrap();
    let disabled = context.find(&["disabled"]).unwrap();
    assert!(!context.action(disabled, Action::Focus));
    assert!(!context.action(disabled, Action::Activate));
    assert!(context.action(active, Action::Activate));
    let mut clicks = 0;
    for _ in 0..2 {
        frame(&mut context, |ui| {
            if ui.button_keyed("active", "Active").clicked() {
                clicks += 1;
            }
        });
    }
    assert_eq!(clicks, 1);
    frame(&mut context, |_| {});
    assert!(!context.action(active, Action::Activate));
    assert!(!context.action(active, Action::Focus));
}

#[test]
fn a_local_paint_change_reuses_the_other_subtree_and_reports_local_damage() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>, color| {
        ui.column("left", Style::default(), |ui| {
            ui.label_styled(
                "changing",
                "same size",
                Style {
                    foreground: Some(color),
                    ..Style::default()
                },
            );
        });
        ui.column("right", Style::default(), |ui| {
            for index in 0..20 {
                ui.label_keyed(index.to_string(), "unchanged");
            }
        });
    };
    frame(&mut context, |ui| view(ui, Color::WHITE));
    let bounds = context
        .node(context.find(&["left", "changing"]).unwrap())
        .unwrap()
        .clip;
    let stats = frame(&mut context, |ui| view(ui, Color::rgb(200, 0, 0)));
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(stats.paint_nodes, 3, "root, changed group, changed label");
    assert_eq!(stats.paint_cache_hits, 1, "whole stable subtree reused");
    assert_eq!(context.damage_regions(), &[bounds]);
    let mut fresh = Context::new();
    frame(&mut fresh, |ui| view(ui, Color::rgb(200, 0, 0)));
    assert_eq!(
        context.scene(),
        fresh.scene(),
        "cached and full paint agree"
    );
    frame(&mut context, |ui| view(ui, Color::rgb(200, 0, 0)));
    assert!(context.damage_regions().is_empty());
}

#[test]
fn cached_interaction_and_scroll_paint_match_fresh_paint() {
    let mut context = Context::new();
    frame(&mut context, |ui| scroll_list(ui, 10));
    let target = context.find(&["list", "8"]).unwrap();
    context.action(target, Action::Focus);
    frame(&mut context, |ui| scroll_list(ui, 10));
    let mut fresh = Context::new();
    frame(&mut fresh, |ui| scroll_list(ui, 10));
    fresh.action(fresh.find(&["list", "8"]).unwrap(), Action::Focus);
    frame(&mut fresh, |ui| scroll_list(ui, 10));
    assert_eq!(context.scene(), fresh.scene());
    context.event(Event::FocusLost);
    frame(&mut context, |ui| scroll_list(ui, 10));
    assert_ne!(
        context.scene(),
        fresh.scene(),
        "focus decoration must be invalidated"
    );
}

struct CounterComponent {
    count: usize,
}
impl Component for CounterComponent {
    fn ui(&mut self, ui: &mut Ui<'_>) {
        if ui
            .button_keyed("increment", format!("Count {}", self.count))
            .clicked()
        {
            self.count += 1;
        }
    }
}

#[test]
fn component_scopes_keep_application_state_and_child_identity_separate() {
    let mut a = CounterComponent { count: 0 };
    let mut b = CounterComponent { count: 10 };
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.component("a", Style::default(), &mut a);
        ui.component("b", Style::default(), &mut b);
    });
    let target = context.find(&["a", "increment"]).unwrap();
    context.action(target, Action::Activate);
    frame(&mut context, |ui| {
        ui.component("b", Style::default(), &mut b);
        ui.component("a", Style::default(), &mut a);
    });
    assert_eq!((a.count, b.count), (1, 10));
    assert_eq!(context.find(&["a", "increment"]), Some(target));
}

fn custom_button(color: Color) -> CustomWidget {
    CustomWidget {
        size: Size::new(40.0, 40.0),
        semantics: Semantics {
            role: Role::Button,
            label: "Custom action".into(),
            disabled: false,
            focusable: true,
        },
        primitives: vec![Primitive::Quad {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
            },
            color,
            radius: 2.0,
        }],
    }
}

#[test]
fn custom_leaf_shares_activation_and_updates_paint_without_layout() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.custom("custom", Style::default(), custom_button(Color::WHITE));
    });
    let target = context.find(&["custom"]).unwrap();
    context.action(target, Action::Focus);
    context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(Key::Enter));
    let mut clicked = false;
    let stats = frame(&mut context, |ui| {
        clicked = ui
            .custom(
                "custom",
                Style::default(),
                custom_button(Color::rgb(0, 255, 0)),
            )
            .clicked();
    });
    assert!(clicked);
    assert_eq!(stats.layout_passes, 0);
    assert_eq!(
        context.node(target).unwrap().semantics.label,
        "Custom action"
    );
    assert!(
        context
            .scene()
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Quad { color, .. } if *color == Color::rgb(0, 255, 0)))
    );
}

#[test]
#[should_panic(expected = "custom widget clip underflow")]
fn custom_widgets_cannot_pop_the_host_clip() {
    let mut widget = custom_button(Color::WHITE);
    widget.primitives.insert(0, Primitive::PopClip);
    frame(&mut Context::new(), |ui| {
        ui.custom("invalid", Style::default(), widget);
    });
}

#[test]
fn a_nonfocusable_custom_button_remains_pointer_activatable() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        let mut widget = custom_button(Color::WHITE);
        widget.semantics.focusable = false;
        ui.custom("custom", Style::default(), widget)
    };
    frame(&mut context, |ui| {
        view(ui);
    });
    context.event(Event::PointerMoved(Point { x: 10.0, y: 10.0 }));
    context.event(Event::PointerDown);
    assert!(context.focused().is_none());
    context.event(Event::PointerUp);
    let mut clicked = false;
    frame(&mut context, |ui| {
        clicked = view(ui).clicked();
    });
    assert!(clicked);
}
