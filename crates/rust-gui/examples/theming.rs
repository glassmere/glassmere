use rust_gui::{
    AppOptions, Button, Color, FontFamily, Label, Length, Paint, Size, Style, TextWrap, Theme, run,
};
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
            ui.scroll(Style { width: Length::Percent(1.0), height: Length::Percent(1.0), padding: 28.0, gap: 16.0, ..Style::default() }, |ui| {
                ui.label(Label::new(format!("Theming · {} theme", theme.name)).style(title));
                ui.label(Label::new(format!("Edit {} while this window is open: classes and tokens reload within half a second. The theme follows the OS preference until a button below overrides it.", stylesheet.display())).style(muted.clone()));
                ui.row(Style { gap: 8.0, ..Style::default() }, |ui| {
                    if ui.button("Dark theme").clicked() {
                        ui.set_theme(Theme::dark());
                    }
                    if ui.button("Light theme").clicked() {
                        ui.set_theme(Theme::light());
                    }
                });
                ui.container(Style { padding: 20.0, gap: 12.0, radius: 12.0, width: Length::Percent(1.0), background: Some(theme.surface), ..Style::default() }, |ui| {
                    ui.label(Label::new("Stylesheet classes").style(Style { font_weight: Some(700), ..Style::default() }));
                    ui.row(Style { gap: 8.0, ..Style::default() }, |ui| {
                        if ui.button(Button::new("Save (primary)").style(primary.clone())).clicked() {
                            saves += 1;
                        }
                        if ui.button(Button::new("Delete (danger)").style(danger)).clicked() {
                            deleted += 1;
                        }
                        ui.button(Button::new("Disabled primary").enabled(false).style(primary));
                    });
                    ui.label(Label::new(format!("Saved {saves} · deleted {deleted}")).style(muted.clone()));
                    ui.label(Label::new("class danger extends primary { background: #dc2626 }").style(code));
                });
                ui.container(Style { padding: 20.0, gap: 8.0, radius: 12.0, width: Length::Percent(1.0), background: Some(theme.surface), font_family: Some(FontFamily::Serif), foreground: Some(theme.accent), ..Style::default() }, |ui| {
                    ui.label(Label::new("Inherited text properties").style(Style { font_weight: Some(700), ..Style::default() }));
                    ui.label("This label sets nothing: serif and accent color come from the container.");
                    ui.label(Label::new("This one overrides the size only.").style(Style { font_size: Some(22.0), ..Style::default() }));
                    ui.row(Style::default(), |ui| {
                        ui.label("Nested rows inherit too.");
                    });
                });
                ui.row(Style { gap: 16.0, ..Style::default() }, |ui| {
                    // A ramp and a shadow, from the typed API.
                    ui.container(Style { padding: 16.0, radius: 12.0, width: Length::Px(220.0), height: Length::Px(90.0), ..Style::default() }
                        .gradient(rust_gui::Gradient::linear((0.0, 0.0), (1.0, 1.0), theme.accent, theme.control))
                        .shadow(rust_gui::Shadow::lift(rust_gui::Color(0, 0, 0, 150), 10.0, 6.0)), |ui| {
                        ui.label(Label::new("Gradient and shadow").style(Style { font_weight: Some(700), foreground: Some(theme.text), ..Style::default() }));
                    });
                    ui.container(Style { padding: 16.0, radius: 45.0, width: Length::Px(90.0), height: Length::Px(90.0), ..Style::default() }
                        .gradient(rust_gui::Gradient::radial(theme.accent, rust_gui::Color(0, 0, 0, 0))), |_| {});
                    ui.container(Style { padding: 16.0, radius: 12.0, width: Length::Px(180.0), height: Length::Px(90.0), background: Some(theme.window), ..Style::default() }
                        .shadow(rust_gui::Shadow::new(rust_gui::Color(0, 0, 0, 170), 8.0).inset()), |ui| {
                        ui.label(Label::new("Inset").style(Style { foreground: Some(theme.text_muted), ..Style::default() }));
                    });
                });
                ui.container(Style { padding: 20.0, gap: 12.0, radius: 12.0, width: Length::Percent(1.0), background: Some(theme.surface), ..Style::default() }, |ui| {
                    ui.label(Label::new("Typed classes and state overrides in Rust").style(Style { font_weight: Some(700), ..Style::default() }));
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
                    ui.row(Style { gap: 12.0, ..Style::default() }, |ui| {
                        ui.button(Button::new("Outlined action").style(outlined));
                        ui.text_input("Name", &mut name);
                    });
                    ui.checkbox("Notifications", &mut notify);
                    ui.slider("Volume", &mut volume, 0.0..=100.0, 5.0);
                    ui.label(Label::new("Built-in controls paint from theme tokens: control, hover, pressed, focus, accent, track and selection.").style(Style { wrap: TextWrap::Word, width: Length::Percent(1.0), ..muted }));
                });
            });
        },
    )
}
