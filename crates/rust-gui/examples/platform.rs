//! What the desktop itself provides: a menu, a tray icon, file dialogs,
//! notifications, and the files the system started the application with.
//!
//! ```sh
//! cargo run -p rust-gui --example platform --features platform
//! ```
use rust_gui::{
    AppOptions, Color, Command, FileRequest, Length, Menu, MenuEntry, Notification, Shortcut, Size,
    Style, TextWrap, Tray, run,
};

fn menu(saved: bool) -> Menu {
    Menu::new([
        MenuEntry::submenu(
            "File",
            [
                MenuEntry::Command(Command::with_id(
                    "open",
                    "Open...",
                    Some(Shortcut::primary('o')),
                )),
                MenuEntry::Command(Command::with_id(
                    "save",
                    "Save as...",
                    Some(Shortcut::primary('s')),
                )),
                MenuEntry::Separator,
                MenuEntry::Command(Command {
                    enabled: saved,
                    ..Command::with_id("notify", "Say it was saved", None)
                }),
            ],
        ),
        MenuEntry::submenu(
            "Window",
            [MenuEntry::Command(Command::with_id(
                "quit",
                "Close the window",
                None,
            ))],
        ),
    ])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut chosen: Vec<String> = Vec::new();
    let mut saved = false;
    let mut said = 0;
    run(
        AppOptions {
            title: "Rust GUI / Desktop".into(),
            size: Size::new(640.0, 460.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        move |ui| {
            if smoke {
                ui.request_redraw();
            }
            // The platform's own menu where there is one, and a tray icon
            // everywhere. Both send the commands declared below.
            ui.set_menu(menu(saved));
            ui.set_tray(Some(
                Tray::new("Rust GUI desktop example").with_menu(menu(saved)),
            ));
            let page = Style {
                padding: 20.0,
                gap: 14.0,
                width: Length::Percent(1.0),
                ..Style::default()
            };
            ui.column("page", page, |ui| {
                // On Windows and Linux a menu bar belongs to the window, and
                // this draws it. Choosing an item activates the same command
                // the platform's menu does.
                ui.menu_bar("bar", &menu(saved));
                ui.label_styled(
                    "title",
                    "Desktop integration",
                    Style {
                        font_size: Some(26.0),
                        font_weight: Some(700),
                        ..Style::default()
                    },
                );
                ui.label_styled(
                    "intro",
                    "The menu above, the tray icon and the buttons below all go through the same \
                     commands. Start the example with a file name to see it arrive.",
                    Style {
                        width: Length::Percent(1.0),
                        wrap: TextWrap::Word,
                        ..Style::default()
                    },
                );
                ui.row("buttons", Style::default().gap(10.0), |ui| {
                    if ui
                        .command_button(
                            "open",
                            &Command::with_id("open", "Open...", Some(Shortcut::primary('o'))),
                        )
                        .clicked()
                        && !smoke
                    {
                        ui.choose_file(
                            FileRequest::open_many("Open some files")
                                .with_filter("Text", ["txt", "md"])
                                .with_filter("Every file", ["*"]),
                        );
                    }
                    if ui
                        .command_button(
                            "save",
                            &Command::with_id("save", "Save as...", Some(Shortcut::primary('s'))),
                        )
                        .clicked()
                    {
                        saved = true;
                        if !smoke {
                            ui.choose_file(FileRequest::save("Save the notes").named("notes.txt"));
                        }
                    }
                    let say = Command {
                        enabled: saved,
                        ..Command::with_id("notify", "Say it was saved", None)
                    };
                    if ui.command_button("notify", &say).clicked() {
                        said += 1;
                        if !smoke {
                            ui.notify(Notification::new(
                                "Rust GUI",
                                "The notes were saved.".to_owned(),
                            ));
                        }
                    }
                    if ui
                        .command_button("quit", &Command::with_id("quit", "Close the window", None))
                        .clicked()
                        && !smoke
                    {
                        std::process::exit(0);
                    }
                });
                if let Some(paths) = ui.chosen_files() {
                    chosen = paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect();
                }
                let opened: Vec<String> = ui
                    .opened_files()
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                for (key, label, paths) in [
                    ("opened", "Started with", &opened),
                    ("chosen", "Chosen in a dialog", &chosen),
                ] {
                    ui.label_styled(
                        key,
                        format!(
                            "{label}: {}",
                            if paths.is_empty() {
                                "nothing".to_owned()
                            } else {
                                paths.join(", ")
                            }
                        ),
                        Style {
                            foreground: Some(Color::rgb(103, 232, 206)),
                            width: Length::Percent(1.0),
                            wrap: TextWrap::Word,
                            ..Style::default()
                        },
                    );
                }
                ui.label_keyed("said", format!("Notifications posted: {said}"));
            });
            if smoke && said > 0 {
                println!("PLATFORM saved={saved} said={said}");
            }
        },
    )
}
