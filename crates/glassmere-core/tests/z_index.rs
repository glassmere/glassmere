//! Siblings that overlap are stacked by `z_index`, then by declaration order.
use glassmere_core::{
    Button, Context, Event, Length, MonospaceMeasurer, NodeId, Style, StyleSheet, Ui,
};
use glassmere_scene::{Point, Size};

fn piece(z: i32, x: f32) -> Style {
    Style::button()
        .width(Length::Px(80.0))
        .height(Length::Px(40.0))
        .offset(x, 10.0)
        .z_index(z)
}

fn stage(zs: [i32; 3]) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(300.0, 100.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_, ()>| {
            ui.keyed("stage", |ui| {
                ui.container(
                    Style::default()
                        .width(Length::Px(200.0))
                        .height(Length::Px(80.0)),
                    |ui| {
                        for (name, (z, x)) in ["a", "b", "c"]
                            .into_iter()
                            .zip(zs.into_iter().zip([0.0, 30.0, 60.0]))
                        {
                            ui.keyed(name, |ui| ui.button(Button::new(name).style(piece(z, x))));
                        }
                    },
                )
            });
        },
    );
    context
}

fn over(context: &Context, at: Point) -> Option<NodeId> {
    let hit = context.hit_test(at);
    assert!(hit.is_some(), "something is under the pointer");
    hit
}

#[test]
fn later_siblings_are_on_top_unless_z_index_says_otherwise() {
    // All three cover x 60..80: the last declared is hit.
    let context = stage([0, 0, 0]);
    let at = Point { x: 70.0, y: 30.0 };
    assert_eq!(over(&context, at), context.find(&["stage", "c"]));
    // The first, lifted, is hit instead; and one pushed down is not.
    let context = stage([5, 0, 0]);
    assert_eq!(over(&context, at), context.find(&["stage", "a"]));
    let context = stage([0, 0, -1]);
    assert_eq!(over(&context, at), context.find(&["stage", "b"]));
    // Equal values keep the declaration order.
    let context = stage([2, 2, 2]);
    assert_eq!(over(&context, at), context.find(&["stage", "c"]));
}

#[test]
fn a_lifted_sibling_is_painted_last_and_a_click_reaches_it() {
    let mut context = stage([5, 0, 0]);
    let a = context.find(&["stage", "a"]).unwrap();
    let c = context.find(&["stage", "c"]).unwrap();
    let scene = context.scene();
    let order = |id: NodeId| {
        let bounds = context.node(id).unwrap().bounds;
        scene
            .primitives
            .iter()
            .position(|p| matches!(p, glassmere_scene::Primitive::Quad { rect, .. } if rect.x == bounds.x && rect.width == bounds.width))
            .unwrap()
    };
    assert!(order(a) > order(c), "a is painted after c");
    context.event(Event::PointerMoved(Point { x: 70.0, y: 30.0 }));
    assert_eq!(context.hovered(), Some(a));
}

#[test]
fn a_change_of_z_index_restacks_without_moving_anything() {
    let mut context = Context::new();
    let declare = |context: &mut Context, z: i32| {
        context.run(
            Size::new(300.0, 100.0),
            &mut MonospaceMeasurer,
            |ui: &mut Ui<'_, ()>| {
                ui.keyed("stage", |ui| {
                    ui.container(
                        Style::default()
                            .width(Length::Px(200.0))
                            .height(Length::Px(80.0)),
                        |ui| {
                            ui.keyed("a", |ui| ui.button(Button::new("a").style(piece(z, 0.0))));
                            ui.keyed("b", |ui| ui.button(Button::new("b").style(piece(0, 40.0))));
                        },
                    )
                })
            },
        );
    };
    declare(&mut context, 0);
    let at = Point { x: 60.0, y: 30.0 };
    assert_eq!(over(&context, at), context.find(&["stage", "b"]));
    let painted_first = |context: &Context| {
        let (a, b) = (
            context
                .node(context.find(&["stage", "a"]).unwrap())
                .unwrap()
                .bounds,
            context
                .node(context.find(&["stage", "b"]).unwrap())
                .unwrap()
                .bounds,
        );
        let at = |x: f32| {
            context.scene().primitives.iter().position(
                |p| matches!(p, glassmere_scene::Primitive::Quad { rect, .. } if rect.x == x),
            )
        };
        (
            at(a.x).unwrap() < at(b.x).unwrap(),
            at(b.x).unwrap() < at(a.x).unwrap(),
        )
    };
    assert_eq!(painted_first(&context), (true, false));
    declare(&mut context, 3);
    assert_eq!(over(&context, at), context.find(&["stage", "a"]));
    assert_eq!(
        painted_first(&context),
        (false, true),
        "a is painted over b"
    );
    declare(&mut context, 0);
    assert_eq!(over(&context, at), context.find(&["stage", "b"]));
    assert_eq!(painted_first(&context), (true, false));
}

#[test]
fn a_stylesheet_takes_z_index() {
    let sheet =
        StyleSheet::parse("class lifted {\nz-index: 7\n}\nclass under {\nz-index: -2\n}").unwrap();
    assert_eq!(sheet.class("lifted").unwrap().z_index, 7);
    assert_eq!(sheet.class("under").unwrap().z_index, -2);
    assert!(StyleSheet::parse("class bad {\nz-index: high\n}").is_err());
    let _ = Length::Auto;
}
