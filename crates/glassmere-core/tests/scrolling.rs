use glassmere_core::{
    Action, Context, Direction, Event, Key, Length, MonospaceMeasurer, NodeId, Style, TextWrap, Ui,
};
use glassmere_core::{Button, Label};
use glassmere_scene::{Point, Primitive, Rect, Size};

const ROOT: Size = Size::new(600.0, 400.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

/// A 300 by 200 view over a row of three 300 px buttons, then a tall spacer.
fn wide(ui: &mut Ui<'_>) {
    ui.keyed("view", |ui| {
        ui.scroll(sized(300.0, 200.0), |ui| {
            ui.keyed("strip", |ui| {
                ui.row(Style::default().gap(0.0), |ui| {
                    for key in ["first", "second", "third"] {
                        ui.keyed(key, |ui| {
                            ui.button(Button::new(key).enabled(true).style(sized(300.0, 40.0)))
                        });
                    }
                })
            });
            ui.keyed("tall", |ui| {
                ui.label(Label::new("").style(sized(10.0, 500.0)))
            });
        })
    });
}

fn find(context: &Context, path: &[&str]) -> NodeId {
    context.find(path).unwrap()
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(find(context, path)).unwrap().bounds
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

fn thumbs(context: &Context) -> Vec<Rect> {
    let scrollbar = context.theme().scrollbar;
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Quad { rect, color, .. } if *color == scrollbar => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn content_wider_than_a_scroll_view_scrolls_sideways_with_the_wheel_and_is_clipped() {
    let mut context = Context::new();
    frame(&mut context, wide);
    let view = find(&context, &["view"]);
    let scroll = context.node(view).unwrap().scroll.unwrap();
    assert_eq!((scroll.maximum_x, scroll.offset_x), (600.0, 0.0));
    assert!(scroll.maximum > 0.0, "and it still scrolls down");

    context.event(Event::PointerMoved(Point { x: 150.0, y: 100.0 }));
    assert!(context.event(Event::Scroll {
        delta_x: 250.0,
        delta_y: 0.0
    }));
    frame(&mut context, wide);
    assert_eq!(bounds(&context, &["view", "strip", "first"]).x, -250.0);
    assert_eq!(bounds(&context, &["view", "strip", "second"]).x, 50.0);
    // Children never paint or take clicks outside the view.
    let second = context
        .node(find(&context, &["view", "strip", "second"]))
        .unwrap();
    assert_eq!((second.clip.x, second.clip.width), (50.0, 250.0));
    assert_eq!(context.hit_test(Point { x: 320.0, y: 20.0 }), None);
    assert_eq!(
        context.hit_test(Point { x: 20.0, y: 20.0 }),
        Some(find(&context, &["view", "strip", "first"]))
    );

    // One diagonal gesture moves both axes; offsets clamp at their ends.
    context.event(Event::Scroll {
        delta_x: 5000.0,
        delta_y: 30.0,
    });
    frame(&mut context, wide);
    let scroll = context.node(view).unwrap().scroll.unwrap();
    assert_eq!((scroll.offset_x, scroll.offset), (600.0, 30.0));
    assert!(!context.event(Event::Scroll {
        delta_x: 10.0,
        delta_y: 0.0
    }));
}

#[test]
fn both_scrollbars_show_only_where_there_is_somewhere_to_go() {
    let mut context = Context::new();
    frame(&mut context, wide);
    let bars = thumbs(&context);
    assert_eq!(bars.len(), 2);
    let across = bars.iter().find(|bar| bar.height == 4.0).unwrap();
    // A third of the content is in view, so the thumb is a third of the track.
    assert_eq!((across.x, across.y, across.width), (0.0, 196.0, 100.0));
    context.action(find(&context, &["view"]), Action::ScrollToX(600.0));
    frame(&mut context, wide);
    let across = thumbs(&context)
        .into_iter()
        .find(|bar| bar.height == 4.0)
        .unwrap();
    assert_eq!(across.x, 200.0);

    // Wrapped text and full-width children do not overflow: no sideways bar.
    let mut context = Context::new();
    let narrow = |ui: &mut Ui<'_>| {
        ui.keyed("view", |ui| {
            ui.scroll(sized(300.0, 200.0).padding(12.0), |ui| {
                ui.keyed("text", |ui| {
                    ui.label(
                        Label::new("a long paragraph that has to wrap inside the view ".repeat(8))
                            .style(
                                Style::default()
                                    .width(Length::Percent(1.0))
                                    .wrap(TextWrap::Word),
                            ),
                    )
                });
                ui.keyed("wide", |ui| {
                    ui.button(
                        Button::new("full width")
                            .enabled(true)
                            .style(Style::button().width(Length::Percent(1.0))),
                    )
                });
            })
        });
    };
    frame(&mut context, narrow);
    let scroll = context
        .node(find(&context, &["view"]))
        .unwrap()
        .scroll
        .unwrap();
    assert_eq!(scroll.maximum_x, 0.0);
    assert!(thumbs(&context).iter().all(|bar| bar.width == 4.0));
}

#[test]
fn arrow_keys_scroll_a_focused_view_and_focus_reveals_sideways() {
    let mut context = Context::new();
    frame(&mut context, wide);
    let view = find(&context, &["view"]);
    context.action(view, Action::Focus);
    key(&mut context, Key::ArrowRight);
    key(&mut context, Key::ArrowRight);
    key(&mut context, Key::ArrowLeft);
    frame(&mut context, wide);
    assert_eq!(context.node(view).unwrap().scroll.unwrap().offset_x, 40.0);

    // Focusing a button that is out of view brings all of it in.
    context.action(find(&context, &["view", "strip", "third"]), Action::Focus);
    frame(&mut context, wide);
    let third = bounds(&context, &["view", "strip", "third"]);
    assert_eq!((third.x, third.width), (0.0, 300.0));
    context.action(find(&context, &["view", "strip", "first"]), Action::Focus);
    frame(&mut context, wide);
    assert_eq!(bounds(&context, &["view", "strip", "first"]).x, 0.0);
}

#[test]
fn the_innermost_view_that_can_still_move_takes_the_wheel() {
    let mut context = Context::new();
    let nested = |ui: &mut Ui<'_>| {
        ui.keyed("outer", |ui| {
            ui.scroll(sized(400.0, 300.0), |ui| {
                ui.keyed("content", |ui| {
                    ui.row(Style::default().gap(0.0), |ui| {
                        ui.keyed("inner", |ui| {
                            ui.scroll(sized(200.0, 100.0), |ui| {
                                ui.keyed("long", |ui| {
                                    ui.label(Label::new("").style(sized(260.0, 20.0)))
                                });
                            })
                        });
                        ui.keyed("rest", |ui| {
                            ui.label(Label::new("").style(sized(600.0, 20.0)))
                        });
                    })
                });
            })
        });
    };
    frame(&mut context, nested);
    context.event(Event::PointerMoved(Point { x: 50.0, y: 50.0 }));
    let offsets = |context: &Context| {
        let offset = |path: &[&str]| {
            context
                .node(find(context, path))
                .unwrap()
                .scroll
                .unwrap()
                .offset_x
        };
        (offset(&["outer", "content", "inner"]), offset(&["outer"]))
    };
    context.event(Event::Scroll {
        delta_x: 50.0,
        delta_y: 0.0,
    });
    assert_eq!(offsets(&context), (50.0, 0.0));
    // The inner view has 10 px left; the gesture that exhausts it stays there,
    // and the next one moves the page.
    context.event(Event::Scroll {
        delta_x: 50.0,
        delta_y: 0.0,
    });
    assert_eq!(offsets(&context), (60.0, 0.0));
    context.event(Event::Scroll {
        delta_x: 50.0,
        delta_y: 0.0,
    });
    assert_eq!(offsets(&context), (60.0, 50.0));
}

#[test]
fn right_to_left_views_overflow_and_scroll_from_the_right() {
    let mut context = Context::new();
    context.set_direction(Direction::Rtl);
    frame(&mut context, wide);
    let view = find(&context, &["view"]);
    let scroll = context.node(view).unwrap().scroll.unwrap();
    assert_eq!((scroll.maximum_x, scroll.offset_x), (600.0, 0.0));
    // The first button sits at the start, which is the right edge of the view.
    let view_bounds = context.node(view).unwrap().bounds;
    let first = bounds(&context, &["view", "strip", "first"]);
    assert_eq!(first.x + first.width, view_bounds.x + view_bounds.width);
    let across = thumbs(&context)
        .into_iter()
        .find(|bar| bar.height == 4.0)
        .unwrap();
    assert_eq!(across.x + across.width, view_bounds.x + view_bounds.width);

    // Scrolling physically left goes toward the end of the content.
    context.action(view, Action::ScrollByX(-250.0));
    frame(&mut context, wide);
    assert_eq!(context.node(view).unwrap().scroll.unwrap().offset_x, 250.0);
    let first = bounds(&context, &["view", "strip", "first"]);
    assert_eq!(
        first.x + first.width,
        view_bounds.x + view_bounds.width + 250.0
    );
    // Revealing the last button scrolls to the far left of the content.
    context.action(find(&context, &["view", "strip", "third"]), Action::Focus);
    frame(&mut context, wide);
    assert_eq!(
        bounds(&context, &["view", "strip", "third"]).x,
        view_bounds.x
    );
}
