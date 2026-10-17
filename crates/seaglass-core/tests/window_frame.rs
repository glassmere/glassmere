//! A window that draws its own title bar: moving it, sizing it and the three
//! buttons. The host is a test here, taking the commands and answering with
//! what it did.
use seaglass_core::{
    App, AppDriver, Button, Context, Effects, Event, Length, MonospaceMeasurer, ResizeEdge, Style,
    Ui, WindowCommand, WindowInfo,
};
use seaglass_scene::{Point, Size};
use std::time::Duration;

const ROOT: Size = Size::new(400.0, 300.0);

#[derive(Clone)]
enum Msg {
    Menu,
    Window(WindowCommand),
}

#[derive(Default)]
struct Chrome {
    clicks: usize,
}

impl App for Chrome {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Menu => self.clicks += 1,
            Msg::Window(command) => effects.window_command(command),
        }
    }

    /// A window with a title bar of its own: a label, a button inside the bar,
    /// the window buttons, and the resize edges over everything.
    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("bar", |ui| {
            ui.title_bar(
                Style {
                    padding: 6.0,
                    gap: 8.0,
                    width: Length::Percent(1.0),
                    ..Style::default()
                },
                |ui| {
                    ui.keyed("title", |ui| ui.label("My application"));
                    ui.keyed("menu", |ui| {
                        ui.button(Button::new("File").on_click(Msg::Menu))
                    });
                    // Empty space, which is where a title bar is grabbed.
                    ui.keyed("gap", |ui| ui.container(Style::default().grow(1.0), |_| {}));
                    ui.keyed("buttons", |ui| ui.window_buttons(Msg::Window));
                },
            )
        });
        ui.keyed("body", |ui| ui.label("Content"));
        ui.keyed("edges", |ui| ui.resize_frame(6.0));
    }
}

/// The application, its driver and the context, run together as a window would.
struct Harness {
    context: Context,
    driver: AppDriver<Msg>,
    app: Chrome,
}

impl Harness {
    fn new() -> Self {
        Self {
            context: Context::new(),
            driver: AppDriver::new(|| true),
            app: Chrome::default(),
        }
    }

    fn frame(&mut self) {
        self.driver.frame(
            &mut self.app,
            &mut self.context,
            ROOT,
            &mut MonospaceMeasurer,
        );
    }
}

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

#[test]
fn pressing_the_bar_moves_the_window_and_pressing_it_twice_maximizes_it() {
    let mut h = Harness::new();
    h.frame();
    assert!(h.context.take_window_commands().is_empty());

    // The empty stretch of the strip, which is where it is grabbed.
    let empty = centre(&h.context, &["bar", "gap"]);
    h.context.set_now(Duration::from_millis(100));
    press(&mut h.context, empty);
    assert_eq!(h.context.take_window_commands(), [WindowCommand::Drag]);

    // Again, soon and near: that is the gesture for maximizing.
    h.context.set_now(Duration::from_millis(300));
    press(&mut h.context, empty);
    assert_eq!(
        h.context.take_window_commands(),
        [WindowCommand::ToggleMaximize]
    );
    // A third, long after the second, is a move again.
    h.context.set_now(Duration::from_secs(2));
    press(&mut h.context, empty);
    assert_eq!(h.context.take_window_commands(), [WindowCommand::Drag]);
}

#[test]
fn a_control_in_the_bar_is_pressed_rather_than_the_window_moved() {
    let mut h = Harness::new();
    h.frame();
    let at = centre(&h.context, &["bar", "menu"]);
    press(&mut h.context, at);
    assert!(
        h.context.take_window_commands().is_empty(),
        "the button took the press"
    );
    h.frame();
    assert_eq!(h.app.clicks, 1);
}

#[test]
fn the_window_buttons_ask_for_what_they_say() {
    let mut h = Harness::new();
    h.frame();
    for (key, command) in [
        ("minimize", WindowCommand::Minimize),
        ("maximize", WindowCommand::ToggleMaximize),
        ("close", WindowCommand::Close),
    ] {
        let at = centre(&h.context, &["bar", "buttons", key]);
        press(&mut h.context, at);
        // The button sends its command as a message; the app asks the window
        // for it, and the host takes it from the batch.
        h.frame();
        assert_eq!(h.context.take_window_commands(), [command], "{key}");
    }
}

#[test]
fn the_middle_button_follows_what_the_host_says_about_the_window() {
    let mut h = Harness::new();
    let glyph = |context: &Context| {
        context
            .details(context.find(&["bar", "buttons", "maximize"]).unwrap())
            .unwrap()
            .text
    };
    h.frame();
    let restored = glyph(&h.context);
    h.context.set_window_info(WindowInfo {
        maximized: true,
        focused: true,
    });
    h.frame();
    assert_ne!(glyph(&h.context), restored, "it says restore now");
    h.context.set_window_info(WindowInfo::default());
    h.frame();
    assert_eq!(glyph(&h.context), restored);
}

#[test]
fn the_edges_lie_along_the_window_and_each_one_pulls_its_own_way() {
    let mut h = Harness::new();
    h.frame();
    // The first frame does not know the size yet; the second does.
    h.frame();
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
        press(&mut h.context, inside);
        assert_eq!(
            h.context.take_window_commands(),
            [WindowCommand::Resize(edge)],
            "{}",
            edge.name()
        );
        // And the host is told which cursor to show there.
        let hovered = h.context.hovered().expect("a grip under the pointer");
        assert_eq!(h.context.resize_grip(hovered), Some(edge));
    }
    // The middle of the window is not an edge.
    press(&mut h.context, Point { x: 200.0, y: 150.0 });
    assert!(h.context.take_window_commands().is_empty());
}

#[test]
fn a_window_too_small_for_its_own_edges_has_none() {
    let mut context = Context::new();
    context.run(Size::new(8.0, 8.0), &mut MonospaceMeasurer, |ui| {
        ui.keyed("edges", |ui| ui.resize_frame(6.0));
    });
    context.run(Size::new(8.0, 8.0), &mut MonospaceMeasurer, |ui| {
        ui.keyed("edges", |ui| ui.resize_frame(6.0));
    });
    assert!(context.find(&["edges", "north"]).is_none());
}
