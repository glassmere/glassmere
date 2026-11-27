//! A row whose children wrap onto new lines when they do not fit.
use glassmere_core::{Align, Context, Length, MonospaceMeasurer, Style, StyleSheet, Ui};
use glassmere_scene::{Rect, Size};

fn tile() -> Style {
    Style::default()
        .width(Length::Px(40.0))
        .height(Length::Px(20.0))
}

fn at(context: &Context, name: &str) -> Rect {
    context
        .node(context.find(&["row", name]).unwrap())
        .unwrap()
        .bounds
}

fn tiles(wrap: bool, gap: f32) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 300.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            let row = Style::default()
                .width(Length::Px(100.0))
                .gap(gap)
                .align_items(Align::Start)
                .wrap_items(wrap);
            ui.keyed("row", |ui| {
                ui.row(row, |ui| {
                    for name in ["a", "b", "c", "d", "e"] {
                        ui.keyed(name, |ui| ui.container(tile(), |_| {}));
                    }
                })
            });
        },
    );
    context
}

#[test]
fn children_that_do_not_fit_go_on_a_new_line() {
    let context = tiles(true, 0.0);
    let (a, b, c, d, e) = (
        at(&context, "a"),
        at(&context, "b"),
        at(&context, "c"),
        at(&context, "d"),
        at(&context, "e"),
    );
    // Two of 40 fit in 100; the third starts the next line.
    assert_eq!((a.y, b.y), (c.y - 20.0, a.y));
    assert_eq!((c.x, d.x), (a.x, b.x), "each line starts at the start edge");
    assert_eq!((c.y, d.y), (d.y, e.y - 20.0));
    assert_eq!(e.x, a.x);
    assert_eq!(b.x, a.x + 40.0);
}

#[test]
fn the_gap_is_between_the_lines_too() {
    let context = tiles(true, 10.0);
    let (a, b, c) = (at(&context, "a"), at(&context, "b"), at(&context, "c"));
    assert_eq!(b.x - a.x, 50.0, "40 and 10");
    assert_eq!(c.y - a.y, 30.0, "20 and 10");
}

#[test]
fn without_wrapping_they_stay_on_one_line() {
    let context = tiles(false, 0.0);
    let (a, e) = (at(&context, "a"), at(&context, "e"));
    assert_eq!(a.y, e.y);
    assert_eq!(e.x - a.x, 160.0, "past the 100 px of the row");
}

#[test]
fn a_stylesheet_turns_wrapping_on() {
    let sheet =
        StyleSheet::parse("class cloud {\nwrap-items: true\n}\nclass line {\nwrap-items: false\n}")
            .unwrap();
    assert!(sheet.class("cloud").unwrap().wrap_items);
    assert!(!sheet.class("line").unwrap().wrap_items);
    assert!(StyleSheet::parse("class bad {\nwrap-items: maybe\n}").is_err());
}

fn lines(content: Option<glassmere_core::Justify>) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 300.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            let mut row = Style::default()
                .width(Length::Px(100.0))
                .height(Length::Px(200.0))
                .gap(0.0)
                .align_items(Align::Start)
                .wrap_items(true);
            row.align_content = content;
            ui.keyed("row", |ui| {
                ui.row(row, |ui| {
                    for name in ["a", "b", "c", "d", "e"] {
                        ui.keyed(name, |ui| ui.container(tile(), |_| {}));
                    }
                })
            });
        },
    );
    context
}

#[test]
fn align_content_shares_the_room_left_across_the_lines() {
    use glassmere_core::Justify;
    // Three lines of 20 in 200: 140 is left over.
    let ys = |content| {
        let context = lines(content);
        let top = at(&context, "a").y;
        ["a", "c", "e"].map(|name| at(&context, name).y - top)
    };
    assert_eq!(ys(Some(Justify::Start)), [0.0, 20.0, 40.0]);
    assert_eq!(ys(Some(Justify::SpaceBetween)), [0.0, 90.0, 180.0]);
    let evenly = ys(Some(Justify::SpaceEvenly));
    assert_eq!(evenly[1] - evenly[0], evenly[2] - evenly[1]);
    assert!(evenly[1] - evenly[0] > 20.0);
    // Unset, the lines are as the engine puts them: stretched across the room.
    let stretched = ys(None);
    assert_eq!(stretched, ys(Some(Justify::Stretch)));
}

#[test]
fn a_stylesheet_takes_align_content() {
    let sheet =
        StyleSheet::parse("class cloud {\nwrap-items: true\nalign-content: space-between\n}")
            .unwrap();
    assert_eq!(
        sheet.class("cloud").unwrap().align_content,
        Some(glassmere_core::Justify::SpaceBetween)
    );
    assert!(StyleSheet::parse("class x {\nalign-content: nope\n}").is_err());
}
