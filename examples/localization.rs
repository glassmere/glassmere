//! Localization: Fluent catalogs, plurals, a language switch at run time, and
//! the framework's own controls following along. Run it with `--features icu`
//! and numbers and dates follow the language as well. German leaves `name`
//! untranslated on purpose, to show the fallback to English.
use glassmere::fluent::FluentMessages;
use glassmere::{
    App, AppOptions, Button, Date, DateStyle, Effects, Label, Length, Messages, NumberOptions,
    Size, Style, TextWrap, Toast, Ui, run, tr,
};

const LOCALES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/locales");

fn messages(locale: &str) -> Option<Box<dyn Messages>> {
    let mut messages = FluentMessages::new(locale, "en").ok()?;
    if let Err(error) = messages.load_dir(LOCALES) {
        eprintln!("catalog not loaded: {error}");
    }
    Some(Box::new(messages))
}

/// Switches the messages, and with `--features icu` the number and date format.
fn set_locale(effects: &mut Effects<'_, Msg>, locale: &str) {
    effects.set_messages(messages(locale));
    // With `--features icu`, numbers and dates follow the language too.
    #[cfg(feature = "icu")]
    effects.set_format(
        glassmere::icu::IcuFormat::new(locale)
            .ok()
            .map(|format| Box::new(format) as Box<dyn glassmere::LocaleFormat>),
    );
}

#[derive(Clone)]
enum Msg {
    Locale(String),
    Add,
    Remove,
    Weight(f64),
    Name(String),
    /// The toast text, localized when the button was declared.
    Save(String),
}

struct Localization {
    start: String,
    count: i32,
    weight: f64,
    name: String,
}

impl App for Localization {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        set_locale(effects, &self.start);
    }

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Locale(locale) => set_locale(effects, &locale),
            Msg::Add => self.count += 1,
            Msg::Remove => {
                if self.count > 0 {
                    self.count -= 1;
                }
            }
            Msg::Weight(weight) => self.weight = weight,
            Msg::Name(name) => self.name = name,
            Msg::Save(text) => effects.toast(Toast::success(text)),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let page = Style {
            padding: 28.0,
            gap: 16.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            ui.label(
                Label::new(format!("{} · {}", ui.tr("title"), ui.locale())).style(Style {
                    font_size: Some(26.0),
                    ..Style::default()
                }),
            );
            ui.label(
                Label::new(ui.tr("intro")).style(
                    Style::default()
                        .width(Length::Percent(1.0))
                        .wrap(TextWrap::Word),
                ),
            );
            ui.row(Style::default().gap(8.0), |ui| {
                ui.label(ui.tr("language"));
                for (locale, label) in [("en", "English"), ("cs-CZ", "Čeština"), ("de", "Deutsch")]
                {
                    ui.keyed(locale, |ui| {
                        ui.button(Button::new(label).on_click(Msg::Locale(locale.to_owned())))
                    });
                }
            });
            ui.row(Style::default().gap(8.0), |ui| {
                ui.button(Button::new(ui.tr("add")).on_click(Msg::Add));
                ui.button(Button::new(ui.tr("remove")).on_click(Msg::Remove));
            });
            // `tr!` checks the key and its arguments against locales/en.ftl while compiling.
            ui.label(tr!(ui, "files", count = self.count));
            let today = Date {
                year: 2026,
                month: 9,
                day: 21,
            };
            ui.label(format!(
                "{}  ·  {}",
                ui.format_number(1_234_567.891, 2),
                ui.format_date(today, DateStyle::Long)
            ));
            // The stepper buttons' names are the framework's and follow the language.
            ui.label(ui.tr("weight"));
            ui.number_input(
                ui.tr("weight"),
                self.weight,
                NumberOptions::default()
                    .range(0.0, 100.0)
                    .step(0.5)
                    .suffix(" kg"),
                Msg::Weight,
            );
            ui.label(ui.tr("name"));
            ui.text_input(ui.tr("name"), &self.name, Msg::Name);
            // The text is localized here, where `ui` is; `update` only raises it.
            let saved = tr!(ui, "saved", name = self.name.as_str());
            ui.button(Button::new(ui.tr("save")).on_click(Msg::Save(saved)));
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let start = std::env::args()
        .skip_while(|arg| arg != "--locale")
        .nth(1)
        .unwrap_or_else(|| "en".into());
    run(
        AppOptions {
            title: "Glassmere / Localization".into(),
            size: Size::new(760.0, 560.0),
            frame_limit: smoke.then_some(4),
            ..AppOptions::default()
        },
        Localization {
            start,
            count: 2,
            weight: 12.5,
            name: String::from("report.pdf"),
        },
    )
}
