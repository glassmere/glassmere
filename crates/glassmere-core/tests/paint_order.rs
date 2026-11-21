//! A node's own primitives go around its children in the scene: what it draws
//! first, then the children, then what is drawn over them and the pops that
//! close what the node opened.

use glassmere_core::{
    Action, BackgroundImage, Calendar, Context, Date, Effect, Event, Length, MonospaceMeasurer,
    NodeSnapshot, Style, Tab, Ui,
};
use glassmere_scene::{Color, Image, Point, Primitive, Scene, Size};
use std::time::Duration;

const WINDOW: Size = Size::new(480.0, 360.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(WINDOW, &mut MonospaceMeasurer, view);
}

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

fn middle(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

/// What each primitive is: all that the order of a subtree is read from.
fn kinds(primitives: &[Primitive]) -> Vec<&'static str> {
    primitives
        .iter()
        .map(|primitive| match primitive {
            Primitive::PushClip(_) => "push",
            Primitive::PopClip => "pop",
            Primitive::PushLayer { .. } => "layer",
            Primitive::PopLayer => "unlayer",
            Primitive::Quad { .. } => "quad",
            Primitive::Text { .. } | Primitive::StyledText { .. } => "text",
            Primitive::Gradient { .. } => "gradient",
            Primitive::Path { .. } => "path",
            Primitive::Image { .. } => "image",
            _ => "other",
        })
        .collect()
}

/// What `count` labels draw: a clip, the text and the pop.
fn labels(count: usize) -> Vec<&'static str> {
    ["push", "text", "pop"].repeat(count)
}

/// Where the clip that opens at `start` is closed.
fn closed(primitives: &[Primitive], start: usize) -> usize {
    let mut open = 0;
    for (at, primitive) in primitives.iter().enumerate().skip(start) {
        match primitive {
            Primitive::PushClip(_) => open += 1,
            Primitive::PopClip => {
                open -= 1;
                if open == 0 {
                    return at;
                }
            }
            _ => {}
        }
    }
    panic!("the clip at {start} is never closed");
}

/// Where a node opens a clip on its bounds.
fn opened(context: &Context, path: &[&str]) -> usize {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    context
        .scene()
        .primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::PushClip(rect) if *rect == bounds))
        .expect("the node opens a clip on its bounds")
}

/// The clip a node opens on its bounds, and all that is drawn in it.
fn subtree<'a>(context: &'a Context, path: &[&str]) -> &'a [Primitive] {
    let primitives = &context.scene().primitives;
    let start = opened(context, path);
    &primitives[start..=closed(primitives, start)]
}

/// Whether the end of its strip leaves nothing of a tab in view.
fn past(tab: &NodeSnapshot) -> bool {
    tab.clip.width <= 0.0
}

/// Whether the end of its strip cuts a tab, leaving part of it in view.
fn cut(tab: &NodeSnapshot) -> bool {
    tab.clip.width > 0.0 && tab.clip != tab.bounds
}

/// The scenes are the same, which for a NaN or a negative zero takes the text
/// they print as well.
fn assert_same(scene: &Scene, wanted: &Scene, step: &str) {
    assert_eq!(scene, wanted, "{step}");
    assert_eq!(format!("{scene:?}"), format!("{wanted:?}"), "{step}");
}

#[test]
fn a_scroll_bar_is_drawn_over_the_rows_and_the_clip_it_is_in_closes_after_it() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("view", |ui| {
            ui.scroll(sized(200.0, 60.0), |ui| {
                for row in 0..4 {
                    ui.keyed(format!("row{row}"), |ui| ui.label(format!("row {row}")));
                }
            })
        });
        ui.keyed("after", |ui| ui.label("after"));
    });
    let primitives = &context.scene().primitives;
    let start = opened(&context, &["view"]);
    let end = closed(primitives, start);
    let view = &primitives[start..=end];
    // The clip of the view, its four rows, the thumb, and the pop of the clip.
    let mut wanted = vec!["push"];
    wanted.extend(labels(4));
    wanted.extend(["quad", "pop"]);
    assert_eq!(kinds(view), wanted);
    let thumb = context.theme().scrollbar;
    assert!(matches!(view[view.len() - 2], Primitive::Quad { color, .. } if color == thumb));
    // What follows the view in the scene is the label after it, and then the
    // pop of the root's clip.
    let mut after = labels(1);
    after.push("pop");
    assert_eq!(kinds(&primitives[end + 1..]), after);
}

#[test]
fn a_layer_opens_before_the_clip_of_its_node_and_closes_after_it() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("faded", |ui| {
            ui.container(
                Style::default()
                    .opacity(0.5)
                    .background(Color(10, 20, 30, 255)),
                |ui| {
                    ui.keyed("a", |ui| ui.label("one"));
                    ui.keyed("b", |ui| ui.label("two"));
                },
            )
        });
        ui.keyed("after", |ui| ui.label("after"));
    });
    let primitives = &context.scene().primitives;
    let open = primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::PushLayer { .. }))
        .unwrap();
    let close = primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::PopLayer))
        .unwrap();
    // The layer, the clip and fill of the node, its two labels, and the pops
    // in the order opposite to the pushes.
    let mut wanted = vec!["layer", "push", "quad"];
    wanted.extend(labels(2));
    wanted.extend(["pop", "unlayer"]);
    assert_eq!(kinds(&primitives[open..=close]), wanted);
}

#[test]
fn a_tab_cut_by_its_strip_is_drawn_in_a_clip_of_the_strip_and_the_shade_is_over_the_tabs() {
    let tabs: Vec<Tab> = (0..8)
        .map(|index| Tab::new(format!("Tab {index}")))
        .collect();
    let mut context = Context::new();
    context.run(Size::new(400.0, 360.0), &mut MonospaceMeasurer, |ui| {
        ui.keyed("tabs", |ui| {
            ui.tabs(0, &tabs, |_| (), |ui, _| ui.label("panel"));
        });
    });
    let strip = context.find(&["tabs", "$strip"]).unwrap();
    let children = context.node(strip).unwrap().children;
    let shown: Vec<_> = children[..tabs.len()]
        .iter()
        .map(|id| context.node(*id).unwrap())
        .collect();
    assert!(shown.iter().any(past), "a tab is past the end of the strip");
    assert!(
        shown.iter().any(cut),
        "a tab is cut by the end of the strip"
    );

    // A tab past the end draws nothing. One cut by it is drawn in a clip of the
    // strip, which opens first and closes last, and a clip of its own inside.
    // The selected tab has a fill, and the strip ends with the button that
    // scrolls on and then the shade over the tabs.
    let mut wanted = vec!["push"];
    for (index, tab) in shown.iter().enumerate() {
        wanted.extend(if past(tab) {
            vec![]
        } else if cut(tab) {
            vec!["push", "push", "text", "pop", "pop"]
        } else if index == 0 {
            vec!["push", "quad", "text", "pop"]
        } else {
            labels(1)
        });
    }
    wanted.extend(["push", "quad", "text", "pop", "gradient", "pop"]);
    assert_eq!(kinds(subtree(&context, &["tabs", "$strip"])), wanted);

    // A cut tab opens a clip on what is in view of it, then one on its bounds,
    // and closes both after its text.
    for tab in shown.iter().filter(|tab| cut(tab)) {
        let primitives = &context.scene().primitives;
        let at = primitives
            .iter()
            .position(
                |primitive| matches!(primitive, Primitive::PushClip(rect) if *rect == tab.clip),
            )
            .expect("the clip of the strip");
        assert!(matches!(primitives[at + 1], Primitive::PushClip(rect) if rect == tab.bounds));
        assert_eq!(closed(primitives, at), at + 4);
    }
}

#[test]
fn the_dot_under_today_and_the_ring_of_the_cursor_are_drawn_over_every_day() {
    let today = Date::new(2026, 9, 17).unwrap();
    let selected = Date::new(2026, 9, 21).unwrap();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("calendar", |ui| {
            ui.calendar(
                Calendar::new("Departure").today(today),
                Some(selected),
                |_| (),
            );
        });
    };
    let mut context = Context::new();
    frame(&mut context, view);
    let grid = context.find(&["calendar", "$grid"]).unwrap();
    assert!(context.action(grid, Action::Focus));
    frame(&mut context, view);

    let grid = subtree(&context, &["calendar", "$grid"]);
    let order = kinds(grid);
    // The clip of the grid opens it, and the dot, the ring and the pop of that
    // clip end it. The names of the seven weekdays and the 42 days are all
    // drawn before the dot.
    assert_eq!(order[0], "push");
    assert_eq!(order[order.len() - 3..], ["quad", "path", "pop"]);
    let texts = order.iter().filter(|kind| **kind == "text").count();
    assert_eq!(texts, 7 + 42);
    assert!(order.iter().rposition(|kind| *kind == "text").unwrap() < order.len() - 3);
    let theme = context.theme();
    assert!(matches!(
        grid[grid.len() - 3],
        Primitive::Quad { rect, color, .. } if color == theme.accent && rect.width == 4.0
    ));
    assert!(matches!(
        &grid[grid.len() - 2],
        Primitive::Path { fill: None, stroke: Some(stroke), .. } if stroke.color == theme.focus
    ));
}

#[test]
fn a_node_that_draws_more_than_eight_primitives_before_its_children_draws_them_all_first() {
    let pieces = Image::from_rgba8(30, 30, vec![255; 30 * 30 * 4]).unwrap();
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("framed", |ui| {
            ui.column(
                sized(100.0, 100.0)
                    .background_image(BackgroundImage::new(&pieces).slice(10.0, 10.0, 10.0, 10.0)),
                |ui| ui.label("inside"),
            )
        });
        ui.keyed("after", |ui| ui.label("after"));
    });
    // The clip, the nine pieces of the picture, the label, and the pop.
    let mut wanted = vec!["push"];
    wanted.extend(["image"; 9]);
    wanted.extend(labels(1));
    wanted.push("pop");
    assert_eq!(kinds(subtree(&context, &["framed"])), wanted);
}

#[test]
fn a_range_slider_draws_its_label_and_its_track_before_the_thumbs_that_are_its_children() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("range", |ui| {
            ui.range_slider("Span", 20.0..=60.0, 0.0..=100.0, 1.0, |_| ());
        });
    });
    let range = subtree(&context, &["range"]);
    // Its clip, its label, the track and the part of it between the thumbs,
    // each thumb in a clip of its own, and the pop.
    assert_eq!(
        kinds(range),
        [
            "push", "text", "quad", "quad", "push", "quad", "pop", "push", "quad", "pop", "pop"
        ]
    );
    let theme = context.theme();
    let quads: Vec<Color> = range
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Quad { color, .. } => Some(*color),
            _ => None,
        })
        .collect();
    assert_eq!(quads, [theme.track, theme.accent, theme.thumb, theme.thumb]);
}

/// A label, and under it a scroll view of three rows that slides out when it is
/// removed.
fn panel(ui: &mut Ui<'_>, shown: bool) {
    ui.label("always");
    if shown {
        ui.keyed("panel", |ui| {
            ui.scroll(
                sized(120.0, 40.0).exit(Effect::slide(0.0, 20.0, 200)),
                |ui| {
                    for row in 0..3 {
                        ui.keyed(format!("row{row}"), |ui| ui.label(format!("row {row}")));
                    }
                },
            )
        });
    }
}

#[test]
fn a_node_that_slides_out_leaves_a_ghost_of_what_it_drew_in_the_order_it_drew_it() {
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    frame(&mut context, |ui| panel(ui, true));
    let primitives = &context.scene().primitives;
    let start = opened(&context, &["panel"]);
    let drawn = primitives[start..=closed(primitives, start)].to_vec();
    // Its clip, the three rows, the thumb of its scroll bar and the pop.
    let mut wanted = vec!["push"];
    wanted.extend(labels(3));
    wanted.extend(["quad", "pop"]);
    assert_eq!(kinds(&drawn), wanted);

    context.set_now(Duration::from_millis(100));
    frame(&mut context, |ui| panel(ui, false));
    assert!(context.animating());
    // The ghost is the last thing in the scene: in a layer, inside a clip, with
    // all that the node drew in it.
    let primitives = &context.scene().primitives;
    let ghost = &primitives[primitives.len() - drawn.len() - 4..];
    assert!(matches!(
        ghost,
        [
            Primitive::PushClip(_),
            Primitive::PushLayer { .. },
            ..,
            Primitive::PopLayer,
            Primitive::PopClip
        ]
    ));
    assert_eq!(ghost[2..ghost.len() - 2], drawn[..]);
}

/// A row of buttons, a scroll view of rows with a thumb over them, and a faded
/// box.
fn mixed(ui: &mut Ui<'_>) {
    ui.keyed("buttons", |ui| {
        ui.row(Style::default(), |ui| {
            for index in 0..3 {
                ui.keyed(format!("b{index}"), |ui| {
                    ui.button(format!("Button {index}"));
                });
            }
        })
    });
    ui.keyed("view", |ui| {
        ui.scroll(sized(200.0, 60.0), |ui| {
            for row in 0..4 {
                ui.keyed(format!("row{row}"), |ui| {
                    ui.button(format!("Row {row}"));
                });
            }
        })
    });
    ui.keyed("faded", |ui| {
        ui.container(
            Style::default()
                .opacity(0.5)
                .background(Color(10, 20, 30, 255)),
            |ui| {
                ui.keyed("inside", |ui| {
                    ui.button("Faded");
                });
            },
        )
    });
}

#[test]
fn a_repaint_that_reuses_chunks_is_the_scene_of_a_context_that_paints_every_node() {
    let mut context = Context::new();
    frame(&mut context, mixed);
    let mut last = context.scene().clone();
    for path in [
        ["buttons", "b1"],
        ["view", "row2"],
        ["faded", "inside"],
        ["view", "row0"],
        ["buttons", "b0"],
    ] {
        let at = middle(&context, &path);
        context.event(Event::PointerMoved(at));
        frame(&mut context, mixed);
        assert_ne!(*context.scene(), last, "{path:?} changed nothing");
        last = context.scene().clone();
        // A context with the pointer there from its first frame draws every
        // node once, the hover included.
        let mut fresh = Context::new();
        fresh.event(Event::PointerMoved(at));
        frame(&mut fresh, mixed);
        assert_eq!(fresh.hovered(), context.hovered(), "{path:?}");
        assert_same(context.scene(), fresh.scene(), &format!("{path:?}"));
    }
}
