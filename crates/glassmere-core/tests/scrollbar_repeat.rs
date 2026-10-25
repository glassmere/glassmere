mod support;

use glassmere_core::{Event, Label, Length, Style, Ui};
use glassmere_scene::{Point, Size};
use std::time::Duration;
use support::Driven;

const ROOT: Size = Size::new(600.0, 400.0);

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

/// A 300 by 200 view over content 1000 px tall and 900 px wide.
fn view(_: &(), ui: &mut Ui<'_, glassmere_core::app::Action<()>>) {
    ui.keyed("view", |ui| {
        ui.scroll(sized(300.0, 200.0), |ui| {
            ui.keyed("content", |ui| {
                ui.label(Label::new("").style(sized(900.0, 1000.0)))
            })
        })
    });
}

fn driven() -> Driven<()> {
    Driven::new(ROOT, (), view)
}

fn offsets(driven: &Driven<()>) -> (f32, f32) {
    let id = driven.context.find(&["view"]).unwrap();
    let scroll = driven.context.node(id).unwrap().scroll.unwrap();
    (scroll.offset_x, scroll.offset)
}

fn at(driven: &mut Driven<()>, milliseconds: u64) {
    driven.context.set_now(Duration::from_millis(milliseconds));
    driven.frame();
}

fn press(driven: &mut Driven<()>, point: Point) {
    driven.context.event(Event::PointerMoved(point));
    driven.context.event(Event::PointerDown);
    driven.frame();
}

/// On the vertical track below the 40 px thumb.
const BELOW: Point = Point { x: 297.0, y: 190.0 };
/// On the horizontal track after the 100 px thumb.
const BESIDE: Point = Point { x: 250.0, y: 198.0 };

#[test]
fn a_press_on_a_track_pages_once_and_repeats_after_the_delay() {
    let mut driven = driven();
    at(&mut driven, 1000);
    press(&mut driven, BELOW);
    assert_eq!(offsets(&driven), (0.0, 200.0), "paged once on the press");
    assert_eq!(
        driven.context.next_wake(),
        Some(Duration::from_millis(1400))
    );

    at(&mut driven, 1399);
    assert_eq!(offsets(&driven).1, 200.0, "nothing before the delay");
    at(&mut driven, 1400);
    assert_eq!(offsets(&driven).1, 400.0);
    assert_eq!(
        driven.context.next_wake(),
        Some(Duration::from_millis(1450))
    );
    at(&mut driven, 1449);
    assert_eq!(offsets(&driven).1, 400.0);
    at(&mut driven, 1450);
    assert_eq!(offsets(&driven).1, 600.0, "then at a steady interval");
}

#[test]
fn a_repeating_track_press_stops_when_the_thumb_reaches_the_pointer() {
    let mut driven = driven();
    at(&mut driven, 0);
    press(&mut driven, BELOW);
    for step in 0..10 {
        at(&mut driven, 400 + step * 50);
    }
    // At 800 the thumb covers the pointer; no page goes past it.
    assert_eq!(offsets(&driven).1, 800.0);
    assert_eq!(driven.context.next_wake(), None);
}

#[test]
fn a_repeating_track_press_stops_on_release_and_when_the_pointer_leaves() {
    let mut driven = driven();
    at(&mut driven, 0);
    press(&mut driven, BELOW);
    driven.context.event(Event::PointerUp);
    assert_eq!(driven.context.next_wake(), None);
    at(&mut driven, 5000);
    assert_eq!(offsets(&driven).1, 200.0);

    let mut driven = self::driven();
    at(&mut driven, 0);
    press(&mut driven, BELOW);
    driven
        .context
        .event(Event::PointerMoved(Point { x: 100.0, y: 100.0 }));
    assert_eq!(driven.context.next_wake(), None);
    at(&mut driven, 5000);
    assert_eq!(offsets(&driven).1, 200.0);
}

#[test]
fn a_sideways_track_press_repeats_too() {
    let mut driven = driven();
    at(&mut driven, 0);
    press(&mut driven, BESIDE);
    assert_eq!(offsets(&driven), (300.0, 0.0));
    at(&mut driven, 400);
    // The thumb, 100 px long at 100, now covers the pointer at 250.
    assert_eq!(offsets(&driven), (600.0, 0.0), "the second page");
    assert_eq!(driven.context.next_wake(), Some(Duration::from_millis(450)));
    at(&mut driven, 450);
    assert_eq!(offsets(&driven).0, 600.0, "and the thumb has the pointer");
}

#[test]
fn dragging_the_thumb_does_not_repeat() {
    let mut driven = driven();
    at(&mut driven, 0);
    press(&mut driven, Point { x: 297.0, y: 10.0 });
    assert_eq!(driven.context.next_wake(), None);
    driven
        .context
        .event(Event::PointerMoved(Point { x: 297.0, y: 50.0 }));
    driven.frame();
    let dragged = offsets(&driven);
    assert!(dragged.1 > 0.0);
    at(&mut driven, 5000);
    assert_eq!(offsets(&driven), dragged);
}
