//! Milestone 35, phase 2G: custom widgets, event areas, gestures and drag and
//! drop send messages, and a batch of input declares the view at most once.
use rust_gui_core::{
    App, AppDriver, Context, CustomWidget, DragOutcome, DragPayload, DragSourceBuilder, DropKind,
    DropTargetBuilder, Effects, Event, EventArea, GestureBuilder, GestureResponse, Length,
    MonospaceMeasurer, Style, TouchPhase, Ui, UiEvent, WidgetBuilder,
};
use rust_gui_scene::{Point, Size};
use rust_gui_testing::Harness;
use std::cell::Cell;

const ROOT: Size = Size::new(400.0, 400.0);

#[derive(Clone, Debug)]
enum Msg {
    Pointer(UiEvent),
    Gesture(GestureResponse),
    Dropped(DragPayload),
    Ended(DragOutcome),
    Enable(bool),
}

#[derive(Default)]
struct Areas {
    pad_enabled: bool,
    pointer: Vec<UiEvent>,
    gestures: Vec<GestureResponse>,
    drops: Vec<DragPayload>,
    ends: Vec<DragOutcome>,
    views: Cell<usize>,
}

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

impl App for Areas {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message {
            Msg::Pointer(event) => self.pointer.push(event),
            Msg::Gesture(gesture) => self.gestures.push(gesture),
            Msg::Dropped(payload) => self.drops.push(payload),
            Msg::Ended(outcome) => self.ends.push(outcome),
            Msg::Enable(enabled) => self.pad_enabled = enabled,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        ui.enabled(self.pad_enabled, |ui| {
            ui.keyed("pad", |ui| {
                // A widget of the application's own: its pointer events are
                // sent as messages, in its own coordinates.
                let pad = WidgetBuilder::new(|_: &rust_gui_core::WidgetInput| CustomWidget {
                    size: Size::new(100.0, 60.0),
                    input: EventArea::pointer(),
                    ..CustomWidget::default()
                })
                .on_event(|event| Some(Msg::Pointer(event)));
                ui.widget(Style::default(), pad)
            });
        });
        ui.keyed("canvas", |ui| {
            ui.gesture_area(
                GestureBuilder::new().on_gesture(Msg::Gesture),
                sized(200.0, 200.0),
                |_| {},
            )
        });
        ui.keyed("bin", |ui| {
            ui.drop_target(
                DropTargetBuilder::new("Bin", DropKind::Text).on_drop(Msg::Dropped),
                sized(120.0, 40.0),
                |_| {},
            )
        });
        ui.keyed("source", |ui| {
            ui.drag_source(
                DragSourceBuilder::new("Card", DragPayload::Text("card".into()))
                    .on_drag_end(Msg::Ended),
                sized(80.0, 30.0),
                |ui| ui.label("Card"),
            )
        });
    }
}

fn harness() -> Harness<Areas> {
    Harness::app(
        ROOT,
        Areas {
            pad_enabled: true,
            ..Areas::default()
        },
    )
}

fn centre(harness: &Harness<Areas>, path: &[&str]) -> Point {
    let bounds = harness
        .context()
        .node(harness.find(path))
        .expect("a declared node")
        .bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

#[test]
fn a_custom_widgets_pointer_events_arrive_as_messages_in_its_own_coordinates() {
    let mut harness = harness();
    harness.run();
    let pad = harness.find(&["pad"]);
    let bounds = harness.context().node(pad).unwrap().bounds;
    let stats = harness.click(pad);
    assert!(stats.declared, "a message declares the view");
    let at = centre(&harness, &["pad"]);
    let local = Point {
        x: at.x - bounds.x,
        y: at.y - bounds.y,
    };
    assert_eq!(local.x, bounds.width / 2.0);
    let pointer = &harness.state().pointer;
    assert!(
        pointer
            .iter()
            .any(|event| matches!(event, UiEvent::PointerDown(p) if *p == local)),
        "a press at the centre, in the widget's own coordinates: {pointer:?}"
    );
    assert!(
        pointer
            .iter()
            .any(|event| matches!(event, UiEvent::PointerUp(p) if *p == local)),
        "{pointer:?}"
    );
}

#[test]
fn a_disabled_custom_widget_sends_nothing() {
    let mut harness = harness();
    harness.run();
    harness.send(Msg::Enable(false));
    let pad = harness.find(&["pad"]);
    let before = harness.state().pointer.len();
    harness.click(pad);
    assert_eq!(harness.state().pointer.len(), before);
}

#[test]
fn a_gesture_area_sends_its_gesture_as_a_message() {
    let mut harness = harness();
    harness.run();
    let start = centre(&harness, &["canvas"]);
    let touch = |harness: &mut Harness<Areas>, phase, position| {
        harness.event(Event::Touch {
            id: 1,
            phase,
            position,
            force: None,
            pen: None,
        });
    };
    touch(&mut harness, TouchPhase::Started, start);
    touch(
        &mut harness,
        TouchPhase::Moved,
        Point {
            x: start.x + 20.0,
            y: start.y,
        },
    );
    touch(
        &mut harness,
        TouchPhase::Moved,
        Point {
            x: start.x + 60.0,
            y: start.y,
        },
    );
    harness.run();
    let panned: f32 = harness.state().gestures.iter().map(|g| g.pan_x).sum();
    assert!(
        panned > 0.0,
        "the finger panned: {:?}",
        harness.state().gestures
    );
    touch(&mut harness, TouchPhase::Ended, start);
    harness.run();
}

#[test]
fn a_drop_on_a_target_sends_the_payload() {
    let mut harness = harness();
    harness.run();
    let position = centre(&harness, &["bin"]);
    let accepted = harness.event(Event::DropAt {
        position,
        payload: DragPayload::Text("hello".into()),
    });
    assert!(accepted);
    harness.run();
    assert_eq!(
        harness.state().drops,
        vec![DragPayload::Text("hello".into())]
    );
}

#[test]
fn a_drag_that_leaves_the_window_sends_how_it_ended() {
    let mut app = Areas {
        pad_enabled: true,
        ..Areas::default()
    };
    let mut context = Context::new();
    let mut driver = AppDriver::new(|| true);
    let size = ROOT;
    driver.frame(&mut app, &mut context, size, &mut MonospaceMeasurer);
    let start = centre_of(&context, &["source"]);
    context.event(Event::PointerMoved(start));
    context.event(Event::PointerDown);
    // The pointer leaves the window; the host takes the drag over.
    context.event(Event::PointerMoved(Point { x: -40.0, y: 20.0 }));
    assert!(context.export_drag().is_some());
    driver.frame(&mut app, &mut context, size, &mut MonospaceMeasurer);
    context.finish_exported_drag(DragOutcome::Dropped);
    driver.frame(&mut app, &mut context, size, &mut MonospaceMeasurer);
    assert_eq!(app.ends, vec![DragOutcome::Dropped]);
}

fn centre_of(context: &Context, path: &[&str]) -> Point {
    let id = context.find(path).expect("a declared node");
    let bounds = context.node(id).expect("a live node").bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

#[test]
fn a_batch_of_many_events_declares_the_view_once() {
    let mut harness = harness();
    harness.run();
    let views = harness.state().views.get();
    let pad = harness.find(&["pad"]);
    let bounds = harness.context().node(pad).unwrap().bounds;
    let inside = Point {
        x: bounds.x + 10.0,
        y: bounds.y + 10.0,
    };
    // Three moves and nothing else, one batch: the widget hears all three.
    for step in 0..3 {
        harness.event(Event::PointerMoved(Point {
            x: inside.x + step as f32,
            y: inside.y,
        }));
    }
    let stats = harness.run();
    assert_eq!(stats.messages, 3);
    assert!(stats.declared);
    assert_eq!(harness.state().views.get(), views + 1);
    assert_eq!(harness.state().pointer.len(), 3);

    // A batch with nothing in it declares nothing.
    let stats = harness.run();
    assert_eq!(stats.messages, 0);
    assert!(!stats.declared);
    assert_eq!(harness.state().views.get(), views + 1);
}
