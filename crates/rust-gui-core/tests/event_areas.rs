use rust_gui_core::Button;
use rust_gui_core::{
    Context, Event, EventArea, Key, Length, MonospaceMeasurer, Style, TextEdit, UiEvent,
};
use rust_gui_scene::{Point, Size};

const ROOT: Size = Size::new(300.0, 300.0);

#[derive(Default)]
struct Seen {
    outer: Vec<UiEvent>,
    inner: Vec<UiEvent>,
    clicks: usize,
    typed: String,
}

/// An outer area around an inner one, with a button and a field inside.
fn page(context: &mut Context, seen: &mut Seen, outer: EventArea, inner: EventArea) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        seen.outer = ui.keyed("outer", |ui| {
            ui.event_area(outer, Style::default().padding(4.0), |ui| {
                seen.inner = ui.keyed("inner", |ui| {
                    ui.event_area(inner, Style::default().padding(4.0), |ui| {
                        if ui
                            .keyed("press", |ui| {
                                ui.button(
                                    Button::new("Press").enabled(true).style(
                                        Style::button()
                                            .width(Length::Px(120.0))
                                            .height(Length::Px(40.0)),
                                    ),
                                )
                            })
                            .clicked()
                        {
                            seen.clicks += 1;
                        }
                        ui.keyed("field", |ui| ui.text_input("Field", &mut seen.typed));
                    })
                });
            })
        });
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
    let (mut context, mut seen) = (Context::new(), Seen::default());
    let watching = EventArea::pointer();
    page(&mut context, &mut seen, watching, EventArea::default());
    let button = centre(&context, &["outer", "inner", "press"]);
    click(&mut context, button);
    page(&mut context, &mut seen, watching, EventArea::default());

    // The button still works, and the area was told what reached it.
    assert_eq!(seen.clicks, 1);
    assert_eq!(
        seen.outer,
        [
            UiEvent::PointerMoved(button),
            UiEvent::PointerDown(button),
            UiEvent::PointerUp(button),
        ]
    );
    // An area that watches nothing hears nothing.
    assert!(seen.inner.is_empty());
    // And what was reported is not reported again.
    page(&mut context, &mut seen, watching, EventArea::default());
    assert!(seen.outer.is_empty());
}

#[test]
fn a_capturing_area_takes_the_event_before_the_widgets_and_the_inner_one() {
    let (mut context, mut seen) = (Context::new(), Seen::default());
    let blocking = EventArea::pointer().capturing().consuming();
    let inner = EventArea::pointer();
    page(&mut context, &mut seen, blocking, inner);
    let button = centre(&context, &["outer", "inner", "press"]);
    click(&mut context, button);
    page(&mut context, &mut seen, blocking, inner);

    // Nothing below it saw anything: not the inner area, not the button.
    assert_eq!(seen.clicks, 0);
    assert!(seen.inner.is_empty());
    assert_eq!(seen.outer.len(), 3);
    assert_eq!(context.hovered(), None, "not even the hover");
}

#[test]
fn the_innermost_area_answers_first_on_the_way_up_and_may_stop_there() {
    let (mut context, mut seen) = (Context::new(), Seen::default());
    let outer = EventArea::pointer();
    let inner = EventArea::pointer().consuming();
    page(&mut context, &mut seen, outer, inner);
    let button = centre(&context, &["outer", "inner", "press"]);
    click(&mut context, button);
    page(&mut context, &mut seen, outer, inner);

    // The widgets had it first, then the inner area kept it from the outer.
    assert_eq!(seen.clicks, 1);
    assert_eq!(seen.inner.len(), 3);
    assert!(seen.outer.is_empty());
}

#[test]
fn keys_and_text_follow_the_focus_and_scrolling_follows_the_pointer() {
    let (mut context, mut seen) = (Context::new(), Seen::default());
    let watching = EventArea::all();
    page(&mut context, &mut seen, watching, EventArea::default());
    let field = context.find(&["outer", "inner", "field"]).unwrap();
    context.action(field, rust_gui_core::Action::Focus);
    context.event(Event::KeyDown {
        key: Key::ArrowDown,
        shift: true,
        repeat: false,
    });
    context.event(Event::Text(TextEdit::Insert("x".into())));
    context.event(Event::PointerMoved(centre(&context, &["outer", "inner"])));
    context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 12.0,
    });
    page(&mut context, &mut seen, watching, EventArea::default());
    assert_eq!(seen.typed, "x", "the field still got it");
    assert!(seen.outer.contains(&UiEvent::Key {
        key: Key::ArrowDown,
        shift: true
    }));
    assert!(seen.outer.contains(&UiEvent::Text("x".into())));
    assert!(seen.outer.contains(&UiEvent::Scroll {
        delta_x: 0.0,
        delta_y: 12.0
    }));
}

#[test]
fn a_widget_tells_the_area_above_it_something_of_its_own() {
    let mut context = Context::new();
    let (mut heard, mut tick) = (Vec::new(), 0);
    let declare = |context: &mut Context, heard: &mut Vec<UiEvent>, tick: &mut usize| {
        *tick += 1;
        let sending = *tick;
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            *heard = ui.keyed("watch", |ui| {
                ui.event_area(EventArea::custom(), Style::default(), |ui| {
                    ui.keyed("inside", |ui| {
                        ui.column(Style::default(), |ui| {
                            ui.emit("chose", format!("tick {sending}"));
                        })
                    });
                })
            });
        });
        // A declaration that carries an emitted event asks for another.
        context.stats().activations
    };
    let sent = |tick: usize| UiEvent::Custom {
        name: "chose".into(),
        data: format!("tick {tick}"),
    };
    // Nothing is emitted from a node that does not exist yet, and there is no
    // area to hear it on that frame either.
    assert_eq!(declare(&mut context, &mut heard, &mut tick), 0);
    assert!(heard.is_empty());
    assert!(declare(&mut context, &mut heard, &mut tick) > 0);
    assert!(heard.is_empty(), "it arrives on the next declaration");
    declare(&mut context, &mut heard, &mut tick);
    assert_eq!(heard, [sent(2)]);
    declare(&mut context, &mut heard, &mut tick);
    assert_eq!(heard, [sent(3)], "and each one only once");
}

#[test]
fn a_view_without_areas_dispatches_exactly_as_before() {
    let (mut context, mut clicks) = (Context::new(), 0);
    let declare = |context: &mut Context, clicks: &mut usize| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            if ui.keyed("press", |ui| ui.button("Press")).clicked() {
                *clicks += 1;
            }
        });
    };
    declare(&mut context, &mut clicks);
    let button = centre(&context, &["press"]);
    click(&mut context, button);
    declare(&mut context, &mut clicks);
    assert_eq!(clicks, 1);
    assert!(context.hovered().is_some());
}
