use rust_gui_core::{
    Context, Date, DateStyle, LocaleFormat, MonospaceMeasurer, NumberOptions, Time,
};
use rust_gui_icu::IcuFormat;
use rust_gui_scene::Size;

const DAY: Date = Date {
    year: 2026,
    month: 9,
    day: 21,
};
const AFTERNOON: Time = Time {
    hour: 14,
    minute: 5,
    second: 9,
};

#[test]
fn numbers_dates_and_times_follow_the_locale() {
    let english = IcuFormat::new("en-US").unwrap();
    assert_eq!(english.number(1_234_567.5, 2), "1,234,567.50");
    assert_eq!(english.date(DAY, DateStyle::Short), "9/21/26");
    assert_eq!(english.date(DAY, DateStyle::Long), "September 21, 2026");
    assert_eq!(english.time(AFTERNOON, false), "2:05\u{202f}PM");

    let czech = IcuFormat::new("cs_CZ").unwrap();
    assert_eq!(czech.locale(), "cs-CZ");
    assert_eq!(czech.number(1_234_567.5, 2), "1\u{a0}234\u{a0}567,50");
    assert_eq!(czech.number(-0.5, 1), "-0,5");
    assert_eq!(czech.date(DAY, DateStyle::Medium), "21. 9. 2026");
    assert_eq!(czech.date(DAY, DateStyle::Long), "21. září 2026");
    assert_eq!(czech.time(AFTERNOON, true), "14:05:09");

    let swiss = IcuFormat::new("de-CH").unwrap();
    assert_eq!(swiss.number(1_234_567.5, 2), "1'234'567.50");
    assert_eq!(swiss.date(DAY, DateStyle::Long), "21. September 2026");

    assert!(IcuFormat::new("not a locale").is_err());
    // Out of range values do not panic.
    assert_eq!(czech.number(f64::NAN, 2), "NaN");
    assert_eq!(
        czech.date(
            Date {
                year: 2026,
                month: 13,
                day: 40
            },
            DateStyle::Short
        ),
        "2026-13-40"
    );
}

#[test]
fn parsing_accepts_what_the_locale_prints_and_what_people_type() {
    for locale in [
        "en-US", "cs-CZ", "de-CH", "de-DE", "fr-FR", "ar-EG", "hi-IN",
    ] {
        let format = IcuFormat::new(locale).unwrap();
        for value in [0.0, 12.5, -0.5, 1_234_567.25, -98_765.0] {
            let text = format.number(value, 2);
            assert_eq!(
                format.parse_number(&text),
                Some(value),
                "{locale}: {text:?}"
            );
        }
    }
    let czech = IcuFormat::new("cs-CZ").unwrap();
    // An ordinary space, no grouping at all, a typographic minus, an exponent.
    assert_eq!(czech.parse_number("1 234,5"), Some(1234.5));
    assert_eq!(czech.parse_number("1234,5"), Some(1234.5));
    assert_eq!(czech.parse_number(" \u{2212}3,25 "), Some(-3.25));
    assert_eq!(czech.parse_number("1,5e3"), Some(1500.0));
    assert_eq!(czech.parse_number("12 kg"), None);
    assert_eq!(czech.parse_number(""), None);
    // In English a comma groups; it is not a decimal separator.
    let english = IcuFormat::new("en-US").unwrap();
    assert_eq!(english.parse_number("1,234.5"), Some(1234.5));
    assert_eq!(english.parse_number("1,5"), Some(15.0));
}

#[test]
fn the_number_input_shows_and_reads_numbers_in_the_installed_locale() {
    use rust_gui_core::{Action, Event, Key, TextEdit};
    let mut context = Context::new();
    context.set_format(Some(Box::new(IcuFormat::new("cs-CZ").unwrap())));
    let mut price = 1234.5;
    let frame = |context: &mut Context, price: &mut f64| {
        context.run(Size::new(400.0, 300.0), &mut MonospaceMeasurer, |ui| {
            let options = NumberOptions::default().step(0.25).suffix(" Kč");
            ui.number_input("price", "Cena", price, options);
        });
    };
    frame(&mut context, &mut price);
    let field = context.find(&["price", "$field"]).unwrap();
    let shown = |context: &Context| context.node(field).unwrap().value.unwrap();
    assert_eq!(shown(&context), "1\u{a0}234,50 Kč");
    // Typed the Czech way, with an ordinary space and without the unit.
    context.action(field, Action::Focus);
    context.event(Event::Text(TextEdit::SelectAll));
    context.event(Event::Text(TextEdit::Insert("2 500,75".into())));
    context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
    frame(&mut context, &mut price);
    frame(&mut context, &mut price);
    assert_eq!(price, 2500.75);
    assert_eq!(shown(&context), "2\u{a0}500,75 Kč");

    // Without an installed format everything stays plain, as before.
    context.set_format(None);
    frame(&mut context, &mut price);
    assert_eq!(shown(&context), "2500.75 Kč");
    assert_eq!(context.format_date(DAY, DateStyle::Long), "2026-09-21");
    assert_eq!(context.format_time(AFTERNOON, false), "14:05");
}
