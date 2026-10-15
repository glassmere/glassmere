use rust_gui_core::{
    Action, App, AppDriver, Component, Context, CustomWidget, Effects, Event, Key, Length,
    MonospaceMeasurer, Role, Semantics, Style, Ui,
};
mod support;

use rust_gui_core::app::Action as Message;
use rust_gui_core::{Button, Label};
use rust_gui_scene::{Color, Point, Primitive, Rect, Size};
use support::{Driven, act};

const ROOT: Size = Size::new(400.0, 400.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> rust_gui_core::FrameStats {
    context.run(Size::new(400.0, 400.0), &mut MonospaceMeasurer, view)
}

fn scroll_list(ui: &mut Ui<'_>, count: usize) {
    ui.keyed("list", |ui| {
        ui.scroll(
            Style {
                width: Length::Px(160.0),
                height: Length::Px(100.0),
                gap: 0.0,
                ..Style::default()
            },
            |ui| {
                for index in 0..count {
                    ui.keyed(index.to_string(), |ui| {
                        ui.button(
                            Button::new(format!("Item {index}"))
                                .enabled(true)
                                .style(Style {
                                    width: Length::Px(120.0),
                                    height: Length::Px(40.0),
                                    padding: 0.0,
                                    ..Style::button()
                                }),
                        )
                    });
                }
            },
        )
    });
}

#[test]
fn scrolling_moves_paint_and_hit_testing_without_layout_and_clamps_after_removal() {
    let mut context = Context::new();
    frame(&mut context, |ui| scroll_list(ui, 10));
    let list = context.find(&["list"]).unwrap();
    let item = context.find(&["list", "2"]).unwrap();
    context.event(Event::PointerMoved(Point { x: 10.0, y: 10.0 }));
    assert!(context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 80.0
    }));
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
        ui.keyed("outer", |ui| {
            ui.scroll(
                Style {
                    width: Length::Px(200.0),
                    height: Length::Px(100.0),
                    gap: 0.0,
                    ..Style::default()
                },
                |ui| {
                    ui.keyed("inner", |ui| {
                        ui.scroll(
                            Style {
                                width: Length::Px(160.0),
                                height: Length::Px(80.0),
                                gap: 0.0,
                                ..Style::default()
                            },
                            |ui| {
                                for index in 0..5 {
                                    ui.keyed(index.to_string(), |ui| {
                                        ui.label(Label::new("line").style(Style {
                                            height: Length::Px(40.0),
                                            ..Style::default()
                                        }))
                                    });
                                }
                            },
                        )
                    });
                    ui.keyed("tail", |ui| {
                        ui.label(Label::new("tail").style(Style {
                            height: Length::Px(200.0),
                            ..Style::default()
                        }))
                    });
                },
            )
        });
    });
    let inner = context.find(&["outer", "inner"]).unwrap();
    let outer = context.find(&["outer"]).unwrap();
    context.event(Event::PointerMoved(Point { x: 10.0, y: 10.0 }));
    context.action(inner, Action::ScrollTo(10000.0));
    assert!(context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 20.0
    }));
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

/// The active button's clicks, and whether it is declared at all.
struct Active {
    clicks: usize,
    shown: bool,
}

fn active_view(state: &Active, ui: &mut Ui<'_, Message<Active>>) {
    if state.shown {
        ui.keyed("active", |ui| {
            ui.button(Button::new("Active").on_click(act(|s: &mut Active| s.clicks += 1)))
        });
    }
    ui.keyed("disabled", |ui| {
        ui.button(
            Button::new("Disabled")
                .enabled(false)
                .style(Style::button()),
        )
    });
}

#[test]
fn accessibility_activation_is_once_and_ignores_disabled_and_removed_ids() {
    let mut app = Driven::new(
        ROOT,
        Active {
            clicks: 0,
            shown: true,
        },
        active_view,
    );
    let active = app.context.find(&["active"]).unwrap();
    let disabled = app.context.find(&["disabled"]).unwrap();
    assert!(!app.context.action(disabled, Action::Focus));
    assert!(!app.context.action(disabled, Action::Activate));
    assert!(app.context.action(active, Action::Activate));
    app.frame();
    app.frame();
    assert_eq!(app.state().clicks, 1);
    app.state_mut().shown = false;
    app.frame();
    assert!(!app.context.action(active, Action::Activate));
    assert!(!app.context.action(active, Action::Focus));
}

#[test]
fn a_local_paint_change_reuses_the_other_subtree_and_reports_local_damage() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>, color| {
        ui.keyed("left", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("changing", |ui| {
                    ui.label(Label::new("same size").style(Style {
                        foreground: Some(color),
                        ..Style::default()
                    }))
                });
            })
        });
        ui.keyed("right", |ui| {
            ui.column(Style::default(), |ui| {
                for index in 0..20 {
                    ui.keyed(index.to_string(), |ui| ui.label("unchanged"));
                }
            })
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

#[derive(Clone)]
enum CounterMessage {
    Increment,
}

struct CounterComponent {
    count: usize,
}

impl Component for CounterComponent {
    type Message = CounterMessage;

    fn update(&mut self, message: CounterMessage, _effects: &mut Effects<'_, CounterMessage>) {
        match message {
            CounterMessage::Increment => self.count += 1,
        }
    }

    fn view(&self, ui: &mut Ui<'_, CounterMessage>) {
        ui.keyed("increment", |ui| {
            ui.button(
                Button::new(format!("Count {}", self.count)).on_click(CounterMessage::Increment),
            )
        });
    }
}

#[derive(Clone)]
enum PairMessage {
    A(CounterMessage),
    B(CounterMessage),
}

struct Pair {
    a: CounterComponent,
    b: CounterComponent,
    swapped: bool,
}

impl App for Pair {
    type Message = PairMessage;

    fn update(&mut self, message: PairMessage, effects: &mut Effects<'_, PairMessage>) {
        match message {
            PairMessage::A(message) => self.a.update(message, &mut effects.map(PairMessage::A)),
            PairMessage::B(message) => self.b.update(message, &mut effects.map(PairMessage::B)),
        }
    }

    fn view(&self, ui: &mut Ui<'_, PairMessage>) {
        let a = |ui: &mut Ui<'_, PairMessage>| {
            ui.keyed("a", |ui| {
                ui.component(Style::default(), &self.a, PairMessage::A)
            });
        };
        let b = |ui: &mut Ui<'_, PairMessage>| {
            ui.keyed("b", |ui| {
                ui.component(Style::default(), &self.b, PairMessage::B)
            });
        };
        if self.swapped {
            b(ui);
            a(ui);
        } else {
            a(ui);
            b(ui);
        }
    }
}

#[test]
fn component_scopes_keep_application_state_and_child_identity_separate() {
    let mut app = Pair {
        a: CounterComponent { count: 0 },
        b: CounterComponent { count: 10 },
        swapped: false,
    };
    let mut driver = AppDriver::new(|| true);
    let mut context = Context::new();
    let size = Size::new(400.0, 400.0);
    driver.frame(&mut app, &mut context, size, &mut MonospaceMeasurer);
    let target = context.find(&["a", "increment"]).unwrap();
    context.action(target, Action::Activate);
    app.swapped = true;
    let stats = driver.frame(&mut app, &mut context, size, &mut MonospaceMeasurer);
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!((app.a.count, app.b.count), (1, 10));
    assert_eq!(context.find(&["a", "increment"]), Some(target));
    assert_eq!(
        context.node(target).unwrap().semantics.label,
        "Count 1",
        "the one declaration after the click already shows its result"
    );
}

fn custom_button(color: Color) -> CustomWidget {
    CustomWidget {
        size: Size::new(40.0, 40.0),
        semantics: Semantics {
            role: Role::Button,
            label: "Custom action".into(),
            focusable: true,
            ..Semantics::default()
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
        ..CustomWidget::default()
    }
}

// M35: `Ui::custom` has no message form (its handler is always empty), so a
// custom leaf's activation is only its `Response::clicked`. Stays on it.
#[test]
fn custom_leaf_shares_activation_and_updates_paint_without_layout() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("custom", |ui| {
            ui.custom(Style::default(), custom_button(Color::WHITE))
        });
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
            .keyed("custom", |ui| {
                ui.custom(Style::default(), custom_button(Color::rgb(0, 255, 0)))
            })
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
#[should_panic(expected = "custom widget pops a clip it never pushed")]
fn custom_widgets_cannot_pop_the_host_clip() {
    let mut widget = custom_button(Color::WHITE);
    widget.primitives.insert(0, Primitive::PopClip);
    frame(&mut Context::new(), |ui| {
        ui.keyed("invalid", |ui| ui.custom(Style::default(), widget));
    });
}

// M35: as above, `Ui::custom` has no message form. Stays on `clicked`.
#[test]
fn a_nonfocusable_custom_button_remains_pointer_activatable() {
    let mut context = Context::new();
    fn view<'a>(ui: &mut Ui<'a>) -> rust_gui_core::Response<'a> {
        let mut widget = custom_button(Color::WHITE);
        widget.semantics.focusable = false;
        ui.keyed("custom", |ui| ui.custom(Style::default(), widget))
    }
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
