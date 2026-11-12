//! What the desktop itself provides: a menu, a tray icon, file dialogs,
//! notifications, and the files the system started the application with.
//!
//! ```sh
//! cargo run -p glassmere-examples --example platform --features platform
//! ```
use glassmere::{
    App, AppOptions, Color, Command, Effects, FileRequest, Image, Label, Length, Menu, MenuEntry,
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
                // A submenu opens beside its row, in the drawn menu bar as
                // in a native one.
                MenuEntry::submenu(
                    "After saving",
                    [MenuEntry::Command(Command {
                        enabled: saved,
                        ..Command::with_id("notify", "Say it was saved", None)
                    })],
                ),
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

/// A teal disc, drawn here so the example needs no image file. The window
/// and the tray icon both use it.
fn icon() -> Image {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - SIZE as f32 / 2.0;
            let dy = y as f32 + 0.5 - SIZE as f32 / 2.0;
            // One pixel of soft edge.
            let coverage =
                (SIZE as f32 / 2.0 - 1.0 - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[103, 232, 206, (coverage * 255.0) as u8]);
        }
    }
    Image::from_rgba8(SIZE, SIZE, rgba).expect("the size matches the pixels")
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
            Tray::new("Glassmere desktop example")
                .with_icon(icon())
                .with_menu(menu(self.saved)),
        ));
        let page = Style {
            padding: 20.0,
            gap: 14.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            // On Windows and Linux a menu bar belongs to the window, and
            // this draws it. Choosing an item activates the same command
            // the platform's menu does.
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
    run(
        AppOptions {
            title: "Glassmere / Desktop".into(),
            size: Size::new(640.0, 460.0),
            // How the desktop knows the application: on Linux a
            // `glassmere-platform-example.desktop` file would give it its
            // taskbar icon. Notifications are posted under this name too.
            app_id: Some("glassmere-platform-example".into()),
            icon: Some(icon()),
            frame_limit: smoke.then_some(20),
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
