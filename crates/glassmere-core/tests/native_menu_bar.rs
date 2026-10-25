//! What the host is told about the menu a view declares with `Ui::set_menu`:
//! whether the view drew it itself, which is when the host leaves out a menu
//! bar of the window's own.
mod support;

use glassmere_core::{App, AppBar, Command, Effects, Menu, MenuEntry, Ui};
use glassmere_scene::Size;
use support::Host;

const ROOT: Size = Size::new(400.0, 300.0);

fn menu() -> Menu {
    Menu::new([MenuEntry::submenu(
        "File",
        [MenuEntry::Command(Command::new("Open", None))],
    )])
}

fn other() -> Menu {
    Menu::new([MenuEntry::submenu(
        "Edit",
        [MenuEntry::Command(Command::new("Copy", None))],
    )])
}

/// A view that declares `menu()` and draws what it is told to.
struct Shell(fn(&mut Ui<'_, ()>));

impl App for Shell {
    type Message = ();

    fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}

    fn view(&self, ui: &mut Ui<'_, ()>) {
        ui.set_menu(menu());
        (self.0)(ui);
    }
}

fn drawn(view: fn(&mut Ui<'_, ()>)) -> bool {
    let mut h = Host::new(Shell(view), ROOT);
    h.frame();
    h.context.menu_drawn()
}

#[test]
fn a_menu_nobody_draws_is_the_hosts_to_show() {
    assert!(!drawn(|_| {}));
}

#[test]
fn a_menu_in_the_app_bar_or_in_a_bar_of_the_body_is_drawn() {
    assert!(drawn(|ui| ui.app_bar(AppBar::new().menu(menu()))));
    assert!(drawn(|ui| ui.menu_bar(&menu())));
}

#[test]
fn another_menu_drawn_is_not_the_one_declared() {
    assert!(!drawn(|ui| ui.menu_bar(&other())));
    assert!(!drawn(|ui| ui.app_bar(AppBar::new())));
}

#[test]
fn a_body_bar_is_a_bar_of_its_own_under_the_app_bar() {
    let mut h = Host::new(
        Shell(|ui| {
            ui.app_bar(AppBar::new());
            ui.keyed("bar", |ui| ui.menu_bar(&menu()));
        }),
        ROOT,
    );
    h.frame();
    assert!(h.context.find(&["$title-bar", "$menu"]).is_none());
    let bar = h.context.node(h.context.find(&["$title-bar"]).unwrap());
    let body = h.context.node(h.context.find(&["bar"]).unwrap());
    assert!(body.unwrap().bounds.y >= bar.unwrap().bounds.height);
    assert!(h.context.menu_drawn());
}

#[test]
fn the_menu_is_given_to_the_host_once() {
    let mut h = Host::new(Shell(|_| {}), ROOT);
    h.frame();
    assert_eq!(h.context.take_menu(), Some(menu()));
    h.frame();
    assert_eq!(h.context.take_menu(), None);
}
