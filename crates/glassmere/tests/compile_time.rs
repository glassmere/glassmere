//! What the compiler checks before the application runs.
//!
//! `style_sheet!` parses the file while this test crate compiles, with the
//! framework's own parser, so a stylesheet that would fail at start fails
//! the build instead. The test below is what it produces; the check itself
//! has already happened by the time it runs.
#![cfg(feature = "derive")]

use glassmere::{Color, StyleSheet, Theme, style_sheet};

#[test]
fn a_stylesheet_is_parsed_while_compiling_and_cannot_fail_at_start() {
    let sheet: StyleSheet = style_sheet!("../../examples/theming.style");
    let title = sheet.class("title").expect("the title class");
    assert_eq!(title.font_size, Some(30.0));
    assert!(sheet.class("nonesuch").is_none());
    // The same text, parsed again the ordinary way, gives the same thing.
    let text = include_str!("../../../examples/theming.style");
    let again = StyleSheet::parse(text).expect("it parses");
    assert_eq!(
        again.class("title").map(|style| style.font_size),
        Some(Some(30.0))
    );
}

/// `examples/theming.style` declares variables and nests rules, so the macro
/// above already accepted both; this is what it made of them. A sheet that
/// misuses either is a compile error too, which the macro's own tests check.
#[test]
fn variables_and_nested_rules_are_accepted_while_compiling() {
    let sheet: StyleSheet = style_sheet!("../../examples/theming.style");
    let amber = Color::rgb(0xf5, 0x9e, 0x0b);
    assert_eq!(sheet.apply_to(&Theme::dark()).accent, amber);
    let primary = sheet.class("primary").expect("the primary class");
    assert_eq!(primary.background, Some(amber));
    let states = primary.states.as_ref().expect("its states");
    assert_eq!(states.hover.background, Some(Color::rgb(0xfb, 0xbf, 0x24)));
    assert_eq!(states.focused.radius, Some(2.0));
    let large = sheet.class("primary.large").expect("the nested class");
    assert_eq!((large.padding, large.background), (18.0, Some(amber)));
    let danger = sheet.class("danger").expect("the danger class");
    let danger_states = danger.states.as_ref().expect("its states");
    assert_eq!(
        danger_states.hover.background,
        Some(Color::rgb(0xef, 0x44, 0x44))
    );
    assert_eq!(
        danger_states.disabled.foreground, states.disabled.foreground,
        "extends keeps what the base nested"
    );
}
