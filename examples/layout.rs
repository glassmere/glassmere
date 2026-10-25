use glassmere::{
    Align, App, AppOptions, AutoFlow, Button, Effects, GridArea, Justify, Label, Length, Size,
    Style, TextInput, TextWrap, Track, TrackSize, Ui, run,
};

#[derive(Clone)]
enum Msg {
    Name(String),
    Volume(f64),
    Notify(bool),
    Shrink(bool),
}

struct Layout {
    name: String,
    volume: f64,
    notify: bool,
    shrink: bool,
}

impl App for Layout {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Name(name) => self.name = name,
            Msg::Volume(volume) => self.volume = volume,
            Msg::Notify(notify) => self.notify = notify,
            Msg::Shrink(shrink) => self.shrink = shrink,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let theme = ui.theme().clone();
        let heading = Style::default().font_weight(700);
        let muted = Style::default()
            .font_size(13.0)
            .foreground(theme.text_muted)
            .wrap(TextWrap::Word)
            .width(Length::Percent(1.0));
        let card = Style::default()
            .background(theme.surface)
            .radius(10.0)
            .padding(12.0)
            .align_items(Align::Center)
            .justify_content(Justify::Center);
        let full = Style::default().width(Length::Percent(1.0));
        let page = full
            .clone()
            .height(Length::Percent(1.0))
            .padding(28.0)
            .gap(18.0);
        let gallery = full
            .clone()
            .columns(vec![Track::repeat(3, vec![Track::Fr(1.0)])])
            .auto_rows(vec![Track::Px(90.0)])
            .gap(12.0);
        let form = full
            .clone()
            .columns(vec![Track::Auto, Track::Fr(1.0)])
            .align_items(Align::Center)
            .gap(10.0);
        let field = Style::button()
            .width(Length::Auto)
            .height(Length::Px(44.0))
            .padding(10.0);
        let spread = full
            .clone()
            .height(Length::Px(84.0))
            .justify_content(Justify::SpaceBetween)
            .align_items(Align::Center)
            .background(theme.surface)
            .radius(10.0)
            .padding(8.0);
        let ratio_card = card
            .clone()
            .min_width(Length::Px(120.0))
            .max_width(Length::Px(260.0))
            .aspect_ratio(16.0 / 9.0);
        let strip = full
            .clone()
            .height(Length::Px(104.0))
            .padding(8.0)
            .radius(10.0)
            .background(theme.surface);
        let strip_card = card
            .clone()
            .width(Length::Px(150.0))
            .height(Length::Px(80.0));
        ui.scroll(page, |ui| {
            ui.label(Label::new("Layout").style(Style::default().font_size(28.0).font_weight(700)));
            ui.label(
                Label::new(
                    "Grid tracks with auto-placement and explicit areas, min/max constraints, \
                     aspect ratios, alignment and opt-in shrinking. Resize the window to see fr \
                     tracks and percentages respond.",
                )
                .style(muted.clone()),
            );

            ui.label(
                Label::new(
                    "Grid gallery · repeat(3, 1fr), 90px implicit rows, one card spans two columns",
                )
                .style(heading.clone()),
            );
            ui.grid(gallery, |ui| {
                for index in 1..=5 {
                    ui.keyed(format!("card-{index}"), |ui| {
                        ui.container(card.clone(), |ui| {
                            ui.label(format!("Card {index}"));
                        })
                    });
                }
                let spanning = card
                    .clone()
                    .area(GridArea::span(1, 3, 2, 1))
                    .background(theme.selection);
                ui.container(spanning, |ui| {
                    ui.label("area(1, 3, span 2 × 1)");
                });
            });

            ui.label(
                Label::new(
                    "Named areas · a head over side and body over a foot, each child placed by name",
                )
                .style(heading.clone()),
            );
            let shell = full
                .clone()
                .columns(vec![Track::Px(120.0), Track::Fr(1.0)])
                .auto_rows(vec![Track::Px(44.0)])
                .areas(["head head", "side body", "foot foot"])
                .align_items(Align::Stretch)
                .row_gap(6.0)
                .column_gap(12.0);
            ui.grid(shell, |ui| {
                for name in ["foot", "body", "side", "head"] {
                    ui.keyed(name, |ui| {
                        ui.container(card.clone().area_name(name.to_owned()), |ui| {
                            ui.label(name);
                        })
                    });
                }
            });

            ui.label(
                Label::new(
                    "Auto-fill · repeat(auto-fill, minmax(110px, 1fr)) adds a column whenever                      another 110 px fits",
                )
                .style(heading.clone()),
            );
            let tiles = full
                .clone()
                .columns(vec![Track::auto_fill(vec![Track::MinMax(
                    TrackSize::Px(110.0),
                    TrackSize::Fr(1.0),
                )])])
                .auto_rows(vec![Track::Px(56.0)])
                .gap(10.0);
            ui.grid(tiles, |ui| {
                for index in 1..=7 {
                    ui.keyed(format!("tile-{index}"), |ui| {
                        ui.container(card.clone(), |ui| {
                            ui.label(format!("Tile {index}"));
                        })
                    });
                }
            });

            ui.label(
                Label::new(
                    "Dense flow · wide cards leave holes and the small ones after them fill them",
                )
                .style(heading.clone()),
            );
            let packed = full
                .clone()
                .columns(vec![Track::repeat(4, vec![Track::Fr(1.0)])])
                .auto_rows(vec![Track::Px(48.0)])
                .auto_flow(AutoFlow::RowDense)
                .gap(10.0);
            ui.grid(packed, |ui| {
                for (index, span) in [1, 3, 2, 1, 1, 3].into_iter().enumerate() {
                    let style = card.clone().area(GridArea::auto_span(span, 1));
                    let style = if span > 1 {
                        style.background(theme.selection)
                    } else {
                        style
                    };
                    ui.keyed(format!("packed-{index}"), |ui| {
                        ui.container(style, |ui| {
                            ui.label(format!("{index} · span {span}"));
                        })
                    });
                }
            });

            ui.label(
                Label::new(
                    "Form · columns [auto, 1fr], rows centered; auto-width items stretch to their cell",
                )
                .style(heading.clone()),
            );
            ui.grid(form, |ui| {
                ui.label("Name");
                ui.text_input(
                    TextInput::new("Name").placeholder("Your name").style(field),
                    &self.name,
                    Msg::Name,
                );
                ui.label("Volume");
                ui.slider("Volume", self.volume, 0.0..=100.0, 5.0, Msg::Volume);
                ui.label("Notify");
                ui.checkbox("Send notifications", self.notify, Msg::Notify);
            });

            ui.label(
                Label::new(
                    "Alignment · space-between, centered cross axis, one child aligned to the end; \
                     second row aligns text bottoms",
                )
                .style(heading.clone()),
            );
            ui.row(spread, |ui| {
                ui.button("Short");
                ui.button(Button::new("Taller button").style(Style::button().padding(22.0)));
                ui.button(
                    Button::new("align_self(End)").style(Style::button().align_self(Align::End)),
                );
            });
            ui.row(Style::default().align_items(Align::Baseline).gap(10.0), |ui| {
                ui.label(
                    Label::new("Baseline").style(Style::default().font_size(34.0).font_weight(700)),
                );
                ui.label(Label::new("alignment").style(Style::default().font_size(22.0)));
                ui.label(
                    Label::new("lines text bottoms up across sizes")
                        .style(Style::default().font_size(14.0).foreground(theme.text_muted)),
                );
            });

            ui.label(
                Label::new(
                    "Constraints · 16:9 cards bounded by min 120 / max 260 width; a row that \
                     shrinks only when asked",
                )
                .style(heading.clone()),
            );
            ui.row(full.clone().gap(12.0), |ui| {
                for (index, share) in [0.15, 0.3, 0.45].into_iter().enumerate() {
                    ui.keyed(format!("ratio-{index}"), |ui| {
                        ui.container(ratio_card.clone().width(Length::Percent(share)), |ui| {
                            ui.label(format!("{:.0}% → 16:9", share * 100.0));
                        })
                    });
                }
            });
            ui.checkbox("Let the wide children below shrink to fit", self.shrink, Msg::Shrink);
            let factor = if self.shrink { 1.0 } else { 0.0 };
            ui.row(full.clone().gap(12.0), |ui| {
                let wide = card.clone().width(Length::Px(520.0));
                ui.container(wide.clone().min_width(Length::Px(200.0)).shrink(factor), |ui| {
                    ui.label("520 px, min 200");
                });
                ui.container(wide.shrink(factor), |ui| {
                    ui.label("520 px");
                });
            });
            ui.label(
                Label::new(
                    "Scrolling · both axes: Shift+wheel, a touchpad, or arrow keys once the view \
                     has focus",
                )
                .style(heading),
            );
            ui.scroll(strip, |ui| {
                ui.row(Style::default().gap(8.0), |ui| {
                    for index in 1..=16 {
                        ui.keyed(format!("card-{index}"), |ui| {
                            ui.container(strip_card.clone(), |ui| {
                                ui.label(format!("Card {index}"));
                            })
                        });
                    }
                });
            });
            ui.label(
                Label::new(
                    "Grid, constraints and alignment map onto Taffy; children keep their exact \
                     size unless shrink is set.",
                )
                .style(muted),
            );
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Layout".into(),
            size: Size::new(900.0, 820.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        Layout {
            name: String::from("Grace"),
            volume: 65.0,
            notify: false,
            shrink: true,
        },
    )
}
