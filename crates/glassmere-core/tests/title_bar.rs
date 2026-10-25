//! The title bar the framework draws for a window that declares a menu, and
//! the menu bar it draws there. The host is a test here: it switches the bar
//! on, as it does once the system's title bar is off, and takes the window
//! commands.
mod support;

use glassmere_core::{
    App, Command, Context, Effects, Event, Menu, MenuEntry, MenuItem, Role, Ui, WindowCommand,
};
use glassmere_scene::{Point, Size};
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

/// A view that declares its menu, asks for it to be drawn too, and has some
/// content that fills the window.
struct Shell;

impl App for Shell {
    type Message = ();

    fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}

    fn view(&self, ui: &mut Ui<'_, ()>) {
        ui.set_menu(menu());
        ui.keyed("bar", |ui| ui.menu_bar(&menu()));
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

fn with_title_bar() -> Host<Shell> {
    let mut h = Host::new(Shell, ROOT);
    h.frame();
    h.context.set_title_bar(Some("Shell".to_owned()));
    h.frame();
    h
}

#[test]
fn a_window_has_no_title_bar_until_the_host_asks_for_one() {
    let mut h = Host::new(Shell, ROOT);
    h.frame();
    assert!(h.context.find(&["$title-bar"]).is_none());
    // Its own menu bar is what it has.
    assert!(h.context.find(&["bar", "menu0"]).is_some());
}

#[test]
fn the_title_bar_holds_the_title_the_declared_menu_and_the_window_buttons() {
    let h = with_title_bar();
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
fn the_views_own_menu_bar_draws_nothing_while_the_title_bar_shows_the_menu() {
    let mut h = with_title_bar();
    assert!(h.context.find(&["bar", "menu0"]).is_none());
    h.context.set_title_bar(None);
    h.frame();
    assert!(h.context.find(&["$title-bar"]).is_none());
    assert!(h.context.find(&["bar", "menu0"]).is_some());
}

#[test]
fn the_content_sits_under_the_bar_and_fills_what_is_left() {
    let h = with_title_bar();
    let bar = h.context.node(h.context.find(&["$title-bar"]).unwrap());
    let content = h.context.node(h.context.find(&["$content"]).unwrap());
    let (bar, content) = (bar.unwrap().bounds, content.unwrap().bounds);
    assert_eq!(bar.y, 0.0);
    assert_eq!(bar.width, ROOT.width);
    assert!(content.y >= bar.y + bar.height);
    assert!((content.y + content.height - ROOT.height).abs() < 0.5);
}

#[test]
fn the_buttons_ask_the_window_without_the_app_handling_a_message() {
    let mut h = with_title_bar();
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
    let mut h = with_title_bar();
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

#[test]
fn a_separator_is_a_rule_in_the_dropdown_and_no_item() {
    let mut h = with_title_bar();
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
