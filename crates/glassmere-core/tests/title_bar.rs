//! The app bar: the title bar a view places itself, with the menu in it. The
//! host is a test here: it takes the window commands.
mod support;

use glassmere_core::{
    App, AppBar, Command, Context, Effects, Event, Key, Menu, MenuEntry, MenuItem, ResizeEdge,
    Role, Style, Ui, WindowCommand,
};
use glassmere_scene::{Point, Primitive, Size};
use std::time::Duration;
use support::Host;

const ROOT: Size = Size::new(400.0, 300.0);

fn menu() -> Menu {
    Menu::new([MenuEntry::submenu(
        "File",
        [
            MenuEntry::Command(Command::with_id("open", "Open", None)),
            MenuEntry::Separator,
            MenuEntry::Command(Command::with_id("quit", "Quit", None)),
        ],
    )])
}

/// A view that places what it is told to before a body.
struct Shell(fn(&mut Ui<'_, ()>));

impl App for Shell {
    type Message = ();

    fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}

    fn view(&self, ui: &mut Ui<'_, ()>) {
        (self.0)(ui);
        ui.keyed("body", |ui| ui.label("Content"));
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

fn host(view: fn(&mut Ui<'_, ()>)) -> Host<Shell> {
    let mut h = Host::new(Shell(view), ROOT);
    h.context.set_window_title("Shell");
    h.frame();
    h
}

/// An app bar with the menu in it.
fn shell() -> Host<Shell> {
    host(|ui| ui.app_bar(AppBar::new().menu(menu())))
}

#[test]
fn the_app_bar_holds_the_window_title_the_menu_and_the_window_buttons() {
    let h = shell();
    let title = h.context.find(&["$title-bar", "$title"]).unwrap();
    assert_eq!(h.context.details(title).unwrap().text, "Shell");
    assert!(h.context.find(&["$title-bar", "$menu", "menu0"]).is_some());
    for key in ["minimize", "maximize", "close"] {
        assert!(
            h.context.find(&["$title-bar", "$buttons", key]).is_some(),
            "{key}"
        );
    }
}

#[test]
fn a_bar_without_a_menu_has_none_and_a_title_of_its_own_replaces_the_windows() {
    let h = host(|ui| ui.app_bar(AppBar::new().title("Mine")));
    let title = h.context.find(&["$title-bar", "$title"]).unwrap();
    assert_eq!(h.context.details(title).unwrap().text, "Mine");
    assert!(h.context.find(&["$title-bar", "$menu"]).is_none());
}

#[test]
fn a_window_wants_no_system_title_bar_exactly_while_the_view_places_one() {
    assert!(shell().context.wants_frameless());
    assert!(!host(|_| {}).context.wants_frameless());
    // The low-level strip counts too.
    let strip = host(|ui| ui.title_bar(Style::default(), |ui| ui.label("Mine")));
    assert!(strip.context.wants_frameless());
}

#[test]
fn the_edges_lie_over_the_body_and_the_bar() {
    let mut h = shell();
    // The first frame does not know the size yet.
    h.frame();
    press(&mut h.context, Point { x: 200.0, y: 3.0 });
    assert_eq!(
        h.context.take_window_commands(),
        [WindowCommand::Resize(ResizeEdge::North)]
    );
}

#[test]
fn the_body_sits_under_the_bar() {
    let h = shell();
    let bar = h.context.node(h.context.find(&["$title-bar"]).unwrap());
    let body = h.context.node(h.context.find(&["body"]).unwrap());
    let (bar, body) = (bar.unwrap().bounds, body.unwrap().bounds);
    assert_eq!(bar.y, 0.0);
    assert_eq!(bar.width, ROOT.width);
    assert!(body.y >= bar.y + bar.height);
}

#[test]
fn the_window_buttons_are_where_they_are_drawn_from_the_first_frame() {
    // The platform learns where they are before the pointer ever reaches
    // them: no second frame, no input.
    let h = shell();
    for (key, rect) in [
        ("minimize", h.context.minimize_button()),
        ("maximize", h.context.maximize_button()),
        ("close", h.context.close_button()),
    ] {
        let node = h.context.find(&["$title-bar", "$buttons", key]).unwrap();
        let drawn = h.context.node(node).unwrap().bounds;
        assert!(drawn.width > 0.0, "{key}");
        assert_eq!(rect, Some(drawn), "{key}");
    }
}

#[test]
fn the_buttons_ask_the_window_without_the_app_handling_a_message() {
    let mut h = shell();
    for (key, command) in [
        ("minimize", WindowCommand::Minimize),
        ("maximize", WindowCommand::ToggleMaximize),
        ("close", WindowCommand::Close),
    ] {
        let at = centre(&h.context, &["$title-bar", "$buttons", key]);
        press(&mut h.context, at);
        h.frame();
        assert_eq!(h.context.take_window_commands(), [command], "{key}");
    }
}

#[test]
fn pressing_the_gap_moves_the_window_and_pressing_a_menu_opens_it() {
    let mut h = shell();
    let gap = centre(&h.context, &["$title-bar", "$gap"]);
    h.context.set_now(Duration::from_millis(100));
    press(&mut h.context, gap);
    assert_eq!(h.context.take_window_commands(), [WindowCommand::Drag]);

    let heading = centre(&h.context, &["$title-bar", "$menu", "menu0"]);
    press(&mut h.context, heading);
    h.frame();
    assert!(h.context.take_window_commands().is_empty());
    assert!(
        h.context
            .find(&["$title-bar", "$menu", "menu0", "$popup"])
            .is_some(),
        "the dropdown is open"
    );
}

/// Whether the scene draws the focus colour anywhere.
fn draws_a_focus_ring(context: &Context) -> bool {
    let focus = context.theme().focus;
    context
        .scene()
        .primitives
        .iter()
        .any(|primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == focus))
}

#[test]
fn a_press_in_the_bar_takes_focus_without_a_ring_and_the_keyboard_brings_it_back() {
    let mut h = shell();
    let heading = centre(&h.context, &["$title-bar", "$menu", "menu0"]);
    press(&mut h.context, heading);
    h.frame();
    assert!(h.context.focused().is_some(), "the menu has the keyboard");
    assert!(!draws_a_focus_ring(&h.context));

    // Closing the dropdown and moving on by keyboard shows where focus is.
    h.context.event(Event::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    });
    h.context.event(Event::KeyDown {
        key: Key::Tab,
        shift: false,
        repeat: false,
    });
    h.frame();
    assert!(draws_a_focus_ring(&h.context));
}

#[test]
fn a_separator_is_a_rule_in_the_dropdown_and_no_item() {
    let mut h = shell();
    let heading = centre(&h.context, &["$title-bar", "$menu", "menu0"]);
    press(&mut h.context, heading);
    h.frame();
    let item = |index: &str| {
        h.context
            .find(&["$title-bar", "$menu", "menu0", "$popup", index])
            .unwrap()
    };
    let role = |index: &str| h.context.node(item(index)).unwrap().semantics.role;
    assert_eq!(role("0"), Role::MenuItem);
    assert_ne!(role("1"), Role::MenuItem, "the separator");
    assert_eq!(role("2"), Role::MenuItem);
    assert!(MenuItem::separator().label.is_empty());
}
