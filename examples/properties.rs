//! A property grid editing the card it sits next to. The rows and the
//! messages come from `#[derive(Properties)]` on the card; two more rows are
//! added by hand through the builder, which is what the derive expands to.
use glassmere::{
    Align, App, AppOptions, Button, Color, Effects, Justify, Length, Properties,
    PropertyGridOptions, PropertyGridState, PropertyMsg, Size, Style, Ui, run,
};

#[derive(Properties)]
struct Corners {
    #[property(min = 0, max = 60, suffix = " px", default = 16)]
    radius: f64,
    #[property(min = 0, max = 60, suffix = " px", default = 20)]
    padding: f64,
}

#[derive(Properties)]
struct Card {
    #[property(group = "Content")]
    title: String,
    #[property(default = true)]
    bold: bool,
    #[property(choices("Start", "Center", "End"))]
    align: usize,
    #[property(
        group = "Layout",
        min = 80,
        max = 420,
        step = 10,
        suffix = " px",
        default = 260
    )]
    width: f64,
    #[property(min = 80, max = 420, step = 10, suffix = " px", default = 140)]
    height: f64,
    #[property(flatten)]
    corners: Corners,
    #[property(group = "Appearance", default = Color::rgb(22, 105, 99))]
    fill: Color,
    #[property(default = Color::WHITE)]
    text: Color,
    #[property(min = 0.1, max = 1.0, step = 0.05, default = 1.0)]
    opacity: f64,
    visible: bool,
}

#[derive(Clone, Debug)]
enum Msg {
    Grid(PropertyMsg),
    Card(CardMsg),
    /// The "Preset" row's button: sets two fields at once.
    Sunset,
}

struct Gallery {
    state: PropertyGridState,
    card: Card,
    last: String,
}

impl App for Gallery {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Grid(grid) => self.state.apply(&grid),
            Msg::Card(card) => {
                self.last = format!("{card:?}");
                self.card.apply(card);
            }
            Msg::Sunset => {
                self.last = "Preset: sunset".into();
                self.card.fill = Color::rgb(232, 120, 72);
                self.card.text = Color::rgb(40, 20, 50);
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let card = &self.card;
        let page = Style {
            padding: 24.0,
            gap: 24.0,
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Style::default()
        };
        ui.row(page, |ui| {
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
            ui.column(stage, |ui| {
                if card.visible {
                    let look = Style {
                        width: Length::Px(card.width as f32),
                        height: Length::Px(card.height as f32),
                        radius: card.corners.radius as f32,
                        padding: card.corners.padding as f32,
                        opacity: card.opacity as f32,
                        background: Some(card.fill),
                        foreground: Some(card.text),
                        font_weight: card.bold.then_some(700),
                        justify_content: Justify::Center,
                        align_items: [Align::Start, Align::Center, Align::End][card.align],
                        ..Style::default()
                    };
                    ui.column(look, |ui| ui.label(card.title.clone()));
                }
                ui.label(format!("Last change: {}", self.last));
            });

            let options = PropertyGridOptions::new("Card properties")
                .height(600.0)
                .style(Style::default().width(Length::Px(380.0)));
            ui.property_grid(&self.state, options, Msg::Grid, |grid| {
                card.properties(grid, Msg::Card);
                // Rows the derive cannot know about: a computed value and an
                // action that touches several fields.
                grid.group("Extras", |grid| {
                    grid.readonly("Characters", card.title.chars().count().to_string());
                    grid.custom("Preset", |ui, label| {
                        ui.button(Button::new(format!("{label}: sunset")).on_click(Msg::Sunset));
                    });
                });
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Property grid".into(),
            size: Size::new(960.0, 720.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        Gallery {
            state: PropertyGridState::default(),
            card: Card {
                title: "Hello, properties".into(),
                width: 260.0,
                height: 140.0,
                corners: Corners {
                    radius: 16.0,
                    padding: 20.0,
                },
                fill: Color::rgb(22, 105, 99),
                text: Color::WHITE,
                align: 1,
                bold: true,
                visible: true,
                opacity: 1.0,
            },
            last: String::from("nothing yet"),
        },
    )
}
