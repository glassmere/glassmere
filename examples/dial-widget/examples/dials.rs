//! Three dials in a window, from a crate that knows nothing about the
//! framework beyond its published API. Drag one, turn the wheel over one, or
//! tab to one and use the arrow keys.
use dial_widget::{Dial, DialMsg};
use seaglass::{App, AppOptions, Color, Effects, Label, Length, Size, Style, TextWrap, Ui, run};

#[derive(Clone, Debug)]
enum Msg {
    Dial(usize, DialMsg),
    Enabled(bool),
}

struct Dials {
    dials: [Dial; 3],
    enabled: bool,
    turns: usize,
    smoke: bool,
}

impl App for Dials {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message {
            Msg::Dial(index, message) => {
                if self.dials[index].update(message) {
                    self.turns += 1;
                }
            }
            Msg::Enabled(enabled) => self.enabled = enabled,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        if self.smoke {
            ui.request_redraw();
        }
        let page = Style {
            padding: 24.0,
            gap: 16.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.keyed("page", |ui| {
            ui.column(page, |ui| {
                ui.keyed("title", |ui| {
                    ui.label(Label::new("A widget from another crate").style(Style {
                        font_size: Some(26.0),
                        font_weight: Some(700),
                        ..Style::default()
                    }))
                });
                ui.keyed("intro", |ui| {
                    ui.label(
                        Label::new(
                            "Drag a dial, roll the wheel over it, or tab to it and use the arrow \
                             keys. Shift makes the keys finer.",
                        )
                        .style(Style {
                            width: Length::Percent(1.0),
                            wrap: TextWrap::Word,
                            ..Style::default()
                        }),
                    )
                });
                ui.keyed("enabled", |ui| {
                    ui.checkbox("Enabled", self.enabled, Msg::Enabled)
                });
                ui.enabled(self.enabled, |ui| {
                    ui.keyed("dials", |ui| {
                        ui.row(Style::default().gap(28.0), |ui| {
                            for (index, (dial, label)) in
                                self.dials.iter().zip(["Gain", "Tone", "Mix"]).enumerate()
                            {
                                ui.keyed(format!("column{index}"), |ui| {
                                    ui.column(Style::default().gap(8.0), |ui| {
                                        dial.view(ui, "dial", label, 120.0, move |message| {
                                            Msg::Dial(index, message)
                                        });
                                        ui.keyed("reading", |ui| {
                                            ui.label(format!(
                                                "{label}: {:.0}%",
                                                dial.value() * 100.0
                                            ))
                                        });
                                    })
                                });
                            }
                        })
                    });
                });
                ui.keyed("turns", |ui| {
                    ui.label(Label::new(format!("{} turns", self.turns)).style(Style {
                        foreground: Some(Color::rgb(103, 232, 206)),
                        ..Style::default()
                    }))
                });
            })
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let app = Dials {
        dials: [Dial::new(0.2), Dial::new(0.55), Dial::new(0.9)],
        enabled: true,
        turns: 0,
        smoke,
    };
    run(
        AppOptions {
            title: "Dial widget".into(),
            size: Size::new(560.0, 420.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        app,
    )?;
    Ok(())
}
