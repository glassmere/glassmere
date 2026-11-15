//! The month a calendar shows and where its keys take the cursor.

use crate::{Date, Key, Weekday};

/// Where a calendar starts when it has neither a selection nor a minimum.
const FALLBACK: Date = Date {
    year: 2000,
    month: 1,
    day: 1,
};

/// The weeks a month view always has, so that it keeps its height.
pub(crate) const WEEKS: usize = 6;

/// A month as a calendar shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Month {
    /// The day the keys move from. It lies in `min..=max` and in the month on
    /// show, which is therefore the cursor's.
    pub cursor: Date,
    pub selected: Option<Date>,
    pub min: Option<Date>,
    pub max: Option<Date>,
    pub first: Weekday,
}

/// What a calendar keeps between declarations: where the cursor is, and the
/// selection it was moved from. A different selection is the application
/// moving it, and the cursor follows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    pub cursor: Date,
    pub seen: Option<Date>,
}

fn clamp(date: Date, min: Option<Date>, max: Option<Date>) -> Date {
    let date = max.map_or(date, |max| date.min(max));
    min.map_or(date, |min| date.max(min))
}

impl Month {
    /// The month for `view`, or for the selection, the minimum or a fixed
    /// date when there is none or the selection has changed since.
    pub fn new(
        view: Option<View>,
        selected: Option<Date>,
        min: Option<Date>,
        max: Option<Date>,
        first: Weekday,
    ) -> Self {
        let cursor = view
            .filter(|view| view.seen == selected)
            .map_or_else(|| selected.or(min).unwrap_or(FALLBACK), |view| view.cursor);
        Self {
            cursor: clamp(cursor, min, max),
            selected,
            min,
            max,
            first,
        }
    }

    /// The month with the cursor on `cursor`.
    pub fn at(&self, cursor: Date) -> Self {
        Self {
            cursor: clamp(cursor, self.min, self.max),
            ..self.clone()
        }
    }

    pub fn view(&self) -> View {
        View {
            cursor: self.cursor,
            seen: self.selected,
        }
    }

    pub fn shown(&self) -> Date {
        self.cursor.first_of_month()
    }

    pub fn enabled(&self, date: Date) -> bool {
        self.min.is_none_or(|min| date >= min) && self.max.is_none_or(|max| date <= max)
    }

    /// How many days `date` is after the first day of its week.
    fn offset(&self, date: Date) -> i64 {
        ((date.weekday().index() + 7 - self.first.index()) % 7) as i64
    }

    /// The date in the top corner of the grid: the first day of the week the
    /// month begins in.
    fn start(&self) -> Date {
        let shown = self.shown();
        shown.plus_days(-self.offset(shown))
    }

    pub fn date(&self, row: usize, column: usize) -> Date {
        self.start().plus_days((row * 7 + column) as i64)
    }

    /// The row and column of `date` in the grid, if it is in one.
    pub fn position(&self, date: Date) -> Option<(usize, usize)> {
        let days = date.days_from(self.start());
        (0..(WEEKS * 7) as i64)
            .contains(&days)
            .then_some(((days / 7) as usize, (days % 7) as usize))
    }

    /// Where `key` takes the cursor, for the keys that move it.
    pub fn moved(&self, key: Key, rtl: bool) -> Option<Date> {
        let cursor = self.cursor;
        // Left and Right follow the reading direction on screen.
        let along = if rtl { -1 } else { 1 };
        let target = match key {
            Key::ArrowLeft => cursor.plus_days(-along),
            Key::ArrowRight => cursor.plus_days(along),
            Key::ArrowUp => cursor.plus_days(-7),
            Key::ArrowDown => cursor.plus_days(7),
            Key::PageUp => cursor.plus_months(-1),
            Key::PageDown => cursor.plus_months(1),
            Key::Home => cursor.plus_days(-self.offset(cursor)),
            Key::End => cursor.plus_days(6 - self.offset(cursor)),
            _ => return None,
        };
        Some(clamp(target, self.min, self.max))
    }

    /// Whether any day of the month `months` away is enabled.
    pub fn can_step(&self, months: i32) -> bool {
        let first = self.shown().plus_months(months);
        let last = first.plus_months(1).plus_days(-1);
        self.min.is_none_or(|min| last >= min) && self.max.is_none_or(|max| first <= max)
    }

    /// The cursor `months` away, on the same day of the month where there is
    /// one.
    pub fn stepped(&self, months: i32) -> Date {
        clamp(self.cursor.plus_months(months), self.min, self.max)
    }
}
