//! What a view asks the desktop for, and what comes back. The host is a
//! test here: it takes the requests and answers them.
mod support;

use glassmere_core::{
    Action, App, Command, Context, Effects, Event, FileKind, FileRequest, Key, Menu, MenuEntry,
    MonospaceMeasurer, Notification, Style, Tray, Ui,
};
use glassmere_scene::Size;
use std::path::PathBuf;
use support::{Driven, Host, act};

const ROOT: Size = Size::new(400.0, 300.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = glassmere_core::app::Action<S>;

#[derive(Clone)]
enum FileMsg {
    Save,
    Chosen(Vec<PathBuf>),
}

#[derive(Default)]
struct Files {
    chosen: Option<Vec<PathBuf>>,
}

impl App for Files {
    type Message = FileMsg;

    fn update(&mut self, message: FileMsg, effects: &mut Effects<'_, FileMsg>) {
        match message {
            FileMsg::Save => effects.choose_files(
                FileRequest::save("Save the report")
                    .with_filter("Reports", ["pdf", "html"])
                    .named("report.pdf"),
                FileMsg::Chosen,
            ),
            FileMsg::Chosen(paths) => self.chosen = Some(paths),
        }
    }

    fn view(&self, ui: &mut Ui<'_, FileMsg>) {
        ui.keyed("state", |ui| ui.label("ready"));
    }
}

#[test]
fn a_file_dialog_is_asked_for_once_and_answered_later() {
    let mut host = Host::new(Files::default(), ROOT);
    host.frame();
    assert!(host.context.take_file_request().is_none());
    host.send(FileMsg::Save);
    host.frame();
    let request = host.context.take_file_request().expect("the dialog");
    assert_eq!(request.kind, FileKind::Save);
    assert_eq!(request.title, "Save the report");
    assert_eq!(request.file_name.as_deref(), Some("report.pdf"));
    assert_eq!(request.filters[0].1, ["pdf", "html"]);
    assert!(host.context.take_file_request().is_none(), "asked once");
    assert_eq!(
        host.app.chosen, None,
        "nothing is known until the host answers"
    );

    host.context
        .set_chosen_files(vec![PathBuf::from("report.pdf")]);
    host.frame();
    assert_eq!(host.app.chosen, Some(vec![PathBuf::from("report.pdf")]));
    // Cancelling is an answer of no files, not the absence of one. Each
    // dialog is one request with one answer, so this is a second request.
    host.send(FileMsg::Save);
    host.frame();
    assert!(host.context.take_file_request().is_some());
    host.context.set_chosen_files(Vec::new());
    host.frame();
    assert_eq!(host.app.chosen, Some(Vec::new()));
}

#[derive(Clone)]
enum NoticeMsg {
    Post,
}

#[derive(Default)]
struct Notices;

impl App for Notices {
    type Message = NoticeMsg;

    fn update(&mut self, _: NoticeMsg, effects: &mut Effects<'_, NoticeMsg>) {
        effects.notify(Notification::new("Saved", "The report was saved."));
        effects.notify(Notification::new("Sent", "And sent.").urgent());
    }

    fn view(&self, ui: &mut Ui<'_, NoticeMsg>) {
        ui.keyed("state", |ui| ui.label("ready"));
    }
}

#[test]
fn notifications_queue_in_order_and_are_taken_once() {
    let mut host = Host::new(Notices, ROOT);
    host.send(NoticeMsg::Post);
    host.frame();
    let posted = host.context.take_notifications();
    assert_eq!(posted.len(), 2);
    assert_eq!(posted[0].title, "Saved");
    assert!(posted[1].urgent);
    assert!(host.context.take_notifications().is_empty());
}

#[test]
fn a_menu_is_handed_over_only_when_it_changes() {
    let mut context = Context::new();
    let menu = |extra: bool| {
        Menu::new([
            MenuEntry::submenu(
                "File",
                [
                    MenuEntry::Command(Command::new("Open", None)),
                    MenuEntry::Separator,
                    MenuEntry::Command(Command::new("Quit", None)),
                ],
            ),
            MenuEntry::submenu(
                "Edit",
                if extra {
                    vec![MenuEntry::Command(Command::new("Undo", None))]
                } else {
                    Vec::new()
                },
            ),
        ])
    };
    let declare = |context: &mut Context, extra: bool| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            ui.set_menu(menu(extra));
            ui.set_tray(Some(Tray::new("Reports").with_menu(menu(extra))));
            ui.keyed("state", |ui| ui.label("ready"));
        });
    };

    declare(&mut context, false);
    let given = context.take_menu().expect("the first menu");
    assert_eq!(given.entries.len(), 2);
    assert!(context.take_tray().is_some());
    declare(&mut context, false);
    assert!(context.take_menu().is_none(), "the same menu again");
    assert!(context.take_tray().is_none());
    declare(&mut context, true);
    assert!(context.take_menu().is_some(), "a changed menu");
    assert_eq!(
        context
            .take_tray()
            .flatten()
            .map(|tray| tray.tooltip)
            .as_deref(),
        Some("Reports")
    );
}

#[derive(Default)]
struct Open {
    enabled: bool,
    opened: usize,
}

fn open_page(state: &Open, ui: &mut Ui<'_, Handler<Open>>) {
    let open = Command {
        enabled: state.enabled,
        ..Command::with_id("open", "Open", None)
    };
    ui.keyed("open", |ui| {
        ui.command_button(&open, act(|s: &mut Open| s.opened += 1));
    });
    ui.keyed("state", |ui| ui.label("ready"));
}

/// A menu item does what the command's button does, by the same route.
#[test]
fn a_command_chosen_outside_the_window_activates_the_command_inside_it() {
    let mut driven = Driven::new(
        ROOT,
        Open {
            enabled: true,
            ..Open::default()
        },
        open_page,
    );
    assert!(driven.context.event(Event::Command("open".into())));
    driven.frame();
    assert_eq!(driven.state().opened, 1);
    // An id nobody declares does nothing at all.
    assert!(!driven.context.event(Event::Command("nonesuch".into())));
    driven.frame();
    assert_eq!(driven.state().opened, 1);
    // A disabled command is not activated from outside either.
    driven.state_mut().enabled = false;
    driven.frame();
    assert!(!driven.context.event(Event::Command("open".into())));
    driven.frame();
    assert_eq!(driven.state().opened, 1);
}

#[test]
fn files_the_system_opened_with_are_there_from_the_first_frame() {
    let mut context = Context::new();
    context.set_opened_files(vec![PathBuf::from("notes.md")]);
    let mut seen = Vec::new();
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        seen = ui.opened_files().to_vec();
        ui.keyed("page", |ui| ui.column(Style::default(), |_| {}));
    });
    assert_eq!(seen, [PathBuf::from("notes.md")]);
}

#[derive(Default)]
struct Bar {
    opened: usize,
    saved: usize,
}

/// The menu the framework draws itself, for the platforms where a menu
/// belongs to the window. Choosing an item takes the same route a native
/// menu does: it activates the command the view declared.
fn bar_page(_: &Bar, ui: &mut Ui<'_, Handler<Bar>>) {
    let menu = Menu::new([MenuEntry::submenu(
        "File",
        [
            MenuEntry::Command(Command::with_id("open", "Open...", None)),
            MenuEntry::Separator,
            MenuEntry::Command(Command::with_id("save", "Save as...", None)),
        ],
    )]);
    ui.keyed("bar", |ui| {
        ui.menu_bar(&menu);
    });
    // The commands themselves, declared as any view would.
    ui.keyed("open", |ui| {
        ui.command_button(
            &Command::with_id("open", "Open", None),
            act(|s: &mut Bar| s.opened += 1),
        );
    });
    ui.keyed("save", |ui| {
        ui.command_button(
            &Command::with_id("save", "Save", None),
            act(|s: &mut Bar| s.saved += 1),
        );
    });
}

#[test]
fn the_menu_bar_activates_the_command_its_item_names() {
    let mut driven = Driven::new(ROOT, Bar::default(), bar_page);
    let file = driven
        .context
        .find(&["bar", "menu0"])
        .expect("the File menu");
    assert!(driven.context.action(file, Action::Expand));
    driven.frame();
    // Three lines: two commands and the separator between them, which is
    // there but cannot be chosen.
    let separator = driven
        .context
        .find(&["bar", "menu0", "$popup", "1"])
        .unwrap();
    assert!(!driven.context.action(separator, Action::Activate));
    let item = driven
        .context
        .find(&["bar", "menu0", "$popup", "2"])
        .unwrap();
    assert!(driven.context.action(item, Action::Activate));
    // Choosing it closes the menu and activates the command, which the
    // button reports on the next declaration.
    driven.frame();
    driven.frame();
    assert_eq!((driven.state().opened, driven.state().saved), (0, 1));
}

#[derive(Default)]
struct Nested {
    picked: Vec<&'static str>,
    /// Puts a long heading before the menu, so that it opens far along.
    pushed: bool,
}

fn nested_menu(pushed: bool) -> Menu {
    let command = |id: &str, label: &str| MenuEntry::Command(Command::with_id(id, label, None));
    let mut unavailable = Command::with_id("c", "c.txt", None);
    unavailable.enabled = false;
    let lead = pushed.then(|| {
        MenuEntry::submenu(
            "A long heading before the menu under test",
            [command("open", "Open...")],
        )
    });
    Menu::new(lead.into_iter().chain([MenuEntry::submenu(
        "File",
        [
            command("open", "Open..."),
            MenuEntry::submenu(
                "Recent",
                [
                    command("a", "a.txt"),
                    MenuEntry::submenu(
                        "More",
                        [command("b", "b.txt"), MenuEntry::Command(unavailable)],
                    ),
                ],
            ),
            command("save", "Save"),
        ],
    )]))
}

fn nested_page(state: &Nested, ui: &mut Ui<'_, Handler<Nested>>) {
    ui.keyed("bar", |ui| ui.menu_bar(&nested_menu(state.pushed)));
    for id in ["open", "a", "b", "c", "save"] {
        ui.keyed(id, |ui| {
            ui.command_button(
                &Command::with_id(id, id, None),
                act(move |s: &mut Nested| s.picked.push(id)),
            );
        });
    }
}

const FILE: [&str; 3] = ["bar", "menu0", "$popup"];
const RECENT: [&str; 5] = ["bar", "menu0", "$popup", "1", "$popup"];
const MORE: [&str; 7] = ["bar", "menu0", "$popup", "1", "$popup", "1", "$popup"];

fn press(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

fn row(context: &Context, panel: &[&str], index: &str) -> glassmere_core::NodeSnapshot {
    let path: Vec<&str> = panel.iter().copied().chain([index]).collect();
    context.node(context.find(&path).unwrap()).unwrap()
}

fn hover(context: &mut Context, rect: glassmere_scene::Rect) {
    context.event(Event::PointerMoved(glassmere_scene::Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }));
}

/// Room for three menus side by side.
const WIDE: Size = Size::new(900.0, 400.0);

fn open_file(driven: &mut Driven<Nested>) {
    let file = driven.context.find(&["bar", "menu0"]).unwrap();
    assert!(driven.context.action(file, Action::Expand));
    driven.frame();
}

#[test]
fn submenus_open_beside_their_row_to_any_depth_from_the_keyboard() {
    let mut driven = Driven::new(WIDE, Nested::default(), nested_page);
    open_file(&mut driven);
    // A submenu is one row of its menu, closed until it is asked for.
    let recent = row(&driven.context, &FILE, "1");
    assert_eq!(recent.semantics.role, glassmere_core::Role::MenuItem);
    assert_eq!(recent.semantics.label, "Recent");
    assert_eq!(recent.expanded, Some(false));
    assert!(driven.context.find(&RECENT).is_none());
    assert!(
        driven
            .context
            .find(&["bar", "menu0", "$popup", "3"])
            .is_none()
    );

    // Down to it, and the arrow that points the reading way opens it.
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    let recent = row(&driven.context, &FILE, "1");
    assert_eq!(recent.expanded, Some(true));
    let panel = driven
        .context
        .node(driven.context.find(&RECENT).unwrap())
        .unwrap();
    assert_eq!(panel.semantics.role, glassmere_core::Role::Menu);
    // Beside the row, lapping its end, and level with it.
    let end = recent.bounds.x + recent.bounds.width;
    assert!(
        (panel.bounds.x - (end - 2.0)).abs() < 0.5,
        "{:?}",
        panel.bounds
    );
    assert!((panel.bounds.y - (recent.bounds.y - 4.0)).abs() < 0.5);
    assert!(
        panel.clip.width > 200.0,
        "it is not cut by the menu it is in"
    );
    // The row with the keyboard is reported on the button that has the focus.
    let file = driven.context.find(&["bar", "menu0"]).unwrap();
    let first = driven
        .context
        .find(&["bar", "menu0", "$popup", "1", "$popup", "0"])
        .unwrap();
    let button = driven.context.node(file).unwrap();
    assert!(button.focused);
    assert_eq!(button.active_descendant, Some(first));

    // A third level, the same way.
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::Enter);
    driven.frame();
    let more = driven
        .context
        .node(driven.context.find(&MORE).unwrap())
        .unwrap();
    let parent = row(&driven.context, &RECENT, "1");
    assert!((more.bounds.x - (parent.bounds.x + parent.bounds.width - 2.0)).abs() < 0.5);
    // What cannot be chosen is there and stays unchosen.
    assert!(row(&driven.context, &MORE, "1").semantics.disabled);

    // The other arrow goes back out one level at a time.
    press(&mut driven.context, Key::ArrowLeft);
    driven.frame();
    assert!(driven.context.find(&MORE).is_none());
    assert!(driven.context.find(&RECENT).is_some());
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert!(driven.context.find(&MORE).is_some());

    // Enter on a command picks it, closes every level and activates it.
    press(&mut driven.context, Key::Enter);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["b"]);
    assert!(driven.context.find(&FILE).is_none());
    assert!(driven.context.node(file).unwrap().focused);

    // Escape leaves a submenu first, then the menu.
    open_file(&mut driven);
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    press(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(driven.context.find(&RECENT).is_none());
    assert!(driven.context.find(&FILE).is_some());
    press(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(driven.context.find(&FILE).is_none());
    assert_eq!(driven.state().picked, ["b"]);
}

#[test]
fn submenus_follow_the_pointer_and_a_click_picks_in_them() {
    let mut driven = Driven::new(WIDE, Nested::default(), nested_page);
    open_file(&mut driven);
    // Resting on the row opens its menu; a click on the row leaves it open.
    let recent = row(&driven.context, &FILE, "1").bounds;
    hover(&mut driven.context, recent);
    driven.frame();
    assert!(driven.context.find(&RECENT).is_some());
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert!(driven.context.find(&RECENT).is_some());
    assert!(driven.state().picked.is_empty());

    // Into it and on to the next level, which opens in turn.
    let more = row(&driven.context, &RECENT, "1").bounds;
    hover(&mut driven.context, more);
    driven.frame();
    assert!(driven.context.find(&MORE).is_some());
    // Back to a row of the first menu: both submenus close.
    let save = row(&driven.context, &FILE, "2").bounds;
    hover(&mut driven.context, save);
    driven.frame();
    assert!(driven.context.find(&RECENT).is_none());
    assert!(driven.context.find(&FILE).is_some());

    // A click on a command in a submenu picks it.
    hover(&mut driven.context, recent);
    driven.frame();
    let a = row(&driven.context, &RECENT, "0").bounds;
    hover(&mut driven.context, a);
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().picked, ["a"]);
    assert!(driven.context.find(&FILE).is_none());

    // A press outside every level closes them all.
    open_file(&mut driven);
    hover(&mut driven.context, recent);
    driven.frame();
    driven
        .context
        .event(Event::PointerMoved(glassmere_scene::Point {
            x: WIDE.width - 5.0,
            y: WIDE.height - 5.0,
        }));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert!(driven.context.find(&FILE).is_none());
    assert_eq!(driven.state().picked, ["a"]);
}

#[test]
fn a_submenu_opens_on_the_other_side_where_the_window_ends() {
    let state = Nested {
        pushed: true,
        ..Nested::default()
    };
    // Wide enough for the menu where its heading is, not for one beside it.
    let mut driven = Driven::new(Size::new(700.0, 400.0), state, nested_page);
    let file = driven.context.find(&["bar", "menu1"]).unwrap();
    assert!(driven.context.action(file, Action::Expand));
    driven.frame();
    press(&mut driven.context, Key::ArrowDown);
    press(&mut driven.context, Key::ArrowRight);
    driven.frame();
    let menu = ["bar", "menu1", "$popup"];
    let recent = row(&driven.context, &menu, "1").bounds;
    let panel = driven
        .context
        .node(
            driven
                .context
                .find(&["bar", "menu1", "$popup", "1", "$popup"])
                .unwrap(),
        )
        .unwrap()
        .bounds;
    assert!(recent.x + recent.width <= 700.0, "{recent:?}");
    assert!(recent.x + recent.width + panel.width > 700.0, "{recent:?}");
    assert!(
        (panel.x + panel.width - (recent.x + 2.0)).abs() < 0.5,
        "{panel:?} beside {recent:?}"
    );
    assert!(panel.x >= 0.0);
}
