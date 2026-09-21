use rust_gui_core::{
    Animation, Context, DialogOptions, Easing, Effect, Event, Keyframe, Length, MonospaceMeasurer,
    Paint, Repeat, Style, StyleSheet, Transform, Transition, Ui,
};
use rust_gui_scene::{Affine, Color, Point, Primitive, Rect, Size};
use std::time::Duration;

const ROOT: Size = Size::new(500.0, 400.0);
const RED: Color = Color(200, 0, 0, 255);
const BLUE: Color = Color(0, 0, 200, 255);

fn frame_at(context: &mut Context, milliseconds: u64, view: impl FnOnce(&mut Ui<'_>)) {
    context.set_now(Duration::from_millis(milliseconds));
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

/// The layer whose bounds equal `rect`, as (opacity, transform).
fn layer(context: &Context, rect: Rect) -> Option<(f32, Affine)> {
    context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::PushLayer {
                bounds,
                opacity,
                transform,
            } if *bounds == rect => Some((*opacity, *transform)),
            _ => None,
        })
}

fn quad_color(context: &Context, rect: Rect) -> Option<Color> {
    context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::Quad { rect: r, color, .. } if *r == rect => Some(*color),
            _ => None,
        })
}

fn balanced(context: &Context) {
    let mut depth = 0i32;
    for primitive in &context.scene().primitives {
        match primitive {
            Primitive::PushLayer { .. } => depth += 1,
            Primitive::PopLayer => depth -= 1,
            _ => {}
        }
        assert!(depth >= 0);
    }
    assert_eq!(depth, 0);
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn easing_curves_hit_their_endpoints_and_bend_the_right_way() {
    for easing in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::CubicBezier(0.2, 0.0, 0.0, 1.0),
    ] {
        assert!(close(easing.apply(0.0), 0.0), "{easing:?}");
        assert!(close(easing.apply(1.0), 1.0), "{easing:?}");
    }
    assert!(close(Easing::Linear.apply(0.25), 0.25));
    assert!(Easing::EaseIn.apply(0.5) < 0.5);
    assert!(Easing::EaseOut.apply(0.5) > 0.5);
    assert!(close(Easing::EaseInOut.apply(0.5), 0.5));
    // Overshooting control points are allowed on the value axis.
    assert!(Easing::CubicBezier(0.3, 2.0, 0.7, 2.0).apply(0.5) > 1.0);
}

#[test]
fn transitions_interpolate_state_changes_retarget_from_the_current_value_and_settle() {
    let mut context = Context::new();
    let view = |color: Color| {
        move |ui: &mut Ui<'_>| {
            ui.label_styled(
                "box",
                "x",
                Style::default()
                    .background(color)
                    .transition(Transition::new(100).easing(Easing::Linear)),
            );
        }
    };
    frame_at(&mut context, 0, view(RED));
    let rect = bounds(&context, &["box"]);
    // No transition on first appearance.
    assert_eq!(quad_color(&context, rect), Some(RED));
    assert!(!context.animating());

    frame_at(&mut context, 1000, view(BLUE));
    assert_eq!(quad_color(&context, rect), Some(RED));
    assert!(context.animating());
    frame_at(&mut context, 1050, view(BLUE));
    assert_eq!(quad_color(&context, rect), Some(Color(100, 0, 100, 255)));

    // Interrupt half way: the way back starts from the mixed colour.
    frame_at(&mut context, 1050, view(RED));
    frame_at(&mut context, 1100, view(RED));
    assert_eq!(quad_color(&context, rect), Some(Color(150, 0, 50, 255)));
    frame_at(&mut context, 1200, view(RED));
    assert_eq!(quad_color(&context, rect), Some(RED));
    assert!(!context.animating());

    // Settled nodes are served from the paint cache again.
    context.set_now(Duration::from_millis(1300));
    let stats = context.run(ROOT, &mut MonospaceMeasurer, view(RED));
    assert!(!stats.scene_rebuilt);
}

#[test]
fn hover_state_can_animate_opacity_and_transform_through_a_layer() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.button_styled(
            "lift",
            "Lift",
            true,
            Style::button()
                .transition(Transition::new(100).easing(Easing::Linear))
                .on_hover(Paint {
                    opacity: Some(0.5),
                    transform: Some(Transform::scale(1.2)),
                    ..Paint::default()
                }),
        );
    };
    frame_at(&mut context, 0, view);
    let rect = bounds(&context, &["lift"]);
    assert!(layer(&context, rect).is_none());
    context.event(Event::PointerMoved(Point {
        x: rect.x + 5.0,
        y: rect.y + 5.0,
    }));
    frame_at(&mut context, 0, view);
    frame_at(&mut context, 50, view);
    let (opacity, transform) = layer(&context, rect).unwrap();
    assert!(close(opacity, 0.75));
    assert!(close(transform.a, 1.1) && close(transform.d, 1.1));
    // Scaling happens about the center, so the center stays put.
    let center = Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    };
    let moved = transform.apply(center);
    assert!(close(moved.x, center.x) && close(moved.y, center.y));
    balanced(&context);
    // Hit testing keeps using the layout bounds.
    assert_eq!(context.hovered(), context.find(&["lift"]));
}

#[test]
fn enter_effects_play_once_and_exit_effects_leave_a_fading_ghost() {
    let mut context = Context::new();
    let view = |shown: bool| {
        move |ui: &mut Ui<'_>| {
            ui.label("always");
            if shown {
                ui.label_styled(
                    "toast",
                    "Saved",
                    Style::default()
                        .enter(Effect {
                            transition: Transition::new(100).easing(Easing::Linear),
                            ..Effect::slide(0.0, 20.0, 100)
                        })
                        .exit(Effect {
                            transition: Transition::new(200).easing(Easing::Linear),
                            ..Effect::fade(200)
                        }),
                );
            }
        }
    };
    frame_at(&mut context, 0, view(false));
    frame_at(&mut context, 100, view(true));
    let rect = bounds(&context, &["toast"]);
    let (opacity, transform) = layer(&context, rect).unwrap();
    assert!(close(opacity, 0.0) && close(transform.f, 20.0));
    frame_at(&mut context, 150, view(true));
    let (opacity, transform) = layer(&context, rect).unwrap();
    assert!(close(opacity, 0.5) && close(transform.f, 10.0));
    frame_at(&mut context, 200, view(true));
    assert!(layer(&context, rect).is_none());
    assert!(!context.animating());

    // Removal: the node is gone at once, its image fades for 200 ms.
    frame_at(&mut context, 1000, view(false));
    assert!(context.find(&["toast"]).is_none());
    assert!(context.animating());
    frame_at(&mut context, 1100, view(false));
    let (opacity, _) = layer(&context, rect).unwrap();
    assert!(close(opacity, 0.5));
    balanced(&context);
    frame_at(&mut context, 1200, view(false));
    assert!(layer(&context, rect).is_none());
    assert!(!context.animating());
}

#[test]
fn layout_transitions_draw_from_the_old_rectangle_toward_the_new_one() {
    let mut context = Context::new();
    let view = |spacer: bool| {
        move |ui: &mut Ui<'_>| {
            if spacer {
                ui.label_styled("spacer", "", Style::default().height(Length::Px(100.0)));
            }
            ui.label_styled(
                "card",
                "card",
                Style::default().layout_transition(Transition::new(100).easing(Easing::Linear)),
            );
        }
    };
    frame_at(&mut context, 0, view(false));
    let before = bounds(&context, &["card"]);
    assert!(layer(&context, before).is_none());
    frame_at(&mut context, 1000, view(true));
    let after = bounds(&context, &["card"]);
    assert!(after.y > before.y);
    // Layout is final at once; only the drawing lags behind.
    let (_, transform) = layer(&context, after).unwrap();
    assert!(close(transform.f, before.y - after.y));
    frame_at(&mut context, 1050, view(true));
    let (_, transform) = layer(&context, after).unwrap();
    assert!(close(transform.f, (before.y - after.y) / 2.0));
    frame_at(&mut context, 1100, view(true));
    assert!(layer(&context, after).is_none());
    assert!(!context.animating());
}

#[test]
fn keyframes_loop_alternate_finish_and_freeze_under_reduced_motion() {
    let mut context = Context::new();
    let view = |repeat: Repeat| {
        move |ui: &mut Ui<'_>| {
            ui.label_styled(
                "pulse",
                "x",
                Style::default().animation(
                    Animation::new(
                        vec![
                            Keyframe::at(0.0).opacity(1.0),
                            Keyframe::at(1.0).opacity(0.0),
                        ],
                        100,
                    )
                    .repeat(repeat)
                    .alternate(true),
                ),
            );
        }
    };
    let opacity = |context: &Context| {
        let rect = bounds(context, &["pulse"]);
        layer(context, rect).map_or(1.0, |(opacity, _)| opacity)
    };
    frame_at(&mut context, 0, view(Repeat::Forever));
    assert!(close(opacity(&context), 1.0));
    frame_at(&mut context, 50, view(Repeat::Forever));
    assert!(close(opacity(&context), 0.5));
    // Second iteration runs backwards: three quarters in is a quarter of the way.
    frame_at(&mut context, 175, view(Repeat::Forever));
    assert!(close(opacity(&context), 0.75));
    frame_at(&mut context, 100_050, view(Repeat::Forever));
    assert!(context.animating());

    // A changed animation restarts; two alternating runs end where they began.
    frame_at(&mut context, 200_000, view(Repeat::Count(2)));
    frame_at(&mut context, 200_050, view(Repeat::Count(2)));
    assert!(close(opacity(&context), 0.5));
    frame_at(&mut context, 200_300, view(Repeat::Count(2)));
    assert!(close(opacity(&context), 1.0));
    assert!(!context.animating());

    context.set_reduced_motion(true);
    frame_at(&mut context, 300_000, view(Repeat::Forever));
    frame_at(&mut context, 300_050, view(Repeat::Forever));
    assert!(!context.animating());
}

#[test]
fn reduced_motion_jumps_transitions_and_skips_enter_and_exit() {
    let mut context = Context::new();
    context.set_reduced_motion(true);
    let view = |shown: bool, color: Color| {
        move |ui: &mut Ui<'_>| {
            if shown {
                ui.label_styled(
                    "box",
                    "x",
                    Style::default()
                        .background(color)
                        .transition(Transition::new(100))
                        .enter(Effect::fade(100))
                        .exit(Effect::fade(100)),
                );
            }
        }
    };
    frame_at(&mut context, 0, view(true, RED));
    let rect = bounds(&context, &["box"]);
    assert!(layer(&context, rect).is_none());
    frame_at(&mut context, 10, view(true, BLUE));
    assert_eq!(quad_color(&context, rect), Some(BLUE));
    frame_at(&mut context, 20, view(false, BLUE));
    assert!(layer(&context, rect).is_none());
    assert!(!context.animating());
}

#[test]
fn stylesheets_declare_transitions_transforms_and_effects() {
    let sheet = StyleSheet::parse(
        "class card {\n\
         opacity: 0.9\n\
         scale: 1.5 2\n\
         rotate: 45\n\
         transition: 150ms ease-in-out 20ms\n\
         layout-transition: 0.2s cubic-bezier(0.2, 0, 0, 1)\n\
         enter-opacity: 0\n\
         enter-translate: 0 12\n\
         enter-transition: 120ms linear\n\
         exit-scale: 0.8\n\
         hover.opacity: 0.5\n\
         hover.translate: 0 -2\n\
         }",
    )
    .unwrap();
    let style = sheet.class("card").unwrap();
    assert_eq!(style.opacity, 0.9);
    assert_eq!(style.transform.scale, (1.5, 2.0));
    assert_eq!(style.transform.rotate, 45.0);
    assert_eq!(
        style.transition,
        Some(Transition::new(150).easing(Easing::EaseInOut).delay(20))
    );
    assert_eq!(
        style.layout_transition,
        Some(Transition::new(200).easing(Easing::CubicBezier(0.2, 0.0, 0.0, 1.0)))
    );
    let enter = style.enter.unwrap();
    assert_eq!(enter.opacity, 0.0);
    assert_eq!(enter.transform.translate, (0.0, 12.0));
    assert_eq!(
        enter.transition,
        Transition::new(120).easing(Easing::Linear)
    );
    let exit = style.exit.unwrap();
    assert_eq!((exit.opacity, exit.transform.scale), (1.0, (0.8, 0.8)));
    let hover = style.states.as_ref().unwrap().hover;
    assert_eq!(hover.opacity, Some(0.5));
    assert_eq!(hover.transform.unwrap().translate, (0.0, -2.0));

    assert!(StyleSheet::parse("class a {\ntransition: fast\n}").is_err());
    assert!(StyleSheet::parse("class a {\ntranslate: 1\n}").is_err());
}

#[test]
fn a_dialog_panel_with_an_exit_effect_fades_after_the_dialog_closes() {
    let mut context = Context::new();
    let view = |mut open: bool| {
        move |ui: &mut Ui<'_>| {
            ui.dialog(
                "confirm",
                &mut open,
                DialogOptions::new("Sure?").style(Style::default().exit(Effect {
                    transition: Transition::new(100).easing(Easing::Linear),
                    ..Effect::zoom(0.9, 100)
                })),
                |ui, _| ui.label("body"),
            );
        }
    };
    frame_at(&mut context, 0, view(true));
    let layers = |context: &Context| {
        context
            .scene()
            .primitives
            .iter()
            .filter(|primitive| matches!(primitive, Primitive::PushLayer { .. }))
            .count()
    };
    assert_eq!(layers(&context), 0);
    frame_at(&mut context, 1000, view(false));
    frame_at(&mut context, 1050, view(false));
    assert_eq!(layers(&context), 1);
    assert!(context.animating());
    balanced(&context);
    frame_at(&mut context, 1100, view(false));
    assert_eq!(layers(&context), 0);
    assert!(!context.animating());
}
