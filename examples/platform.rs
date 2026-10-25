//! What the desktop itself provides: a menu, a tray icon, file dialogs,
//! notifications, and the files the system started the application with.
//!
//! The menu is in the title bar, on the row with the window's title and its
//! buttons. `--separate-menu` puts it in a bar of its own under the system's
//! title bar instead.
//!
//! ```sh
//! cargo run -p glassmere-examples --example platform --features platform
//! cargo run -p glassmere-examples --example platform --features platform -- --separate-menu
//! ```
use glassmere::{
    App, AppOptions, Color, Command, Effects, FileRequest, Label, Length, Menu, MenuBarPlacement,
    MenuEntry, Notification, Shortcut, Size, Style, TextWrap, Tray, Ui, run,
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

struct Platform {
    smoke: bool,
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
        let page = Style {
            padding: 20.0,
            gap: 14.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            // Where the window shows the menu above somewhere else, in its
            // title bar or in a native menu bar, this draws nothing; where it
            // does not, this draws it. Choosing an item activates the same
            // command the platform's menu does.
            ui.menu_bar(&menu(self.saved));
            ui.label(Label::new("Desktop integration").style(Style {
                font_size: Some(26.0),
                font_weight: Some(700),
                ..Style::default()
            }));
            ui.label(
                Label::new(
                    "The menu above, the tray icon and the buttons below all go through the same \
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
    let separate = std::env::args().any(|arg| arg == "--separate-menu");
    run(
        AppOptions {
            title: "Glassmere / Desktop".into(),
            size: Size::new(640.0, 460.0),
            frame_limit: smoke.then_some(20),
            menu_bar: if separate {
                MenuBarPlacement::Separate
            } else {
                MenuBarPlacement::TitleBar
            },
            ..AppOptions::default()
        },
        Platform {
            smoke,
            chosen: Vec::new(),
            saved: false,
            said: 0,
        },
    )?;
    Ok(())
}
