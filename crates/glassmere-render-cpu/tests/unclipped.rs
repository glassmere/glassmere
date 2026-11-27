//! What hangs over an unclipped box is drawn, also when the box has a group
//! opacity, by the real CPU renderer.
use glassmere_core::{Context, Length, MonospaceMeasurer, Style, Transform, Ui};
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{Color, Size};

const WIDTH: u32 = 300;
const HEIGHT: u32 = 100;

/// The pixel at `x`, `y` after drawing a 60 by 40 unclipped box at the top
/// left with `opacity`, holding a red 200 by 40 child that hangs over it.
fn pixel(unclipped: bool, opacity: f32, x: u32, y: u32) -> [u8; 4] {
    moved(unclipped, opacity, Transform::IDENTITY, x, y)
}

fn moved(unclipped: bool, opacity: f32, transform: Transform, x: u32, y: u32) -> [u8; 4] {
    let mut context = Context::new();
    context.run(
        Size::new(WIDTH as f32, HEIGHT as f32),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("page", |ui| {
                ui.row(
                    Style::default()
                        .width(Length::Px(280.0))
                        .height(Length::Px(80.0)),
                    |ui| {
                        ui.keyed("box", |ui| {
                            ui.row(
                                Style::default()
                                    .unclipped(unclipped)
                                    .opacity(opacity)
                                    .transform(transform)
                                    .width(Length::Px(60.0))
                                    .height(Length::Px(40.0)),
                                |ui| {
                                    ui.keyed("wide", |ui| {
                                        ui.container(
                                            Style::default()
                                                .background(Color(255, 0, 0, 255))
                                                .width(Length::Px(200.0))
                                                .height(Length::Px(40.0)),
                                            |_| {},
                                        )
                                    });
                                },
                            )
                        });
                    },
                )
            });
        },
    );
    let pixels = CpuRenderer::new()
        .render(
            [WIDTH, HEIGHT],
            1.0,
            context.scene(),
            Color(255, 255, 255, 255),
        )
        .to_vec();
    let at = ((y * WIDTH + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

fn reddish(pixel: [u8; 4]) -> bool {
    pixel[0] > 200 && pixel[1] < 200 && pixel[2] < 200
}

#[test]
fn a_clipped_box_does_not_draw_the_part_that_hangs_over() {
    assert!(reddish(pixel(false, 1.0, 30, 20)), "inside is drawn");
    assert!(!reddish(pixel(false, 1.0, 120, 20)), "past the edge is cut");
}

#[test]
fn an_unclipped_box_draws_it() {
    assert!(reddish(pixel(true, 1.0, 120, 20)));
}

#[test]
fn an_unclipped_box_with_opacity_draws_it_too() {
    let inside = pixel(true, 0.5, 30, 20);
    let beyond = pixel(true, 0.5, 120, 20);
    assert!(
        inside[1] > 60 && inside[1] < 200,
        "half red inside: {inside:?}"
    );
    assert!(
        beyond[1] > 60 && beyond[1] < 200,
        "half red beyond the box as well: {beyond:?}"
    );
}

#[test]
fn a_transformed_unclipped_box_moves_what_hangs_over_with_it() {
    let down = Transform {
        translate: (0.0, 40.0),
        ..Transform::IDENTITY
    };
    // The child, and the part that hangs over, are 40 lower.
    assert!(
        reddish(moved(true, 1.0, down, 120, 60)),
        "moved with the box"
    );
    assert!(
        !reddish(moved(true, 1.0, down, 120, 20)),
        "and not where it was"
    );
}
