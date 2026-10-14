//! What a view asks the desktop for, and what comes back. The host is a
//! test here: it takes the requests and answers them.
use rust_gui_core::{
    Command, Context, Event, FileKind, FileRequest, Menu, MenuEntry, MonospaceMeasurer,
    Notification, Style, Tray, Ui,
};
use rust_gui_scene::Size;
use std::path::PathBuf;

const ROOT: Size = Size::new(400.0, 300.0);

#[test]
fn a_file_dialog_is_asked_for_once_and_answered_later() {
    let mut context = Context::new();
    let mut ask = false;
    let mut chosen: Option<Vec<PathBuf>> = None;
    let declare = |context: &mut Context, ask: &mut bool, chosen: &mut Option<Vec<PathBuf>>| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            if *ask {
                *ask = false;
                ui.choose_file(
                    FileRequest::save("Save the report")
                        .with_filter("Reports", ["pdf", "html"])
                        .named("report.pdf"),
                );
            }
            *chosen = ui.chosen_files().map(<[PathBuf]>::to_vec);
            ui.keyed("state", |ui| ui.label("ready"));
        });
    };

    declare(&mut context, &mut ask, &mut chosen);
    assert!(context.take_file_request().is_none());
    ask = true;
    declare(&mut context, &mut ask, &mut chosen);
    let request = context.take_file_request().expect("the dialog");
    assert_eq!(request.kind, FileKind::Save);
    assert_eq!(request.title, "Save the report");
    assert_eq!(request.file_name.as_deref(), Some("report.pdf"));
    assert_eq!(request.filters[0].1, ["pdf", "html"]);
    assert!(context.take_file_request().is_none(), "asked once");
    assert_eq!(chosen, None, "nothing is known until the host answers");

    context.set_chosen_files(vec![PathBuf::from("report.pdf")]);
    declare(&mut context, &mut ask, &mut chosen);
    assert_eq!(chosen, Some(vec![PathBuf::from("report.pdf")]));
    // Cancelling is an answer of no files, not the absence of one.
    context.set_chosen_files(Vec::new());
    declare(&mut context, &mut ask, &mut chosen);
    assert_eq!(chosen, Some(Vec::new()));
}

#[test]
fn notifications_queue_in_order_and_are_taken_once() {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.notify(Notification::new("Saved", "The report was saved."));
        ui.notify(Notification::new("Sent", "And sent.").urgent());
        ui.keyed("state", |ui| ui.label("ready"));
    });
    let posted = context.take_notifications();
    assert_eq!(posted.len(), 2);
    assert_eq!(posted[0].title, "Saved");
    assert!(posted[1].urgent);
    assert!(context.take_notifications().is_empty());
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

/// A menu item does what the command's button does, by the same route.
#[test]
fn a_command_chosen_outside_the_window_activates_the_command_inside_it() {
    let mut context = Context::new();
    let mut opened = 0;
    let mut enabled = true;
    let declare = |context: &mut Context, opened: &mut usize, enabled: bool| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            let open = Command {
                enabled,
                ..Command::with_id("open", "Open", None)
            };
            if ui.keyed("open", |ui| ui.command_button(&open)).clicked() {
                *opened += 1;
            }
            ui.keyed("state", |ui| ui.label("ready"));
        });
    };
    declare(&mut context, &mut opened, enabled);
    assert!(context.event(Event::Command("open".into())));
    declare(&mut context, &mut opened, enabled);
    assert_eq!(opened, 1);
    // An id nobody declares does nothing at all.
    assert!(!context.event(Event::Command("nonesuch".into())));
    declare(&mut context, &mut opened, enabled);
    assert_eq!(opened, 1);
    // A disabled command is not activated from outside either.
    enabled = false;
    declare(&mut context, &mut opened, enabled);
    assert!(!context.event(Event::Command("open".into())));
    declare(&mut context, &mut opened, enabled);
    assert_eq!(opened, 1);
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

/// The menu the framework draws itself, for the platforms where a menu
/// belongs to the window. Choosing an item takes the same route a native
/// menu does: it activates the command the view declared.
#[test]
fn the_menu_bar_activates_the_command_its_item_names() {
    use rust_gui_core::Action;
    let mut context = Context::new();
    let mut opened = 0;
    let mut saved = 0;
    let declare = |context: &mut Context, opened: &mut usize, saved: &mut usize| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            let menu = Menu::new([MenuEntry::submenu(
                "File",
                [
                    MenuEntry::Command(Command::with_id("open", "Open...", None)),
                    MenuEntry::Separator,
                    MenuEntry::Command(Command::with_id("save", "Save as...", None)),
                ],
            )]);
            ui.keyed("bar", |ui| ui.menu_bar(&menu));
            // The commands themselves, declared as any view would.
            if ui
                .keyed("open", |ui| {
                    ui.command_button(&Command::with_id("open", "Open", None))
                })
                .clicked()
            {
                *opened += 1;
            }
            if ui
                .keyed("save", |ui| {
                    ui.command_button(&Command::with_id("save", "Save", None))
                })
                .clicked()
            {
                *saved += 1;
            }
        });
    };
    declare(&mut context, &mut opened, &mut saved);
    let file = context.find(&["bar", "menu0"]).expect("the File menu");
    assert!(context.action(file, Action::Expand));
    declare(&mut context, &mut opened, &mut saved);
    // Three lines: two commands and the separator between them, which is
    // there but cannot be chosen.
    let separator = context.find(&["bar", "menu0", "$popup", "1"]).unwrap();
    assert!(!context.action(separator, Action::Activate));
    let item = context.find(&["bar", "menu0", "$popup", "2"]).unwrap();
    assert!(context.action(item, Action::Activate));
    // Choosing it closes the menu and activates the command, which the
    // button reports on the next declaration.
    declare(&mut context, &mut opened, &mut saved);
    declare(&mut context, &mut opened, &mut saved);
    assert_eq!((opened, saved), (0, 1));
}
