use rust_gui::{AppOptions, Color, FontFamily, Length, Paint, Size, Style, TextWrap, Theme, run};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let stylesheet = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/theming.style"
    ));
    let mut name = String::from("Ada");
    let mut volume = 40.0;
    let mut notify = true;
    let mut saves = 0;
    let mut deleted = 0;
    run(
        AppOptions {
            title: "Rust GUI / Theming".into(),
            size: Size::new(860.0, 720.0),
            frame_limit: smoke.then_some(3),
            stylesheet: Some(stylesheet.clone()),
            ..AppOptions::default()
        },
        move |ui| {
            // Read tokens before borrowing `ui` mutably for declarations.
            let theme = ui.theme().clone();
            let title = ui.class("title");
            let muted = ui.class("muted");
            let code = ui.class("code");
            let primary = ui.class("primary");
            let danger = ui.class("danger");
            ui.scroll("page", Style { width: Length::Percent(1.0), height: Length::Percent(1.0), padding: 28.0, gap: 16.0, ..Style::default() }, |ui| {
                ui.label_styled("title", format!("Theming · {} theme", theme.name), title);
                ui.label_styled("intro", format!("Edit {} while this window is open: classes and tokens reload within half a second. The theme follows the OS preference until a button below overrides it.", stylesheet.display()), muted.clone());
                ui.row("themes", Style { gap: 8.0, ..Style::default() }, |ui| {
                    if ui.button_keyed("dark", "Dark theme").clicked() {
                        ui.set_theme(Theme::dark());
                    }
                    if ui.button_keyed("light", "Light theme").clicked() {
                        ui.set_theme(Theme::light());
                    }
                });
                ui.container("panel", Style { padding: 20.0, gap: 12.0, radius: 12.0, width: Length::Percent(1.0), background: Some(theme.surface), ..Style::default() }, |ui| {
                    ui.label_styled("heading", "Stylesheet classes", Style { font_weight: Some(700), ..Style::default() });
                    ui.row("actions", Style { gap: 8.0, ..Style::default() }, |ui| {
                        if ui.button_styled("save", "Save (primary)", true, primary.clone()).clicked() {
                            saves += 1;
                        }
                        if ui.button_styled("delete", "Delete (danger)", true, danger).clicked() {
                            deleted += 1;
                        }
                        ui.button_styled("disabled", "Disabled primary", false, primary);
                    });
                    ui.label_styled("counts", format!("Saved {saves} · deleted {deleted}"), muted.clone());
                    ui.label_styled("snippet", "class danger extends primary { background: #dc2626 }", code);
                });
                ui.container("inherit", Style { padding: 20.0, gap: 8.0, radius: 12.0, width: Length::Percent(1.0), background: Some(theme.surface), font_family: Some(FontFamily::Serif), foreground: Some(theme.accent), ..Style::default() }, |ui| {
                    ui.label_styled("heading", "Inherited text properties", Style { font_weight: Some(700), ..Style::default() });
                    ui.label("This label sets nothing: serif and accent color come from the container.");
                    ui.label_styled("override", "This one overrides the size only.", Style { font_size: Some(22.0), ..Style::default() });
                    ui.row("nested", Style::default(), |ui| {
                        ui.label("Nested rows inherit too.");
                    });
                });
                ui.container("states", Style { padding: 20.0, gap: 12.0, radius: 12.0, width: Length::Percent(1.0), background: Some(theme.surface), ..Style::default() }, |ui| {
                    ui.label_styled("heading", "Typed classes and state overrides in Rust", Style { font_weight: Some(700), ..Style::default() });
                    let outlined = Style::button()
                        .background(Color::TRANSPARENT)
                        .foreground(theme.accent)
                        .on_hover(Paint::background(theme.control_hover))
                        .on_pressed(Paint {
                            background: Some(theme.accent),
                            foreground: Some(theme.window),
                            ..Paint::default()
                        })
                        .on_focused(Paint { radius: Some(2.0), ..Paint::default() });
                    ui.row("controls", Style { gap: 12.0, ..Style::default() }, |ui| {
                        ui.button_styled("outlined", "Outlined action", true, outlined);
                        ui.text_input("name", "Name", &mut name);
                    });
                    ui.checkbox("notify", "Notifications", &mut notify);
                    ui.slider("volume", "Volume", &mut volume, 0.0..=100.0, 5.0);
                    ui.label_styled("hint", "Built-in controls paint from theme tokens: control, hover, pressed, focus, accent, track and selection.", Style { wrap: TextWrap::Word, width: Length::Percent(1.0), ..muted });
                });
            });
        },
    )
}
