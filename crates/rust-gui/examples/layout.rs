use rust_gui::{Align, AppOptions, GridArea, Justify, Length, Size, Style, TextWrap, Track, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut name = String::from("Grace");
    let mut volume = 65.0;
    let mut notify = false;
    let mut shrink = true;
    run(
        AppOptions {
            title: "Rust GUI / Layout".into(),
            size: Size::new(900.0, 820.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
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
            ui.scroll("page", Style::default().width(Length::Percent(1.0)).height(Length::Percent(1.0)).padding(28.0).gap(18.0), |ui| {
                ui.label_styled("title", "Layout", Style::default().font_size(28.0).font_weight(700));
                ui.label_styled("intro", "Grid tracks with auto-placement and explicit areas, min/max constraints, aspect ratios, alignment and opt-in shrinking. Resize the window to see fr tracks and percentages respond.", muted.clone());

                ui.label_styled("gallery-heading", "Grid gallery · repeat(3, 1fr), 90px implicit rows, one card spans two columns", heading.clone());
                ui.grid("gallery", Style::default().width(Length::Percent(1.0)).columns(vec![Track::repeat(3, vec![Track::Fr(1.0)])]).auto_rows(vec![Track::Px(90.0)]).gap(12.0), |ui| {
                    for index in 1..=5 {
                        ui.container(format!("card-{index}"), card.clone(), |ui| {
                            ui.label_keyed("label", format!("Card {index}"));
                        });
                    }
                    ui.container("wide", card.clone().area(GridArea::span(1, 3, 2, 1)).background(theme.selection), |ui| {
                        ui.label_keyed("label", "area(1, 3, span 2 × 1)");
                    });
                });

                ui.label_styled("form-heading", "Form · columns [auto, 1fr], rows centered; auto-width items stretch to their cell", heading.clone());
                ui.grid("form", Style::default().width(Length::Percent(1.0)).columns(vec![Track::Auto, Track::Fr(1.0)]).align_items(Align::Center).gap(10.0), |ui| {
                    ui.label("Name");
                    ui.text_input_styled("name", "Name", &mut name, "Your name", Style::button().width(Length::Auto).height(Length::Px(44.0)).padding(10.0));
                    ui.label("Volume");
                    ui.slider("volume", "Volume", &mut volume, 0.0..=100.0, 5.0);
                    ui.label("Notify");
                    ui.checkbox("notify", "Send notifications", &mut notify);
                });

                ui.label_styled("align-heading", "Alignment · space-between, centered cross axis, one child aligned to the end; second row aligns text bottoms", heading.clone());
                ui.row("aligned", Style::default().width(Length::Percent(1.0)).height(Length::Px(84.0)).justify_content(Justify::SpaceBetween).align_items(Align::Center).background(theme.surface).radius(10.0).padding(8.0), |ui| {
                    ui.button_styled("short", "Short", true, Style::button());
                    ui.button_styled("tall", "Taller button", true, Style::button().padding(22.0));
                    ui.button_styled("end", "align_self(End)", true, Style::button().align_self(Align::End));
                });
                ui.row("baseline", Style::default().align_items(Align::Baseline).gap(10.0), |ui| {
                    ui.label_styled("big", "Baseline", Style::default().font_size(34.0).font_weight(700));
                    ui.label_styled("mid", "alignment", Style::default().font_size(22.0));
                    ui.label_styled("small", "lines text bottoms up across sizes", Style::default().font_size(14.0).foreground(theme.text_muted));
                });

                ui.label_styled("constraints-heading", "Constraints · 16:9 cards bounded by min 120 / max 260 width; a row that shrinks only when asked", heading.clone());
                ui.row("ratios", Style::default().width(Length::Percent(1.0)).gap(12.0), |ui| {
                    for (index, share) in [0.15, 0.3, 0.45].into_iter().enumerate() {
                        ui.container(format!("ratio-{index}"), card.clone().width(Length::Percent(share)).min_width(Length::Px(120.0)).max_width(Length::Px(260.0)).aspect_ratio(16.0 / 9.0), |ui| {
                            ui.label_keyed("label", format!("{:.0}% → 16:9", share * 100.0));
                        });
                    }
                });
                ui.checkbox("shrink", "Let the wide children below shrink to fit", &mut shrink);
                let factor = if shrink { 1.0 } else { 0.0 };
                ui.row("shrinking", Style::default().width(Length::Percent(1.0)).gap(12.0), |ui| {
                    ui.container("first", card.clone().width(Length::Px(520.0)).min_width(Length::Px(200.0)).shrink(factor), |ui| {
                        ui.label_keyed("label", "520 px, min 200");
                    });
                    ui.container("second", card.clone().width(Length::Px(520.0)).shrink(factor), |ui| {
                        ui.label_keyed("label", "520 px");
                    });
                });
                ui.label_styled("scrolling-heading", "Scrolling · both axes: Shift+wheel, a touchpad, or arrow keys once the view has focus", heading.clone());
                ui.scroll("strip", Style::default().width(Length::Percent(1.0)).height(Length::Px(104.0)).padding(8.0).radius(10.0).background(theme.surface), |ui| {
                    ui.row("cards", Style::default().gap(8.0), |ui| {
                        for index in 1..=16 {
                            ui.container(format!("card-{index}"), card.clone().width(Length::Px(150.0)).height(Length::Px(80.0)), |ui| {
                                ui.label_keyed("label", format!("Card {index}"));
                            });
                        }
                    });
                });
                ui.label_styled("footer", "Grid, constraints and alignment map onto Taffy; children keep their exact size unless shrink is set.", muted);
            });
        },
    )
}
