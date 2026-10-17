mod support;

use seaglass_core::app::Action;
use seaglass_core::{
    AreaBuilder, Button, Context, CustomWidget, Event, EventArea, Key, Length, Role, Semantics,
    Style, TextEdit, Ui, UiEvent, WidgetBuilder, WidgetInput,
};
use seaglass_scene::{Point, Size};
use support::{Driven, act, on};

const ROOT: Size = Size::new(300.0, 300.0);

#[derive(Default)]
struct Seen {
    outer_area: EventArea,
    inner_area: EventArea,
    outer: Vec<UiEvent>,
    inner: Vec<UiEvent>,
    clicks: usize,
    typed: String,
}

fn outer_event(event: UiEvent) -> Option<Action<Seen>> {
    Some(act(move |s: &mut Seen| s.outer.push(event.clone())))
}

fn inner_event(event: UiEvent) -> Option<Action<Seen>> {
    Some(act(move |s: &mut Seen| s.inner.push(event.clone())))
}

/// An outer area around an inner one, with a button and a field inside.
fn page(state: &Seen, ui: &mut Ui<'_, Action<Seen>>) {
    ui.keyed("outer", |ui| {
        ui.event_area(
            Style::default().padding(4.0),
            AreaBuilder::new(state.outer_area).on_event(outer_event),
            |ui| {
                ui.keyed("inner", |ui| {
                    ui.event_area(
                        Style::default().padding(4.0),
                        AreaBuilder::new(state.inner_area).on_event(inner_event),
                        |ui| {
                            ui.keyed("press", |ui| {
                                ui.button(
                                    Button::new("Press")
                                        .enabled(true)
                                        .style(
                                            Style::button()
                                                .width(Length::Px(120.0))
                                                .height(Length::Px(40.0)),
                                        )
                                        .on_click(act(|s: &mut Seen| s.clicks += 1)),
                                );
                            });
                            ui.keyed("field", |ui| {
                                ui.text_input(
                                    "Field",
                                    &state.typed,
                                    on(|s: &mut Seen, text: String| s.typed = text),
                                );
                            });
                        },
                    );
                });
            },
        );
    });
}

fn centre(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

fn click(context: &mut Context, at: Point) {
    context.event(Event::PointerMoved(at));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

#[test]
fn an_area_sees_what_happened_inside_it_after_the_widgets_did() {
    let watching = EventArea::pointer();
    let mut driven = Driven::new(
        ROOT,
        Seen {
            outer_area: watching,
            ..Seen::default()
        },
        page,
    );
    let button = centre(&driven.context, &["outer", "inner", "press"]);
    click(&mut driven.context, button);
    driven.frame();

    // The button still works, and the area was told what reached it.
    assert_eq!(driven.state().clicks, 1);
    assert_eq!(
        driven.state().outer,
        [
            UiEvent::PointerMoved(button),
            UiEvent::PointerDown(button),
            UiEvent::PointerUp(button),
        ]
    );
    // An area that watches nothing hears nothing.
    assert!(driven.state().inner.is_empty());
    // And what was reported is not reported again.
    let reported = driven.state().outer.len();
    driven.frame();
    assert_eq!(driven.state().outer.len(), reported);
}

#[test]
fn a_capturing_area_takes_the_event_before_the_widgets_and_the_inner_one() {
    let mut driven = Driven::new(
        ROOT,
        Seen {
            outer_area: EventArea::pointer().capturing().consuming(),
            inner_area: EventArea::pointer(),
            ..Seen::default()
        },
        page,
    );
    let button = centre(&driven.context, &["outer", "inner", "press"]);
    click(&mut driven.context, button);
    driven.frame();

    // Nothing below it saw anything: not the inner area, not the button.
    assert_eq!(driven.state().clicks, 0);
    assert!(driven.state().inner.is_empty());
    assert_eq!(driven.state().outer.len(), 3);
    assert_eq!(driven.context.hovered(), None, "not even the hover");
}

#[test]
fn the_innermost_area_answers_first_on_the_way_up_and_may_stop_there() {
    let mut driven = Driven::new(
        ROOT,
        Seen {
            outer_area: EventArea::pointer(),
            inner_area: EventArea::pointer().consuming(),
            ..Seen::default()
        },
        page,
    );
    let button = centre(&driven.context, &["outer", "inner", "press"]);
    click(&mut driven.context, button);
    driven.frame();

    // The widgets had it first, then the inner area kept it from the outer.
    assert_eq!(driven.state().clicks, 1);
    assert_eq!(driven.state().inner.len(), 3);
    assert!(driven.state().outer.is_empty());
}

#[test]
fn keys_and_text_follow_the_focus_and_scrolling_follows_the_pointer() {
    let mut driven = Driven::new(
        ROOT,
        Seen {
            outer_area: EventArea::all(),
            ..Seen::default()
        },
        page,
    );
    let field = driven.context.find(&["outer", "inner", "field"]).unwrap();
    driven.context.action(field, seaglass_core::Action::Focus);
    driven.context.event(Event::KeyDown {
        key: Key::ArrowDown,
        shift: true,
        repeat: false,
    });
    driven
        .context
        .event(Event::Text(TextEdit::Insert("x".into())));
    let inside = centre(&driven.context, &["outer", "inner"]);
    driven.context.event(Event::PointerMoved(inside));
    driven.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 12.0,
    });
    driven.frame();
    assert_eq!(driven.state().typed, "x", "the field still got it");
    assert!(driven.state().outer.contains(&UiEvent::Key {
        key: Key::ArrowDown,
        shift: true
    }));
    assert!(driven.state().outer.contains(&UiEvent::Text("x".into())));
    assert!(driven.state().outer.contains(&UiEvent::Scroll {
        delta_x: 0.0,
        delta_y: 12.0
    }));
}

#[derive(Default)]
struct Tell {
    heard: Vec<UiEvent>,
    chose: usize,
}

fn tell(_: &Tell, ui: &mut Ui<'_, Action<Tell>>) {
    ui.keyed("watch", |ui| {
        ui.event_area(
            Style::default(),
            AreaBuilder::new(EventArea::pointer()).on_event(|event: UiEvent| {
                Some(act(move |s: &mut Tell| s.heard.push(event.clone())))
            }),
            |ui| {
                ui.keyed("inside", |ui| {
                    ui.widget(
                        Style::default(),
                        WidgetBuilder::new(|_: &WidgetInput| CustomWidget {
                            size: Size::new(40.0, 20.0),
                            input: EventArea::pointer(),
                            ..CustomWidget::default()
                        })
                        .on_event(|event| match event {
                            UiEvent::PointerDown(_) => Some(act(|s: &mut Tell| s.chose += 1)),
                            _ => None,
                        }),
                    );
                });
            },
        );
    });
}

/// A widget inside an area sends its own message, and that message and the
/// area's events arrive in the same batch.
#[test]
fn a_widget_inside_an_area_sends_its_message_in_the_batch_the_area_hears_it() {
    let mut driven = Driven::new(ROOT, Tell::default(), tell);
    let widget = centre(&driven.context, &["watch", "inside"]);
    click(&mut driven.context, widget);
    driven.frame();
    assert_eq!(driven.state().chose, 1, "the widget's own message");
    assert_eq!(
        driven.state().heard.len(),
        3,
        "the area hears the same batch"
    );
}

#[test]
fn a_view_without_areas_dispatches_exactly_as_before() {
    let mut driven = Driven::new(
        ROOT,
        Seen::default(),
        |_: &Seen, ui: &mut Ui<'_, Action<Seen>>| {
            ui.keyed("press", |ui| {
                ui.button(Button::new("Press").on_click(act(|s: &mut Seen| s.clicks += 1)));
            });
        },
    );
    let button = centre(&driven.context, &["press"]);
    click(&mut driven.context, button);
    driven.frame();
    assert_eq!(driven.state().clicks, 1);
    assert!(driven.context.hovered().is_some());
}

#[derive(Default)]
struct Pressed {
    heard: Vec<UiEvent>,
}

fn pressable(_: &Pressed, ui: &mut Ui<'_, Action<Pressed>>) {
    ui.keyed("widget", |ui| {
        ui.widget(
            Style::default(),
            WidgetBuilder::new(|_: &WidgetInput| CustomWidget {
                size: Size::new(40.0, 20.0),
                semantics: Semantics {
                    role: Role::Button,
                    label: "Press".into(),
                    focusable: true,
                    ..Semantics::default()
                },
                input: EventArea::pointer(),
                ..CustomWidget::default()
            })
            .on_event(|event| Some(act(move |s: &mut Pressed| s.heard.push(event.clone())))),
        );
    });
}

/// Assistive technology's Activate on a widget with `on_event` is an event,
/// and so is a pointer click on it, after its pointer events.
#[test]
fn keys_and_assistive_technology_send_activate_and_a_pointer_click_does_not() {
    let mut driven = Driven::new(ROOT, Pressed::default(), pressable);
    let widget = driven.context.find(&["widget"]).unwrap();
    assert!(
        driven
            .context
            .action(widget, seaglass_core::Action::Activate)
    );
    driven.frame();
    assert_eq!(driven.state().heard, [UiEvent::Activate]);
    driven.state_mut().heard.clear();
    let point = centre(&driven.context, &["widget"]);
    click(&mut driven.context, point);
    driven.frame();
    let heard = &driven.state().heard;
    assert!(heard.iter().any(|e| matches!(e, UiEvent::PointerUp(_))));
    assert!(!heard.contains(&UiEvent::Activate), "{heard:?}");
}

/// Pointer moves that reach areas and widgets which send nothing for them
/// do not run the view: their handlers answer them without a declaration.
#[test]
fn pointer_moves_that_send_nothing_do_not_declare() {
    let mut driven = Driven::new(ROOT, (), |_, ui| {
        ui.keyed("area", |ui| {
            ui.event_area(
                Style::default()
                    .width(Length::Px(100.0))
                    .height(Length::Px(100.0)),
                AreaBuilder::new(EventArea::pointer()).on_event(|_| None),
                |_| {},
            );
        });
        ui.keyed("widget", |ui| {
            ui.widget(
                Style::default(),
                WidgetBuilder::new(|_: &WidgetInput| CustomWidget {
                    size: Size::new(100.0, 100.0),
                    input: EventArea::pointer(),
                    ..CustomWidget::default()
                }),
            );
        });
    });
    for path in ["area", "widget"] {
        let at = centre(&driven.context, &[path]);
        for dx in [0.0, 5.0, 10.0] {
            driven.context.event(Event::PointerMoved(Point {
                x: at.x + dx,
                y: at.y,
            }));
            let stats = driven.frame();
            assert!(!stats.declared, "{path}: a move declared");
        }
    }
}
