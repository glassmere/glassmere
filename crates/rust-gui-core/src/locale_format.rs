//! Locale-aware numbers, dates and times. The context holds a `LocaleFormat`
//! that apps install (the `rust-gui-icu` crate provides one backed by CLDR);
//! without one, numbers are plain and dates are ISO-like.

/// A calendar date in the proleptic Gregorian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
}

/// A time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateStyle {
    /// All numbers, such as 21. 9. 2026 or 9/21/26.
    Short,
    /// An abbreviated month, such as Sep 21, 2026.
    #[default]
    Medium,
    /// A full month name, such as 21. září 2026.
    Long,
}

/// Formatting and number parsing for one locale.
pub trait LocaleFormat {
    /// The number with exactly `decimals` digits after the decimal separator,
    /// grouped and written the way the locale does.
    fn number(&self, value: f64, decimals: usize) -> String;
    /// The number in text a user of this locale typed; `None` when it is not one.
    fn parse_number(&self, text: &str) -> Option<f64>;
    fn date(&self, date: Date, style: DateStyle) -> String;
    fn time(&self, time: Time, seconds: bool) -> String;
}

/// What is used without an installed format: no grouping, a decimal point,
/// and a decimal comma accepted when typing because keypads produce it.
pub(crate) fn plain_number(value: f64, decimals: usize) -> String {
    // Adding zero turns a negative zero into a plain one.
    format!("{:.*}", decimals, value + 0.0)
}

pub(crate) fn plain_parse(text: &str) -> Option<f64> {
    let text = text.trim().replace('\u{2212}', "-");
    let text = if text.contains('.') {
        text
    } else {
        text.replace(',', ".")
    };
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}

impl crate::Context {
    /// Installs locale-aware formatting, or removes it with `None`.
    pub fn set_format(&mut self, format: Option<Box<dyn LocaleFormat>>) {
        self.format = format;
        self.repaint_all();
    }

    pub fn format_number(&self, value: f64, decimals: usize) -> String {
        match &self.format {
            Some(format) => format.number(value, decimals),
            None => plain_number(value, decimals),
        }
    }

    /// The number a user typed, in the installed locale's notation.
    pub fn parse_number(&self, text: &str) -> Option<f64> {
        match &self.format {
            Some(format) => format.parse_number(text),
            None => plain_parse(text),
        }
    }

    /// `2026-09-21` without an installed format.
    pub fn format_date(&self, date: Date, style: DateStyle) -> String {
        match &self.format {
            Some(format) => format.date(date, style),
            None => format!("{:04}-{:02}-{:02}", date.year, date.month, date.day),
        }
    }

    /// `14:05` or `14:05:09` without an installed format.
    pub fn format_time(&self, time: Time, seconds: bool) -> String {
        match &self.format {
            Some(format) => format.time(time, seconds),
            None if seconds => {
                format!("{:02}:{:02}:{:02}", time.hour, time.minute, time.second)
            }
            None => format!("{:02}:{:02}", time.hour, time.minute),
        }
    }
}

impl<'a, M: 'static> crate::Ui<'a, M> {
    pub fn format_number(&self, value: f64, decimals: usize) -> String {
        self.context.format_number(value, decimals)
    }

    pub fn format_date(&self, date: Date, style: DateStyle) -> String {
        self.context.format_date(date, style)
    }

    pub fn format_time(&self, time: Time, seconds: bool) -> String {
        self.context.format_time(time, seconds)
    }
}
