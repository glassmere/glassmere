//! A property grid editing the card it sits next to: grouped rows, a filter,
//! a draggable name column, reset buttons and a custom row.
use rust_gui::{
    Align, AppOptions, Color, Justify, Length, NumberOptions, PropertyGridOptions,
    PropertyGridState, Size, Style, run,
};

struct Card {
    title: String,
    width: f64,
    height: f64,
    radius: f64,
    padding: f64,
    fill: Color,
    text: Color,
    align: usize,
    bold: bool,
    visible: bool,
    opacity: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut state = PropertyGridState::default();
    let mut card = Card {
        title: "Hello, properties".into(),
        width: 260.0,
        height: 140.0,
        radius: 16.0,
        padding: 20.0,
        fill: Color::rgb(22, 105, 99),
        text: Color::WHITE,
        align: 1,
        bold: true,
        visible: true,
        opacity: 1.0,
    };
    let mut last = String::from("nothing yet");
    run(
        AppOptions {
            title: "Rust GUI / Property grid".into(),
            size: Size::new(960.0, 720.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
            let page = Style {
                padding: 24.0,
                gap: 24.0,
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Style::default()
            };
            ui.row("page", page, |ui| {
                let stage = Style {
                    grow: 1.0,
                    height: Length::Percent(1.0),
                    radius: 12.0,
                    background: Some(Color::rgb(24, 35, 49)),
                    align_items: Align::Center,
                    justify_content: Justify::Center,
                    gap: 16.0,
                    ..Style::default()
                };
                ui.column("stage", stage, |ui| {
                    if card.visible {
                        let look = Style {
                            width: Length::Px(card.width as f32),
                            height: Length::Px(card.height as f32),
                            radius: card.radius as f32,
                            padding: card.padding as f32,
                            opacity: card.opacity as f32,
                            background: Some(card.fill),
                            foreground: Some(card.text),
                            font_weight: card.bold.then_some(700),
                            justify_content: Justify::Center,
                            align_items: [Align::Start, Align::Center, Align::End][card.align],
                            ..Style::default()
                        };
                        ui.column("card", look, |ui| ui.label(card.title.clone()));
                    }
                    ui.label_keyed("last", format!("Last change: {last}"));
                });

                let options = PropertyGridOptions::new("Card properties")
                    .height(600.0)
                    .style(Style::default().width(Length::Px(380.0)));
                let response = ui.property_grid("inspector", &mut state, options, |grid| {
                    grid.group("Content", |grid| {
                        grid.text("Title", &mut card.title);
                        grid.default(true).bool("Bold", &mut card.bold);
                        grid.choice("Align", &mut card.align, &["Start", "Center", "End"]);
                        grid.readonly("Characters", card.title.chars().count().to_string());
                    });
                    grid.group("Layout", |grid| {
                        let size = NumberOptions::default()
                            .range(80.0, 420.0)
                            .step(10.0)
                            .suffix(" px");
                        grid.default(260.0)
                            .number("Width", &mut card.width, size.clone());
                        grid.default(140.0).number("Height", &mut card.height, size);
                        grid.group("Box", |grid| {
                            let small = NumberOptions::default().range(0.0, 60.0).suffix(" px");
                            grid.default(16.0)
                                .number("Radius", &mut card.radius, small.clone());
                            grid.default(20.0)
                                .number("Padding", &mut card.padding, small);
                        });
                    });
                    grid.group("Appearance", |grid| {
                        grid.default(Color::rgb(22, 105, 99))
                            .color("Fill", &mut card.fill);
                        grid.default(Color::WHITE).color("Text", &mut card.text);
                        grid.default(1.0).number(
                            "Opacity",
                            &mut card.opacity,
                            NumberOptions::default().range(0.1, 1.0).step(0.05),
                        );
                        grid.bool("Visible", &mut card.visible);
                        grid.custom("Preset", |ui, label| {
                            let clicked = ui
                                .button_keyed("$value", format!("{label}: sunset"))
                                .clicked();
                            if clicked {
                                card.fill = Color::rgb(232, 120, 72);
                                card.text = Color::rgb(40, 20, 50);
                            }
                            clicked
                        });
                    });
                });
                if response.any() {
                    last = response.changed.join(", ");
                }
            });
        },
    )
}
