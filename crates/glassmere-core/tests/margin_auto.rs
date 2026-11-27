//! Automatic margins: `margin: auto` as CSS has it.
use glassmere_core::{AutoEdges, Context, Length, MonospaceMeasurer, Style, StyleSheet, Ui};
use glassmere_scene::{Rect, Size};

fn tile(width: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(20.0))
}

/// Bounds of three children of a row 300 wide and 60 tall.
fn row(styles: [Style; 3], align: glassmere_core::Align) -> [Rect; 3] {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 200.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("row", |ui| {
                ui.row(
                    Style::default()
                        .width(Length::Px(300.0))
                        .height(Length::Px(60.0))
                        .gap(0.0)
                        .align_items(align),
                    |ui| {
                        for (name, style) in ["a", "b", "c"].into_iter().zip(&styles) {
                            ui.keyed(name, |ui| ui.container(style.clone(), |_| {}));
                        }
                    },
                )
            });
        },
    );
    ["a", "b", "c"].map(|name| {
        context
            .node(context.find(&["row", name]).unwrap())
            .unwrap()
            .bounds
    })
}

fn start() -> glassmere_core::Align {
    glassmere_core::Align::Start
}

#[test]
fn an_auto_left_margin_pushes_a_child_to_the_end_of_the_row() {
    let [a, b, c] = row(
        [tile(50.0), tile(50.0), tile(50.0).margin_left_auto()],
        start(),
    );
    assert_eq!((a.x, b.x), (0.0, 50.0));
    assert_eq!(c.x + c.width, 300.0, "the last child sits at the end");
}

#[test]
fn auto_margins_on_both_sides_centre_a_child_and_share_the_room() {
    let [a, b, c] = row(
        [tile(40.0), tile(40.0).margin_x_auto(), tile(40.0)],
        start(),
    );
    // 300 less three of 40 is 180, 90 either side of the middle child.
    assert_eq!(a.x, 0.0);
    assert_eq!(b.x, 40.0 + 90.0);
    assert_eq!(c.x, 300.0 - 40.0);
}

#[test]
fn auto_margins_across_the_row_centre_a_child_vertically() {
    let [a, b, _] = row(
        [tile(40.0), tile(40.0).margin_y_auto(), tile(40.0)],
        start(),
    );
    assert_eq!(a.y, 0.0);
    assert_eq!(b.y, 20.0, "(60 - 20) / 2");
}

#[test]
fn a_margin_that_is_auto_ignores_the_number_beside_it() {
    let mut last = tile(50.0).margin_left_auto();
    last.margin.left = 500.0;
    let [_, _, c] = row([tile(50.0), tile(50.0), last], start());
    // 500 would not fit; the auto side takes what is left, and the child is at the end.
    assert_eq!(c.x + c.width, 300.0);
}

#[test]
fn no_auto_margin_is_what_it_was() {
    assert_eq!(Style::default().margin_auto, AutoEdges::NONE);
    let [a, b, c] = row([tile(50.0), tile(50.0), tile(50.0)], start());
    assert_eq!((a.x, b.x, c.x), (0.0, 50.0, 100.0));
}

#[test]
fn a_stylesheet_has_auto_margins() {
    let sheet = StyleSheet::parse(
        "class centred {\nmargin: 0 auto\n}\nclass end {\nmargin-left: auto\nmargin-top: 4\n}\nclass back {\nmargin: 3\nmargin-left: auto\n}",
    )
    .unwrap();
    let centred = sheet.class("centred").unwrap();
    assert_eq!(centred.margin_auto, AutoEdges::HORIZONTAL);
    assert_eq!(centred.margin.top, 0.0);
    let end = sheet.class("end").unwrap();
    assert!(end.margin_auto.left && !end.margin_auto.top);
    assert_eq!(end.margin.top, 4.0);
    let back = sheet.class("back").unwrap();
    assert!(back.margin_auto.left && !back.margin_auto.right);
    assert_eq!(back.margin.right, 3.0);
    // A number after `auto` takes the side back.
    let again = StyleSheet::parse("class x {\nmargin-left: auto\nmargin-left: 8\n}").unwrap();
    let again = again.class("x").unwrap();
    assert!(!again.margin_auto.left);
    assert_eq!(again.margin.left, 8.0);
    assert!(StyleSheet::parse("class bad {\nmargin: 1 2 3 4 5\n}").is_err());
    assert!(StyleSheet::parse("class bad {\nmargin: sideways\n}").is_err());
}
