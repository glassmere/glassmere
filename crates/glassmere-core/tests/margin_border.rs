//! Margin and border: room outside a box, and a stroke along its edge that
//! takes room inside it.
use glassmere_core::{
    Action, Align, Button, Context, Edges, Length, MonospaceMeasurer, Style, StyleSheet, Ui,
};
use glassmere_scene::{Color, Point, Primitive, Rect, Size};

const WINDOW: Size = Size::new(400.0, 400.0);
const BORDER: Color = Color(200, 40, 40, 255);

fn column() -> Style {
    Style::default().gap(0.0).align_items(Align::Start)
}

fn panel(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

fn run(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(WINDOW, &mut MonospaceMeasurer, view);
}

#[test]
fn a_margin_keeps_neighbours_away_and_is_not_part_of_the_box() {
    let mut context = Context::new();
    run(&mut context, |ui| {
        ui.keyed("page", |ui| {
            ui.column(column(), |ui| {
                ui.keyed("a", |ui| ui.container(panel(100.0, 30.0), |_| {}));
                ui.keyed("b", |ui| {
                    ui.container(panel(100.0, 30.0).margin(12.0), |_| {})
                });
                ui.keyed("c", |ui| ui.container(panel(100.0, 30.0), |_| {}));
            })
        });
    });
    let (a, b, c) = (
        bounds(&context, &["page", "a"]),
        bounds(&context, &["page", "b"]),
        bounds(&context, &["page", "c"]),
    );
    assert_eq!(
        (b.width, b.height),
        (100.0, 30.0),
        "the box is its own size"
    );
    assert_eq!(b.y, a.y + a.height + 12.0, "12 px below the one above");
    assert_eq!(b.x, a.x + 12.0, "and in from the edge");
    assert_eq!(
        c.y,
        b.y + b.height + 12.0,
        "and the one below is kept 12 px away"
    );
}

#[test]
fn a_margin_can_differ_per_side() {
    let mut context = Context::new();
    run(&mut context, |ui| {
        ui.keyed("page", |ui| {
            ui.column(column(), |ui| {
                ui.keyed("a", |ui| ui.container(panel(50.0, 20.0), |_| {}));
                ui.keyed("b", |ui| {
                    ui.container(
                        panel(50.0, 20.0).margin_edges(Edges::new(5.0, 0.0, 0.0, 30.0)),
                        |_| {},
                    )
                });
            })
        });
    });
    let (a, b) = (
        bounds(&context, &["page", "a"]),
        bounds(&context, &["page", "b"]),
    );
    assert_eq!((b.x - a.x, b.y - (a.y + a.height)), (30.0, 5.0));
}

#[test]
fn a_press_in_the_margin_reaches_nothing() {
    use glassmere_core::Event;
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(column(), |ui| {
                ui.keyed("button", |ui| {
                    ui.button(Button::new("Go").style(Style::button().margin(20.0)))
                });
            })
        });
    };
    run(&mut context, view);
    let at = bounds(&context, &["page", "button"]);
    // 10 px outside the box, inside its margin.
    context.event(Event::PointerMoved(Point {
        x: at.x - 10.0,
        y: at.y + at.height / 2.0,
    }));
    run(&mut context, view);
    assert_ne!(
        context.hovered(),
        context.find(&["page", "button"]),
        "the margin is not the button"
    );
}

#[test]
fn a_border_takes_room_inside_the_box_and_the_content_starts_inside_it() {
    let mut context = Context::new();
    run(&mut context, |ui| {
        ui.keyed("page", |ui| {
            ui.column(column(), |ui| {
                ui.keyed("box", |ui| {
                    ui.container(
                        Style::default()
                            .gap(0.0)
                            .padding(6.0)
                            .border(4.0)
                            .align_items(Align::Start),
                        |ui| {
                            ui.keyed("child", |ui| ui.container(panel(50.0, 20.0), |_| {}));
                        },
                    )
                });
            })
        });
    });
    let (outer, child) = (
        bounds(&context, &["page", "box"]),
        bounds(&context, &["page", "box", "child"]),
    );
    assert_eq!(child.x - outer.x, 10.0, "the padding and the border");
    assert_eq!(outer.width, 50.0 + 2.0 * 10.0, "the box grew by both");
    assert_eq!(outer.height, 20.0 + 2.0 * 10.0);
}

#[test]
fn text_in_a_bordered_button_starts_inside_the_border() {
    let measure = |border: f32| {
        let mut context = Context::new();
        run(&mut context, |ui| {
            ui.keyed("button", |ui| {
                ui.button(Button::new("Go").style(Style::button().border(border)))
            });
        });
        let at = bounds(&context, &["button"]);
        let text = context
            .scene()
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                Primitive::Text { rect, text, .. } if text == "Go" => Some(rect.x),
                _ => None,
            })
            .expect("the label");
        (text - at.x, at.width)
    };
    let (plain_x, plain_width) = measure(0.0);
    let (bordered_x, bordered_width) = measure(3.0);
    assert_eq!(
        bordered_x - plain_x,
        3.0,
        "the text is pushed in by the border"
    );
    assert_eq!(
        bordered_width - plain_width,
        6.0,
        "and the button is wider by both"
    );
}

fn strokes(context: &Context, color: Color) -> Vec<Primitive> {
    context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| match primitive {
            Primitive::Quad { color: c, .. } => *c == color,
            Primitive::Path {
                stroke: Some(stroke),
                ..
            } => stroke.color == color,
            _ => false,
        })
        .cloned()
        .collect()
}

#[test]
fn a_square_border_is_four_bars_and_a_rounded_one_a_stroked_path() {
    let draw = |radius: f32| {
        let mut context = Context::new();
        run(&mut context, |ui| {
            ui.keyed("box", |ui| {
                ui.container(
                    panel(80.0, 40.0)
                        .border(2.0)
                        .border_color(BORDER)
                        .radius(radius),
                    |_| {},
                )
            });
        });
        strokes(&context, BORDER)
    };
    let square = draw(0.0);
    assert_eq!(square.len(), 4, "{square:?}");
    assert!(
        square
            .iter()
            .all(|bar| matches!(bar, Primitive::Quad { .. }))
    );
    let round = draw(8.0);
    let [Primitive::Path { stroke, fill, .. }] = round.as_slice() else {
        panic!("one stroked path");
    };
    assert_eq!((stroke.unwrap().width, *fill), (2.0, None));
}

#[test]
fn a_border_without_a_colour_takes_the_themes() {
    let mut context = Context::new();
    run(&mut context, |ui| {
        ui.keyed("box", |ui| {
            ui.container(panel(80.0, 40.0).border(2.0), |_| {})
        });
    });
    let track = context.theme().track;
    assert!(!strokes(&context, track).is_empty());
}

#[test]
fn the_focus_ring_sits_inside_the_border() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("button", |ui| {
            ui.button(Button::new("Go").style(Style::button().border(4.0).border_color(BORDER)))
        });
    };
    run(&mut context, view);
    let button = context.find(&["button"]).unwrap();
    context.action(button, Action::Focus);
    run(&mut context, view);
    let at = bounds(&context, &["button"]);
    let focus = context.theme().focus;
    let ring = strokes(&context, focus);
    assert!(!ring.is_empty());
    for primitive in ring {
        let outline = match primitive {
            Primitive::Quad { rect, .. } => rect,
            Primitive::Path { path, .. } => path.bounds(),
            _ => unreachable!(),
        };
        assert!(
            outline.x >= at.x + 4.0 - 0.01 && outline.y >= at.y + 4.0 - 0.01,
            "{outline:?} is outside the 4 px border of {at:?}"
        );
    }
}

#[test]
fn a_stylesheet_sets_margin_and_border() {
    let sheet = StyleSheet::parse(
        "class tile {\n\
         margin: 4 8\n\
         border: 2 #c82828\n\
         }\n\
         class lopsided {\n\
         margin: 1 2 3 4\n\
         margin-left: 9\n\
         }\n\
         class thin {\n\
         border-width: 1\n\
         border-color: #00ff00\n\
         }",
    )
    .expect("a sheet");
    let tile = sheet.class("tile").unwrap();
    assert_eq!(tile.margin, Edges::new(4.0, 8.0, 4.0, 8.0));
    assert_eq!(
        (tile.border, tile.border_color),
        (2.0, Some(Color(200, 40, 40, 255)))
    );
    assert_eq!(
        sheet.class("lopsided").unwrap().margin,
        Edges::new(1.0, 2.0, 3.0, 9.0)
    );
    let thin = sheet.class("thin").unwrap();
    assert_eq!(
        (thin.border, thin.border_color),
        (1.0, Some(Color(0, 255, 0, 255)))
    );
    assert!(StyleSheet::parse("class bad {\nmargin: 1 2 3 4 5\n}").is_err());
    assert!(StyleSheet::parse("class bad {\nborder: 2 red green\n}").is_err());
}
