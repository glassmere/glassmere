//! A window with no system title bar, drawing its own: a strip that moves
//! the window, a menu inside it, the three window buttons, and edges that
//! resize it.
//!
//! Press the strip to move the window, press it twice to maximize it, and
//! pull any edge or corner to resize.
use glassmere::{
    App, AppOptions, Color, Command, Effects, Label, Length, Menu, MenuEntry, Shortcut, Size,
    Style, TextWrap, Ui, WindowCommand, run,
};

#[derive(Clone)]
enum Msg {
    New,
    Quit,
    Window(WindowCommand),
}

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

struct TitleBar {
    news: u32,
    smoke: bool,
}

impl App for TitleBar {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::New => self.news += 1,
            Msg::Quit => {
                if !self.smoke {
                    effects.window_command(WindowCommand::Close);
                }
            }
            Msg::Window(command) => effects.window_command(command),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
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
                ui.window_buttons(Msg::Window);
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
            ui.command_button(
                &Command::with_id("new", "New window", Some(Shortcut::primary('n'))),
                Msg::New,
            );
            ui.label(
                Label::new(format!(
                    "{} · {} asked for",
                    if maximized { "Maximized" } else { "Restored" },
                    self.news
                ))
                .style(Style {
                    foreground: Some(Color::rgb(103, 232, 206)),
                    ..Style::default()
                }),
            );
            ui.command_button(&Command::with_id("quit", "Quit", None), Msg::Quit);
        });
        // Last, so the edges lie over everything else.
        ui.resize_frame(6.0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Title bar".into(),
            size: Size::new(720.0, 480.0),
            // Without the system's frame the window has nothing but what the
            // view draws, which is the point of this example.
            decorations: false,
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        TitleBar { news: 0, smoke },
    )
}
