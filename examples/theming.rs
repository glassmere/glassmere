use seaglass::{
    App, AppOptions, Button, Color, Effects, FontFamily, Gradient, Label, Length, Paint, Shadow,
    Size, Style, TextWrap, Theme, Ui, run,
};
use std::path::PathBuf;

#[derive(Clone)]
enum Msg {
    Dark,
    Light,
    Save,
    Delete,
    Name(String),
    Notify(bool),
    Volume(f64),
}

struct Theming {
    stylesheet: PathBuf,
    name: String,
    volume: f64,
    notify: bool,
    saves: u32,
    deleted: u32,
}

impl App for Theming {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Dark => effects.set_theme(Theme::dark()),
            Msg::Light => effects.set_theme(Theme::light()),
            Msg::Save => self.saves += 1,
            Msg::Delete => self.deleted += 1,
            Msg::Name(name) => self.name = name,
            Msg::Notify(notify) => self.notify = notify,
            Msg::Volume(volume) => self.volume = volume,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        // Read tokens before borrowing `ui` mutably for declarations.
        let theme = ui.theme().clone();
        let title = ui.class("title");
        let muted = ui.class("muted");
        let code = ui.class("code");
        let primary = ui.class("primary");
        let danger = ui.class("danger");
        let bold = Style {
            font_weight: Some(700),
            ..Style::default()
        };
        let page = Style {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            padding: 28.0,
            gap: 16.0,
            ..Style::default()
        };
        let panel = Style {
            padding: 20.0,
            gap: 12.0,
            radius: 12.0,
            width: Length::Percent(1.0),
            background: Some(theme.surface),
            ..Style::default()
        };
        let inherited = Style {
            gap: 8.0,
            font_family: Some(FontFamily::Serif),
            foreground: Some(theme.accent),
            ..panel.clone()
        };
        let swatch = Style {
            padding: 16.0,
            radius: 12.0,
            ..Style::default()
        };
        let outlined = Style::button()
            .background(Color::TRANSPARENT)
            .foreground(theme.accent)
            .on_hover(Paint::background(theme.control_hover))
            .on_pressed(Paint {
                background: Some(theme.accent),
                foreground: Some(theme.window),
                ..Paint::default()
            })
            .on_focused(Paint {
                radius: Some(2.0),
                ..Paint::default()
            });
        ui.scroll(page, |ui| {
            ui.label(Label::new(format!("Theming · {} theme", theme.name)).style(title));
            ui.label(
                Label::new(format!(
                    "Edit {} while this window is open: classes and tokens reload within half a \
                     second. The theme follows the OS preference until a button below overrides it.",
                    self.stylesheet.display()
                ))
                .style(muted.clone()),
            );
            ui.row(Style::default().gap(8.0), |ui| {
                ui.button(Button::new("Dark theme").on_click(Msg::Dark));
                ui.button(Button::new("Light theme").on_click(Msg::Light));
            });
            ui.container(panel.clone(), |ui| {
                ui.label(Label::new("Stylesheet classes").style(bold.clone()));
                ui.row(Style::default().gap(8.0), |ui| {
                    ui.button(
                        Button::new("Save (primary)")
                            .style(primary.clone())
                            .on_click(Msg::Save),
                    );
                    ui.button(
                        Button::new("Delete (danger)")
                            .style(danger)
                            .on_click(Msg::Delete),
                    );
                    ui.button(Button::new("Disabled primary").enabled(false).style(primary));
                });
                ui.label(
                    Label::new(format!("Saved {} · deleted {}", self.saves, self.deleted))
                        .style(muted.clone()),
                );
                ui.label(Label::new("class danger extends primary { background: #dc2626 }").style(code));
            });
            ui.container(inherited, |ui| {
                ui.label(Label::new("Inherited text properties").style(bold.clone()));
                ui.label("This label sets nothing: serif and accent color come from the container.");
                ui.label(Label::new("This one overrides the size only.").style(Style {
                    font_size: Some(22.0),
                    ..Style::default()
                }));
                ui.row(Style::default(), |ui| {
                    ui.label("Nested rows inherit too.");
                });
            });
            ui.row(Style::default().gap(16.0), |ui| {
                // A ramp and a shadow, from the typed API.
                let lifted = Style {
                    width: Length::Px(220.0),
                    height: Length::Px(90.0),
                    ..swatch.clone()
                }
                .gradient(Gradient::linear(
                    (0.0, 0.0),
                    (1.0, 1.0),
                    theme.accent,
                    theme.control,
                ))
                .shadow(Shadow::lift(Color(0, 0, 0, 150), 10.0, 6.0));
                ui.container(lifted, |ui| {
                    ui.label(Label::new("Gradient and shadow").style(Style {
                        font_weight: Some(700),
                        foreground: Some(theme.text),
                        ..Style::default()
                    }));
                });
                let disc = Style {
                    radius: 45.0,
                    width: Length::Px(90.0),
                    height: Length::Px(90.0),
                    ..swatch.clone()
                }
                .gradient(Gradient::radial(theme.accent, Color(0, 0, 0, 0)));
                ui.container(disc, |_| {});
                let inset = Style {
                    width: Length::Px(180.0),
                    height: Length::Px(90.0),
                    background: Some(theme.window),
                    ..swatch
                }
                .shadow(Shadow::new(Color(0, 0, 0, 170), 8.0).inset());
                ui.container(inset, |ui| {
                    ui.label(Label::new("Inset").style(Style {
                        foreground: Some(theme.text_muted),
                        ..Style::default()
                    }));
                });
            });
            ui.container(panel, |ui| {
                ui.label(Label::new("Typed classes and state overrides in Rust").style(bold));
                ui.row(Style::default().gap(12.0), |ui| {
                    ui.button(Button::new("Outlined action").style(outlined));
                    ui.text_input("Name", &self.name, Msg::Name);
                });
                ui.checkbox("Notifications", self.notify, Msg::Notify);
                ui.slider("Volume", self.volume, 0.0..=100.0, 5.0, Msg::Volume);
                ui.label(
                    Label::new(
                        "Built-in controls paint from theme tokens: control, hover, pressed, focus, \
                         accent, track and selection.",
                    )
                    .style(Style {
                        wrap: TextWrap::Word,
                        width: Length::Percent(1.0),
                        ..muted
                    }),
                );
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let stylesheet = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/theming.style"));
    run(
        AppOptions {
            title: "Seaglass / Theming".into(),
            size: Size::new(860.0, 720.0),
            frame_limit: smoke.then_some(3),
            stylesheet: Some(stylesheet.clone()),
            ..AppOptions::default()
        },
        Theming {
            stylesheet,
            name: String::from("Ada"),
            volume: 40.0,
            notify: true,
            saves: 0,
            deleted: 0,
        },
    )
}
