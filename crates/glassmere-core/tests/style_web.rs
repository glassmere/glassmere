//! `Style::web()`: the flex defaults of CSS, for someone coming from the web.
use glassmere_core::{Context, Length, MonospaceMeasurer, Style, Ui};
use glassmere_scene::{Rect, Size};

fn at(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

/// A row 200 wide of three children 100 wide, in the window's top left.
fn row(parent: Style, child: impl Fn() -> Style) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 300.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("row", |ui| {
                ui.row(
                    parent
                        .clone()
                        .width(Length::Px(200.0))
                        .height(Length::Px(50.0)),
                    |ui| {
                        for name in ["a", "b", "c"] {
                            ui.keyed(name, |ui| {
                                ui.container(child().width(Length::Px(100.0)), |_| {})
                            });
                        }
                    },
                )
            });
        },
    );
    context
}

#[test]
fn the_defaults_are_the_ones_css_has() {
    let web = Style::web();
    assert_eq!((web.gap, web.shrink), (0.0, 1.0));
    let default = Style::default();
    assert_eq!((default.gap, default.shrink), (12.0, 0.0));
}

#[test]
fn children_are_side_by_side_with_no_gap_and_shrink_to_fit_as_on_the_web() {
    // The web puts three 100 px items in a 200 px flex row 66.67 px each,
    // touching.
    let context = row(Style::web(), Style::web);
    let (a, b, c) = (
        at(&context, &["row", "a"]),
        at(&context, &["row", "b"]),
        at(&context, &["row", "c"]),
    );
    for width in [a.width, b.width, c.width] {
        assert!((width - 200.0 / 3.0).abs() < 0.01, "{width}");
    }
    assert!((b.x - (a.x + a.width)).abs() < 0.01, "no gap");
    assert!((c.x - (b.x + b.width)).abs() < 0.01, "no gap");
    // Stretched across the row's height, which no child set.
    assert_eq!((a.height, b.height, c.height), (50.0, 50.0, 50.0));
}

#[test]
fn the_default_is_what_it_was() {
    let context = row(Style::default(), Style::default);
    let (a, b) = (at(&context, &["row", "a"]), at(&context, &["row", "b"]));
    assert_eq!(a.width, 100.0, "the children overflow rather than shrink");
    assert_eq!(b.x - (a.x + a.width), 12.0, "the default gap");
    assert_eq!(a.height, 0.0, "and keep their own size across");
}
