//! `text_decoration`: an underline or a line through a single line of text.
use glassmere_core::{
    Context, Label, Length, Link, MonospaceMeasurer, Style, StyleSheet, TextDecoration, TextWrap,
    Ui,
};
use glassmere_scene::{Color, Primitive, Rect, Size};

const INK: Color = Color(220, 40, 40, 255);

fn label(style: Style) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(300.0, 100.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("text", |ui| {
                ui.label(Label::new("Total").style(style.clone().foreground(INK)))
            });
        },
    );
    context
}

/// The one-pixel rules the label drew in its colour, and the box of its text.
fn rules(context: &Context) -> (Vec<Rect>, Rect) {
    let rules = context
        .scene()
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Quad { rect, color, .. } if *color == INK && rect.height == 1.0 => {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    let text = context
        .scene()
        .primitives
        .iter()
        .find_map(|p| match p {
            Primitive::Text { rect, .. } | Primitive::StyledText { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the text is drawn");
    (rules, text)
}

#[test]
fn no_decoration_is_what_it_was() {
    assert!(rules(&label(Style::default())).0.is_empty());
}

#[test]
fn an_underline_is_a_rule_under_the_text_as_wide_as_the_text() {
    let (rules, text) = rules(&label(
        Style::default().text_decoration(TextDecoration::Underline),
    ));
    assert_eq!(rules.len(), 1);
    let rule = rules[0];
    assert_eq!(rule.x, text.x);
    assert!(
        rule.y > text.y + text.height / 2.0,
        "{rule:?} against {text:?}"
    );
    assert!(rule.width > 0.0 && rule.width <= text.width + 0.01);
}

#[test]
fn a_line_through_is_a_rule_through_the_middle() {
    let (strike, text) = rules(&label(
        Style::default().text_decoration(TextDecoration::LineThrough),
    ));
    let (under, _) = rules(&label(
        Style::default().text_decoration(TextDecoration::Underline),
    ));
    assert_eq!(strike.len(), 1);
    let middle = text.y + text.height / 2.0;
    assert!(
        (strike[0].y + 0.5 - middle).abs() < 1.0,
        "{:?} against {middle}",
        strike[0]
    );
    assert!(strike[0].y < under[0].y, "through the text, not under it");
    assert_eq!(strike[0].width, under[0].width);
}

#[test]
fn text_that_wraps_has_none() {
    let (rules, _) = rules(&label(
        Style::default()
            .width(Length::Px(40.0))
            .wrap(TextWrap::Word)
            .text_decoration(TextDecoration::Underline),
    ));
    assert!(rules.is_empty());
}

#[test]
fn a_link_is_underlined_whatever_the_style_says() {
    let mut context = Context::new();
    context.run(
        Size::new(300.0, 100.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("link", |ui| {
                ui.link(Link::new("More").style(Style::default().foreground(INK)))
            });
        },
    );
    assert_eq!(rules(&context).0.len(), 1);
}

#[test]
fn a_stylesheet_has_text_decoration() {
    let sheet = StyleSheet::parse(
        "class gone {\ntext-decoration: line-through\n}\nclass link {\ntext-decoration: underline\n}\nclass plain {\ntext-decoration: none\n}",
    )
    .unwrap();
    assert_eq!(
        sheet.class("gone").unwrap().text_decoration,
        TextDecoration::LineThrough
    );
    assert_eq!(
        sheet.class("link").unwrap().text_decoration,
        TextDecoration::Underline
    );
    assert_eq!(
        sheet.class("plain").unwrap().text_decoration,
        TextDecoration::None
    );
    assert!(StyleSheet::parse("class bad {\ntext-decoration: overline\n}").is_err());
}
