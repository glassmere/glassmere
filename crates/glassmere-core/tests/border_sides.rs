//! A border of a different width on each side of a node that holds others.
use glassmere_core::{Context, Edges, Length, MonospaceMeasurer, Style, StyleSheet, Ui};
use glassmere_scene::{Color, Primitive, Rect, Size};

fn run(style: Style) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(300.0, 200.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("box", |ui| {
                ui.column(style.clone(), |ui| {
                    ui.keyed("child", |ui| {
                        ui.container(
                            Style::default()
                                .width(Length::Px(50.0))
                                .height(Length::Px(20.0)),
                            |_| {},
                        )
                    });
                })
            });
        },
    );
    context
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

const RULE: Color = Color(200, 40, 40, 255);

fn quads(context: &Context) -> Vec<Rect> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Quad { rect, color, .. } if *color == RULE => Some(*rect),
            _ => None,
        })
        .collect()
}

fn base() -> Style {
    Style::default().width(Length::Px(120.0)).border_color(RULE)
}

#[test]
fn each_side_takes_room_by_its_own_width() {
    let context = run(base().border_sides(Edges::new(1.0, 2.0, 3.0, 4.0)));
    let (frame, child) = (
        bounds(&context, &["box"]),
        bounds(&context, &["box", "child"]),
    );
    assert_eq!((child.x - frame.x, child.y - frame.y), (4.0, 1.0));
    // 1 + 20 + 3 tall.
    assert_eq!(frame.height, 24.0);
}

#[test]
fn a_rule_on_one_side_draws_one_strip() {
    let context = run(base().border_sides(Edges::new(0.0, 0.0, 2.0, 0.0)));
    let frame = bounds(&context, &["box"]);
    assert_eq!(
        quads(&context),
        vec![Rect {
            x: frame.x,
            y: frame.y + frame.height - 2.0,
            width: frame.width,
            height: 2.0
        }]
    );
    assert_eq!(frame.height, 22.0);
}

#[test]
fn four_sides_make_four_strips_that_do_not_overlap() {
    let context = run(base().border_sides(Edges::new(2.0, 3.0, 4.0, 5.0)));
    let frame = bounds(&context, &["box"]);
    let strips = quads(&context);
    assert_eq!(strips.len(), 4);
    let area: f32 = strips.iter().map(|r| r.width * r.height).sum();
    let inner = (frame.width - 8.0) * (frame.height - 6.0);
    assert_eq!(area, frame.width * frame.height - inner);
}

#[test]
fn the_one_border_still_works_and_sides_take_its_place() {
    let context = run(base().border(2.0));
    let frame = bounds(&context, &["box"]);
    assert_eq!(frame.height, 24.0);
    // Four equal sides take the room and cover the ground the one border does.
    let area =
        |context: &Context| -> f32 { quads(context).iter().map(|r| r.width * r.height).sum() };
    let sided = run(base().border_sides(Edges::all(2.0)));
    assert_eq!(bounds(&sided, &["box"]), frame);
    // The one border's strips overlap in the four 2 by 2 corners; these do not.
    assert_eq!(area(&sided), area(&context) - 16.0);
    // A node that draws its own content ignores the sides.
    let mut context = Context::new();
    context.run(
        Size::new(300.0, 200.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("b", |ui| {
                ui.button(
                    glassmere_core::Button::new("Go")
                        .style(Style::button().border_sides(Edges::new(0.0, 30.0, 0.0, 0.0))),
                )
            });
        },
    );
    let plain = {
        let mut c = Context::new();
        c.run(
            Size::new(300.0, 200.0),
            &mut MonospaceMeasurer,
            |ui: &mut Ui<'_>| {
                ui.keyed("b", |ui| ui.button(glassmere_core::Button::new("Go")));
            },
        );
        bounds(&c, &["b"])
    };
    assert_eq!(bounds(&context, &["b"]).width, plain.width);
}

#[test]
fn a_stylesheet_has_a_border_per_side() {
    let sheet = StyleSheet::parse(
        "class rule {\nborder-bottom-width: 2\nborder-color: #c82828\n}\nclass frame {\nborder-width: 1\nborder-left-width: 4\n}",
    )
    .unwrap();
    assert_eq!(
        sheet.class("rule").unwrap().border_sides,
        Some(Edges::new(0.0, 0.0, 2.0, 0.0))
    );
    assert_eq!(
        sheet.class("frame").unwrap().border_sides,
        Some(Edges::new(1.0, 1.0, 1.0, 4.0))
    );
}
