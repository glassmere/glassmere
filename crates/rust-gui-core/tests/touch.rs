use rust_gui_core::{
    Context, DiagnosticKind, DragPayload, DropKind, Event, Length, MenuItem, MonospaceMeasurer,
    Style, TouchPhase, Ui,
};
use rust_gui_scene::{Point, Size};
use std::time::Duration;

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
fn page(context: &mut Context, page: &mut Page) {
    page.menu = None;
    page.dropped = None;
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.row("top", Style::default().gap(20.0), |ui| {
            if ui
                .button_styled("tap", "Tap", true, sized(100.0, 40.0))
                .clicked()
            {
                page.clicks += 1;
            }
            ui.column("left", sized(200.0, 40.0), |ui| {
                ui.slider("slider", "Left", &mut page.left, 0.0..=100.0, 1.0);
            });
            ui.column("right", sized(200.0, 40.0), |ui| {
                ui.slider("slider", "Right", &mut page.right, 0.0..=100.0, 1.0);
            });
        });
        let items = [MenuItem::new("Copy"), MenuItem::new("Paste")];
        page.menu = ui.context_menu("area", &items, sized(400.0, 80.0), |ui| {
            ui.row("cards", Style::default().gap(20.0), |ui| {
                ui.drag_source(
                    "card",
                    "Card",
                    DragPayload::Text("hello".into()),
                    sized(100.0, 60.0),
                    |ui| ui.label("Card"),
                );
                page.dropped =
                    ui.drop_target("bin", "Bin", DropKind::Text, sized(100.0, 60.0), |_| {});
            });
        });
        ui.scroll("list", sized(300.0, 200.0), |ui| {
            for index in 0..40 {
                if ui
                    .button_styled(format!("row{index}"), "Row", true, sized(280.0, 40.0))
                    .clicked()
                {
                    page.clicks += 100;
                }
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
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let button = center(&context, &["top", "tap"]);
    assert!(touch(&mut context, 1, TouchPhase::Started, button));
    assert_eq!(context.touches().count(), 1);
    // A little wobble is still a tap.
    touch(
        &mut context,
        1,
        TouchPhase::Moved,
        at(button.x + 4.0, button.y + 3.0),
    );
    touch(&mut context, 1, TouchPhase::Ended, button);
    page(&mut context, &mut state);
    assert_eq!(state.clicks, 1);
    assert_eq!(context.touches().count(), 0);
    // The mouse was never there.
    assert_eq!(context.hovered(), None);
    assert_eq!(context.captured(), None);
}

#[test]
fn a_moving_press_pans_without_activating_and_keeps_going_after_release() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let start = center(&context, &["list", "row2"]);
    context.set_now(Duration::ZERO);
    touch(&mut context, 1, TouchPhase::Started, start);
    // Sixty pixels up in sixty milliseconds.
    for step in 1..=6u64 {
        context.set_now(Duration::from_millis(step * 10));
        touch(
            &mut context,
            1,
            TouchPhase::Moved,
            at(start.x, start.y - 10.0 * step as f32),
        );
    }
    assert_eq!(offset(&context), 60.0, "the content follows the finger");
    touch(
        &mut context,
        1,
        TouchPhase::Ended,
        at(start.x, start.y - 60.0),
    );
    page(&mut context, &mut state);
    assert_eq!(state.clicks, 0, "a pan is not a tap");
    assert!(context.animating(), "the fling wants frames");

    // A thousand pixels per second, decaying: it slows down and then stops.
    let mut travelled = Vec::new();
    for frame in 1..=200u64 {
        context.set_now(Duration::from_millis(60 + frame * 16));
        let before = offset(&context);
        page(&mut context, &mut state);
        travelled.push(offset(&context) - before);
        if !context.animating() {
            break;
        }
    }
    assert!(!context.animating(), "a fling ends by itself");
    assert!(travelled.len() > 60, "{}", travelled.len());
    // Scrolling a page for a second repaints it every frame, and that is the
    // framework's doing, not a mistake of the app's.
    assert!(
        context
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.kind != DiagnosticKind::Repaint),
        "{:?}",
        context.diagnostics()
    );
    assert!(travelled[0] > 10.0 && travelled[0] < 20.0, "{travelled:?}");
    assert!(travelled[5] < travelled[0], "{travelled:?}");
    let total = offset(&context) - 60.0;
    // Speed times the decay constant, less the tail below the minimum speed.
    assert!((280.0..=325.0).contains(&total), "{total}");
    assert_eq!(state.clicks, 0);
}

#[test]
fn a_touch_stops_a_fling_and_reduced_motion_never_starts_one() {
    let fling = |context: &mut Context, state: &mut Page| {
        let start = center(context, &["list", "row2"]);
        context.set_now(Duration::ZERO);
        touch(context, 1, TouchPhase::Started, start);
        for step in 1..=6u64 {
            context.set_now(Duration::from_millis(step * 10));
            touch(
                context,
                1,
                TouchPhase::Moved,
                at(start.x, start.y - 10.0 * step as f32),
            );
        }
        touch(context, 1, TouchPhase::Ended, at(start.x, start.y - 60.0));
        page(context, state);
        start
    };
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let start = fling(&mut context, &mut state);
    assert!(context.animating());
    touch(&mut context, 2, TouchPhase::Started, start);
    assert!(!context.animating(), "a finger catches the content");

    let (mut context, mut state) = (Context::new(), Page::default());
    context.set_reduced_motion(true);
    page(&mut context, &mut state);
    fling(&mut context, &mut state);
    assert!(!context.animating());
    assert_eq!(offset(&context), 60.0);
}

#[test]
fn a_resting_press_opens_the_context_menu_and_does_not_click() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let bin = center(&context, &["area", "cards", "bin"]);
    context.set_now(Duration::from_secs(1));
    touch(&mut context, 1, TouchPhase::Started, bin);
    assert_eq!(
        context.next_wake(),
        Some(Duration::from_millis(1500)),
        "the host is asked to come back"
    );
    context.set_now(Duration::from_millis(1400));
    page(&mut context, &mut state);
    assert!(context.find(&["area", "$popup"]).is_none(), "not yet");
    context.set_now(Duration::from_millis(1500));
    page(&mut context, &mut state);
    page(&mut context, &mut state);
    assert!(context.find(&["area", "$popup"]).is_some());
    assert_eq!(context.next_wake(), None);
    // Lifting the finger neither closes the menu nor clicks what is under it.
    touch(&mut context, 1, TouchPhase::Ended, bin);
    page(&mut context, &mut state);
    assert!(context.find(&["area", "$popup"]).is_some());
    // A tap on an item chooses it.
    let paste = context.find(&["area", "$popup"]).unwrap();
    let paste = context.node(paste).unwrap().bounds;
    let item = at(paste.x + 20.0, paste.y + paste.height * 0.75);
    touch(&mut context, 2, TouchPhase::Started, item);
    touch(&mut context, 2, TouchPhase::Ended, item);
    page(&mut context, &mut state);
    assert_eq!(state.menu, Some(1));
}

#[test]
fn a_resting_press_on_a_drag_source_picks_it_up_and_a_moving_one_does_not() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let card = center(&context, &["area", "cards", "card"]);
    let bin = center(&context, &["area", "cards", "bin"]);
    // Moving at once is a pan, even from a drag source.
    context.set_now(Duration::ZERO);
    touch(&mut context, 1, TouchPhase::Started, card);
    touch(&mut context, 1, TouchPhase::Moved, bin);
    touch(&mut context, 1, TouchPhase::Ended, bin);
    page(&mut context, &mut state);
    assert_eq!(state.dropped, None);

    context.set_now(Duration::from_secs(10));
    touch(&mut context, 1, TouchPhase::Started, card);
    context.set_now(Duration::from_millis(10_500));
    page(&mut context, &mut state);
    assert!(
        context.find(&["area", "$popup"]).is_none(),
        "a drag, not the menu"
    );
    touch(&mut context, 1, TouchPhase::Moved, bin);
    touch(&mut context, 1, TouchPhase::Ended, bin);
    page(&mut context, &mut state);
    assert_eq!(state.dropped, Some(DragPayload::Text("hello".into())));
}

#[test]
fn two_fingers_move_two_sliders_and_a_second_finger_cannot_steal_one() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let left = context
        .node(context.find(&["top", "left", "slider"]).unwrap())
        .unwrap()
        .bounds;
    let right = context
        .node(context.find(&["top", "right", "slider"]).unwrap())
        .unwrap()
        .bounds;
    let middle = |bounds: rust_gui_scene::Rect, share: f32| {
        at(
            bounds.x + bounds.width * share,
            bounds.y + bounds.height / 2.0,
        )
    };
    touch(&mut context, 1, TouchPhase::Started, middle(left, 0.1));
    touch(&mut context, 2, TouchPhase::Started, middle(right, 0.1));
    // Interleaved, as a host delivers them. Far beyond the slop: a slider
    // follows the finger instead of panning.
    touch(&mut context, 1, TouchPhase::Moved, middle(left, 0.5));
    touch(&mut context, 2, TouchPhase::Moved, middle(right, 0.9));
    touch(&mut context, 1, TouchPhase::Moved, middle(left, 0.25));
    page(&mut context, &mut state);
    // The thumb is inset by 12 px at both ends of the track.
    let value = |bounds: rust_gui_scene::Rect, share: f32| {
        f64::from(((bounds.width * share - 12.0) / (bounds.width - 24.0) * 100.0).round())
    };
    assert_eq!(state.left, value(left, 0.25));
    assert_eq!(state.right, value(right, 0.9));
    assert_ne!(state.left, state.right);

    // A third finger lands on the left slider, which is taken: it does nothing.
    touch(&mut context, 3, TouchPhase::Started, middle(left, 0.8));
    touch(&mut context, 3, TouchPhase::Moved, middle(left, 0.7));
    touch(&mut context, 3, TouchPhase::Ended, middle(left, 0.7));
    page(&mut context, &mut state);
    assert_eq!(state.left, value(left, 0.25));

    // The mouse is one more pointer, and it never saw any of this.
    assert_eq!(context.captured(), None);
    touch(&mut context, 1, TouchPhase::Ended, middle(left, 0.25));
    touch(&mut context, 2, TouchPhase::Cancelled, middle(right, 0.9));
    assert_eq!(context.touches().count(), 0);
}

#[test]
fn losing_focus_or_the_pressed_node_drops_the_contact_safely() {
    let (mut context, mut state) = (Context::new(), Page::default());
    page(&mut context, &mut state);
    let button = center(&context, &["top", "tap"]);
    touch(&mut context, 1, TouchPhase::Started, button);
    // The whole page goes away while the finger is down.
    context.run(ROOT, &mut MonospaceMeasurer, |ui| ui.label("gone"));
    touch(
        &mut context,
        1,
        TouchPhase::Moved,
        at(button.x + 50.0, button.y),
    );
    touch(
        &mut context,
        1,
        TouchPhase::Ended,
        at(button.x + 50.0, button.y),
    );
    page(&mut context, &mut state);
    assert_eq!(state.clicks, 0);

    touch(&mut context, 2, TouchPhase::Started, button);
    context.event(Event::FocusLost);
    assert_eq!(context.touches().count(), 0);
    page(&mut context, &mut state);
    assert_eq!(state.clicks, 0);
}

#[test]
fn a_rest_or_a_second_tap_in_text_selects_a_word_and_the_drag_extends_it() {
    let mut context = Context::new();
    let mut typed = String::from("alpha beta gamma");
    let declare = |context: &mut Context, typed: &mut String| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            ui.text_input("field", "Field", typed);
        });
    };
    fn selected(context: &Context, text: &str) -> Option<String> {
        let node = context.node(context.find(&["field"]).unwrap()).unwrap();
        node.text_selection
            .filter(|selection| selection.anchor != selection.cursor)
            .map(|selection| text[selection.range()].to_owned())
    }
    declare(&mut context, &mut typed);
    let bounds = context
        .node(context.find(&["field"]).unwrap())
        .unwrap()
        .bounds;
    // The first word begins at the start edge, wherever the text is measured.
    let start = at(bounds.x + 2.0, bounds.y + bounds.height / 2.0);
    let along = |across: f32| at(bounds.x + 2.0 + across, bounds.y + bounds.height / 2.0);

    // Resting in the text selects the word under the finger, without a menu.
    context.set_now(Duration::ZERO);
    touch(&mut context, 1, TouchPhase::Started, start);
    context.set_now(Duration::from_millis(600));
    declare(&mut context, &mut typed);
    assert_eq!(selected(&context, &typed).as_deref(), Some("alpha"));
    assert!(context.find(&["field", "$popup"]).is_none(), "no menu");

    // Dragging on from there extends the selection instead of panning.
    touch(
        &mut context,
        1,
        TouchPhase::Moved,
        along(bounds.width / 2.0),
    );
    declare(&mut context, &mut typed);
    let extended = selected(&context, &typed).unwrap_or_default();
    assert!(
        extended.starts_with("alpha") && extended.len() > 5,
        "{extended}"
    );
    touch(
        &mut context,
        1,
        TouchPhase::Ended,
        along(bounds.width / 2.0),
    );
    declare(&mut context, &mut typed);

    // A second tap selects the word under it as well.
    context.set_now(Duration::from_millis(2000));
    for when in [2000, 2100] {
        context.set_now(Duration::from_millis(when));
        touch(&mut context, 2, TouchPhase::Started, start);
        touch(&mut context, 2, TouchPhase::Ended, start);
    }
    declare(&mut context, &mut typed);
    assert_eq!(selected(&context, &typed).as_deref(), Some("alpha"));
    // And typing replaces what was selected, which is the point of it.
    context.event(Event::Text(rust_gui_core::TextEdit::Insert("one".into())));
    declare(&mut context, &mut typed);
    assert_eq!(typed, "one beta gamma");
}
