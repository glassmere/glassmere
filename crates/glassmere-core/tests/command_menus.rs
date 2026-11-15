//! Menus drawn from a `Menu`: the button `Ui::command_menu` and the area
//! `Ui::command_context_menu`. Their submenus, keys and picks are those of
//! `Ui::menu_bar`, so these tests walk the same ground from the other entry.
mod support;

use glassmere_core::{
    Action, Command, Context, Direction, Event, Key, Length, Menu, MenuEntry, NodeSnapshot, Role,
    Shortcut, Style, TouchPhase, Ui,
};
use glassmere_scene::{Point, Rect, Size};
use std::time::Duration;
use support::{Driven, act, center, shows};

/// Room for a menu and the submenus that open beside it.
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

fn area_page(_: &Page, ui: &mut Ui<'_, Handler<Page>>) {
    ui.keyed("area", |ui| {
        ui.command_context_menu(
            &menu(),
            Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Px(200.0)),
            |ui| {
                ui.keyed("text", |ui| ui.label("Right-click me"));
                ui.keyed("inner", |ui| ui.button("Inner"));
            },
        );
    });
    ui.keyed("outside", |ui| ui.button("Outside"));
    commands(ui);
}

const AREA: [&str; 1] = ["area"];

fn open_area(driven: &mut Driven<Page>, at: Point) {
    assert!(driven.context.event(Event::ContextMenu(at)));
    driven.frame();
}

#[test]
fn a_command_context_menu_opens_at_the_pointer_with_rules_hints_and_submenus() {
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());
    assert!(
        !driven
            .context
            .event(Event::ContextMenu(Point { x: 10.0, y: 300.0 })),
        "outside the area"
    );
    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    let panel = node(&driven.context, &at(&AREA, &["$popup"]));
    assert_eq!((panel.bounds.x, panel.bounds.y), (100.0, 60.0));
    assert_eq!(panel.bounds.width, 220.0);
    assert_eq!(panel.semantics.role, Role::Menu);
    assert!(panel.is_popup);
    let open = node(&driven.context, &at(&AREA, &["$popup", "0"]));
    assert_eq!(open.semantics.role, Role::MenuItem);
    assert_eq!(
        open.semantics.label,
        format!("Open..., {}", Shortcut::primary('o').describe())
    );
    assert!(
        node(&driven.context, &at(&AREA, &["$popup", "1"]))
            .semantics
            .disabled
    );
    let row = node(&driven.context, &at(&AREA, &["$popup", "2"]));
    assert_eq!(row.semantics.label, "Recent");
    assert_eq!(row.expanded, Some(false));
    assert!(
        node(&driven.context, &at(&AREA, &["$popup", "4"]))
            .semantics
            .disabled
    );
}

#[test]
fn a_command_context_menu_nests_from_the_keyboard_and_a_pick_activates_its_command() {
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    // The keyboard opens the menu below the focused control inside the area.
    let inner = driven.context.find(&at(&AREA, &["inner"])).unwrap();
    driven.context.action(inner, Action::Focus);
    let inner_bounds = driven.context.node(inner).unwrap().bounds;
    press(&mut driven.context, Key::ContextMenu);
    driven.frame();
    let panel = node(&driven.context, &at(&AREA, &["$popup"]));
    assert_eq!(
        (panel.bounds.x, panel.bounds.y),
        (inner_bounds.x, inner_bounds.y + inner_bounds.height)
    );
    // Escape closes it, and the focus is where it was.
    press(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());
    assert!(node(&driven.context, &at(&AREA, &["inner"])).focused);

    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    let row = node(&driven.context, &at(&AREA, &["$popup", "2"]));
    assert_eq!(row.expanded, Some(true));
    let panel = found(&driven.context, &recent(&AREA)).expect("Recent is open");
    assert!((panel.bounds.x - (row.bounds.x + row.bounds.width - 2.0)).abs() < 0.5);
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::Enter);
    driven.frame();
    assert!(found(&driven.context, &more(&AREA)).is_some());
    press(&mut driven.context, Key::Enter);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["b"]);
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());
    assert!(node(&driven.context, &at(&AREA, &["inner"])).focused);
    driven.frame();
    assert_eq!(driven.state().picked, ["b"], "a pick is sent once");

    // Left goes back out a level, and Escape out of the menu, with no pick.
    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    press(&mut driven.context, Key::ArrowLeft);
    driven.frame();
    assert!(found(&driven.context, &recent(&AREA)).is_none());
    press(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());
    assert_eq!(driven.state().picked, ["b"]);

    // A command on the first level is chosen the same way.
    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    press(&mut driven.context, Key::Enter);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["b", "open"]);
}

#[test]
fn a_command_context_menu_opens_a_submenu_at_once_on_a_click_and_picks_with_the_pointer() {
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    let row = node(&driven.context, &at(&AREA, &["$popup", "2"])).bounds;
    driven.context.event(Event::PointerMoved(point(row)));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert!(found(&driven.context, &recent(&AREA)).is_some());
    assert!(driven.state().picked.is_empty(), "a submenu is not a pick");
    let a = node(&driven.context, &at(&AREA, &["$popup", "2", "$popup", "0"]));
    driven.context.event(Event::PointerMoved(point(a.bounds)));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["a"]);
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());

    // A press outside every level closes them all and picks nothing.
    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    let row = node(&driven.context, &at(&AREA, &["$popup", "2"])).bounds;
    driven.context.event(Event::PointerMoved(point(row)));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    let outside = center(&driven.context, &["outside"]);
    driven.context.event(Event::PointerMoved(outside));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());
    assert_eq!(driven.state().picked, ["a"]);
}

#[test]
fn a_resting_touch_opens_a_command_context_menu_and_a_tap_in_a_submenu_picks() {
    let touch = |context: &mut Context, id: u64, phase: TouchPhase, position: Point| {
        context.event(Event::Touch {
            id,
            phase,
            position,
            force: None,
            pen: None,
        })
    };
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    let text = center(&driven.context, &at(&AREA, &["text"]));
    driven.context.set_now(Duration::from_secs(1));
    touch(&mut driven.context, 1, TouchPhase::Started, text);
    driven.context.set_now(Duration::from_millis(1500));
    driven.frame();
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_some());
    touch(&mut driven.context, 1, TouchPhase::Ended, text);
    driven.frame();
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_some());
    // A tap on a submenu row opens it; a tap in it picks.
    let row = node(&driven.context, &at(&AREA, &["$popup", "2"])).bounds;
    touch(&mut driven.context, 2, TouchPhase::Started, point(row));
    touch(&mut driven.context, 2, TouchPhase::Ended, point(row));
    driven.frame();
    let a = node(&driven.context, &at(&AREA, &["$popup", "2", "$popup", "0"]));
    touch(&mut driven.context, 3, TouchPhase::Started, point(a.bounds));
    touch(&mut driven.context, 3, TouchPhase::Ended, point(a.bounds));
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["a"]);
}

#[test]
fn assistive_technology_reaches_the_commands_of_a_command_context_menu() {
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    let row = driven.context.find(&at(&AREA, &["$popup", "2"])).unwrap();
    assert!(driven.context.action(row, Action::Expand));
    driven.frame();
    assert_eq!(driven.context.node(row).unwrap().expanded, Some(true));
    let inner = driven
        .context
        .find(&at(&AREA, &["$popup", "2", "$popup", "1"]))
        .unwrap();
    assert!(driven.context.action(inner, Action::Expand));
    driven.frame();
    assert!(found(&driven.context, &more(&AREA)).is_some());
    let blocked = driven
        .context
        .find(&at(&AREA, &["$popup", "2", "$popup", "1", "$popup", "1"]))
        .unwrap();
    assert!(!driven.context.action(blocked, Action::Activate));
    driven.frame();
    assert!(driven.state().picked.is_empty());
    let unavailable = driven.context.find(&at(&AREA, &["$popup", "4"])).unwrap();
    assert!(!driven.context.action(unavailable, Action::Expand));
    let b = driven
        .context
        .find(&at(&AREA, &["$popup", "2", "$popup", "1", "$popup", "0"]))
        .unwrap();
    assert!(driven.context.action(b, Action::Activate));
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["b"]);
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());
}

#[test]
fn a_right_to_left_command_context_menu_opens_its_submenus_on_the_left() {
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    driven.context.set_direction(Direction::Rtl);
    driven.frame();
    open_area(&mut driven, Point { x: 500.0, y: 60.0 });
    let row = node(&driven.context, &at(&AREA, &["$popup", "2"]));
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert!(
        found(&driven.context, &recent(&AREA)).is_none(),
        "the wrong arrow opens nothing"
    );
    press(&mut driven.context, Key::ArrowLeft);
    driven.frame();
    let panel = found(&driven.context, &recent(&AREA)).expect("Recent is open");
    assert!(
        (panel.bounds.x + panel.bounds.width - (row.bounds.x + 2.0)).abs() < 0.5,
        "{:?} beside {:?}",
        panel.bounds,
        row.bounds
    );
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert!(found(&driven.context, &recent(&AREA)).is_none());
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_some());
}

#[test]
fn a_command_context_menu_opens_a_submenu_on_the_other_side_where_the_window_ends() {
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    // The menu fits, and a submenu beside it would not.
    open_area(&mut driven, Point { x: 640.0, y: 60.0 });
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    let row = node(&driven.context, &at(&AREA, &["$popup", "2"])).bounds;
    let panel = found(&driven.context, &recent(&AREA)).expect("Recent is open");
    assert!(
        row.x + row.width + panel.bounds.width > WIDE.width,
        "{row:?}"
    );
    assert!(
        (panel.bounds.x + panel.bounds.width - (row.x + 2.0)).abs() < 0.5,
        "{:?} beside {row:?}",
        panel.bounds
    );
}

#[test]
fn a_disabled_command_context_menu_does_not_open() {
    let mut driven = Driven::new(WIDE, Page::default(), |_, ui| {
        ui.keyed("area", |ui| {
            ui.enabled(false, |ui| {
                ui.command_context_menu(
                    &menu(),
                    Style::default()
                        .width(Length::Px(400.0))
                        .height(Length::Px(200.0)),
                    |ui| ui.label("Nothing"),
                );
            });
        });
    });
    driven
        .context
        .event(Event::ContextMenu(Point { x: 100.0, y: 60.0 }));
    driven.frame();
    assert!(driven.context.find(&at(&AREA, &["$popup"])).is_none());
}

/// Lets the pointer rest where it is for `millis` more.
fn rest(driven: &mut Driven<Page>, millis: u64) {
    let now = driven.context.now() + Duration::from_millis(millis);
    driven.context.set_now(now);
    driven.frame();
}

fn hover(driven: &mut Driven<Page>, rect: Rect) {
    driven.context.event(Event::PointerMoved(point(rect)));
}

/// A submenu waits for the pointer to rest on its row, stays open while the
/// pointer crosses the next row on the way into it, and closes once the
/// pointer rests on another row. `base` is the menu button or area.
fn hover_walks_the_submenus(driven: &mut Driven<Page>, base: &[&str]) {
    let recent_row = node(&driven.context, &at(base, &["$popup", "2"])).bounds;
    let save_row = node(&driven.context, &at(base, &["$popup", "3"])).bounds;
    let start = driven.context.now();
    hover(driven, recent_row);
    assert_eq!(
        driven.context.next_wake(),
        Some(start + Duration::from_millis(200))
    );
    rest(driven, 199);
    assert!(found(&driven.context, &recent(base)).is_none(), "not yet");
    rest(driven, 1);
    assert!(found(&driven.context, &recent(base)).is_some());
    assert_eq!(driven.context.next_wake(), None, "an open menu is idle");

    // Across the next row and into the submenu before the wait is over.
    hover(driven, save_row);
    rest(driven, 100);
    let a = node(&driven.context, &at(base, &["$popup", "2", "$popup", "0"])).bounds;
    hover(driven, a);
    rest(driven, 1000);
    assert!(found(&driven.context, &recent(base)).is_some());
    assert!(driven.state().picked.is_empty());

    // Resting on the next row takes the menu back.
    hover(driven, save_row);
    rest(driven, 199);
    assert!(found(&driven.context, &recent(base)).is_some());
    rest(driven, 1);
    assert!(found(&driven.context, &recent(base)).is_none());
    assert!(driven.context.find(&at(base, &["$popup"])).is_some());
}

#[test]
fn a_command_menu_submenu_waits_for_the_pointer_and_stays_on_the_way_to_it() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    open_button(&mut driven);
    driven.context.set_now(Duration::from_secs(1));
    hover_walks_the_submenus(&mut driven, &BUTTON);
}

#[test]
fn a_command_context_menu_submenu_waits_for_the_pointer_and_stays_on_the_way_to_it() {
    let mut driven = Driven::new(WIDE, Page::default(), area_page);
    open_area(&mut driven, Point { x: 100.0, y: 60.0 });
    driven.context.set_now(Duration::from_secs(1));
    hover_walks_the_submenus(&mut driven, &AREA);
}

#[test]
fn a_right_to_left_command_menu_opens_a_hovered_submenu_on_the_left() {
    let mut driven = Driven::new(WIDE, Page::default(), button_page);
    driven.context.set_direction(Direction::Rtl);
    driven.frame();
    open_button(&mut driven);
    let row = node(&driven.context, &at(&BUTTON, &["$popup", "2"])).bounds;
    hover(&mut driven, row);
    rest(&mut driven, 199);
    assert!(found(&driven.context, &recent(&BUTTON)).is_none());
    rest(&mut driven, 1);
    let panel = found(&driven.context, &recent(&BUTTON)).expect("Recent is open");
    assert!(
        (panel.bounds.x + panel.bounds.width - (row.x + 2.0)).abs() < 0.5,
        "{:?} beside {row:?}",
        panel.bounds
    );
}
