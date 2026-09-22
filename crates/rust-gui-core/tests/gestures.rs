use rust_gui_core::{
    Context, Event, GestureResponse, Length, MenuItem, MonospaceMeasurer, Style, TouchPhase, Ui,
};
use rust_gui_scene::{Point, Size};
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

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
struct Page {
    gesture: GestureResponse,
    clicks: usize,
    menu: Option<usize>,
    with_menu: bool,
}

/// A 40 px strip, then a 400 by 300 canvas with a button in its corner,
/// inside a scroll view that could pan.
fn page(context: &mut Context, page: &mut Page) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.scroll("view", sized(500.0, 400.0), |ui| {
            ui.label_styled("strip", "", sized(10.0, 40.0));
            let items = [MenuItem::new("Reset")];
            let mut canvas = |ui: &mut Ui<'_>| {
                page.gesture = ui.gesture_area("canvas", sized(400.0, 300.0), |ui| {
                    if ui
                        .button_styled("inside", "Inside", true, sized(100.0, 40.0))
                        .clicked()
                    {
                        page.clicks += 1;
                    }
                });
            };
            if page.with_menu {
                page.menu = ui.context_menu("menu", &items, Style::default(), canvas);
            } else {
                canvas(ui);
            }
            ui.label_styled("tall", "", sized(10.0, 900.0));
        });
    });
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
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    assert_eq!(state.gesture, GestureResponse::default());
    assert!(!state.gesture.changed());

    touch(&mut context, 1, TouchPhase::Started, at(200.0, 200.0));
    touch(&mut context, 1, TouchPhase::Moved, at(204.0, 203.0));
    page(&mut context, &mut state);
    assert!(state.gesture.active && !state.gesture.changed());
    assert_eq!(state.gesture.pressure, Some(0.25));
    // Past the slop the picture catches up with the finger.
    touch(&mut context, 1, TouchPhase::Moved, at(230.0, 240.0));
    touch(&mut context, 1, TouchPhase::Moved, at(250.0, 220.0));
    page(&mut context, &mut state);
    assert_eq!((state.gesture.pan_x, state.gesture.pan_y), (50.0, 20.0));
    assert_eq!(
        state.gesture.center,
        local(&context, at(250.0, 220.0)),
        "from the canvas corner"
    );
    assert_eq!((state.gesture.scale, state.gesture.rotation), (1.0, 0.0));
    // Reported once, and the scroll view under the canvas stayed put.
    page(&mut context, &mut state);
    assert!(!state.gesture.changed() && state.gesture.active);
    assert_eq!(offset(&context), 0.0);
    touch(&mut context, 1, TouchPhase::Ended, at(250.0, 220.0));
    page(&mut context, &mut state);
    assert!(!state.gesture.active);
    assert_eq!(state.gesture.tapped, None, "a pan is not a tap");
    assert!(!context.animating(), "and it does not fling anything");

    touch(&mut context, 1, TouchPhase::Started, at(300.0, 100.0));
    touch(&mut context, 1, TouchPhase::Ended, at(301.0, 100.0));
    page(&mut context, &mut state);
    assert_eq!(state.gesture.tapped, local(&context, at(301.0, 100.0)));
    page(&mut context, &mut state);
    assert_eq!(state.gesture.tapped, None);
}

#[test]
fn two_fingers_scale_turn_and_move_about_the_point_between_them() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    touch(&mut context, 1, TouchPhase::Started, at(150.0, 200.0));
    touch(&mut context, 2, TouchPhase::Started, at(250.0, 200.0));
    // Apart: from 100 px to 200 px, the middle stays.
    touch(&mut context, 1, TouchPhase::Moved, at(125.0, 200.0));
    touch(&mut context, 2, TouchPhase::Moved, at(275.0, 200.0));
    touch(&mut context, 1, TouchPhase::Moved, at(100.0, 200.0));
    touch(&mut context, 2, TouchPhase::Moved, at(300.0, 200.0));
    page(&mut context, &mut state);
    assert!(close(state.gesture.scale, 2.0), "{:?}", state.gesture);
    assert!(close(state.gesture.pan_x, 0.0) && close(state.gesture.pan_y, 0.0));
    assert!(close(state.gesture.rotation, 0.0));
    assert_eq!(state.gesture.center, local(&context, at(200.0, 200.0)));
    assert_eq!(state.gesture.pressure, Some(0.5), "the firmer of the two");

    // A quarter turn clockwise: the right finger goes below the left one.
    touch(&mut context, 2, TouchPhase::Moved, at(100.0, 400.0));
    page(&mut context, &mut state);
    assert!(
        close(state.gesture.rotation, FRAC_PI_2),
        "{:?}",
        state.gesture
    );
    assert!(close(state.gesture.scale, 1.0));
    assert_eq!((state.gesture.pan_x, state.gesture.pan_y), (-100.0, 100.0));

    // Both fingers the same way: a pan and nothing else.
    touch(&mut context, 1, TouchPhase::Moved, at(130.0, 200.0));
    touch(&mut context, 2, TouchPhase::Moved, at(130.0, 400.0));
    page(&mut context, &mut state);
    assert!(close(state.gesture.pan_x, 30.0) && close(state.gesture.pan_y, 0.0));
    assert!(close(state.gesture.scale, 1.0), "{:?}", state.gesture);
    assert!(state.gesture.rotation.abs() < 1e-3);

    // One finger lifts without a jump, and nothing counts as a tap.
    touch(&mut context, 2, TouchPhase::Ended, at(130.0, 400.0));
    touch(&mut context, 1, TouchPhase::Moved, at(140.0, 200.0));
    touch(&mut context, 1, TouchPhase::Ended, at(140.0, 200.0));
    page(&mut context, &mut state);
    assert_eq!((state.gesture.pan_x, state.gesture.pan_y), (10.0, 0.0));
    assert_eq!(state.gesture.tapped, None);
    assert!(!state.gesture.active);
}

#[test]
fn controls_inside_are_pressed_and_touches_outside_pan_the_page() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let inside = context.find(&["view", "canvas", "inside"]).unwrap();
    let bounds = context.node(inside).unwrap().bounds;
    let button = at(bounds.x + 20.0, bounds.y + 20.0);
    touch(&mut context, 1, TouchPhase::Started, button);
    touch(&mut context, 1, TouchPhase::Ended, button);
    page(&mut context, &mut state);
    assert_eq!(state.clicks, 1);
    assert!(!state.gesture.changed() && state.gesture.tapped.is_none());

    // The strip above the canvas belongs to the scroll view.
    touch(&mut context, 1, TouchPhase::Started, at(450.0, 380.0));
    touch(&mut context, 1, TouchPhase::Moved, at(450.0, 340.0));
    touch(&mut context, 1, TouchPhase::Cancelled, at(450.0, 340.0));
    page(&mut context, &mut state);
    assert_eq!(offset(&context), 40.0);
    assert!(!state.gesture.changed());
}

#[test]
fn a_trackpad_pinch_or_ctrl_wheel_scales_about_the_pointer_and_falls_through_elsewhere() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    assert!(!context.event(Event::Zoom(1.1)), "no pointer yet");
    context.event(Event::PointerMoved(at(120.0, 140.0)));
    assert!(context.event(Event::Zoom(1.1)));
    assert!(context.event(Event::Zoom(1.1)));
    assert!(context.event(Event::Rotate(0.5)));
    page(&mut context, &mut state);
    assert!(close(state.gesture.scale, 1.21));
    assert!(close(state.gesture.rotation, 0.5));
    assert_eq!(state.gesture.center, local(&context, at(120.0, 140.0)));
    assert!(!state.gesture.active, "no finger is down");
    // Nonsense from a driver is refused.
    assert!(!context.event(Event::Zoom(0.0)));
    assert!(!context.event(Event::Zoom(f32::NAN)));
    // Outside the canvas the host keeps the wheel for scrolling.
    context.event(Event::PointerMoved(at(450.0, 20.0)));
    assert!(!context.event(Event::Zoom(1.1)));
    page(&mut context, &mut state);
    assert!(!state.gesture.changed());
}

#[test]
fn resting_on_a_canvas_opens_its_menu_or_else_keeps_the_finger_for_a_pan() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    context.set_now(Duration::ZERO);
    touch(&mut context, 1, TouchPhase::Started, at(200.0, 200.0));
    assert_eq!(context.next_wake(), Some(Duration::from_millis(500)));
    context.set_now(Duration::from_millis(600));
    page(&mut context, &mut state);
    assert_eq!(context.next_wake(), None, "asked once");
    // No menu here, so the rested finger still pans.
    touch(&mut context, 1, TouchPhase::Moved, at(200.0, 260.0));
    touch(&mut context, 1, TouchPhase::Ended, at(200.0, 260.0));
    page(&mut context, &mut state);
    assert_eq!(state.gesture.pan_y, 60.0);

    let (mut context, mut state) = (Context::new(), Page::default());
    state.with_menu = true;
    page(&mut context, &mut state);
    context.set_now(Duration::ZERO);
    touch(&mut context, 1, TouchPhase::Started, at(200.0, 200.0));
    context.set_now(Duration::from_millis(600));
    page(&mut context, &mut state);
    page(&mut context, &mut state);
    assert!(context.find(&["view", "menu", "$popup"]).is_some());
    touch(&mut context, 1, TouchPhase::Moved, at(200.0, 260.0));
    touch(&mut context, 1, TouchPhase::Ended, at(200.0, 260.0));
    page(&mut context, &mut state);
    assert!(!state.gesture.changed() && state.gesture.tapped.is_none());
}

#[test]
fn a_second_tap_and_a_flick_are_told_apart_from_a_tap_and_a_pan() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let spot = at(200.0, 200.0);
    let tap = |context: &mut Context, at: Point, when: u64| {
        context.set_now(Duration::from_millis(when));
        touch(context, 1, TouchPhase::Started, at);
        touch(context, 1, TouchPhase::Ended, at);
    };
    tap(&mut context, spot, 1000);
    page(&mut context, &mut state);
    assert!(state.gesture.tapped.is_some() && state.gesture.double_tapped.is_none());

    // Soon after and near enough: a double tap, which is a tap as well.
    tap(&mut context, at(spot.x + 4.0, spot.y), 1200);
    page(&mut context, &mut state);
    assert!(state.gesture.tapped.is_some() && state.gesture.double_tapped.is_some());
    // A third does not pair with the second.
    tap(&mut context, spot, 1300);
    page(&mut context, &mut state);
    assert!(state.gesture.double_tapped.is_none());
    // Too late, and too far.
    tap(&mut context, spot, 2000);
    page(&mut context, &mut state);
    tap(&mut context, at(spot.x + 60.0, spot.y), 2100);
    page(&mut context, &mut state);
    assert!(state.gesture.double_tapped.is_none());

    // A flick: far and fast in one direction.
    context.set_now(Duration::from_millis(3000));
    touch(&mut context, 1, TouchPhase::Started, spot);
    for step in 1..=4u64 {
        context.set_now(Duration::from_millis(3000 + step * 10));
        touch(
            &mut context,
            1,
            TouchPhase::Moved,
            at(spot.x - 30.0 * step as f32, spot.y),
        );
    }
    touch(
        &mut context,
        1,
        TouchPhase::Ended,
        at(spot.x - 120.0, spot.y),
    );
    page(&mut context, &mut state);
    let swipe = state.gesture.swipe.expect("a swipe");
    assert_eq!(swipe.direction, rust_gui_core::SwipeDirection::Left);
    assert!(swipe.speed > 1000.0, "{swipe:?}");
    assert!(state.gesture.tapped.is_none(), "a flick is not a tap");

    // A slow drag over the same distance is a pan, not a swipe.
    context.set_now(Duration::from_millis(4000));
    touch(&mut context, 1, TouchPhase::Started, spot);
    for step in 1..=4u64 {
        context.set_now(Duration::from_millis(4000 + step * 200));
        touch(
            &mut context,
            1,
            TouchPhase::Moved,
            at(spot.x - 30.0 * step as f32, spot.y),
        );
    }
    touch(
        &mut context,
        1,
        TouchPhase::Ended,
        at(spot.x - 120.0, spot.y),
    );
    page(&mut context, &mut state);
    assert!(state.gesture.swipe.is_none());
    assert!(state.gesture.pan_x < -100.0, "it panned instead");
}
