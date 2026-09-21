use rust_gui::{AppOptions, Color, Length, Style, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
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
            ui.hot_state(|| format!("{count} {show_details} {reversed}"));
            let message = ui.watched_text(motd);
            ui.column("page", Style { padding: 36.0, gap: 24.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
            ui.label_styled("eyebrow", "RUST GUI  /  FIRST MILESTONE", Style { font_size: Some(12.0), foreground: Some(Color::rgb(103, 232, 206)), ..Style::default() });
            ui.label_styled("title", "A small API. A retained engine.", Style { font_size: Some(30.0), ..Style::default() });
            ui.label_keyed("intro", "Native window, stable identity, flex layout and GPU rendering.");
            ui.label_styled("motd", message.as_deref().unwrap_or("").trim_end(), Style { font_size: Some(13.0), foreground: Some(Color::rgb(153, 171, 193)), ..Style::default() });
            ui.container("counter", Style { padding: 24.0, gap: 18.0, radius: 14.0, background: Some(Color::rgb(24, 35, 49)), width: Length::Percent(1.0), ..Style::default() }, |ui| {
                ui.label_styled("count", format!("Counter   {count}"), Style { font_size: Some(36.0), ..Style::default() });
                ui.row("actions", Style::default(), |ui| {
                    if ui.button_keyed("decrement", "- Decrease").clicked() { count = count.saturating_sub(1); }
                    if ui.button_styled("increment", "+ Increase", true, Style { background: Some(Color::rgb(22, 105, 99)), ..Style::button() }).clicked() { count = count.saturating_add(1); }
                    if ui.button_styled("reset", "Reset", count != 0, Style::button()).clicked() { count = 0; }
                });
                ui.label_styled("keyboard", "Tab / Shift+Tab to focus. Enter or Space to activate.", Style { font_size: Some(13.0), foreground: Some(Color::rgb(153, 171, 193)), ..Style::default() });
            });
            ui.row("toggles", Style::default(), |ui| {
                if ui.button_keyed("details", if show_details { "Hide details" } else { "Show details" }).clicked() { show_details = !show_details; }
                if ui.button_keyed("reorder", "Reorder keyed items").clicked() { reversed = !reversed; }
            });
            if show_details {
                ui.column("details", Style { gap: 8.0, ..Style::default() }, |ui| {
                    let items = if reversed { [("paint", "03  Scene commands are independent of widgets."), ("layout", "02  Unchanged declarations skip layout and paint."), ("identity", "01  Keys preserve identity when siblings move.")] }
                        else { [("identity", "01  Keys preserve identity when siblings move."), ("layout", "02  Unchanged declarations skip layout and paint."), ("paint", "03  Scene commands are independent of widgets.")] };
                    for (key, text) in items { ui.label_keyed(key, text); }
                });
            }
            ui.label_styled("footer", "Kernel prototype / text editing, scrolling and native accessibility are next.", Style { font_size: Some(12.0), foreground: Some(Color::rgb(127, 145, 166)), ..Style::default() });
        });
        },
    )
}
