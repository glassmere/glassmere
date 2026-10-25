//! CLDR formatting through ICU4X as a `glassmere_core::LocaleFormat`.
//!
//! ```ignore
//! effects.set_format(glassmere::icu::IcuFormat::new("cs-CZ").ok().map(|format| Box::new(format) as _));
//! ui.label(ui.format_number(1234567.5, 2));                 // 1 234 567,50
//! ui.label(ui.format_date(Date { year: 2026, month: 9, day: 21 }, DateStyle::Long));
//! ```
//!
//! CLDR data for every locale is compiled in. Only the number, date and time
//! patterns used here are linked, which came to about one megabyte in the
//! localization example; it is an opt-in feature all the same.
use glassmere_core::{Date, DateStyle, LocaleFormat, Time};
use icu::datetime::fieldsets::{T, YMD};
use icu::datetime::input::{Date as IcuDate, Time as IcuTime};
use icu::datetime::{DateTimeFormatter, NoCalendarFormatter};
use icu::decimal::DecimalFormatter;
use icu::decimal::input::Decimal;
use icu::locale::Locale;

pub struct IcuFormat {
    locale: String,
    decimal: DecimalFormatter,
    dates: [DateTimeFormatter<YMD>; 3],
    times: [NoCalendarFormatter<T>; 2],
    /// What this locale puts between whole and fraction, and between groups,
    /// learned from its own output so that parsing accepts what it prints.
    decimal_separator: String,
    grouping_separator: String,
}

impl IcuFormat {
    /// `locale` is a BCP 47 tag such as `cs-CZ`, also with extensions such as
    /// `th-u-nu-thai`. A language without data falls back as CLDR says.
    pub fn new(locale: &str) -> Result<Self, String> {
        let tag = locale.replace('_', "-");
        let parsed: Locale = tag
            .parse()
            .map_err(|error| format!("`{locale}` is not a locale: {error}"))?;
        let failed =
            |what: &str, error: &dyn std::fmt::Display| format!("{what} for {tag}: {error}");
        let decimal = DecimalFormatter::try_new(parsed.clone().into(), Default::default())
            .map_err(|error| failed("no number format", &error))?;
        let date = |set: YMD| {
            DateTimeFormatter::try_new(parsed.clone().into(), set)
                .map_err(|error| failed("no date format", &error))
        };
        let time = |set: T| {
            NoCalendarFormatter::try_new(parsed.clone().into(), set)
                .map_err(|error| failed("no time format", &error))
        };
        let mut format = Self {
            locale: tag.clone(),
            dates: [
                date(YMD::short())?,
                date(YMD::medium())?,
                date(YMD::long())?,
            ],
            times: [time(T::hm())?, time(T::hms())?],
            decimal,
            decimal_separator: String::new(),
            grouping_separator: String::new(),
        };
        // 1.5 shows the decimal separator; 1234567 shows the grouping one.
        let between = |text: String| -> String {
            text.chars()
                .filter(|character| !is_digit(*character))
                .collect()
        };
        format.decimal_separator = between(format.number(1.5, 1));
        let grouped = between(format.number(1_234_567.0, 0));
        format.grouping_separator = grouped.chars().next().map(String::from).unwrap_or_default();
        Ok(format)
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }
}

/// ASCII digits and the digits of other scripts that ICU may print.
fn digit_value(character: char) -> Option<u32> {
    character.to_digit(10).or_else(|| {
        // Every Unicode decimal digit block is ten consecutive code points
        // starting at a multiple of sixteen or at x6/xE; these are the ones
        // numbering systems of living languages use.
        const ZEROS: [u32; 12] = [
            0x0660, 0x06F0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0BE6, 0x0C66, 0x0CE6, 0x0D66, 0x0E50,
            0x1040,
        ];
        let code = character as u32;
        ZEROS
            .iter()
            .find(|zero| (**zero..**zero + 10).contains(&code))
            .map(|zero| code - zero)
    })
}

fn is_digit(character: char) -> bool {
    digit_value(character).is_some()
}

impl LocaleFormat for IcuFormat {
    fn number(&self, value: f64, decimals: usize) -> String {
        if !value.is_finite() {
            return value.to_string();
        }
        let decimals = decimals.min(15);
        let scaled = (value * 10f64.powi(decimals as i32)).round();
        // Beyond what an i64 holds the digits are noise anyway.
        if scaled.abs() >= 9.0e18 {
            return format!("{value:.0}");
        }
        let mut decimal = Decimal::from(scaled as i64);
        decimal.multiply_pow10(-(decimals as i16));
        self.decimal.format(&decimal).to_string()
    }

    fn parse_number(&self, text: &str) -> Option<f64> {
        let mut plain = String::with_capacity(text.len());
        let mut rest = text.trim();
        while let Some(character) = rest.chars().next() {
            let width = character.len_utf8();
            if let Some(digit) = digit_value(character) {
                plain.push(char::from_digit(digit, 10)?);
            } else if !self.decimal_separator.is_empty()
                && rest.starts_with(self.decimal_separator.as_str())
            {
                plain.push('.');
                rest = &rest[self.decimal_separator.len()..];
                continue;
            } else if !(rest.starts_with(self.grouping_separator.as_str())
                && !self.grouping_separator.is_empty()
                // People type an ordinary space, or none, where the locale
                // prints a no-break one.
                || character.is_whitespace()
                || character == '\u{2019}'
                // Invisible direction marks that right-to-left locales print.
                || matches!(character, '\u{61c}' | '\u{200e}' | '\u{200f}')
                || character == '+' && plain.is_empty())
            {
                if matches!(character, '-' | '\u{2212}') && plain.is_empty() {
                    plain.push('-');
                } else if matches!(character, 'e' | 'E') && !plain.is_empty() {
                    plain.push('e');
                } else if character == '-' && plain.ends_with('e') {
                    plain.push('-');
                } else {
                    return None;
                }
            }
            rest = &rest[width..];
        }
        plain
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
    }

    fn date(&self, date: Date, style: DateStyle) -> String {
        let Ok(date) = IcuDate::try_new_iso(date.year, date.month, date.day) else {
            return format!("{:04}-{:02}-{:02}", date.year, date.month, date.day);
        };
        let formatter = &self.dates[match style {
            DateStyle::Short => 0,
            DateStyle::Medium => 1,
            DateStyle::Long => 2,
            _ => 1,
        }];
        formatter.format(&date).to_string()
    }

    fn time(&self, time: Time, seconds: bool) -> String {
        let Ok(time) = IcuTime::try_new(time.hour, time.minute, time.second, 0) else {
            return format!("{:02}:{:02}", time.hour, time.minute);
        };
        self.times[usize::from(seconds)].format(&time).to_string()
    }
}
