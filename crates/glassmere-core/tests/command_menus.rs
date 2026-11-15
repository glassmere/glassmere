//! Menus drawn from a `Menu`: the button `Ui::command_menu` and the area
//! `Ui::command_context_menu`. Their submenus, keys and picks are those of
//! `Ui::menu_bar`, so these tests walk the same ground from the other entry.
mod support;

use glassmere_core::{
    Action, Command, Context, Direction, Event, Key, Menu, MenuEntry, NodeSnapshot, Role, Shortcut,
    Style, Ui,
};
use glassmere_scene::{Point, Rect, Size};
use support::{Driven, act, shows};

/// Room for three menus side by side.
const WIDE: Size = Size::new(900.0, 400.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = glassmere_core::app::Action<S>;

#[derive(Default)]
struct Page {
    picked: Vec<&'static str>,
}

/// Open, a rule, Recent (a.txt and More: b.txt and a c.txt that cannot be
/// chosen), Save, and a submenu with nothing in it that can be chosen.
fn menu() -> Menu {
    let command = |id: &str, label: &str| MenuEntry::Command(Command::with_id(id, label, None));
    let unavailable = |id: &str, label: &str| {
        MenuEntry::Command(Command {
            enabled: false,
            ..Command::with_id(id, label, None)
        })
    };
    Menu::new([
        MenuEntry::Command(Command::with_id(
            "open",
            "Open...",
            Some(Shortcut::primary('o')),
        )),
        MenuEntry::Separator,
        MenuEntry::submenu(
            "Recent",
            [
                command("a", "a.txt"),
                MenuEntry::submenu("More", [command("b", "b.txt"), unavailable("c", "c.txt")]),
            ],
        ),
        command("save", "Save"),
        MenuEntry::submenu("Unavailable", [unavailable("d", "d.txt")]),
    ])
}

/// The commands the menu names, declared as a view does: choosing an item
/// activates the command whose button this is.
fn commands(ui: &mut Ui<'_, Handler<Page>>) {
    for id in ["open", "a", "b", "c", "d", "save"] {
        ui.keyed(id, |ui| {
            ui.command_button(
                &Command::with_id(id, id, None),
                act(move |state: &mut Page| state.picked.push(id)),
            );
        });
    }
}

fn button_page(_: &Page, ui: &mut Ui<'_, Handler<Page>>) {
    ui.keyed("file", |ui| {
        ui.command_menu("File", &menu());
    });
    commands(ui);
}

const BUTTON: [&str; 1] = ["file"];

fn at<'a>(base: &[&'a str], tail: &[&'a str]) -> Vec<&'a str> {
    base.iter().chain(tail).copied().collect()
}

fn node(context: &Context, path: &[&str]) -> NodeSnapshot {
    context.node(context.find(path).unwrap()).unwrap()
}

fn press(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

fn point(rect: Rect) -> Point {
    Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }
}

fn open_button(driven: &mut Driven<Page>) {
    let button = driven.context.find(&BUTTON).unwrap();
    assert!(driven.context.action(button, Action::Expand));
    driven.frame();
}

/// The submenu panels under the menu `base` opened: Recent, then More.
fn recent(base: &[&str]) -> Vec<String> {
    at(base, &["$popup", "2", "$popup"])
        .into_iter()
        .map(String::from)
        .collect()
}

fn more(base: &[&str]) -> Vec<String> {
    at(base, &["$popup", "2", "$popup", "1", "$popup"])
        .into_iter()
        .map(String::from)
        .collect()
}

fn found(context: &Context, path: &[String]) -> Option<NodeSnapshot> {
    let path: Vec<&str> = path.iter().map(String::as_str).collect();
    context.find(&path).and_then(|id| context.node(id))
}

#[test]
fn a_command_menu_is_a_menu_button_that_opens_its_menu_with_rules_hints_and_submenus() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    let button = node(&driven.context, &BUTTON);
    assert_eq!(button.semantics.role, Role::MenuButton);
    assert_eq!(button.semantics.label, "File");
    assert_eq!(button.expanded, Some(false));
    assert!(!button.semantics.disabled);
    assert!(shows(&driven.context, "File  ▾"));
    assert_eq!((button.bounds.width, button.bounds.height), (220.0, 44.0));

    open_button(&mut driven);
    let button = node(&driven.context, &BUTTON);
    assert_eq!(button.expanded, Some(true));
    let panel = node(&driven.context, &at(&BUTTON, &["$popup"]));
    assert_eq!(panel.semantics.role, Role::Menu);
    assert!(panel.is_popup);
    assert_eq!(panel.bounds.width, 220.0, "as wide as the button");
    // The shortcut is written beside the item, and read with it.
    let open = node(&driven.context, &at(&BUTTON, &["$popup", "0"]));
    assert_eq!(open.semantics.role, Role::MenuItem);
    assert_eq!(
        open.semantics.label,
        format!("Open..., {}", Shortcut::primary('o').describe())
    );
    assert!(shows(&driven.context, &Shortcut::primary('o').describe()));
    // The rule is a row that cannot be chosen.
    let rule = node(&driven.context, &at(&BUTTON, &["$popup", "1"]));
    assert!(rule.semantics.disabled);
    // A submenu is one row, closed until it is asked for.
    let recent_row = node(&driven.context, &at(&BUTTON, &["$popup", "2"]));
    assert_eq!(recent_row.semantics.role, Role::MenuItem);
    assert_eq!(recent_row.semantics.label, "Recent");
    assert_eq!(recent_row.expanded, Some(false));
    assert!(found(&driven.context, &recent(&BUTTON)).is_none());
    // One with nothing in it that can be chosen is there and cannot be.
    let unavailable = node(&driven.context, &at(&BUTTON, &["$popup", "4"]));
    assert!(unavailable.semantics.disabled);
}

#[test]
fn a_command_menu_nests_from_the_keyboard_and_a_pick_activates_its_command() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    open_button(&mut driven);
    // Down skips the rule, and the arrow that points the reading way opens
    // the submenu of the row.
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    let row = node(&driven.context, &at(&BUTTON, &["$popup", "2"]));
    assert_eq!(row.expanded, Some(true));
    let panel = found(&driven.context, &recent(&BUTTON)).expect("Recent is open");
    assert_eq!(panel.semantics.role, Role::Menu);
    // Beside its row, lapping its end, and level with it.
    assert!((panel.bounds.x - (row.bounds.x + row.bounds.width - 2.0)).abs() < 0.5);
    assert!((panel.bounds.y - (row.bounds.y - 4.0)).abs() < 0.5);
    // The row with the keyboard is reported on the button, which has the focus.
    let button = node(&driven.context, &BUTTON);
    let first = driven
        .context
        .find(&at(&BUTTON, &["$popup", "2", "$popup", "0"]))
        .unwrap();
    assert!(button.focused);
    assert_eq!(button.active_descendant, Some(first));

    // A third level, the same way, with Enter.
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::Enter);
    driven.frame();
    let panel = found(&driven.context, &more(&BUTTON)).expect("More is open");
    let parent = node(
        &driven.context,
        &at(&BUTTON, &["$popup", "2", "$popup", "1"]),
    );
    assert!((panel.bounds.x - (parent.bounds.x + parent.bounds.width - 2.0)).abs() < 0.5);
    // The command that cannot be chosen is there and stays unchosen.
    let blocked = node(
        &driven.context,
        &at(&BUTTON, &["$popup", "2", "$popup", "1", "$popup", "1"]),
    );
    assert!(blocked.semantics.disabled);

    // The other arrow goes back out one level at a time.
    press(&mut driven.context, Key::ArrowLeft);
    driven.frame();
    assert!(found(&driven.context, &more(&BUTTON)).is_none());
    assert!(found(&driven.context, &recent(&BUTTON)).is_some());
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert!(found(&driven.context, &more(&BUTTON)).is_some());

    // Enter on a command closes every level and activates the command, which
    // the host is asked to declare once more to hear of.
    press(&mut driven.context, Key::Enter);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["b"]);
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_none());
    assert!(node(&driven.context, &BUTTON).focused);
    driven.frame();
    assert_eq!(driven.state().picked, ["b"], "a pick is sent once");

    // Escape leaves a submenu first, then the menu, and picks nothing.
    open_button(&mut driven);
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    press(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(found(&driven.context, &recent(&BUTTON)).is_none());
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_some());
    press(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_none());
    assert_eq!(driven.state().picked, ["b"]);

    // A command of the first level is chosen the same way; the menu opens
    // with the first thing that can be chosen highlighted.
    open_button(&mut driven);
    press(&mut driven.context, Key::Enter);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["b", "open"]);
}

#[test]
fn a_command_menu_opens_a_submenu_at_once_on_a_click_and_picks_with_the_pointer() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    open_button(&mut driven);
    let row = node(&driven.context, &at(&BUTTON, &["$popup", "2"])).bounds;
    driven.context.event(Event::PointerMoved(point(row)));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert!(
        found(&driven.context, &recent(&BUTTON)).is_some(),
        "no waiting for a click"
    );
    assert!(driven.state().picked.is_empty(), "a submenu is not a pick");
    // A click on a command in it picks that command.
    let a = node(
        &driven.context,
        &at(&BUTTON, &["$popup", "2", "$popup", "0"]),
    );
    driven.context.event(Event::PointerMoved(point(a.bounds)));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["a"]);
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_none());

    // A press outside every level closes them all and picks nothing.
    open_button(&mut driven);
    let row = node(&driven.context, &at(&BUTTON, &["$popup", "2"])).bounds;
    driven.context.event(Event::PointerMoved(point(row)));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    driven.context.event(Event::PointerMoved(Point {
        x: WIDE.width - 5.0,
        y: WIDE.height - 5.0,
    }));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_none());
    assert_eq!(driven.state().picked, ["a"]);
}

#[test]
fn assistive_technology_expands_collapses_and_activates_through_the_nested_menu() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    open_button(&mut driven);
    let row = driven.context.find(&at(&BUTTON, &["$popup", "2"])).unwrap();
    assert!(driven.context.action(row, Action::Expand));
    driven.frame();
    assert_eq!(driven.context.node(row).unwrap().expanded, Some(true));
    let inner = driven
        .context
        .find(&at(&BUTTON, &["$popup", "2", "$popup", "1"]))
        .unwrap();
    assert!(driven.context.action(inner, Action::Expand));
    driven.frame();
    assert!(found(&driven.context, &more(&BUTTON)).is_some());
    // A command that cannot be chosen takes no activation.
    let blocked = driven
        .context
        .find(&at(&BUTTON, &["$popup", "2", "$popup", "1", "$popup", "1"]))
        .unwrap();
    assert!(!driven.context.action(blocked, Action::Activate));
    driven.frame();
    assert!(driven.state().picked.is_empty());
    // One that can is picked from any depth.
    let b = driven
        .context
        .find(&at(&BUTTON, &["$popup", "2", "$popup", "1", "$popup", "0"]))
        .unwrap();
    assert!(driven.context.action(b, Action::Activate));
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["b"]);
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_none());
    assert_eq!(node(&driven.context, &BUTTON).expanded, Some(false));

    // Collapsing the button closes the levels under it.
    open_button(&mut driven);
    let row = driven.context.find(&at(&BUTTON, &["$popup", "2"])).unwrap();
    driven.context.action(row, Action::Expand);
    driven.frame();
    let button = driven.context.find(&BUTTON).unwrap();
    assert!(driven.context.action(button, Action::Collapse));
    driven.frame();
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_none());
    assert_eq!(driven.state().picked, ["b"]);
}

#[test]
fn a_command_menu_sends_only_what_can_be_chosen() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    open_button(&mut driven);
    // A submenu whose commands are all disabled does not open.
    let unavailable = driven.context.find(&at(&BUTTON, &["$popup", "4"])).unwrap();
    assert!(!driven.context.action(unavailable, Action::Expand));
    press(&mut driven.context, Key::End);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert!(
        driven
            .context
            .find(&at(&BUTTON, &["$popup", "4", "$popup"]))
            .is_none()
    );
    // The rule cannot be activated either.
    let rule = driven.context.find(&at(&BUTTON, &["$popup", "1"])).unwrap();
    assert!(!driven.context.action(rule, Action::Activate));
    driven.frame();
    assert!(driven.state().picked.is_empty());

    // The same menu with everything disabled is a disabled button.
    driven.set_view(|_, ui| {
        ui.keyed("file", |ui| {
            let off = Menu::new([MenuEntry::Command(Command {
                enabled: false,
                ..Command::with_id("open", "Open", None)
            })]);
            ui.command_menu("File", &off);
        });
    });
    driven.frame();
    let button = node(&driven.context, &BUTTON);
    assert!(button.semantics.disabled);
    assert!(
        !driven
            .context
            .action(driven.context.find(&BUTTON).unwrap(), Action::Expand)
    );
    // And a menu that is not enabled as a whole is as well.
    driven.set_view(|_, ui| {
        ui.keyed("file", |ui| {
            ui.enabled(false, |ui| {
                ui.command_menu("File", &menu());
            });
        });
    });
    driven.frame();
    assert!(node(&driven.context, &BUTTON).semantics.disabled);
}

#[test]
fn a_right_to_left_command_menu_opens_its_submenus_on_the_left_with_mirrored_arrows() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    driven.context.set_direction(Direction::Rtl);
    driven.frame();
    open_button(&mut driven);
    let row = node(&driven.context, &at(&BUTTON, &["$popup", "2"]));
    // The arrow that points the reading way is the left one.
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert!(
        found(&driven.context, &recent(&BUTTON)).is_none(),
        "the wrong arrow opens nothing"
    );
    press(&mut driven.context, Key::ArrowLeft);
    driven.frame();
    let panel = found(&driven.context, &recent(&BUTTON)).expect("Recent is open");
    assert!(
        (panel.bounds.x + panel.bounds.width - (row.bounds.x + 2.0)).abs() < 0.5,
        "{:?} beside {:?}",
        panel.bounds,
        row.bounds
    );
    assert!(panel.bounds.x >= 0.0);
    // And the other one goes back out.
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert!(found(&driven.context, &recent(&BUTTON)).is_none());
    assert!(driven.context.find(&at(&BUTTON, &["$popup"])).is_some());
}

#[test]
fn a_command_menu_opens_a_submenu_on_the_other_side_where_the_window_ends() {
    // The menu is far enough along for a submenu to fit before it only.
    let mut driven = Driven::new(Size::new(600.0, 500.0), Page::default(), |_, ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default().padding(250.0), |ui| {
                ui.keyed("file", |ui| {
                    ui.command_menu("File", &menu());
                });
            });
        });
        commands(ui);
    });
    let base = ["page", "file"];
    let button = driven.context.find(&base).unwrap();
    assert!(driven.context.action(button, Action::Expand));
    driven.frame();
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    let row = node(&driven.context, &at(&base, &["$popup", "2"])).bounds;
    let panel = found(&driven.context, &recent(&base)).expect("Recent is open");
    assert!(row.x + row.width + panel.bounds.width > 600.0, "{row:?}");
    assert!(
        (panel.bounds.x + panel.bounds.width - (row.x + 2.0)).abs() < 0.5,
        "{:?} beside {row:?}",
        panel.bounds
    );
    assert!(panel.bounds.x >= 0.0);
}
