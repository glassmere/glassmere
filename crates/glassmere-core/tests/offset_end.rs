//! A node pinned to the right and bottom of its parent.
use glassmere_core::{Context, Length, MonospaceMeasurer, Style, Ui};
use glassmere_scene::{Rect, Size};

fn at(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

fn stage(child: Style, parent: (f32, f32)) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 300.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("parent", |ui| {
                ui.container(
                    Style::default()
                        .width(Length::Px(parent.0))
                        .height(Length::Px(parent.1)),
                    |ui| {
                        ui.keyed("child", |ui| ui.container(child.clone(), |_| {}));
                    },
                )
            });
        },
    );
    context
}

fn badge() -> Style {
    Style::default()
        .width(Length::Px(30.0))
        .height(Length::Px(20.0))
}

#[test]
fn the_corner_follows_the_size_of_the_parent() {
    for (width, height) in [(200.0, 100.0), (260.0, 150.0)] {
        let context = stage(badge().offset_end(8.0, 6.0), (width, height));
        let (parent, child) = (
            at(&context, &["parent"]),
            at(&context, &["parent", "child"]),
        );
        assert_eq!(child.x + child.width, parent.x + parent.width - 8.0);
        assert_eq!(child.y + child.height, parent.y + parent.height - 6.0);
        assert_eq!((child.width, child.height), (30.0, 20.0));
    }
}

#[test]
fn it_leaves_the_flow() {
    let context = stage(badge().offset_end(0.0, 0.0), (200.0, 100.0));
    // A parent with nothing else in it, and a child in the corner: nothing
    // pushed the parent's own size.
    assert_eq!(at(&context, &["parent"]).height, 100.0);
}

#[test]
fn both_corners_stretch_a_node_with_no_size() {
    let context = stage(
        Style::default().offset(10.0, 5.0).offset_end(20.0, 15.0),
        (200.0, 100.0),
    );
    let (parent, child) = (
        at(&context, &["parent"]),
        at(&context, &["parent", "child"]),
    );
    assert_eq!(child.x - parent.x, 10.0);
    assert_eq!(child.y - parent.y, 5.0);
    assert_eq!(child.width, 200.0 - 10.0 - 20.0);
    assert_eq!(child.height, 100.0 - 5.0 - 15.0);
}

#[test]
fn offset_alone_is_what_it_was() {
    let context = stage(badge().offset(12.0, 7.0), (200.0, 100.0));
    let (parent, child) = (
        at(&context, &["parent"]),
        at(&context, &["parent", "child"]),
    );
    assert_eq!((child.x - parent.x, child.y - parent.y), (12.0, 7.0));
}
