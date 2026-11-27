//! Padding per side on a node that holds other nodes.
use glassmere_core::{Align, Context, Edges, Length, MonospaceMeasurer, Style, StyleSheet, Ui};
use glassmere_scene::{Rect, Size};

fn at(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

fn build(outer: Style) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 300.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("box", |ui| {
                ui.column(outer.clone(), |ui| {
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

fn outer() -> Style {
    Style::default()
        .width(Length::Px(200.0))
        .align_items(Align::Start)
}

#[test]
fn each_side_pads_by_its_own_amount() {
    let context = build(outer().padding_sides(Edges::new(5.0, 7.0, 11.0, 13.0)));
    let (frame, child) = (at(&context, &["box"]), at(&context, &["box", "child"]));
    assert_eq!((child.x - frame.x, child.y - frame.y), (13.0, 5.0));
    // The box wraps its child and the padding of both sides: 5 + 20 + 11 tall.
    assert_eq!(frame.height, 36.0);
}

#[test]
fn padding_sides_replace_the_one_padding_and_the_border_still_adds() {
    let context = build(
        outer()
            .padding(40.0)
            .padding_sides(Edges::new(0.0, 0.0, 0.0, 10.0))
            .border(2.0),
    );
    let (frame, child) = (at(&context, &["box"]), at(&context, &["box", "child"]));
    assert_eq!((child.x - frame.x, child.y - frame.y), (12.0, 2.0));
}

#[test]
fn a_node_that_draws_its_own_content_keeps_the_one_padding() {
    let mut context = Context::new();
    let mut size = |sides: Option<Edges>| {
        context.run(
            Size::new(400.0, 300.0),
            &mut MonospaceMeasurer,
            |ui: &mut Ui<'_>| {
                let mut style = Style::button();
                style.padding_sides = sides;
                ui.keyed("button", |ui| {
                    ui.button(glassmere_core::Button::new("Go").style(style.clone()))
                });
            },
        );
        at(&context, &["button"])
    };
    let plain = size(None);
    let sided = size(Some(Edges::new(0.0, 50.0, 0.0, 0.0)));
    assert_eq!((plain.width, plain.height), (sided.width, sided.height));
}

#[test]
fn a_stylesheet_has_padding_per_side() {
    let sheet = StyleSheet::parse(
        "class a {\npadding: 4 8\n}\nclass b {\npadding: 6\npadding-left: 20\n}\nclass c {\npadding: 9\n}",
    )
    .unwrap();
    assert_eq!(
        sheet.class("a").unwrap().padding_sides,
        Some(Edges::xy(8.0, 4.0))
    );
    let b = sheet.class("b").unwrap();
    assert_eq!(b.padding_sides, Some(Edges::new(6.0, 6.0, 6.0, 20.0)));
    let c = sheet.class("c").unwrap();
    assert_eq!((c.padding, c.padding_sides), (9.0, None));
}
