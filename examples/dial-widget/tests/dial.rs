//! Driving a third-party widget headlessly, with the same harness the
//! framework's own controls are tested with.
use dial_widget::Dial;
use rust_gui::{Event, Key, Length, Point, Size, Style, Ui};
use rust_gui_testing::Harness;

const ROOT: Size = Size::new(320.0, 240.0);
const DIAMETER: f32 = 120.0;

#[derive(Default)]
struct App {
    dial: Dial,
    changes: usize,
}

fn app(value: f64) -> Harness<App> {
    Harness::new(
        ROOT,
        App {
            dial: Dial::new(value),
            changes: 0,
        },
        |app: &mut App, ui: &mut Ui<'_>| {
            ui.keyed("page", |ui| {
                ui.column(Style::default(), |ui| {
                    if app.dial.ui(ui, "dial", "Gain", DIAMETER) {
                        app.changes += 1;
                    }
                    // Somewhere for the focus to go.
                    ui.keyed("after", |ui| ui.button("After"));
                })
            });
        },
    )
}

/// The middle of the dial, in window coordinates.
fn centre(harness: &Harness<App>) -> Point {
    let bounds = harness
        .context()
        .node(harness.find(&["page", "dial"]))
        .expect("the dial")
        .bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

#[test]
fn the_keyboard_turns_it_and_tab_still_leaves_it() {
    let mut harness = app(0.5);
    harness.run();
    let dial = harness.find(&["page", "dial"]);
    harness.focus(dial);
    assert_eq!(harness.context().focused(), Some(dial), "it takes focus");

    harness.press(Key::ArrowUp);
    assert!((harness.state().dial.value() - 0.55).abs() < 1e-9);
    harness.press(Key::ArrowDown);
    harness.press(Key::ArrowDown);
    assert!((harness.state().dial.value() - 0.45).abs() < 1e-9);
    harness.press_shift(Key::ArrowUp);
    assert!((harness.state().dial.value() - 0.46).abs() < 1e-9, "finer");
    harness.press(Key::Home);
    assert_eq!(harness.state().dial.value(), 0.0);
    harness.press(Key::End);
    assert_eq!(harness.state().dial.value(), 1.0);
    assert_eq!(harness.state().changes, 6);

    // The dial keeps the keys it uses, but never the ones the frame needs.
    harness.press(Key::Tab);
    assert_ne!(harness.context().focused(), Some(dial));
}

#[test]
fn a_drag_turns_it_from_where_it_was_grabbed() {
    let mut harness = app(0.25);
    harness.run();
    let at = centre(&harness);
    harness.event(Event::PointerMoved(at));
    harness.event(Event::PointerDown);
    harness.run();
    // Ninety pixels up is half of the dial's travel.
    harness.event(Event::PointerMoved(Point {
        x: at.x + 40.0,
        y: at.y - 90.0,
    }));
    harness.run();
    assert!((harness.state().dial.value() - 0.75).abs() < 1e-9);
    harness.event(Event::PointerUp);
    harness.run();
    // Moving after the release does nothing.
    harness.event(Event::PointerMoved(Point {
        x: at.x,
        y: at.y - 200.0,
    }));
    harness.run();
    assert!((harness.state().dial.value() - 0.75).abs() < 1e-9);
}

#[test]
fn it_says_what_it_holds() {
    let mut harness = app(0.7);
    harness.run();
    let announced = harness.snapshot_accessibility();
    assert!(
        announced.contains("Slider \"Gain\"")
            && announced.contains("value=\"70%\"")
            && announced.contains("number=70.0"),
        "{announced}"
    );
    assert_eq!(harness.by_label("Gain"), harness.find(&["page", "dial"]));
}

/// The reason the dial captures and consumes: an arrow key belongs to the
/// widget that has the focus, not to the page it sits on.
#[test]
fn an_arrow_key_turns_the_dial_without_scrolling_the_page_under_it() {
    let mut harness = Harness::new(
        Size::new(320.0, 200.0),
        App::default(),
        |app: &mut App, ui: &mut Ui<'_>| {
            ui.keyed("page", |ui| {
                ui.scroll(
                    Style {
                        height: Length::Px(160.0),
                        width: Length::Percent(1.0),
                        ..Style::default()
                    },
                    |ui| {
                        app.dial.ui(ui, "dial", "Gain", DIAMETER);
                        ui.keyed("below", |ui| ui.button("Below"));
                        for line in 0..20 {
                            ui.keyed(format!("filler{line}"), |ui| ui.label("Filler"));
                        }
                    },
                )
            });
        },
    );
    harness.run();
    let scroll = harness.find(&["page"]);
    let offset = |harness: &Harness<App>| {
        harness
            .context()
            .node(scroll)
            .expect("the scroll")
            .scroll
            .expect("a scroll state")
            .offset
    };

    // A plain button inside the same page leaves the arrow key to it.
    harness.focus(harness.find(&["page", "below"]));
    harness.press(Key::ArrowDown);
    assert!(offset(&harness) > 0.0, "the page scrolls for a button");

    let dial = harness.find(&["page", "dial"]);
    harness.focus(dial);
    let before = offset(&harness);
    harness.press(Key::ArrowDown);
    assert!((harness.state().dial.value() - 0.0).abs() < 1e-9, "at zero");
    harness.press(Key::ArrowUp);
    assert!((harness.state().dial.value() - 0.05).abs() < 1e-9);
    assert_eq!(offset(&harness), before, "and the page stayed where it was");
}
