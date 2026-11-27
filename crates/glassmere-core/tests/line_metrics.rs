//! `line_height` and `letter_spacing`: inherited text properties that change
//! the size text measures to.
use glassmere_core::{Context, Label, MonospaceMeasurer, Style, StyleSheet, TextWrap, Ui};
use glassmere_scene::Size;

const ROOT: Size = Size::new(400.0, 300.0);

fn measured(parent: Style, label: Style, text: &str) -> Size {
    let mut context = Context::new();
    let text = text.to_owned();
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.keyed("box", |ui| {
            ui.column(parent.clone(), |ui| {
                ui.keyed("label", |ui| {
                    ui.label(Label::new(text.clone()).style(label.clone()));
                })
            })
        });
    });
    let bounds = context
        .node(context.find(&["box", "label"]).unwrap())
        .unwrap()
        .bounds;
    Size::new(bounds.width, bounds.height)
}

fn base() -> Style {
    Style::default().font_size(10.0)
}

#[test]
fn line_height_scales_the_height_of_a_line() {
    let plain = measured(base(), Style::default(), "ab");
    let loose = measured(base(), Style::default().line_height(2.0), "ab");
    assert_eq!(plain.height, 14.0);
    assert_eq!(loose.height, 20.0);
    assert_eq!(plain.width, loose.width);
}

#[test]
fn line_height_is_inherited() {
    let size = measured(base().line_height(3.0), Style::default(), "ab");
    assert_eq!(size.height, 30.0);
    let own = measured(
        base().line_height(3.0),
        Style::default().line_height(1.0),
        "ab",
    );
    assert_eq!(own.height, 10.0);
}

#[test]
fn every_wrapped_line_takes_the_line_height() {
    let wrapped = Style::default()
        .wrap(TextWrap::Word)
        .width(glassmere_core::Length::Px(30.0));
    let tight = measured(base(), wrapped.clone(), "abcdef abcdef");
    let loose = measured(base(), wrapped.line_height(2.8), "abcdef abcdef");
    assert!(loose.height > tight.height);
    assert_eq!(loose.height / tight.height, 2.0);
}

#[test]
fn letter_spacing_widens_every_character() {
    let plain = measured(base(), Style::default(), "abcd");
    let spaced = measured(base(), Style::default().letter_spacing(2.0), "abcd");
    assert_eq!(spaced.width - plain.width, 8.0);
    let tight = measured(base(), Style::default().letter_spacing(-1.0), "abcd");
    assert_eq!(plain.width - tight.width, 4.0);
    assert_eq!(plain.height, spaced.height);
}

#[test]
fn letter_spacing_is_inherited() {
    let plain = measured(base(), Style::default(), "abcd");
    let spaced = measured(base().letter_spacing(1.0), Style::default(), "abcd");
    assert_eq!(spaced.width - plain.width, 4.0);
}

#[test]
fn the_stylesheet_sets_both() {
    let sheet = StyleSheet::parse("class copy {\nline-height: 2\nletter-spacing: 1.5\n}").unwrap();
    let style = sheet.class("copy").expect("class");
    assert_eq!(style.line_height, Some(2.0));
    assert_eq!(style.letter_spacing, Some(1.5));
    assert!(StyleSheet::parse("class bad {\nline-height: 0\n}").is_err());
}
