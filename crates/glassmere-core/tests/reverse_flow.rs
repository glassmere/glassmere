//! `reverse`: a row or a column laid out from its end, as CSS's `row-reverse`.
use glassmere_core::{Context, Justify, Length, MonospaceMeasurer, Style, StyleSheet, Ui};
use glassmere_scene::{Rect, Size};

fn bounds(row: bool, style: Style) -> [Rect; 3] {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 400.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            let body = |ui: &mut Ui<'_>| {
                for name in ["a", "b", "c"] {
                    ui.keyed(name, |ui| {
                        ui.container(
                            Style::default()
                                .width(Length::Px(50.0))
                                .height(Length::Px(20.0)),
                            |_| {},
                        )
                    });
                }
            };
            ui.keyed("flow", |ui| {
                let style = style
                    .clone()
                    .width(Length::Px(300.0))
                    .height(Length::Px(200.0))
                    .gap(0.0);
                if row {
                    ui.row(style, body)
                } else {
                    ui.column(style, body)
                }
            });
        },
    );
    ["a", "b", "c"].map(|name| {
        context
            .node(context.find(&["flow", name]).unwrap())
            .unwrap()
            .bounds
    })
}

#[test]
fn a_reversed_row_starts_at_the_right_edge() {
    let [a, b, c] = bounds(true, Style::default().reverse(true));
    assert_eq!((a.x, b.x, c.x), (250.0, 200.0, 150.0));
    let [a, b, c] = bounds(true, Style::default());
    assert_eq!((a.x, b.x, c.x), (0.0, 50.0, 100.0));
}

#[test]
fn a_reversed_column_starts_at_the_bottom() {
    let [a, b, c] = bounds(false, Style::default().reverse(true));
    assert_eq!((a.y, b.y, c.y), (180.0, 160.0, 140.0));
}

#[test]
fn the_start_of_justify_content_is_the_end_of_a_reversed_row() {
    // `End` is then the left edge, as in CSS.
    let [a, b, c] = bounds(
        true,
        Style::default().reverse(true).justify_content(Justify::End),
    );
    // Packed into 0 to 150, the first child on the right of the group.
    assert_eq!((a.x, b.x, c.x), (100.0, 50.0, 0.0));
}

#[test]
fn a_stylesheet_has_reverse() {
    let sheet = StyleSheet::parse("class back {\nreverse: true\n}").unwrap();
    assert!(sheet.class("back").unwrap().reverse);
    assert!(StyleSheet::parse("class bad {\nreverse: sideways\n}").is_err());
}
