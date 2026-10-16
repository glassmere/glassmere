//! `Response::pressed` is observed state like `hovered`: reading it makes a
//! change in it declare the view again; a button nobody read only repaints.
use rust_gui_core::{App, AppDriver, Button, Context, Effects, Event, MonospaceMeasurer, Ui};
use rust_gui_scene::{Point, Size};
use std::cell::Cell;

#[derive(Clone)]
enum Msg {
    Click,
}

struct Probe {
    read: bool,
    seen: Cell<bool>,
}

impl App for Probe {
    type Message = Msg;

    fn update(&mut self, Msg::Click: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let response = ui.keyed("b", |ui| {
            ui.button(Button::new("Hold").on_click(Msg::Click))
        });
        if self.read {
            self.seen.set(response.pressed());
        }
    }
}

/// Whether the pointer moving over the button declared the view, and what
/// the declaration after the press read.
fn run(read: bool) -> (bool, bool, bool) {
    let mut app = Probe {
        read,
        seen: Cell::new(false),
    };
    let mut driver = AppDriver::new(|| true);
    let mut context = Context::new();
    let size = Size::new(300.0, 200.0);
    let mut frame = |app: &mut Probe, context: &mut Context| {
        driver.frame(app, context, size, &mut MonospaceMeasurer)
    };
    for _ in 0..3 {
        frame(&mut app, &mut context);
    }
    let button = context.find(&["b"]).expect("the button");
    let bounds = context.node(button).unwrap().bounds;
    let at = Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    };
    context.event(Event::PointerMoved(at));
    let hovered = frame(&mut app, &mut context).declared;
    context.event(Event::PointerDown);
    let pressed = frame(&mut app, &mut context).declared;
    let seen_pressed = app.seen.get();
    context.event(Event::PointerUp);
    frame(&mut app, &mut context);
    (hovered, pressed, seen_pressed && !app.seen.get())
}

#[test]
fn reading_pressed_observes_the_widget() {
    let (hovered, pressed, seen) = run(true);
    assert!(
        hovered,
        "a read widget declares when the pointer reaches it"
    );
    assert!(pressed, "pressing declares");
    assert!(seen, "pressed() was true while held and false after");
}

#[test]
fn a_widget_nobody_read_only_repaints_on_hover() {
    let (hovered, _, _) = run(false);
    assert!(!hovered);
}
