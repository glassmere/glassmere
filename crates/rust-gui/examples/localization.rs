//! Localization: Fluent catalogs, plurals, a language switch at run time, and
//! the framework's own controls following along. German leaves `name`
//! untranslated on purpose, to show the fallback to English.
use rust_gui::fluent::FluentMessages;
use rust_gui::{AppOptions, Length, NumberOptions, Size, Style, TextWrap, Toast, run, tr};

const LOCALES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/locales");

fn messages(locale: &str) -> Option<Box<dyn rust_gui::Messages>> {
    let mut messages = FluentMessages::new(locale, "en").ok()?;
    if let Err(error) = messages.load_dir(LOCALES) {
        eprintln!("catalog not loaded: {error}");
    }
    Some(Box::new(messages))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let start = std::env::args()
        .skip_while(|arg| arg != "--locale")
        .nth(1)
        .unwrap_or_else(|| "en".into());
    let mut wanted = Some(start);
    let (mut count, mut weight, mut name) = (2, 12.5, String::from("report.pdf"));
    run(
        AppOptions {
            title: "Rust GUI / Localization".into(),
            size: Size::new(760.0, 560.0),
            frame_limit: smoke.then_some(4),
            ..AppOptions::default()
        },
        move |ui| {
            if let Some(locale) = wanted.take() {
                ui.set_messages(messages(&locale));
            }
            let page = Style {
                padding: 28.0,
                gap: 16.0,
                width: Length::Percent(1.0),
                ..Style::default()
            };
            ui.column("page", page, |ui| {
                ui.label_styled(
                    "title",
                    format!("{} · {}", ui.tr("title"), ui.locale()),
                    Style {
                        font_size: Some(26.0),
                        ..Style::default()
                    },
                );
                ui.label_styled(
                    "intro",
                    ui.tr("intro"),
                    Style::default()
                        .width(Length::Percent(1.0))
                        .wrap(TextWrap::Word),
                );
                ui.row("languages", Style::default().gap(8.0), |ui| {
                    ui.label_keyed("label", ui.tr("language"));
                    for (locale, label) in
                        [("en", "English"), ("cs-CZ", "Čeština"), ("de", "Deutsch")]
                    {
                        if ui.button_keyed(locale, label).clicked() {
                            wanted = Some(locale.to_owned());
                        }
                    }
                });
                ui.row("files", Style::default().gap(8.0), |ui| {
                    if ui.button_keyed("add", ui.tr("add")).clicked() {
                        count += 1;
                    }
                    if ui.button_keyed("remove", ui.tr("remove")).clicked() && count > 0 {
                        count -= 1;
                    }
                });
                // `tr!` checks the key and its arguments against locales/en.ftl while compiling.
                ui.label_keyed("count", tr!(ui, "files", count = count));
                // The stepper buttons' names are the framework's and follow the language.
                ui.label_keyed("weight-label", ui.tr("weight"));
                ui.number_input(
                    "weight",
                    ui.tr("weight"),
                    &mut weight,
                    NumberOptions::default()
                        .range(0.0, 100.0)
                        .step(0.5)
                        .suffix(" kg"),
                );
                ui.label_keyed("name-label", ui.tr("name"));
                ui.text_input("name", ui.tr("name"), &mut name);
                if ui.button_keyed("save", ui.tr("save")).clicked() {
                    ui.toast(Toast::success(tr!(ui, "saved", name = name.as_str())));
                }
            });
        },
    )
}
