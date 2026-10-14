use rust_gui::{
    AppOptions, Color, FontFamily, Label, Length, MenuItem, NumberOptions, Size, Style, TextInput,
    TextWrap, Toast, run, suggest,
};

const CITIES: [&str; 10] = [
    "Berlin",
    "Bern",
    "Bergen",
    "Bratislava",
    "Brno",
    "Brussels",
    "Bucharest",
    "Budapest",
    "Prague",
    "Paris",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--rtl` mirrors the window as a right-to-left locale would.
    let direction = if std::env::args().any(|arg| arg == "--rtl") {
        rust_gui::DirectionChoice::Rtl
    } else {
        rust_gui::DirectionChoice::Auto
    };
    let mut name = String::from("Hello, 世界 👩‍💻");
    let mut notifications = false;
    let mut mode = 0;
    let mut volume = 50.0;
    let mut theme = 0;
    let mut quantity = 2.5;
    let mut city = String::new();
    let mut action = String::from("none");
    let mut submissions = 0;
    run(
        AppOptions {
            title: "Rust GUI / Text and controls".into(),
            size: Size::new(880.0, 740.0),
            frame_limit: smoke.then_some(3),
            direction,
            ..AppOptions::default()
        },
        move |ui| {
            ui.scroll(Style { width: Length::Percent(1.0), height: Length::Percent(1.0), padding: 28.0, gap: 18.0, ..Style::default() }, |ui| {
            ui.label(Label::new("Text & controls").style(Style { font_size: Some(30.0), font_weight: Some(700), ..Style::default() }));
            ui.label(Label::new("Native text editing and standard controls. Tab moves focus; arrow keys adjust values and navigate choices. Resize the window to see this paragraph wrap.").style(Style {
                width: Length::Percent(1.0), wrap: TextWrap::Word, foreground: Some(Color::rgb(162, 180, 198)), ..Style::default()
            }));
            ui.row(Style { gap: 32.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
                ui.column(Style { width: Length::Px(360.0), gap: 12.0, ..Style::default() }, |ui| {
                    ui.label(Label::new("01 / Text editing").style(Style { font_weight: Some(700), ..Style::default() }));
                    ui.label("Name");
                    if ui.text_input(TextInput::new("Name").placeholder("Type a name").style(Style {
                        width: Length::Px(340.0), height: Length::Px(46.0), padding: 10.0, ..Style::button()
                    }), &mut name).submitted { submissions += 1; }
                    ui.label(Label::new(format!("Text: {name}")).style(Style { width: Length::Px(340.0), wrap: TextWrap::Word, ..Style::default() }));
                    ui.label(format!("Submissions: {submissions}"));
                    ui.label(Label::new("Select with Shift + arrows or drag. Ctrl/Cmd + A, C, X, V and Z handle selection, clipboard and undo. Enter submits.").style(Style {
                        width: Length::Px(340.0), wrap: TextWrap::Word, font_size: Some(14.0), foreground: Some(Color::rgb(162, 180, 198)), ..Style::default()
                    }));
                    ui.label(Label::new("Shaped serif · bold italic").style(Style { font_family: Some(FontFamily::Serif), font_weight: Some(700), italic: Some(true), font_size: Some(22.0), ..Style::default() }));
                    ui.label(Label::new("let native = true;").style(Style { font_family: Some(FontFamily::Monospace), foreground: Some(Color::rgb(103, 232, 206)), ..Style::default() }));
                    ui.enabled(false, |ui| { ui.text_input("Unavailable field", &mut String::from("Unavailable field")); });
                });
                ui.column(Style { width: Length::Px(340.0), gap: 12.0, ..Style::default() }, |ui| {
                    ui.label(Label::new("02 / Choices & actions").style(Style { font_weight: Some(700), ..Style::default() }));
                    ui.checkbox("Notifications", &mut notifications);
                    ui.row(Style { gap: 8.0, ..Style::default() }, |ui| {
                        ui.radio("Quiet", &mut mode, 0); ui.radio("Balanced", &mut mode, 1);
                    });
                    ui.slider("Volume", &mut volume, 0.0..=100.0, 5.0);
                    ui.label("Theme"); ui.combo_box("Theme", &mut theme, &["Dark", "Light", "System"]);
                    // Type, then Enter or Tab commits; arrows, PageUp/PageDown and the buttons step.
                    ui.label("Quantity"); ui.number_input("Quantity", &mut quantity, NumberOptions::default().range(0.0, 20.0).step(0.5).suffix(" kg"));
                    // The app decides what matches; `suggest` is the simple substring source.
                    let matches = suggest(&city, &CITIES, 6);
                    ui.label("City"); ui.autocomplete("City", &mut city, &matches);
                    if let Some(index) = ui.menu("Actions", &[MenuItem::new("Save draft"), MenuItem::disabled("Publish (unavailable)"), MenuItem::new("Reset name")]) {
                        action = if index == 0 { "saved".into() } else { name = "Hello".into(); "reset".into() };
                        // Toasts confirm without taking focus; the action comes back through `toast_action`.
                        if index == 0 { ui.toast(Toast::success("Draft saved").action("Undo").tag("draft")); }
                        else { ui.toast(Toast::new("Name reset")); }
                    }
                    if ui.toast_action().is_some_and(|clicked| clicked.tag == "draft") { action = "save undone".into(); }
                    ui.label(format!("Last action: {action}"));
                    ui.label(format!("State: {notifications} / {mode} / {volume:.0} / {theme}"));
                    ui.label(format!("Fields: {quantity} kg / {city}"));
                    if ui.button("Export in the background").clicked() {
                        // Any thread raises a toast in this window through the sender.
                        let toasts = ui.toast_sender();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            toasts.send(Toast::error("Export failed: the disk is full. Free some space and try again."));
                        });
                    }
                });
            });
            ui.label(Label::new("Event-driven rendering · system fonts · native accessibility").style(Style { font_size: Some(13.0), foreground: Some(Color::rgb(162, 180, 198)), ..Style::default() }));
        });
        },
    )
}
