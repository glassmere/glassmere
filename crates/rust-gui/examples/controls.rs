use rust_gui::{AppOptions, Color, FontFamily, Length, MenuItem, Size, Style, TextWrap, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut name = String::from("Hello, 世界 👩‍💻");
    let mut notifications = false;
    let mut mode = 0;
    let mut volume = 50.0;
    let mut theme = 0;
    let mut action = String::from("none");
    let mut submissions = 0;
    run(
        AppOptions {
            title: "Rust GUI / Text and controls".into(),
            size: Size::new(880.0, 740.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
            ui.scroll("page", Style { width: Length::Percent(1.0), height: Length::Percent(1.0), padding: 28.0, gap: 18.0, ..Style::default() }, |ui| {
            ui.label_styled("title", "Text & controls", Style { font_size: 30.0, font_weight: 700, ..Style::default() });
            ui.label_styled("intro", "Native text editing and standard controls. Tab moves focus; arrow keys adjust values and navigate choices. Resize the window to see this paragraph wrap.", Style {
                width: Length::Percent(1.0), wrap: TextWrap::Word, foreground: Color::rgb(162, 180, 198), ..Style::default()
            });
            ui.row("columns", Style { gap: 32.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
                ui.column("editing", Style { width: Length::Px(360.0), gap: 12.0, ..Style::default() }, |ui| {
                    ui.label_styled("heading", "01 / Text editing", Style { font_weight: 700, ..Style::default() });
                    ui.label("Name");
                    if ui.text_input_styled("name", "Name", &mut name, "Type a name", Style {
                        width: Length::Px(340.0), height: Length::Px(46.0), padding: 10.0, ..Style::button()
                    }).submitted { submissions += 1; }
                    ui.label_styled("echo", format!("Text: {name}"), Style { width: Length::Px(340.0), wrap: TextWrap::Word, ..Style::default() });
                    ui.label_keyed("submitted", format!("Submissions: {submissions}"));
                    ui.label_styled("hint", "Select with Shift + arrows or drag. Ctrl/Cmd + A, C, X, V and Z handle selection, clipboard and undo. Enter submits.", Style {
                        width: Length::Px(340.0), wrap: TextWrap::Word, font_size: 14.0, foreground: Color::rgb(162, 180, 198), ..Style::default()
                    });
                    ui.label_styled("serif", "Shaped serif · bold italic", Style { font_family: FontFamily::Serif, font_weight: 700, italic: true, font_size: 22.0, ..Style::default() });
                    ui.label_styled("mono", "let native = true;", Style { font_family: FontFamily::Monospace, foreground: Color::rgb(103, 232, 206), ..Style::default() });
                    ui.enabled(false, |ui| { ui.text_input("readonly", "Unavailable field", &mut String::from("Unavailable field")); });
                });
                ui.column("choices", Style { width: Length::Px(340.0), gap: 12.0, ..Style::default() }, |ui| {
                    ui.label_styled("heading", "02 / Choices & actions", Style { font_weight: 700, ..Style::default() });
                    ui.checkbox("notifications", "Notifications", &mut notifications);
                    ui.row("mode", Style { gap: 8.0, ..Style::default() }, |ui| {
                        ui.radio("quiet", "Quiet", &mut mode, 0); ui.radio("balanced", "Balanced", &mut mode, 1);
                    });
                    ui.slider("volume", "Volume", &mut volume, 0.0..=100.0, 5.0);
                    ui.label("Theme"); ui.combo_box("theme", "Theme", &mut theme, &["Dark", "Light", "System"]);
                    if let Some(index) = ui.menu("actions", "Actions", &[MenuItem::new("Save draft"), MenuItem::disabled("Publish (unavailable)"), MenuItem::new("Reset name")]) {
                        action = if index == 0 { "saved".into() } else { name = "Hello".into(); "reset".into() };
                    }
                    ui.label_keyed("action", format!("Last action: {action}"));
                    ui.label_keyed("state", format!("State: {notifications} / {mode} / {volume:.0} / {theme}"));
                });
            });
            ui.label_styled("footer", "Event-driven rendering · system fonts · native accessibility", Style { font_size: 13.0, foreground: Color::rgb(162, 180, 198), ..Style::default() });
        });
        },
    )
}
