//! What a view asks the desktop for, and what comes back. The host is a
//! test here: it takes the requests and answers them.
mod support;

use glassmere_core::{
    Action, App, Command, Context, Effects, Event, FileKind, FileRequest, Menu, MenuEntry,
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
