//! Locale-aware numbers, dates and times. The context holds a `LocaleFormat`
//! that apps install (the `glassmere-icu` crate provides one backed by CLDR);
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

impl Date {
    /// The date, or `None` when it does not exist, such as 30 February or
    /// 29 February of a year that is not a leap year.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        ((1..=12).contains(&month) && (1..=days_in_month(year, month)).contains(&day))
            .then_some(Date { year, month, day })
    }

    /// The day of the week. A date written with a month or day that does not
    /// exist is read as the nearest one that does.
    pub fn weekday(&self) -> Weekday {
        // 1970-01-01, day zero, was a Thursday.
        Weekday::ALL[(self.days() + 3).rem_euclid(7) as usize]
    }

    /// The same date with a month and day that exist: an impossible one is
    /// read as the nearest that does.
    pub(crate) fn normalized(self) -> Date {
        let month = self.month.clamp(1, 12);
        Date {
            year: self.year,
            month,
            day: self.day.clamp(1, days_in_month(self.year, month)),
        }
    }

    pub(crate) fn first_of_month(self) -> Date {
        Date { day: 1, ..self }
    }

    /// How many days this date is after `earlier`, or before it when negative.
    pub(crate) fn days_from(self, earlier: Date) -> i64 {
        self.days() - earlier.days()
    }

    /// The date `delta` days later, or earlier when it is negative. Dates
    /// stay inside what `i32` years can write.
    pub(crate) fn plus_days(self, delta: i64) -> Date {
        Date::from_days(self.days().saturating_add(delta))
    }

    /// The date `delta` months later at the same day of the month, or at the
    /// last day of a month that is shorter.
    pub(crate) fn plus_months(self, delta: i32) -> Date {
        let date = self.normalized();
        let months = i64::from(date.year) * 12 + i64::from(date.month) - 1 + i64::from(delta);
        let year = months
            .div_euclid(12)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let month = months.rem_euclid(12) as u8 + 1;
        Date {
            year,
            month,
            day: date.day.min(days_in_month(year, month)),
        }
    }

    /// The date `days` days after 1970-01-01.
    fn from_days(days: i64) -> Date {
        // About what `i32` years hold; beyond it the date stops at the end.
        const LIMIT: i64 = 780_000_000_000;
        let days = days.clamp(-LIMIT, LIMIT) + 719_468;
        let era = days.div_euclid(146_097);
        let day_of_era = days.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Date {
            year: year.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
            month: month as u8,
            day: day as u8,
        }
    }

    /// Days since 1970-01-01, which is negative before it.
    fn days(&self) -> i64 {
        let month = i64::from(self.month.clamp(1, 12));
        let day = i64::from(
            self.day
                .clamp(1, days_in_month(self.year, self.month.clamp(1, 12))),
        );
        // The year starts in March, so that the leap day ends it.
        let year = i64::from(self.year) - i64::from(month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year.rem_euclid(400);
        let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }
}

/// A day of the week.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// From Monday, which is 0, to Sunday.
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    pub(crate) const ALL: [Weekday; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];
}

/// Whether the proleptic Gregorian calendar gives the year a 29 February.
fn leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// How many days `month` (1 to 12) has in `year`.
pub(crate) fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
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
