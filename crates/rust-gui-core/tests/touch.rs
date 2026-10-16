mod support;

use rust_gui_core::app::Action;
use rust_gui_core::{Button, DragPayload, DragSourceBuilder, DropTargetBuilder};
use rust_gui_core::{
    Context, DiagnosticKind, DropKind, Event, Length, MenuItem, Style, TouchPhase, Ui,
};
use rust_gui_scene::{Point, Size};
use std::time::Duration;
use support::{Driven, act, on};

const ROOT: Size = Size::new(600.0, 500.0);

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

fn center(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

fn touch(context: &mut Context, id: u64, phase: TouchPhase, position: Point) -> bool {
    context.event(Event::Touch {
        id,
        phase,
        position,
        force: None,
        pen: None,
    })
}

fn at(x: f32, y: f32) -> Point {
    Point { x, y }
}

#[derive(Default)]
struct Page {
    clicks: usize,
    menu: Option<usize>,
    dropped: Option<DragPayload>,
    left: f64,
    right: f64,
}

/// A button, two sliders, a context area with a drag source and a drop
/// target, and a scroll view over a tall list of buttons.
fn page(state: &Page, ui: &mut Ui<'_, Action<Page>>) {
    ui.keyed("top", |ui| {
        ui.row(Style::default().gap(20.0), |ui| {
            ui.keyed("tap", |ui| {
                ui.button(
                    Button::new("Tap")
                        .enabled(true)
                        .style(sized(100.0, 40.0))
                        .on_click(act(|s: &mut Page| s.clicks += 1)),
                );
            });
            ui.keyed("left", |ui| {
                ui.column(sized(200.0, 40.0), |ui| {
                    ui.keyed("slider", |ui| {
                        ui.slider(
                            "Left",
                            state.left,
                            0.0..=100.0,
                            1.0,
                            on(|s: &mut Page, value: f64| s.left = value),
                        );
                    });
                });
            });
            ui.keyed("right", |ui| {
                ui.column(sized(200.0, 40.0), |ui| {
                    ui.keyed("slider", |ui| {
                        ui.slider(
                            "Right",
                            state.right,
                            0.0..=100.0,
                            1.0,
                            on(|s: &mut Page, value: f64| s.right = value),
                        );
                    });
                });
            });
        });
    });
    let items = [MenuItem::new("Copy"), MenuItem::new("Paste")];
    ui.keyed("area", |ui| {
        ui.context_menu(
            &items,
            sized(400.0, 80.0),
            on(|s: &mut Page, index: usize| s.menu = Some(index)),
            |ui| {
                ui.keyed("cards", |ui| {
                    ui.row(Style::default().gap(20.0), |ui| {
                        ui.keyed("card", |ui| {
                            ui.drag_source(
                                sized(100.0, 60.0),
                                DragSourceBuilder::new("Card", DragPayload::Text("hello".into())),
                                |ui| {
                                    ui.label("Card");
                                },
                            );
                        });
                        ui.keyed("bin", |ui| {
                            ui.drop_target(
                                sized(100.0, 60.0),
                                DropTargetBuilder::new("Bin", DropKind::Text).on_drop(on(
                                    |s: &mut Page, payload: DragPayload| {
                                        s.dropped = Some(payload);
                                    },
                                )),
                                |_| {},
                            );
                        });
                    });
                });
            },
        );
    });
    ui.keyed("list", |ui| {
        ui.scroll(sized(300.0, 200.0), |ui| {
            for index in 0..40 {
                ui.keyed(format!("row{index}"), |ui| {
                    ui.button(
                        Button::new("Row")
                            .enabled(true)
                            .style(sized(280.0, 40.0))
                            .on_click(act(|s: &mut Page| s.clicks += 100)),
                    );
                });
            }
        });
    });
}

fn offset(context: &Context) -> f32 {
    let list = context.find(&["list"]).unwrap();
    context.node(list).unwrap().scroll.unwrap().offset
}

#[test]
fn a_tap_activates_and_leaves_no_hover_behind() {
    let mut driven = Driven::new(ROOT, Page::default(), page);
    let button = center(&driven.context, &["top", "tap"]);
    assert!(touch(&mut driven.context, 1, TouchPhase::Started, button));
    assert_eq!(driven.context.touches().count(), 1);
    // A little wobble is still a tap.
    touch(
        &mut driven.context,
        1,
        TouchPhase::Moved,
        at(button.x + 4.0, button.y + 3.0),
    );
    touch(&mut driven.context, 1, TouchPhase::Ended, button);
    driven.frame();
    assert_eq!(driven.state().clicks, 1);
    assert_eq!(driven.context.touches().count(), 0);
    // The mouse was never there.
    assert_eq!(driven.context.hovered(), None);
    assert_eq!(driven.context.captured(), None);
}

#[test]
fn a_moving_press_pans_without_activating_and_keeps_going_after_release() {
    let mut driven = Driven::new(ROOT, Page::default(), page);
    let start = center(&driven.context, &["list", "row2"]);
    driven.context.set_now(Duration::ZERO);
    touch(&mut driven.context, 1, TouchPhase::Started, start);
    // Sixty pixels up in sixty milliseconds.
    for step in 1..=6u64 {
        driven.context.set_now(Duration::from_millis(step * 10));
        touch(
            &mut driven.context,
            1,
            TouchPhase::Moved,
            at(start.x, start.y - 10.0 * step as f32),
        );
    }
    assert_eq!(
        offset(&driven.context),
        60.0,
        "the content follows the finger"
    );
    touch(
        &mut driven.context,
        1,
        TouchPhase::Ended,
        at(start.x, start.y - 60.0),
    );
    driven.frame();
    assert_eq!(driven.state().clicks, 0, "a pan is not a tap");
    assert!(driven.context.animating(), "the fling wants frames");

    // A thousand pixels per second, decaying: it slows down and then stops.
    let mut travelled = Vec::new();
    for frame in 1..=200u64 {
        driven
            .context
            .set_now(Duration::from_millis(60 + frame * 16));
        let before = offset(&driven.context);
        driven.frame();
        travelled.push(offset(&driven.context) - before);
        if !driven.context.animating() {
            break;
        }
    }
    assert!(!driven.context.animating(), "a fling ends by itself");
    assert!(travelled.len() > 60, "{}", travelled.len());
    // Scrolling a page for a second repaints it every frame, and that is the
    // framework's doing, not a mistake of the app's.
    assert!(
        driven
            .context
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.kind != DiagnosticKind::Repaint),
        "{:?}",
        driven.context.diagnostics()
    );
    assert!(travelled[0] > 10.0 && travelled[0] < 20.0, "{travelled:?}");
    assert!(travelled[5] < travelled[0], "{travelled:?}");
    let total = offset(&driven.context) - 60.0;
    // Speed times the decay constant, less the tail below the minimum speed.
    assert!((280.0..=325.0).contains(&total), "{total}");
    assert_eq!(driven.state().clicks, 0);
}

#[test]
fn a_touch_stops_a_fling_and_reduced_motion_never_starts_one() {
    let fling = |driven: &mut Driven<Page>| {
        let start = center(&driven.context, &["list", "row2"]);
        driven.context.set_now(Duration::ZERO);
        touch(&mut driven.context, 1, TouchPhase::Started, start);
        for step in 1..=6u64 {
            driven.context.set_now(Duration::from_millis(step * 10));
            touch(
                &mut driven.context,
                1,
                TouchPhase::Moved,
                at(start.x, start.y - 10.0 * step as f32),
            );
        }
        touch(
            &mut driven.context,
            1,
            TouchPhase::Ended,
            at(start.x, start.y - 60.0),
        );
        driven.frame();
        start
    };
    let mut driven = Driven::new(ROOT, Page::default(), page);
    let start = fling(&mut driven);
    assert!(driven.context.animating());
    touch(&mut driven.context, 2, TouchPhase::Started, start);
    assert!(!driven.context.animating(), "a finger catches the content");

    let mut driven = Driven::new(ROOT, Page::default(), page);
    driven.context.set_reduced_motion(true);
    fling(&mut driven);
    assert!(!driven.context.animating());
    assert_eq!(offset(&driven.context), 60.0);
}

#[test]
fn a_resting_press_on_a_drag_source_picks_it_up_and_a_moving_one_does_not() {
    let mut driven = Driven::new(ROOT, Page::default(), page);
    let card = center(&driven.context, &["area", "cards", "card"]);
    let bin = center(&driven.context, &["area", "cards", "bin"]);
    // Moving at once is a pan, even from a drag source.
    driven.context.set_now(Duration::ZERO);
    touch(&mut driven.context, 1, TouchPhase::Started, card);
    touch(&mut driven.context, 1, TouchPhase::Moved, bin);
    touch(&mut driven.context, 1, TouchPhase::Ended, bin);
    driven.frame();
    assert_eq!(driven.state().dropped, None);

    driven.context.set_now(Duration::from_secs(10));
    touch(&mut driven.context, 1, TouchPhase::Started, card);
    driven.context.set_now(Duration::from_millis(10_500));
    driven.frame();
    assert!(
        driven.context.find(&["area", "$popup"]).is_none(),
        "a drag, not the menu"
    );
    touch(&mut driven.context, 1, TouchPhase::Moved, bin);
    touch(&mut driven.context, 1, TouchPhase::Ended, bin);
    driven.frame();
    assert_eq!(
        driven.state().dropped,
        Some(DragPayload::Text("hello".into()))
    );
}

#[test]
fn two_fingers_move_two_sliders_and_a_second_finger_cannot_steal_one() {
    let mut driven = Driven::new(ROOT, Page::default(), page);
    let left = driven
        .context
        .node(driven.context.find(&["top", "left", "slider"]).unwrap())
        .unwrap()
        .bounds;
    let right = driven
        .context
        .node(driven.context.find(&["top", "right", "slider"]).unwrap())
        .unwrap()
        .bounds;
    let middle = |bounds: rust_gui_scene::Rect, share: f32| {
        at(
            bounds.x + bounds.width * share,
            bounds.y + bounds.height / 2.0,
        )
    };
    touch(
        &mut driven.context,
        1,
        TouchPhase::Started,
        middle(left, 0.1),
    );
    touch(
        &mut driven.context,
        2,
        TouchPhase::Started,
        middle(right, 0.1),
    );
    // Interleaved, as a host delivers them. Far beyond the slop: a slider
    // follows the finger instead of panning.
    touch(&mut driven.context, 1, TouchPhase::Moved, middle(left, 0.5));
    touch(
        &mut driven.context,
        2,
        TouchPhase::Moved,
        middle(right, 0.9),
    );
    touch(
        &mut driven.context,
        1,
        TouchPhase::Moved,
        middle(left, 0.25),
    );
    driven.frame();
    // The thumb is inset by 12 px at both ends of the track.
    let value = |bounds: rust_gui_scene::Rect, share: f32| {
        f64::from(((bounds.width * share - 12.0) / (bounds.width - 24.0) * 100.0).round())
    };
    assert_eq!(driven.state().left, value(left, 0.25));
    assert_eq!(driven.state().right, value(right, 0.9));
    assert_ne!(driven.state().left, driven.state().right);

    // A third finger lands on the left slider, which is taken: it does nothing.
    touch(
        &mut driven.context,
        3,
        TouchPhase::Started,
        middle(left, 0.8),
    );
    touch(&mut driven.context, 3, TouchPhase::Moved, middle(left, 0.7));
    touch(&mut driven.context, 3, TouchPhase::Ended, middle(left, 0.7));
    driven.frame();
    assert_eq!(driven.state().left, value(left, 0.25));

    // The mouse is one more pointer, and it never saw any of this.
    assert_eq!(driven.context.captured(), None);
    touch(
        &mut driven.context,
        1,
        TouchPhase::Ended,
        middle(left, 0.25),
    );
    touch(
        &mut driven.context,
        2,
        TouchPhase::Cancelled,
        middle(right, 0.9),
    );
    assert_eq!(driven.context.touches().count(), 0);
}

fn gone(_: &Page, ui: &mut Ui<'_, Action<Page>>) {
    ui.label("gone");
}

#[test]
fn losing_focus_or_the_pressed_node_drops_the_contact_safely() {
    let mut driven = Driven::new(ROOT, Page::default(), page);
    let button = center(&driven.context, &["top", "tap"]);
    touch(&mut driven.context, 1, TouchPhase::Started, button);
    // The whole page goes away while the finger is down.
    driven.set_view(gone);
    driven.frame();
    touch(
        &mut driven.context,
        1,
        TouchPhase::Moved,
        at(button.x + 50.0, button.y),
    );
    touch(
        &mut driven.context,
        1,
        TouchPhase::Ended,
        at(button.x + 50.0, button.y),
    );
    driven.set_view(page);
    driven.frame();
    assert_eq!(driven.state().clicks, 0);

    touch(&mut driven.context, 2, TouchPhase::Started, button);
    driven.context.event(Event::FocusLost);
    assert_eq!(driven.context.touches().count(), 0);
    driven.frame();
    assert_eq!(driven.state().clicks, 0);
}

#[derive(Default)]
struct Field {
    typed: String,
}

fn field(state: &Field, ui: &mut Ui<'_, Action<Field>>) {
    ui.keyed("field", |ui| {
        ui.text_input(
            "Field",
            &state.typed,
            on(|s: &mut Field, text: String| s.typed = text),
        );
    });
}

#[test]
fn a_rest_or_a_second_tap_in_text_selects_a_word_and_the_drag_extends_it() {
    let mut driven = Driven::new(
        ROOT,
        Field {
            typed: String::from("alpha beta gamma"),
        },
        field,
    );
    fn selected(context: &Context, text: &str) -> Option<String> {
        let node = context.node(context.find(&["field"]).unwrap()).unwrap();
        node.text_selection
            .filter(|selection| selection.anchor != selection.cursor)
            .map(|selection| text[selection.range()].to_owned())
    }
    let bounds = driven
        .context
        .node(driven.context.find(&["field"]).unwrap())
        .unwrap()
        .bounds;
    // The first word begins at the start edge, wherever the text is measured.
    let start = at(bounds.x + 2.0, bounds.y + bounds.height / 2.0);
    let along = |across: f32| at(bounds.x + 2.0 + across, bounds.y + bounds.height / 2.0);

    // Resting in the text selects the word under the finger, without a menu.
    driven.context.set_now(Duration::ZERO);
    touch(&mut driven.context, 1, TouchPhase::Started, start);
    driven.context.set_now(Duration::from_millis(600));
    driven.frame();
    assert_eq!(
        selected(&driven.context, &driven.state().typed).as_deref(),
        Some("alpha")
    );
    assert!(
        driven.context.find(&["field", "$popup"]).is_none(),
        "no menu"
    );

    // Dragging on from there extends the selection instead of panning.
    touch(
        &mut driven.context,
        1,
        TouchPhase::Moved,
        along(bounds.width / 2.0),
    );
    driven.frame();
    let extended = selected(&driven.context, &driven.state().typed).unwrap_or_default();
    assert!(
        extended.starts_with("alpha") && extended.len() > 5,
        "{extended}"
    );
    touch(
        &mut driven.context,
        1,
        TouchPhase::Ended,
        along(bounds.width / 2.0),
    );
    driven.frame();

    // A second tap selects the word under it as well.
    driven.context.set_now(Duration::from_millis(2000));
    for when in [2000, 2100] {
        driven.context.set_now(Duration::from_millis(when));
        touch(&mut driven.context, 2, TouchPhase::Started, start);
        touch(&mut driven.context, 2, TouchPhase::Ended, start);
    }
    driven.frame();
    assert_eq!(
        selected(&driven.context, &driven.state().typed).as_deref(),
        Some("alpha")
    );
    // And typing replaces what was selected, which is the point of it.
    driven
        .context
        .event(Event::Text(rust_gui_core::TextEdit::Insert("one".into())));
    driven.frame();
    assert_eq!(driven.state().typed, "one beta gamma");
}

#[test]
fn a_resting_press_opens_the_context_menu_and_does_not_click() {
    let mut driven = Driven::new(ROOT, Page::default(), page);
    let bin = center(&driven.context, &["area", "cards", "bin"]);
    driven.context.set_now(Duration::from_secs(1));
    touch(&mut driven.context, 1, TouchPhase::Started, bin);
    assert_eq!(
        driven.context.next_wake(),
        Some(Duration::from_millis(1500)),
        "the host is asked to come back"
    );
    driven.context.set_now(Duration::from_millis(1400));
    driven.frame();
    assert!(
        driven.context.find(&["area", "$popup"]).is_none(),
        "not yet"
    );
    // One batch at the wake opens the menu and declares it.
    driven.context.set_now(Duration::from_millis(1500));
    driven.frame();
    assert!(driven.context.find(&["area", "$popup"]).is_some());
    assert_eq!(driven.context.next_wake(), None);
    // Lifting the finger neither closes the menu nor clicks what is under it.
    touch(&mut driven.context, 1, TouchPhase::Ended, bin);
    driven.frame();
    assert!(driven.context.find(&["area", "$popup"]).is_some());
    assert_eq!(driven.state().clicks, 0);
    // A tap on an item chooses it.
    let popup = driven.context.find(&["area", "$popup"]).unwrap();
    let popup = driven.context.node(popup).unwrap().bounds;
    let item = at(popup.x + 20.0, popup.y + popup.height * 0.75);
    touch(&mut driven.context, 2, TouchPhase::Started, item);
    touch(&mut driven.context, 2, TouchPhase::Ended, item);
    driven.frame();
    assert_eq!(driven.state().menu, Some(1));
}
