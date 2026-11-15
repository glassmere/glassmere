//! Dates: a calendar and date pickers over a range of days, with the names of
//! months and weekdays following the language. Run it with `--features icu`
//! and the dates follow the language as well, and with `--rtl` to mirror it.
use glassmere::fluent::FluentMessages;
use glassmere::{
    App, AppOptions, Button, Calendar, Date, DatePicker, DateStyle, DirectionChoice, Effects,
    Label, Length, Size, Style, TextWrap, Theme, Ui, Weekday, run,
};

/// The first and the last day that can be picked in every control.
const FIRST: Date = Date {
    year: 2026,
    month: 9,
    day: 7,
};
const LAST: Date = Date {
    year: 2026,
    month: 12,
    day: 18,
};

/// What the application says today is. The framework has no clock, and a
/// fixed day keeps the example the same whenever it runs; a real application
/// reads the date from the system.
const TODAY: Date = Date {
    year: 2026,
    month: 9,
    day: 17,
};

#[derive(Clone)]
enum Msg {
    Locale(&'static str),
    Light(bool),
    Appointment(Date),
    Departure(Date),
    Return(Date),
}

struct Dates {
    appointment: Option<Date>,
    departure: Option<Date>,
    back: Option<Date>,
}

/// Switches the messages, and with `--features icu` the date format.
fn set_locale(effects: &mut Effects<'_, Msg>, locale: &str) {
    effects.set_messages(
        FluentMessages::new(locale, "en")
            .ok()
            .map(|messages| Box::new(messages) as Box<dyn glassmere::Messages>),
    );
    #[cfg(feature = "icu")]
    effects.set_format(
        glassmere::icu::IcuFormat::new(locale)
            .ok()
            .map(|format| Box::new(format) as Box<dyn glassmere::LocaleFormat>),
    );
}

impl App for Dates {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Locale(locale) => set_locale(effects, locale),
            Msg::Light(light) => {
                effects.set_theme(if light { Theme::light() } else { Theme::dark() })
            }
            Msg::Appointment(date) => self.appointment = Some(date),
            Msg::Departure(date) => {
                self.departure = Some(date);
                // Coming back before leaving is not a trip.
                if self.back.is_some_and(|back| back < date) {
                    self.back = None;
                }
            }
            Msg::Return(date) => self.back = Some(date),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let page = Style {
            padding: 28.0,
            gap: 16.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        let heading = Style {
            font_size: Some(26.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            ui.label(Label::new("Dates").style(heading));
            ui.label(
                Label::new(
                    "Days before the 7th of September and after the 18th of December \
                     are disabled, and a dot marks today, the 17th of September, which \
                     the application tells each control. Arrows, Page Up, Page Down, \
                     Home, End and Enter work in the calendar, and in the popup of a \
                     picker.",
                )
                .style(
                    Style::default()
                        .width(Length::Px(640.0))
                        .wrap(TextWrap::Word),
                ),
            );
            ui.row(Style::default().gap(8.0), |ui| {
                for (locale, label) in [("en", "English"), ("cs-CZ", "Čeština"), ("de", "Deutsch")]
                {
                    ui.keyed(locale, |ui| {
                        ui.button(Button::new(label).on_click(Msg::Locale(locale)))
                    });
                }
                ui.keyed("dark", |ui| {
                    ui.button(Button::new("Dark").on_click(Msg::Light(false)))
                });
                ui.keyed("light", |ui| {
                    ui.button(Button::new("Light").on_click(Msg::Light(true)))
                });
            });
            ui.row(Style::default().gap(40.0), |ui| {
                ui.column(Style::default().gap(8.0), |ui| {
                    ui.label("Appointment");
                    ui.calendar(
                        Calendar::new("Appointment day")
                            .min(FIRST)
                            .max(LAST)
                            .today(TODAY),
                        self.appointment,
                        Msg::Appointment,
                    );
                });
                ui.column(Style::default().gap(8.0), |ui| {
                    ui.label("Departure");
                    ui.date_picker(
                        DatePicker::new("Departure date")
                            .min(FIRST)
                            .max(LAST)
                            .first_weekday(Weekday::Sunday)
                            .today(TODAY)
                            .date_style(DateStyle::Long),
                        self.departure,
                        Msg::Departure,
                    );
                    ui.label("Return");
                    ui.date_picker(
                        DatePicker::new("Return date")
                            .min(self.departure.unwrap_or(FIRST))
                            .max(LAST)
                            .today(TODAY)
                            .placeholder("Choose a date"),
                        self.back,
                        Msg::Return,
                    );
                    ui.label(match (self.departure, self.back) {
                        (Some(from), Some(to)) => format!(
                            "{} to {}",
                            ui.format_date(from, DateStyle::Medium),
                            ui.format_date(to, DateStyle::Medium)
                        ),
                        _ => "Pick the day you come back.".into(),
                    });
                });
            });
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--rtl` mirrors the window as a right-to-left locale would.
    let direction = if std::env::args().any(|arg| arg == "--rtl") {
        DirectionChoice::Rtl
    } else {
        DirectionChoice::Auto
    };
    run(
        AppOptions {
            title: "Glassmere / Dates".into(),
            size: Size::new(760.0, 640.0),
            frame_limit: smoke.then_some(3),
            direction,
            ..AppOptions::default()
        },
        Dates {
            appointment: Date::new(2026, 9, 21),
            departure: Date::new(2026, 9, 21),
            back: None,
        },
    )
}
