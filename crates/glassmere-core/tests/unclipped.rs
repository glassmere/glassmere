//! `Style::unclipped` (`overflow: visible`): what is inside a box can hang
//! over its edge, in what is drawn and in what is hit.
use glassmere_core::{Context, Event, Length, MonospaceMeasurer, Style, StyleSheet, Ui};
use glassmere_scene::{Point, Primitive, Rect, Size};

const ROOT: Size = Size::new(400.0, 300.0);

fn frame(context: &mut Context, unclipped: bool) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.row(
                Style::default()
                    .gap(0.0)
                    .width(Length::Px(380.0))
                    .height(Length::Px(80.0)),
                |ui| {
                    ui.keyed("box", |ui| {
                        ui.row(
                            Style::default()
                                .unclipped(unclipped)
                                .width(Length::Px(60.0))
                                .height(Length::Px(40.0)),
                            |ui| {
                                ui.keyed("wide", |ui| {
                                    ui.container(
                                        Style::default()
                                            .width(Length::Px(200.0))
                                            .height(Length::Px(40.0)),
                                        |ui| {
                                            ui.keyed("press", |ui| ui.button("Press me"));
                                        },
                                    )
                                });
                            },
                        )
                    });
                },
            )
        });
    });
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

/// The clip a primitive is drawn under, from the pushes and pops before it.
fn clip_of_text(context: &Context, text: &str) -> Option<Rect> {
    let mut stack: Vec<Rect> = Vec::new();
    for primitive in &context.scene().primitives {
        match primitive {
            Primitive::PushClip(rect) => {
                stack.push(stack.last().map_or(*rect, |outer| outer.intersect(*rect)))
            }
            Primitive::PopClip => {
                stack.pop();
            }
            Primitive::Text { text: drawn, .. } | Primitive::StyledText { text: drawn, .. }
                if drawn == text =>
            {
                return stack.last().copied();
            }
            _ => {}
        }
    }
    None
}

#[test]
fn a_clipped_box_cuts_what_hangs_over_its_edge() {
    let mut context = Context::new();
    frame(&mut context, false);
    let outer = bounds(&context, &["page", "box"]);
    let clip = clip_of_text(&context, "Press me").expect("the label is drawn");
    assert!(
        clip.x + clip.width <= outer.x + outer.width + 0.01,
        "{clip:?} {outer:?}"
    );
    let beyond = Point {
        x: outer.x + outer.width + 20.0,
        y: outer.y + 20.0,
    };
    let press = context.find(&["page", "box", "wide", "press"]);
    assert_ne!(
        context.hit_test(beyond),
        press,
        "the cut part cannot be clicked"
    );
}

#[test]
fn an_unclipped_box_lets_it_show_and_be_hit() {
    let mut context = Context::new();
    frame(&mut context, true);
    let outer = bounds(&context, &["page", "box"]);
    let button = bounds(&context, &["page", "box", "wide", "press"]);
    assert!(
        button.x + button.width > outer.x + outer.width + 1.0,
        "it hangs over"
    );
    let clip = clip_of_text(&context, "Press me").expect("the label is drawn");
    assert!(
        clip.x + clip.width > outer.x + outer.width + 1.0,
        "{clip:?} {outer:?}"
    );
    let beyond = Point {
        x: outer.x + outer.width + 20.0,
        y: outer.y + 20.0,
    };
    let press = context.find(&["page", "box", "wide", "press"]);
    assert_eq!(context.hit_test(beyond), press);
    context.event(Event::PointerMoved(beyond));
    assert_eq!(context.hovered(), press);
}

#[test]
fn switching_it_back_restores_the_clip() {
    let mut context = Context::new();
    frame(&mut context, true);
    frame(&mut context, false);
    let outer = bounds(&context, &["page", "box"]);
    let clip = clip_of_text(&context, "Press me").expect("drawn");
    assert!(clip.x + clip.width <= outer.x + outer.width + 0.01);
}

#[test]
fn the_stylesheet_has_overflow() {
    let sheet =
        StyleSheet::parse("class a {\noverflow: visible\n}\nclass b {\noverflow: hidden\n}")
            .unwrap();
    assert!(sheet.class("a").unwrap().unclipped);
    assert!(!sheet.class("b").unwrap().unclipped);
    assert!(StyleSheet::parse("class c {\noverflow: scroll\n}").is_err());
}
