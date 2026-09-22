//! Three dials in a window, from a crate that knows nothing about the
//! framework beyond its published API. Drag one, turn the wheel over one, or
//! tab to one and use the arrow keys.
use dial_widget::Dial;
use rust_gui::{AppOptions, Color, Length, Size, Style, TextWrap, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut dials = [Dial::new(0.2), Dial::new(0.55), Dial::new(0.9)];
    let mut enabled = true;
    let mut turns = 0;
    run(
        AppOptions {
            title: "Dial widget".into(),
            size: Size::new(560.0, 420.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        move |ui| {
            if smoke {
                ui.request_redraw();
            }
            let page = Style {
                padding: 24.0,
                gap: 16.0,
                width: Length::Percent(1.0),
                ..Style::default()
            };
            ui.column("page", page, |ui| {
                ui.label_styled(
                    "title",
                    "A widget from another crate",
                    Style {
                        font_size: Some(26.0),
                        font_weight: Some(700),
                        ..Style::default()
                    },
                );
                ui.label_styled(
                    "intro",
                    "Drag a dial, roll the wheel over it, or tab to it and use the arrow keys. \
                     Shift makes the keys finer.",
                    Style {
                        width: Length::Percent(1.0),
                        wrap: TextWrap::Word,
                        ..Style::default()
                    },
                );
                ui.checkbox("enabled", "Enabled", &mut enabled);
                ui.enabled(enabled, |ui| {
                    ui.row("dials", Style::default().gap(28.0), |ui| {
                        for (index, (dial, label)) in
                            dials.iter_mut().zip(["Gain", "Tone", "Mix"]).enumerate()
                        {
                            ui.column(format!("column{index}"), Style::default().gap(8.0), |ui| {
                                if dial.ui(ui, "dial", label, 120.0) {
                                    turns += 1;
                                }
                                ui.label_keyed(
                                    "reading",
                                    format!("{label}: {:.0}%", dial.value() * 100.0),
                                );
                            });
                        }
                    });
                });
                ui.label_styled(
                    "turns",
                    format!("{turns} turns"),
                    Style {
                        foreground: Some(Color::rgb(103, 232, 206)),
                        ..Style::default()
                    },
                );
            });
        },
    )
}
