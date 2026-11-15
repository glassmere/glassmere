use glassmere_core::{Date, Weekday};

fn date(year: i32, month: u8, day: u8) -> Date {
    Date::new(year, month, day).expect("a date that exists")
}

#[test]
fn new_accepts_exactly_the_dates_that_exist() {
    assert_eq!(
        Date::new(2026, 9, 21),
        Some(Date {
            year: 2026,
            month: 9,
            day: 21
        })
    );
    for (year, month, day) in [
        (2026, 0, 1),
        (2026, 13, 1),
        (2026, 1, 0),
        (2026, 1, 32),
        (2026, 4, 31),
        (2026, 2, 29),
        (2026, 2, 30),
        (2026, 12, 32),
    ] {
        assert_eq!(Date::new(year, month, day), None, "{year}-{month}-{day}");
    }
}

#[test]
fn leap_days_follow_the_four_hundred_year_rule() {
    // Every fourth year, except centuries, except every fourth century.
    for year in [2024, 2000, 1600, 0, -4, -400] {
        assert!(Date::new(year, 2, 29).is_some(), "{year} is a leap year");
    }
    for year in [2023, 2100, 1900, 1700, -100, 2026] {
        assert!(
            Date::new(year, 2, 29).is_none(),
            "{year} is not a leap year"
        );
        assert!(Date::new(year, 2, 28).is_some());
    }
    let lengths: Vec<u8> = (1..=12)
        .map(|month| {
            (1..=31)
                .rev()
                .find(|day| Date::new(2024, month, *day).is_some())
        })
        .map(Option::unwrap)
        .collect();
    assert_eq!(lengths, [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]);
}

#[test]
fn weekday_matches_known_dates_across_centuries() {
    for ((year, month, day), expected) in [
        ((1970, 1, 1), Weekday::Thursday),
        ((1969, 12, 31), Weekday::Wednesday),
        ((2000, 1, 1), Weekday::Saturday),
        ((2000, 2, 29), Weekday::Tuesday),
        ((1900, 1, 1), Weekday::Monday),
        ((1900, 2, 28), Weekday::Wednesday),
        // 1900 had no 29 February, so 1 March follows 28 February.
        ((1900, 3, 1), Weekday::Thursday),
        ((2100, 1, 1), Weekday::Friday),
        ((2100, 3, 1), Weekday::Monday),
        ((2024, 2, 29), Weekday::Thursday),
        ((2026, 9, 21), Weekday::Monday),
        ((1600, 1, 1), Weekday::Saturday),
        // The proleptic calendar runs back before its introduction.
        ((1, 1, 1), Weekday::Monday),
    ] {
        assert_eq!(
            date(year, month, day).weekday(),
            expected,
            "{year}-{month}-{day}"
        );
    }
}

#[test]
fn weekday_advances_by_one_for_every_day_in_four_centuries() {
    let order = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];
    let mut expected = date(1800, 1, 1).weekday();
    let mut days = 0;
    for year in 1800..2200 {
        for month in 1..=12 {
            for day in 1..=31 {
                let Some(date) = Date::new(year, month, day) else {
                    continue;
                };
                assert_eq!(date.weekday(), expected, "{date:?}");
                let at = order
                    .iter()
                    .position(|weekday| *weekday == expected)
                    .unwrap();
                expected = order[(at + 1) % 7];
                days += 1;
            }
        }
    }
    assert_eq!(days, 146_097, "400 Gregorian years");
}

#[test]
fn a_date_written_with_an_impossible_day_never_panics() {
    for (year, month, day) in [
        (2026, 2, 31),
        (2026, 13, 40),
        (2026, 0, 0),
        (i32::MIN, 255, 255),
        (i32::MAX, 12, 31),
        (i32::MIN, 1, 1),
    ] {
        let _ = Date { year, month, day }.weekday();
    }
    // It is read as the nearest date that exists.
    assert_eq!(
        Date {
            year: 2026,
            month: 2,
            day: 31
        }
        .weekday(),
        date(2026, 2, 28).weekday()
    );
}
