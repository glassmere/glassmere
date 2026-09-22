use rust_gui_core::{Context, Gradient, GradientStop, Length, MonospaceMeasurer, Shadow, Style};
use rust_gui_core::{StyleSheet, StyleSheetError};
use rust_gui_scene::{Color, Point, Primitive, Size};

const ROOT: Size = Size::new(200.0, 200.0);

fn scene(style: Style) -> Vec<Primitive> {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.container("card", style.clone(), |_| {});
    });
    context.scene().primitives.clone()
}

/// Where the first primitive of a kind sits, from `after` onward.
fn at(primitives: &[Primitive], from: usize, of: impl Fn(&Primitive) -> bool) -> usize {
    primitives[from..]
        .iter()
        .position(of)
        .map(|found| found + from)
        .expect("the primitive")
}

#[test]
fn a_shadow_is_behind_the_node_and_an_inset_one_is_inside_it() {
    let lift = Shadow::lift(Color(0, 0, 0, 128), 8.0, 4.0).spread(2.0);
    let primitives = scene(
        Style::default()
            .width(Length::Px(80.0))
            .height(Length::Px(40.0))
            .radius(6.0)
            .background(Color::WHITE)
            .shadow(lift),
    );
    let shadow = at(&primitives, 0, |primitive| {
        matches!(primitive, Primitive::Shadow { .. })
    });
    let Primitive::Shadow {
        rect,
        radius,
        color,
        blur,
        spread,
        offset,
        inset,
    } = &primitives[shadow]
    else {
        unreachable!()
    };
    assert_eq!((*radius, *blur, *spread, *inset), (6.0, 8.0, 2.0, false));
    assert_eq!(
        (*color, *offset),
        (Color(0, 0, 0, 128), Point { x: 0.0, y: 4.0 })
    );
    assert_eq!((rect.width, rect.height), (80.0, 40.0));
    // Behind the node: the node's own fill comes after it.
    let fill = at(
        &primitives,
        shadow,
        |primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == Color::WHITE),
    );
    assert!(shadow < fill, "the shadow is painted first");

    // An inset one comes after the fill instead.
    let primitives = scene(
        Style::default()
            .width(Length::Px(80.0))
            .height(Length::Px(40.0))
            .background(Color::WHITE)
            .shadow(Shadow::new(Color(0, 0, 0, 90), 4.0).inset()),
    );
    let fill = at(
        &primitives,
        0,
        |primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == Color::WHITE),
    );
    let shadow = at(&primitives, 0, |primitive| {
        matches!(primitive, Primitive::Shadow { inset: true, .. })
    });
    assert!(shadow > fill, "the inside edge is drawn over the fill");
}

#[test]
fn a_gradient_fills_the_node_over_its_flat_colour() {
    let primitives = scene(
        Style::default()
            .width(Length::Px(80.0))
            .height(Length::Px(40.0))
            .background(Color::WHITE)
            .radius(4.0)
            .gradient(Gradient::down(Color::rgb(255, 0, 0), Color::rgb(0, 0, 255))),
    );
    let fill = at(
        &primitives,
        0,
        |primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == Color::WHITE),
    );
    let gradient = at(&primitives, 0, |primitive| {
        matches!(primitive, Primitive::Gradient { .. })
    });
    assert!(gradient > fill);
    let Primitive::Gradient {
        from,
        to,
        stops,
        radial,
        radius,
        ..
    } = &primitives[gradient]
    else {
        unreachable!()
    };
    assert_eq!(
        (*from, *to),
        (Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 1.0 })
    );
    assert_eq!((stops.len(), *radial, *radius), (2, false, 4.0));
    assert_eq!(stops[0].color, Color::rgb(255, 0, 0));

    // A radial one runs from the middle outward.
    let primitives =
        scene(Style::default().gradient(Gradient::radial(Color::WHITE, Color::rgb(0, 0, 0))));
    let gradient = at(&primitives, 0, |primitive| {
        matches!(primitive, Primitive::Gradient { .. })
    });
    let Primitive::Gradient { from, radial, .. } = &primitives[gradient] else {
        unreachable!()
    };
    assert!(*radial && *from == Point { x: 0.5, y: 0.5 });

    // One stop is not a ramp, so nothing is drawn.
    let primitives = scene(Style::default().gradient(Gradient {
        from: (0.0, 0.0),
        to: (1.0, 1.0),
        stops: vec![GradientStop {
            at: 0.0,
            color: Color::WHITE,
        }],
        radial: false,
    }));
    assert!(
        !primitives
            .iter()
            .any(|primitive| matches!(primitive, Primitive::Gradient { .. }))
    );
}

#[test]
fn a_stylesheet_carries_shadows_and_gradients() {
    let sheet = StyleSheet::parse(
        r#"
        class card { shadow: 0 4 12 2 #00000080; gradient: 0 0 1 1 #ff0000 #00ff00 #0000ff }
        class inside {
            shadow: 1 2 3 #112233ff inset
            gradient: radial 0.5 0.5 1 0.5 #ffffff #000000
        }
        class plain { shadow: none; gradient: none }
        "#,
    )
    .unwrap();
    let card = sheet.class("card").unwrap();
    let shadow = card.shadow.as_ref().expect("a shadow");
    assert_eq!(
        (shadow.offset, shadow.blur, shadow.spread, shadow.inset),
        ((0.0, 4.0), 12.0, 2.0, false)
    );
    assert_eq!(shadow.color, Color(0, 0, 0, 128));
    let gradient = card.gradient.as_ref().expect("a gradient");
    assert_eq!((gradient.from, gradient.to), ((0.0, 0.0), (1.0, 1.0)));
    // Three colours spread evenly along the ramp.
    let stops: Vec<f32> = gradient.stops.iter().map(|stop| stop.at).collect();
    assert_eq!(stops, [0.0, 0.5, 1.0]);
    assert!(!gradient.radial);

    let inside = sheet.class("inside").unwrap();
    assert!(inside.shadow.as_ref().unwrap().inset);
    assert!(inside.gradient.as_ref().unwrap().radial);
    let plain = sheet.class("plain").unwrap();
    assert!(plain.shadow.is_none() && plain.gradient.is_none());

    // And the errors say what the shape is.
    let broken = |text: &str| -> StyleSheetError { StyleSheet::parse(text).unwrap_err() };
    assert!(
        broken("class a { shadow: 1 2 }")
            .message
            .contains("x y blur"),
        "{:?}",
        broken("class a { shadow: 1 2 }")
    );
    let missing = broken("class a { gradient: 0 0 1 1 #ffffff }");
    assert!(missing.message.contains("colour colour"), "{missing:?}");
}
