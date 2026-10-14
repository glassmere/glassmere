//! A window with no system title bar, drawing its own: a strip that moves
//! the window, a menu inside it, the three window buttons, and edges that
//! resize it.
//!
//! Press the strip to move the window, press it twice to maximize it, and
//! pull any edge or corner to resize.
use rust_gui::{
    AppOptions, Color, Command, Label, Length, Menu, MenuEntry, Shortcut, Size, Style, TextWrap,
    run,
};

fn menu() -> Menu {
    Menu::new([MenuEntry::submenu(
        "File",
        [
            MenuEntry::Command(Command::with_id(
                "new",
                "New window",
                Some(Shortcut::primary('n')),
            )),
            MenuEntry::Separator,
            MenuEntry::Command(Command::with_id("quit", "Quit", None)),
        ],
    )])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut news = 0;
    run(
        AppOptions {
            title: "Rust GUI / Title bar".into(),
            size: Size::new(720.0, 480.0),
            // Without the system's frame the window has nothing but what the
            // view draws, which is the point of this example.
            decorations: false,
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        move |ui| {
            if smoke {
                ui.request_redraw();
            }
            let theme = ui.theme().clone();
            let maximized = ui.window().maximized;
            ui.title_bar(
                Style {
                    width: Length::Percent(1.0),
                    padding: 6.0,
                    gap: 10.0,
                    background: Some(theme.surface),
                    ..Style::default()
                },
                |ui| {
                    ui.label(Label::new("Title bar").style(Style {
                        font_weight: Some(700),
                        padding: 6.0,
                        ..Style::default()
                    }));
                    ui.menu_bar(&menu());
                    // Everything after this sits at the far end.
                    ui.container(Style::default().grow(1.0), |_| {});
                    ui.window_buttons();
                },
            );
            let page = Style {
                padding: 24.0,
                gap: 14.0,
                width: Length::Percent(1.0),
                ..Style::default()
            };
            ui.column(page, |ui| {
                ui.label(Label::new("A window of your own").style(Style {
                    font_size: Some(26.0),
                    font_weight: Some(700),
                    ..Style::default()
                }));
                ui.label(
                    Label::new(
                        "The strip above moves this window, and moves it twice to maximize it. The \
                     six pixels along each edge resize it. Nothing here is the system's.",
                    )
                    .style(Style {
                        width: Length::Percent(1.0),
                        wrap: TextWrap::Word,
                        ..Style::default()
                    }),
                );
                if ui
                    .command_button(&Command::with_id(
                        "new",
                        "New window",
                        Some(Shortcut::primary('n')),
                    ))
                    .clicked()
                {
                    news += 1;
                }
                ui.label(
                    Label::new(format!(
                        "{} · {news} asked for",
                        if maximized { "Maximized" } else { "Restored" }
                    ))
                    .style(Style {
                        foreground: Some(Color::rgb(103, 232, 206)),
                        ..Style::default()
                    }),
                );
                if ui
                    .command_button(&Command::with_id("quit", "Quit", None))
                    .clicked()
                    && !smoke
                {
                    ui.window_command(rust_gui::WindowCommand::Close);
                }
            });
            // Last, so the edges lie over everything else.
            ui.resize_frame(6.0);
        },
    )
}
