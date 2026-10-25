//! What the desktop itself provides: a menu, a tray icon, file dialogs,
//! notifications, and the files the system started the application with.
//!
//! The menu is in the app bar, on the row with the window's title and its
//! buttons. `--separate-menu` leaves the menu out of the app bar and draws it
//! with `Ui::menu_bar` in the body, as a second bar under it. `--native-menu`
//! has no app bar at all: the window keeps the system's title bar and, on
//! Windows, the menu is its own menu bar.
//!
//! ```sh
//! cargo run -p glassmere-examples --example platform --features platform
//! cargo run -p glassmere-examples --example platform --features platform -- --separate-menu
//! cargo run -p glassmere-examples --example platform --features platform -- --native-menu
//! ```
use glassmere::{
    App, AppBar, AppOptions, Color, Command, Effects, FileRequest, Label, Length, Menu, MenuEntry,
    Notification, Shortcut, Size, Style, TextWrap, Tray, Ui, run,
};
use std::path::PathBuf;

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

#[derive(Clone)]
enum Msg {
    Open,
    Save,
    Say,
    Quit,
    Chosen(Vec<PathBuf>),
}

#[derive(Clone, Copy, PartialEq)]
enum Bar {
    /// The menu in the app bar.
    Inside,
    /// An app bar, and the menu as a bar of its own under it.
    Separate,
    /// The system's title bar and the platform's menu bar.
    Native,
}

struct Platform {
    smoke: bool,
    bar: Bar,
    chosen: Vec<String>,
    saved: bool,
    said: u32,
}

impl App for Platform {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Open => {
                if !self.smoke {
                    effects.choose_files(
                        FileRequest::open_many("Open some files")
                            .with_filter("Text", ["txt", "md"])
                            .with_filter("Every file", ["*"]),
                        Msg::Chosen,
                    );
                }
            }
            Msg::Save => {
                self.saved = true;
                if !self.smoke {
                    effects.choose_files(
                        FileRequest::save("Save the notes").named("notes.txt"),
                        Msg::Chosen,
                    );
                }
            }
            Msg::Say => {
                self.said += 1;
                if !self.smoke {
                    effects.notify(Notification::new(
                        "Glassmere",
                        "The notes were saved.".to_owned(),
                    ));
                }
            }
            Msg::Quit => {
                if !self.smoke {
                    std::process::exit(0);
                }
            }
            Msg::Chosen(paths) => {
                self.chosen = paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        if self.smoke {
            ui.request_redraw();
        }
        // The platform's own menu where there is one, and a tray icon
        // everywhere. Both send the commands declared below.
        ui.set_menu(menu(self.saved));
        ui.set_tray(Some(
            Tray::new("Glassmere desktop example").with_menu(menu(self.saved)),
        ));
        match self.bar {
            Bar::Inside => ui.app_bar(AppBar::new().menu(menu(self.saved))),
            Bar::Separate => {
                ui.app_bar(AppBar::new());
                ui.menu_bar(&menu(self.saved));
            }
            Bar::Native => {}
        }
        let page = Style {
            padding: 20.0,
            gap: 14.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            ui.label(Label::new("Desktop integration").style(Style {
                font_size: Some(26.0),
                font_weight: Some(700),
                ..Style::default()
            }));
            ui.label(
                Label::new(
                    "The menu, the tray icon and the buttons below all go through the same \
                 commands. Start the example with a file name to see it arrive.",
                )
                .style(Style {
                    width: Length::Percent(1.0),
                    wrap: TextWrap::Word,
                    ..Style::default()
                }),
            );
            ui.row(Style::default().gap(10.0), |ui| {
                ui.command_button(
                    &Command::with_id("open", "Open...", Some(Shortcut::primary('o'))),
                    Msg::Open,
                );
                ui.command_button(
                    &Command::with_id("save", "Save as...", Some(Shortcut::primary('s'))),
                    Msg::Save,
                );
                let say = Command {
                    enabled: self.saved,
                    ..Command::with_id("notify", "Say it was saved", None)
                };
                ui.command_button(&say, Msg::Say);
                ui.command_button(
                    &Command::with_id("quit", "Close the window", None),
                    Msg::Quit,
                );
            });
            let opened: Vec<String> = ui
                .opened_files()
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            for (key, label, paths) in [
                ("opened", "Started with", &opened),
                ("chosen", "Chosen in a dialog", &self.chosen),
            ] {
                ui.keyed(key, |ui| {
                    ui.label(
                        Label::new(format!(
                            "{label}: {}",
                            if paths.is_empty() {
                                "nothing".to_owned()
                            } else {
                                paths.join(", ")
                            }
                        ))
                        .style(Style {
                            foreground: Some(Color::rgb(103, 232, 206)),
                            width: Length::Percent(1.0),
                            wrap: TextWrap::Word,
                            ..Style::default()
                        }),
                    )
                });
            }
            ui.label(format!("Notifications posted: {}", self.said));
        });
        if self.smoke && self.said > 0 {
            println!("PLATFORM saved={} said={}", self.saved, self.said);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let bar = if std::env::args().any(|arg| arg == "--separate-menu") {
        Bar::Separate
    } else if std::env::args().any(|arg| arg == "--native-menu") {
        Bar::Native
    } else {
        Bar::Inside
    };
    run(
        AppOptions {
            title: "Glassmere / Desktop".into(),
            size: Size::new(640.0, 460.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        Platform {
            smoke,
            bar,
            chosen: Vec::new(),
            saved: false,
            said: 0,
        },
    )?;
    Ok(())
}
