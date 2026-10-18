mod support;

use seaglass_core::app::Action;
use seaglass_core::{Button, GestureBuilder, Label};
use seaglass_core::{Context, Event, GestureResponse, Length, MenuItem, Style, TouchPhase, Ui};
use seaglass_scene::{Point, Size};
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;
use support::{Driven, act, on};

const ROOT: Size = Size::new(600.0, 500.0);

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

fn at(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn touch(context: &mut Context, id: u64, phase: TouchPhase, position: Point) -> bool {
    context.event(Event::Touch {
        id,
        phase,
        position,
        force: Some(0.25 * id as f32),
        pen: None,
    })
}

#[derive(Default)]
struct Canvas {
    gesture: GestureResponse,
    /// How many gesture messages arrived, so that a frame's report is known.
    reports: usize,
    clicks: usize,
    menu: Option<usize>,
    with_menu: bool,
}

/// A 40 px strip, then a 400 by 300 canvas with a button in its corner,
/// inside a scroll view that could pan.
fn view(state: &Canvas, ui: &mut Ui<'_, Action<Canvas>>) {
    ui.keyed("view", |ui| {
        ui.scroll(sized(500.0, 400.0), |ui| {
            ui.keyed("strip", |ui| {
                ui.label(Label::new("").style(sized(10.0, 40.0)));
            });
            let items = [MenuItem::new("Reset")];
            let canvas = |ui: &mut Ui<'_, Action<Canvas>>| {
                ui.keyed("canvas", |ui| {
                    ui.gesture_area(
                        sized(400.0, 300.0),
                        GestureBuilder::new().on_gesture(on(
                            |s: &mut Canvas, gesture: GestureResponse| {
                                s.gesture = gesture;
                                s.reports += 1;
                            },
                        )),
                        |ui| {
                            ui.keyed("inside", |ui| {
                                ui.button(
                                    Button::new("Inside")
                                        .enabled(true)
                                        .style(sized(100.0, 40.0))
                                        .on_click(act(|s: &mut Canvas| s.clicks += 1)),
                                );
                            });
                        },
                    );
                });
            };
            if state.with_menu {
                ui.keyed("menu", |ui| {
                    ui.context_menu(
                        &items,
                        Style::default(),
                        on(|s: &mut Canvas, index: usize| s.menu = Some(index)),
                        canvas,
                    );
                });
            } else {
                canvas(ui);
            }
            ui.keyed("tall", |ui| {
                ui.label(Label::new("").style(sized(10.0, 900.0)));
            });
        });
    });
}

/// One frame, and the gesture report it carried: the default when none came.
fn frame(driven: &mut Driven<Canvas>) -> GestureResponse {
    let before = driven.state().reports;
    driven.frame();
    if driven.state().reports > before {
        driven.state().gesture
    } else {
        GestureResponse::default()
    }
}

fn offset(context: &Context) -> f32 {
    let view = context.find(&["view"]).unwrap();
    context.node(view).unwrap().scroll.unwrap().offset
}

/// A window point as the canvas reports it: from its own top left corner.
fn local(context: &Context, point: Point) -> Option<Point> {
    let canvas = context.find(&["view", "canvas"]).unwrap();
    let bounds = context.node(canvas).unwrap().bounds;
    Some(at(point.x - bounds.x, point.y - bounds.y))
}

fn close(left: f32, right: f32) -> bool {
    (left - right).abs() < 1e-3
}

#[test]
fn one_finger_pans_after_the_slop_and_a_still_one_taps() {
    let mut driven = Driven::new(ROOT, Canvas::default(), view);
    assert_eq!(frame(&mut driven), GestureResponse::default());

    // Fingers down are reported at once, with nothing moved yet.
    touch(
        &mut driven.context,
        1,
        TouchPhase::Started,
        at(200.0, 200.0),
    );
    let gesture = frame(&mut driven);
    assert!(gesture.active && !gesture.changed());
    assert_eq!(gesture.pressure, Some(0.25));
    touch(&mut driven.context, 1, TouchPhase::Moved, at(204.0, 203.0));
    assert!(!frame(&mut driven).changed(), "inside the slop");
    assert!(driven.state().gesture.active);
    // Past the slop the picture catches up with the finger.
    touch(&mut driven.context, 1, TouchPhase::Moved, at(230.0, 240.0));
    touch(&mut driven.context, 1, TouchPhase::Moved, at(250.0, 220.0));
    let gesture = frame(&mut driven);
    assert!(gesture.active);
    assert_eq!((gesture.pan_x, gesture.pan_y), (50.0, 20.0));
    assert_eq!(
        gesture.center,
        local(&driven.context, at(250.0, 220.0)),
        "from the canvas corner"
    );
    assert_eq!((gesture.scale, gesture.rotation), (1.0, 0.0));
    // Reported once, and the scroll view under the canvas stayed put.
    assert!(!frame(&mut driven).changed());
    assert_eq!(offset(&driven.context), 0.0);
    touch(&mut driven.context, 1, TouchPhase::Ended, at(250.0, 220.0));
    let gesture = frame(&mut driven);
    assert!(!gesture.active, "lifting is reported");
    assert_eq!(gesture.tapped, None, "a pan is not a tap");
    assert!(
        !driven.context.animating(),
        "and it does not fling anything"
    );

    touch(
        &mut driven.context,
        1,
        TouchPhase::Started,
        at(300.0, 100.0),
    );
    frame(&mut driven);
    touch(&mut driven.context, 1, TouchPhase::Ended, at(301.0, 100.0));
    let gesture = frame(&mut driven);
    assert!(!gesture.active);
    assert_eq!(gesture.tapped, local(&driven.context, at(301.0, 100.0)));
    assert_eq!(frame(&mut driven).tapped, None);
}

#[test]
fn two_fingers_scale_turn_and_move_about_the_point_between_them() {
    let mut driven = Driven::new(ROOT, Canvas::default(), view);
    touch(
        &mut driven.context,
        1,
        TouchPhase::Started,
        at(150.0, 200.0),
    );
    touch(
        &mut driven.context,
        2,
        TouchPhase::Started,
        at(250.0, 200.0),
    );
    // Apart: from 100 px to 200 px, the middle stays.
    touch(&mut driven.context, 1, TouchPhase::Moved, at(125.0, 200.0));
    touch(&mut driven.context, 2, TouchPhase::Moved, at(275.0, 200.0));
    touch(&mut driven.context, 1, TouchPhase::Moved, at(100.0, 200.0));
    touch(&mut driven.context, 2, TouchPhase::Moved, at(300.0, 200.0));
    let gesture = frame(&mut driven);
    assert!(close(gesture.scale, 2.0), "{gesture:?}");
    assert!(close(gesture.pan_x, 0.0) && close(gesture.pan_y, 0.0));
    assert!(close(gesture.rotation, 0.0));
    assert_eq!(gesture.center, local(&driven.context, at(200.0, 200.0)));
    assert_eq!(gesture.pressure, Some(0.5), "the firmer of the two");

    // A quarter turn clockwise: the right finger goes below the left one.
    touch(&mut driven.context, 2, TouchPhase::Moved, at(100.0, 400.0));
    let gesture = frame(&mut driven);
    assert!(close(gesture.rotation, FRAC_PI_2), "{gesture:?}");
    assert!(close(gesture.scale, 1.0));
    assert_eq!((gesture.pan_x, gesture.pan_y), (-100.0, 100.0));

    // Both fingers the same way: a pan and nothing else.
    touch(&mut driven.context, 1, TouchPhase::Moved, at(130.0, 200.0));
    touch(&mut driven.context, 2, TouchPhase::Moved, at(130.0, 400.0));
    let gesture = frame(&mut driven);
    assert!(close(gesture.pan_x, 30.0) && close(gesture.pan_y, 0.0));
    assert!(close(gesture.scale, 1.0), "{gesture:?}");
    assert!(gesture.rotation.abs() < 1e-3);

    // One finger lifts without a jump, and nothing counts as a tap.
    touch(&mut driven.context, 2, TouchPhase::Ended, at(130.0, 400.0));
    touch(&mut driven.context, 1, TouchPhase::Moved, at(140.0, 200.0));
    touch(&mut driven.context, 1, TouchPhase::Ended, at(140.0, 200.0));
    let gesture = frame(&mut driven);
    assert_eq!((gesture.pan_x, gesture.pan_y), (10.0, 0.0));
    assert_eq!(gesture.tapped, None);
    assert!(!gesture.active);
}

#[test]
fn controls_inside_are_pressed_and_touches_outside_pan_the_page() {
    let mut driven = Driven::new(ROOT, Canvas::default(), view);
    let inside = driven.context.find(&["view", "canvas", "inside"]).unwrap();
    let bounds = driven.context.node(inside).unwrap().bounds;
    let button = at(bounds.x + 20.0, bounds.y + 20.0);
    touch(&mut driven.context, 1, TouchPhase::Started, button);
    touch(&mut driven.context, 1, TouchPhase::Ended, button);
    let gesture = frame(&mut driven);
    assert_eq!(driven.state().clicks, 1);
    assert!(!gesture.changed() && gesture.tapped.is_none());

    // The strip above the canvas belongs to the scroll view.
    touch(
        &mut driven.context,
        1,
        TouchPhase::Started,
        at(450.0, 380.0),
    );
    touch(&mut driven.context, 1, TouchPhase::Moved, at(450.0, 340.0));
    touch(
        &mut driven.context,
        1,
        TouchPhase::Cancelled,
        at(450.0, 340.0),
    );
    let gesture = frame(&mut driven);
    assert_eq!(offset(&driven.context), 40.0);
    assert!(!gesture.changed());
}

#[test]
fn a_trackpad_pinch_or_ctrl_wheel_scales_about_the_pointer_and_falls_through_elsewhere() {
    let mut driven = Driven::new(ROOT, Canvas::default(), view);
    assert!(!driven.context.event(Event::Zoom(1.1)), "no pointer yet");
    driven.context.event(Event::PointerMoved(at(120.0, 140.0)));
    assert!(driven.context.event(Event::Zoom(1.1)));
    assert!(driven.context.event(Event::Zoom(1.1)));
    assert!(driven.context.event(Event::Rotate(0.5)));
    let gesture = frame(&mut driven);
    assert!(close(gesture.scale, 1.21));
    assert!(close(gesture.rotation, 0.5));
    assert_eq!(gesture.center, local(&driven.context, at(120.0, 140.0)));
    assert!(!gesture.active, "no finger is down");
    // Nonsense from a driver is refused.
    assert!(!driven.context.event(Event::Zoom(0.0)));
    assert!(!driven.context.event(Event::Zoom(f32::NAN)));
    // Outside the canvas the host keeps the wheel for scrolling.
    driven.context.event(Event::PointerMoved(at(450.0, 20.0)));
    assert!(!driven.context.event(Event::Zoom(1.1)));
    let gesture = frame(&mut driven);
    assert!(!gesture.changed());
}

#[test]
fn resting_on_a_canvas_opens_its_menu_or_else_keeps_the_finger_for_a_pan() {
    let mut driven = Driven::new(ROOT, Canvas::default(), view);
    driven.context.set_now(Duration::ZERO);
    touch(
        &mut driven.context,
        1,
        TouchPhase::Started,
        at(200.0, 200.0),
    );
    assert_eq!(driven.context.next_wake(), Some(Duration::from_millis(500)));
    driven.context.set_now(Duration::from_millis(600));
    frame(&mut driven);
    assert_eq!(driven.context.next_wake(), None, "asked once");
    // No menu here, so the rested finger still pans.
    touch(&mut driven.context, 1, TouchPhase::Moved, at(200.0, 260.0));
    touch(&mut driven.context, 1, TouchPhase::Ended, at(200.0, 260.0));
    let gesture = frame(&mut driven);
    assert_eq!(gesture.pan_y, 60.0);

    let mut driven = Driven::new(
        ROOT,
        Canvas {
            with_menu: true,
            ..Canvas::default()
        },
        view,
    );
    driven.context.set_now(Duration::ZERO);
    touch(
        &mut driven.context,
        1,
        TouchPhase::Started,
        at(200.0, 200.0),
    );
    driven.context.set_now(Duration::from_millis(600));
    frame(&mut driven);
    frame(&mut driven);
    assert!(driven.context.find(&["view", "menu", "$popup"]).is_some());
    touch(&mut driven.context, 1, TouchPhase::Moved, at(200.0, 260.0));
    touch(&mut driven.context, 1, TouchPhase::Ended, at(200.0, 260.0));
    let gesture = frame(&mut driven);
    assert!(!gesture.changed() && gesture.tapped.is_none());
}

#[test]
fn a_second_tap_and_a_flick_are_told_apart_from_a_tap_and_a_pan() {
    let mut driven = Driven::new(ROOT, Canvas::default(), view);
    let spot = at(200.0, 200.0);
    let tap = |context: &mut Context, at: Point, when: u64| {
        context.set_now(Duration::from_millis(when));
        touch(context, 1, TouchPhase::Started, at);
        touch(context, 1, TouchPhase::Ended, at);
    };
    tap(&mut driven.context, spot, 1000);
    let gesture = frame(&mut driven);
    assert!(gesture.tapped.is_some() && gesture.double_tapped.is_none());

    // Soon after and near enough: a double tap, which is a tap as well.
    tap(&mut driven.context, at(spot.x + 4.0, spot.y), 1200);
    let gesture = frame(&mut driven);
    assert!(gesture.tapped.is_some() && gesture.double_tapped.is_some());
    // A third does not pair with the second.
    tap(&mut driven.context, spot, 1300);
    let gesture = frame(&mut driven);
    assert!(gesture.double_tapped.is_none());
    // Too late, and too far.
    tap(&mut driven.context, spot, 2000);
    frame(&mut driven);
    tap(&mut driven.context, at(spot.x + 60.0, spot.y), 2100);
    let gesture = frame(&mut driven);
    assert!(gesture.double_tapped.is_none());

    // A flick: far and fast in one direction.
    driven.context.set_now(Duration::from_millis(3000));
    touch(&mut driven.context, 1, TouchPhase::Started, spot);
    for step in 1..=4u64 {
        driven
            .context
            .set_now(Duration::from_millis(3000 + step * 10));
        touch(
            &mut driven.context,
            1,
            TouchPhase::Moved,
            at(spot.x - 30.0 * step as f32, spot.y),
        );
    }
    touch(
        &mut driven.context,
        1,
        TouchPhase::Ended,
        at(spot.x - 120.0, spot.y),
    );
    let gesture = frame(&mut driven);
    let swipe = gesture.swipe.expect("a swipe");
    assert_eq!(swipe.direction, seaglass_core::SwipeDirection::Left);
    assert!(swipe.speed > 1000.0, "{swipe:?}");
    assert!(gesture.tapped.is_none(), "a flick is not a tap");

    // A slow drag over the same distance is a pan, not a swipe.
    driven.context.set_now(Duration::from_millis(4000));
    touch(&mut driven.context, 1, TouchPhase::Started, spot);
    for step in 1..=4u64 {
        driven
            .context
            .set_now(Duration::from_millis(4000 + step * 200));
        touch(
            &mut driven.context,
            1,
            TouchPhase::Moved,
            at(spot.x - 30.0 * step as f32, spot.y),
        );
    }
    touch(
        &mut driven.context,
        1,
        TouchPhase::Ended,
        at(spot.x - 120.0, spot.y),
    );
    let gesture = frame(&mut driven);
    assert!(gesture.swipe.is_none());
    assert!(gesture.pan_x < -100.0, "it panned instead");
}
