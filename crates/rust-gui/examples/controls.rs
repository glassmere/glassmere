use rust_gui::{
    App, AppOptions, Button, Color, Effects, FontFamily, Label, Length, MenuItem, NumberOptions,
    Size, Style, TextInput, TextWrap, Toast, Ui, run_app, suggest,
};
use std::time::Duration;

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

#[derive(Clone)]
enum Msg {
    Name(String),
    Submitted,
    Notify(bool),
    Mode(usize),
    Volume(f64),
    Theme(usize),
    Quantity(f64),
    City(String),
    Action(usize),
    /// The Undo button of the draft toast.
    Undo,
    Export,
    ExportFailed,
}

struct Controls {
    name: String,
    notifications: bool,
    mode: usize,
    volume: f64,
    theme: usize,
    quantity: f64,
    city: String,
    action: String,
    submissions: u32,
}

/// The menu's items, shared by the view and the `Action` message.
fn actions() -> [MenuItem; 3] {
    [
        MenuItem::new("Save draft"),
        MenuItem::disabled("Publish (unavailable)"),
        MenuItem::new("Reset name"),
    ]
}

impl App for Controls {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Name(name) => self.name = name,
            Msg::Submitted => self.submissions += 1,
            Msg::Notify(on) => self.notifications = on,
            Msg::Mode(mode) => self.mode = mode,
            Msg::Volume(volume) => self.volume = volume,
            Msg::Theme(theme) => self.theme = theme,
            Msg::Quantity(quantity) => self.quantity = quantity,
            Msg::City(city) => self.city = city,
            Msg::Action(index) => {
                if index == 0 {
                    self.action = "saved".into();
                    // Toasts confirm without taking focus; the action comes back as a message.
                    effects.toast_msg(
                        Toast::success("Draft saved").action("Undo").tag("draft"),
                        |_| Msg::Undo,
                    );
                } else {
                    self.action = "reset".into();
                    self.name = "Hello".into();
                    effects.toast(Toast::new("Name reset"));
                }
            }
            Msg::Undo => self.action = "save undone".into(),
            Msg::Export => {
                // Work on a thread; what it sends arrives as a message.
                let _ = effects.spawn(
                    "export",
                    |sender| {
                        std::thread::sleep(Duration::from_millis(1500));
                        let _ = sender.send(());
                    },
                    |()| Msg::ExportFailed,
                );
            }
            Msg::ExportFailed => effects.toast(Toast::error(
                "Export failed: the disk is full. Free some space and try again.",
            )),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.scroll(Style { width: Length::Percent(1.0), height: Length::Percent(1.0), padding: 28.0, gap: 18.0, ..Style::default() }, |ui| {
            ui.label(Label::new("Text & controls").style(Style { font_size: Some(30.0), font_weight: Some(700), ..Style::default() }));
            ui.label(Label::new("Native text editing and standard controls. Tab moves focus; arrow keys adjust values and navigate choices. Resize the window to see this paragraph wrap.").style(Style {
                width: Length::Percent(1.0), wrap: TextWrap::Word, foreground: Some(Color::rgb(162, 180, 198)), ..Style::default()
            }));
            ui.row(Style { gap: 32.0, width: Length::Percent(1.0), ..Style::default() }, |ui| {
                ui.column(Style { width: Length::Px(360.0), gap: 12.0, ..Style::default() }, |ui| {
                    ui.label(Label::new("01 / Text editing").style(Style { font_weight: Some(700), ..Style::default() }));
                    ui.label("Name");
                    ui.text_input_msg(TextInput::new("Name").placeholder("Type a name").style(Style {
                        width: Length::Px(340.0), height: Length::Px(46.0), padding: 10.0, ..Style::button()
                    }).on_submit(Msg::Submitted), &self.name, Msg::Name);
                    ui.label(Label::new(format!("Text: {}", self.name)).style(Style { width: Length::Px(340.0), wrap: TextWrap::Word, ..Style::default() }));
                    ui.label(format!("Submissions: {}", self.submissions));
                    ui.label(Label::new("Select with Shift + arrows or drag. Ctrl/Cmd + A, C, X, V and Z handle selection, clipboard and undo. Enter submits.").style(Style {
                        width: Length::Px(340.0), wrap: TextWrap::Word, font_size: Some(14.0), foreground: Some(Color::rgb(162, 180, 198)), ..Style::default()
                    }));
                    ui.label(Label::new("Shaped serif · bold italic").style(Style { font_family: Some(FontFamily::Serif), font_weight: Some(700), italic: Some(true), font_size: Some(22.0), ..Style::default() }));
                    ui.label(Label::new("let native = true;").style(Style { font_family: Some(FontFamily::Monospace), foreground: Some(Color::rgb(103, 232, 206)), ..Style::default() }));
                    // Disabled, so it never sends: the name's message is only a placeholder here.
                    ui.enabled(false, |ui| { ui.text_input_msg("Unavailable field", "Unavailable field", Msg::Name); });
                });
                ui.column(Style { width: Length::Px(340.0), gap: 12.0, ..Style::default() }, |ui| {
                    ui.label(Label::new("02 / Choices & actions").style(Style { font_weight: Some(700), ..Style::default() }));
                    ui.checkbox_msg("Notifications", self.notifications, Msg::Notify);
                    ui.row(Style { gap: 8.0, ..Style::default() }, |ui| {
                        ui.radio_msg("Quiet", self.mode, 0, Msg::Mode);
                        ui.radio_msg("Balanced", self.mode, 1, Msg::Mode);
                    });
                    ui.slider_msg("Volume", self.volume, 0.0..=100.0, 5.0, Msg::Volume);
                    ui.label("Theme");
                    ui.combo_box_msg("Theme", self.theme, &["Dark", "Light", "System"], Msg::Theme);
                    // Type, then Enter or Tab commits; arrows, PageUp/PageDown and the buttons step.
                    ui.label("Quantity");
                    ui.number_input_msg("Quantity", self.quantity, NumberOptions::default().range(0.0, 20.0).step(0.5).suffix(" kg"), Msg::Quantity);
                    // The app decides what matches; `suggest` is the simple substring source.
                    let matches = suggest(&self.city, &CITIES, 6);
                    ui.label("City");
                    ui.autocomplete_msg("City", &self.city, &matches, Msg::City);
                    ui.menu_msg("Actions", &actions(), Msg::Action);
                    ui.label(format!("Last action: {}", self.action));
                    ui.label(format!("State: {} / {} / {:.0} / {}", self.notifications, self.mode, self.volume, self.theme));
                    ui.label(format!("Fields: {} kg / {}", self.quantity, self.city));
                    ui.button(Button::new("Export in the background").on_click(Msg::Export));
                });
            });
            ui.label(Label::new("Event-driven rendering · system fonts · native accessibility").style(Style { font_size: Some(13.0), foreground: Some(Color::rgb(162, 180, 198)), ..Style::default() }));
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--rtl` mirrors the window as a right-to-left locale would.
    let direction = if std::env::args().any(|arg| arg == "--rtl") {
        rust_gui::DirectionChoice::Rtl
    } else {
        rust_gui::DirectionChoice::Auto
    };
    run_app(
        AppOptions {
            title: "Rust GUI / Text and controls".into(),
            size: Size::new(880.0, 740.0),
            frame_limit: smoke.then_some(3),
            direction,
            ..AppOptions::default()
        },
        Controls {
            name: String::from("Hello, 世界 👩‍💻"),
            notifications: false,
            mode: 0,
            volume: 50.0,
            theme: 0,
            quantity: 2.5,
            city: String::new(),
            action: String::from("none"),
            submissions: 0,
        },
    )
}
