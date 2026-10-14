//! Reproducible headless declaration/layout/paint benchmark, without GPU/font costs.
use rust_gui_core::Label;
use rust_gui_core::{Context, FrameStats, Length, MonospaceMeasurer, Style, Ui};
use rust_gui_scene::{Color, Size};
use std::time::Instant;

fn declare(ui: &mut Ui<'_>, count: usize, color: Color, extra: bool) {
    for group in 0..count.div_ceil(100) {
        ui.keyed(format!("group-{group}"), |ui| {
            ui.column(
                Style {
                    width: Length::Px(400.0),
                    ..Style::default()
                },
                |ui| {
                    for item in group * 100..((group + 1) * 100).min(count) {
                        ui.keyed(item.to_string(), |ui| {
                            ui.label(Label::new(format!("Item {item:05}")).style(Style {
                                foreground: Some(if item == 0 { color } else { Color::WHITE }),
                                ..Style::default()
                            }))
                        });
                    }
                    if extra && group == 0 {
                        ui.keyed("extra", |ui| ui.label("Temporary row"));
                    }
                },
            )
        });
    }
}

fn main() {
    println!("labels,phase,mean_us,p95_us,created,removed,layout,measures,paint_nodes,cache_hits");
    for count in [1000, 10000] {
        let mut context = Context::new();
        for phase in ["cold", "unchanged", "one_color", "toggle_child"] {
            let mut times = Vec::new();
            let mut stats = FrameStats::default();
            for iteration in 0..if phase == "cold" { 1 } else { 40 } {
                let color = if phase == "one_color" && iteration % 2 == 0 {
                    Color::rgb(200, 0, 0)
                } else {
                    Color::WHITE
                };
                let extra = phase == "toggle_child" && iteration % 2 == 0;
                let start = Instant::now();
                stats = context.run(Size::new(800.0, 600.0), &mut MonospaceMeasurer, |ui| {
                    declare(ui, count, color, extra)
                });
                times.push(start.elapsed().as_secs_f64() * 1_000_000.0);
            }
            let mean = times.iter().sum::<f64>() / times.len() as f64;
            times.sort_by(f64::total_cmp);
            let p95 = times[(times.len() * 95 / 100).min(times.len() - 1)];
            println!(
                "{count},{phase},{mean:.1},{p95:.1},{},{},{},{},{},{}",
                stats.created,
                stats.removed,
                stats.layout_passes,
                stats.text_measures,
                stats.paint_nodes,
                stats.paint_cache_hits
            );
        }
    }
}
