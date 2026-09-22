//! What the compiler checks before the application runs.
//!
//! `style_sheet!` parses the file while this test crate compiles, with the
//! framework's own parser, so a stylesheet that would fail at start fails
//! the build instead. The test below is what it produces; the check itself
//! has already happened by the time it runs.
#![cfg(feature = "derive")]

use rust_gui::{Color, Length, StyleSheet, style_sheet};

#[test]
fn a_stylesheet_is_parsed_while_compiling_and_cannot_fail_at_start() {
    let sheet: StyleSheet = style_sheet!("examples/theming.style");
    let title = sheet.class("title").expect("the title class");
    assert_eq!(title.font_size, Some(30.0));
    assert!(sheet.class("nonesuch").is_none());
    // The same text, parsed again the ordinary way, gives the same thing.
    let text = include_str!("../examples/theming.style");
    let again = StyleSheet::parse(text).expect("it parses");
    assert_eq!(
        again.class("title").map(|style| style.font_size),
        Some(Some(30.0))
    );
    let _ = (Color::WHITE, Length::Px(1.0));
}
