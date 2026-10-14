use rust_gui::{AppOptions, Button, Color, Label, Length, Style, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--gpu-hook` draws one rectangle with the application's own wgpu code,
    // through `AppOptions::gpu` and `Primitive::Custom`.
    let hook = std::env::args().any(|arg| arg == "--gpu-hook");
    let drawn = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = drawn.clone();
    let stress = std::env::args().any(|arg| arg == "--stress");
    let arguments: Vec<_> = std::env::args().collect();
    let stress_frames = arguments
        .windows(2)
        .find(|pair| pair[0] == "--frames")
        .map(|pair| pair[1].parse::<u64>())
        .transpose()?
        .unwrap_or(600);
    // Under `rust-gui-dev` the previous run's state comes back after a rebuild.
    let saved = rust_gui::hot::saved_state().unwrap_or_default();
    let mut fields = saved.split_whitespace();
    let mut count: i32 = fields
        .next()
        .and_then(|field| field.parse().ok())
        .unwrap_or(0);
    let mut show_details = fields.next() != Some("false");
    let mut reversed = fields.next() == Some("true");
    let motd = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/counter-motd.txt");
    run(
        AppOptions {
            title: "Rust GUI / Kernel playground".into(),
            gpu: hook.then(|| {
                rust_gui::GpuSetup::new(move |renderer| {
                    let counted = counted.clone();
                    renderer.set_custom_draw(
                        7,
                        Some(Box::new(move |draw| {
                            // A real callback would draw here; this one only
                            // reports that it was asked to, and where.
                            let seen = counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if seen == 0 {
                                println!(
                                    "GPU_HOOK rect={:?} scale={} target={:?}",
                                    draw.rect, draw.scale, draw.target_size
                                );
                            }
                        })),
                    );
                })
            }),
            // `--centered` opens it in the middle of the screen, or of the
            // one named after `--screen`.
            placement: match std::env::args().position(|arg| arg == "--screen") {
                Some(at) => {
                    rust_gui::Placement::Named(std::env::args().nth(at + 1).unwrap_or_default())
                }
                None if std::env::args().any(|arg| arg == "--centered") => {
                    rust_gui::Placement::Centered
                }
                None => rust_gui::Placement::default(),
            },
            frame_limit: if stress {
                Some(stress_frames)
            } else {
                smoke.then_some(2)
            },
            ..AppOptions::default()
        },
        move |ui| {
            if stress {
                count += 1;
                show_details = count % 60 < 30;
                reversed = (count / 30) % 2 == 0;
            }
            if hook {
                // A leaf that says "the application draws this part".
                ui.custom(
                    rust_gui::Style::default(),
                    rust_gui::CustomWidget {
                        size: rust_gui::Size::new(120.0, 60.0),
                        semantics: rust_gui::Semantics {
                            label: "Application drawing".into(),
                            ..rust_gui::Semantics::default()
                        },
                        primitives: vec![rust_gui::Primitive::Custom {
                            rect: rust_gui::Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 120.0,
                                height: 60.0,
                            },
                            id: 7,
                        }],
                        ..rust_gui::CustomWidget::default()
                    },
                );
            }
            ui.hot_state(|| format!("{count} {show_details} {reversed}"));
            let message = ui.watched_text(motd);
            ui.column(Style { padding: 36.0, gap: 24.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
            ui.label(Label::new("RUST GUI  /  FIRST MILESTONE").style(Style { font_size: Some(12.0), foreground: Some(Color::rgb(103, 232, 206)), ..Style::default() }));
            ui.label(Label::new("A small API. A retained engine.").style(Style { font_size: Some(30.0), ..Style::default() }));
            ui.label("Native window, stable identity, flex layout and GPU rendering.");
            ui.label(Label::new(message.as_deref().unwrap_or("").trim_end()).style(Style { font_size: Some(13.0), foreground: Some(Color::rgb(153, 171, 193)), ..Style::default() }));
            ui.container(Style { padding: 24.0, gap: 18.0, radius: 14.0, background: Some(Color::rgb(24, 35, 49)), width: Length::Percent(1.0), ..Style::default() }, |ui| {
                ui.label(Label::new(format!("Counter   {count}")).style(Style { font_size: Some(36.0), ..Style::default() }));
                ui.row(Style::default(), |ui| {
                    if ui.button("- Decrease").clicked() { count = count.saturating_sub(1); }
                    if ui.button(Button::new("+ Increase").style(Style { background: Some(Color::rgb(22, 105, 99)), ..Style::button() })).clicked() { count = count.saturating_add(1); }
                    if ui.button(Button::new("Reset").enabled(count != 0)).clicked() { count = 0; }
                });
                ui.label(Label::new("Tab / Shift+Tab to focus. Enter or Space to activate.").style(Style { font_size: Some(13.0), foreground: Some(Color::rgb(153, 171, 193)), ..Style::default() }));
            });
            ui.row(Style::default(), |ui| {
                if ui.button(if show_details { "Hide details" } else { "Show details" }).clicked() { show_details = !show_details; }
                if ui.button("Reorder keyed items").clicked() { reversed = !reversed; }
            });
            if show_details {
                ui.column(Style { gap: 8.0, ..Style::default() }, |ui| {
                    let items = if reversed { [("paint", "03  Scene commands are independent of widgets."), ("layout", "02  Unchanged declarations skip layout and paint."), ("identity", "01  Keys preserve identity when siblings move.")] }
                        else { [("identity", "01  Keys preserve identity when siblings move."), ("layout", "02  Unchanged declarations skip layout and paint."), ("paint", "03  Scene commands are independent of widgets.")] };
                    for (key, text) in items { ui.keyed(key, |ui| ui.label(text)); }
                });
            }
            ui.label(Label::new("Kernel prototype / text editing, scrolling and native accessibility are next.").style(Style { font_size: Some(12.0), foreground: Some(Color::rgb(127, 145, 166)), ..Style::default() }));
        });
        },
    )
}
