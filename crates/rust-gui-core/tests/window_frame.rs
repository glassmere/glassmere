//! A window that draws its own title bar: moving it, sizing it and the three
//! buttons. The host is a test here, taking the commands and answering with
//! what it did.
use rust_gui_core::{
    Context, Event, Length, MonospaceMeasurer, ResizeEdge, Style, Ui, WindowCommand, WindowInfo,
};
use rust_gui_scene::{Point, Size};
use std::time::Duration;

const ROOT: Size = Size::new(400.0, 300.0);

fn centre(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

fn press(context: &mut Context, at: Point) {
    context.event(Event::PointerMoved(at));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

/// A window with a title bar of its own: a label, a button inside the bar,
/// the window buttons, and the resize edges over everything.
fn frame(context: &mut Context, clicks: &mut usize) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.title_bar(
            "bar",
            Style {
                padding: 6.0,
                gap: 8.0,
                width: Length::Percent(1.0),
                ..Style::default()
            },
            |ui| {
                ui.label_keyed("title", "My application");
                if ui.button_keyed("menu", "File").clicked() {
                    *clicks += 1;
                }
                // Empty space, which is where a title bar is grabbed.
                ui.container("gap", Style::default().grow(1.0), |_| {});
                ui.window_buttons("buttons");
            },
        );
        ui.label_keyed("body", "Content");
        ui.resize_frame("edges", 6.0);
    });
}

#[test]
fn pressing_the_bar_moves_the_window_and_pressing_it_twice_maximizes_it() {
    let mut context = Context::new();
    let mut clicks = 0;
    frame(&mut context, &mut clicks);
    assert!(context.take_window_commands().is_empty());

    // The empty stretch of the strip, which is where it is grabbed.
    let empty = centre(&context, &["bar", "gap"]);
    context.set_now(Duration::from_millis(100));
    press(&mut context, empty);
    assert_eq!(context.take_window_commands(), [WindowCommand::Drag]);

    // Again, soon and near: that is the gesture for maximizing.
    context.set_now(Duration::from_millis(300));
    press(&mut context, empty);
    assert_eq!(
        context.take_window_commands(),
        [WindowCommand::ToggleMaximize]
    );
    // A third, long after the second, is a move again.
    context.set_now(Duration::from_secs(2));
    press(&mut context, empty);
    assert_eq!(context.take_window_commands(), [WindowCommand::Drag]);
}

#[test]
fn a_control_in_the_bar_is_pressed_rather_than_the_window_moved() {
    let mut context = Context::new();
    let mut clicks = 0;
    frame(&mut context, &mut clicks);
    let at = centre(&context, &["bar", "menu"]);
    press(&mut context, at);
    assert!(
        context.take_window_commands().is_empty(),
        "the button took the press"
    );
    frame(&mut context, &mut clicks);
    assert_eq!(clicks, 1);
}

#[test]
fn the_window_buttons_ask_for_what_they_say() {
    let mut context = Context::new();
    let mut clicks = 0;
    frame(&mut context, &mut clicks);
    for (key, command) in [
        ("minimize", WindowCommand::Minimize),
        ("maximize", WindowCommand::ToggleMaximize),
        ("close", WindowCommand::Close),
    ] {
        let at = centre(&context, &["bar", "buttons", key]);
        press(&mut context, at);
        // The button reports its click on the next declaration, which is
        // where the command is asked for.
        frame(&mut context, &mut clicks);
        assert_eq!(context.take_window_commands(), [command], "{key}");
    }
}

#[test]
fn the_middle_button_follows_what_the_host_says_about_the_window() {
    let mut context = Context::new();
    let mut clicks = 0;
    let glyph = |context: &Context| {
        context
            .details(context.find(&["bar", "buttons", "maximize"]).unwrap())
            .unwrap()
            .text
    };
    frame(&mut context, &mut clicks);
    let restored = glyph(&context);
    context.set_window_info(WindowInfo {
        maximized: true,
        focused: true,
    });
    frame(&mut context, &mut clicks);
    assert_ne!(glyph(&context), restored, "it says restore now");
    context.set_window_info(WindowInfo::default());
    frame(&mut context, &mut clicks);
    assert_eq!(glyph(&context), restored);
}

#[test]
fn the_edges_lie_along_the_window_and_each_one_pulls_its_own_way() {
    let mut context = Context::new();
    let mut clicks = 0;
    frame(&mut context, &mut clicks);
    // The first frame does not know the size yet; the second does.
    frame(&mut context, &mut clicks);
    for (edge, inside) in [
        (ResizeEdge::North, Point { x: 200.0, y: 3.0 }),
        (ResizeEdge::South, Point { x: 200.0, y: 297.0 }),
        (ResizeEdge::West, Point { x: 3.0, y: 150.0 }),
        (ResizeEdge::East, Point { x: 397.0, y: 150.0 }),
        (ResizeEdge::NorthWest, Point { x: 3.0, y: 3.0 }),
        (ResizeEdge::NorthEast, Point { x: 397.0, y: 3.0 }),
        (ResizeEdge::SouthWest, Point { x: 3.0, y: 297.0 }),
        (ResizeEdge::SouthEast, Point { x: 397.0, y: 297.0 }),
    ] {
        press(&mut context, inside);
        assert_eq!(
            context.take_window_commands(),
            [WindowCommand::Resize(edge)],
            "{}",
            edge.name()
        );
        // And the host is told which cursor to show there.
        let hovered = context.hovered().expect("a grip under the pointer");
        assert_eq!(context.resize_grip(hovered), Some(edge));
    }
    // The middle of the window is not an edge.
    press(&mut context, Point { x: 200.0, y: 150.0 });
    assert!(context.take_window_commands().is_empty());
}

#[test]
fn a_window_too_small_for_its_own_edges_has_none() {
    let mut context = Context::new();
    context.run(Size::new(8.0, 8.0), &mut MonospaceMeasurer, |ui| {
        ui.resize_frame("edges", 6.0);
    });
    context.run(Size::new(8.0, 8.0), &mut MonospaceMeasurer, |ui| {
        ui.resize_frame("edges", 6.0);
    });
    assert!(context.find(&["edges", "north"]).is_none());
}
