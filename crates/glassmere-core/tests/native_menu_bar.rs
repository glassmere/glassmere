//! `Ui::menu_bar` draws a menu bar unless the host shows the same menu in one
//! of the window's own. The host is a test here.
mod support;

use glassmere_core::{App, Command, Context, Effects, Menu, MenuEntry, Ui};
use glassmere_scene::Size;
use support::Host;

const ROOT: Size = Size::new(400.0, 300.0);

fn menu() -> Menu {
    Menu::new([MenuEntry::submenu(
        "File",
        [MenuEntry::Command(Command::new("Open", None))],
    )])
}

/// A view that declares its menu and asks for it to be drawn too.
struct Shell;

impl App for Shell {
    type Message = ();

    fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}

    fn view(&self, ui: &mut Ui<'_, ()>) {
        ui.keyed("bar", |ui| {
            ui.set_menu(menu());
            ui.menu_bar(&menu());
        });
    }
}

fn drawn(context: &Context) -> bool {
    context.find(&["bar", "menu0"]).is_some()
}

#[test]
fn the_bar_is_drawn_until_the_host_shows_it() {
    let mut h = Host::new(Shell, ROOT);
    h.frame();
    assert!(drawn(&h.context));
    // The host took the menu but has no bar of its own for it.
    assert_eq!(h.context.take_menu(), Some(menu()));
    h.frame();
    assert!(drawn(&h.context));
}

#[test]
fn the_bar_is_left_out_once_the_host_shows_it() {
    let mut h = Host::new(Shell, ROOT);
    h.frame();
    assert_eq!(h.context.take_menu(), Some(menu()));
    h.context.set_native_menu_bar(true);
    h.frame();
    assert!(!drawn(&h.context));
    // The host lost it again, say because the window was given a title bar.
    h.context.set_native_menu_bar(false);
    h.frame();
    assert!(drawn(&h.context));
}

#[test]
fn a_menu_the_host_does_not_show_is_still_drawn() {
    struct Other;
    impl App for Other {
        type Message = ();
        fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}
        fn view(&self, ui: &mut Ui<'_, ()>) {
            ui.keyed("bar", |ui| {
                ui.set_menu(Menu::default());
                ui.menu_bar(&menu());
            });
        }
    }
    let mut h = Host::new(Other, ROOT);
    h.frame();
    h.context.take_menu();
    h.context.set_native_menu_bar(true);
    h.frame();
    assert!(drawn(&h.context));
}
