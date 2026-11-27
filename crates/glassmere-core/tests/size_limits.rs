//! An explicit size is held to its own minimum and maximum, and so is the
//! parent that is sized from it.
use glassmere_core::{Context, Length, MonospaceMeasurer, Style, Ui};
use glassmere_scene::Size;

/// The height of a column and of its one child, for a child of this style.
fn heights(child: Style) -> (f32, f32) {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 600.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("col", |ui| {
                ui.column(Style::default().width(Length::Px(100.0)).gap(0.0), |ui| {
                    ui.keyed("child", |ui| ui.container(child.clone(), |_| {}));
                })
            });
        },
    );
    let height = |path: &[&str]| {
        context
            .node(context.find(path).unwrap())
            .unwrap()
            .bounds
            .height
    };
    (height(&["col"]), height(&["col", "child"]))
}

fn base() -> Style {
    Style::default().width(Length::Px(50.0))
}

#[test]
fn a_parent_is_as_tall_as_its_child_ends_up_not_as_the_size_it_asked_for() {
    // `height: 100px; max-height: 25px` is 25, and so is its parent.
    assert_eq!(
        heights(
            base()
                .height(Length::Px(100.0))
                .max_height(Length::Px(25.0))
        ),
        (25.0, 25.0)
    );
    // `height: 0; min-height: 30px` is 30.
    assert_eq!(
        heights(base().height(Length::Px(0.0)).min_height(Length::Px(30.0))),
        (30.0, 30.0)
    );
}

#[test]
fn a_minimum_above_the_maximum_wins() {
    assert_eq!(
        heights(
            base()
                .height(Length::Px(50.0))
                .min_height(Length::Px(80.0))
                .max_height(Length::Px(20.0))
        ),
        (80.0, 80.0)
    );
}

#[test]
fn a_size_inside_its_limits_is_what_it_was() {
    assert_eq!(
        heights(
            base()
                .height(Length::Px(60.0))
                .min_height(Length::Px(10.0))
                .max_height(Length::Px(100.0))
        ),
        (60.0, 60.0)
    );
    // And a size that is not in pixels is left to the layout engine: content
    // with a maximum height is as tall as the maximum.
    assert_eq!(heights(base().max_height(Length::Px(40.0))).0, 0.0);
}

#[test]
fn the_same_holds_along_a_row() {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 600.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("row", |ui| {
                ui.row(Style::default().gap(0.0), |ui| {
                    ui.keyed("child", |ui| {
                        ui.container(
                            Style::default()
                                .width(Length::Px(300.0))
                                .max_width(Length::Px(120.0))
                                .height(Length::Px(10.0)),
                            |_| {},
                        )
                    });
                })
            });
        },
    );
    let width = |path: &[&str]| {
        context
            .node(context.find(path).unwrap())
            .unwrap()
            .bounds
            .width
    };
    assert_eq!(width(&["row", "child"]), 120.0);
    assert_eq!(width(&["row"]), 120.0);
}
