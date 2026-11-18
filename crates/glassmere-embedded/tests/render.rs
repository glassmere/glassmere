//! The primitives, painted by this renderer and by glassmere-render-cpu.
//!
//! The two agree where a pixel is opaque and fully covered. They differ
//! where blending shows: this renderer mixes sRGB values and the CPU renderer
//! mixes light, so edges and translucent colours drift apart by design.
//! Every comparison states how far and for how many pixels.
//!
//! `EMBEDDED_DUMP=<dir>` writes each frame as `<dir>/render_<name>.ppm`.
mod support;

use glassmere_embedded::{
    EmbeddedMeasurer, EmbeddedRenderer, Framebuffer, ImageFilter, RenderOptions, Rgb888,
};
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{
    Affine, Color, GradientStop, Image, Path, Point, Primitive, Rect, Scene, Size, Stroke,
};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;
const BACKGROUND: Color = Color(30, 40, 70, 255);

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn scene(primitives: Vec<Primitive>) -> Scene {
    Scene {
        size: Size::new(WIDTH as f32, HEIGHT as f32),
        primitives,
    }
}

fn quad(rect: Rect, color: Color, radius: f32) -> Primitive {
    Primitive::Quad {
        rect,
        color,
        radius,
    }
}

fn embedded(scene: &Scene, options: RenderOptions) -> Vec<[u8; 3]> {
    let mut pixels = vec![Rgb888::default(); (WIDTH * HEIGHT) as usize];
    let mut frame = Framebuffer::new(&mut pixels, WIDTH, HEIGHT);
    EmbeddedRenderer::new(Default::default(), options).render(scene, &mut frame, Some(BACKGROUND));
    pixels.iter().map(|pixel| pixel.0).collect()
}

fn cpu(scene: &Scene) -> Vec<[u8; 3]> {
    let mut renderer = CpuRenderer::new();
    let rgba = renderer.render([WIDTH, HEIGHT], 1.0, scene, BACKGROUND);
    rgba.chunks(4)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

/// Paints `primitives` both ways and checks that no pixel is further apart
/// than `tolerance` in any channel.
fn agree(name: &str, primitives: Vec<Primitive>, options: RenderOptions, tolerance: u8) {
    let scene = scene(primitives);
    let (ours, theirs) = (embedded(&scene, options), cpu(&scene));
    support::dump(&format!("render_{name}"), WIDTH, HEIGHT, |x, y| {
        ours[(y * WIDTH + x) as usize]
    });
    let apart: Vec<_> = ours
        .iter()
        .zip(&theirs)
        .enumerate()
        .filter(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > tolerance))
        .map(|(at, (a, b))| (at as u32 % WIDTH, at as u32 / WIDTH, *a, *b))
        .collect();
    assert!(
        apart.is_empty(),
        "{name}: {} pixels differ by over {tolerance}; first: {:?}",
        apart.len(),
        &apart[..apart.len().min(4)]
    );
    // A scene that draws nothing agrees with anything.
    assert!(
        ours.iter().any(|pixel| *pixel != [30, 40, 70]),
        "{name} painted nothing"
    );
}

fn stop(at: f32, color: Color) -> GradientStop {
    GradientStop { at, color }
}

#[test]
fn opaque_rectangles_are_identical() {
    agree(
        "rectangles",
        vec![
            quad(rect(4.0, 4.0, 40.0, 30.0), Color(200, 60, 60, 255), 0.0),
            quad(rect(50.0, 10.0, 30.0, 30.0), Color(60, 200, 60, 255), 0.0),
            quad(rect(30.0, 20.0, 40.0, 30.0), Color(60, 60, 200, 255), 0.0),
            quad(rect(90.0, 10.0, 20.0, 20.0), Color(220, 220, 90, 255), 0.0),
        ],
        RenderOptions::default(),
        0,
    );
}

#[test]
fn rounded_corners_agree_to_within_their_anti_aliasing() {
    agree(
        "rounded",
        vec![
            quad(rect(4.0, 4.0, 50.0, 40.0), Color(200, 60, 60, 255), 12.0),
            quad(rect(60.0, 4.0, 60.0, 30.0), Color(60, 200, 60, 255), 15.0),
            quad(rect(8.0, 52.0, 30.0, 30.0), Color(60, 60, 200, 255), 15.0),
            quad(
                rect(50.0, 50.0, 60.0, 12.0),
                Color(230, 230, 230, 255),
                100.0,
            ),
        ],
        RenderOptions::default(),
        48,
    );
}

#[test]
fn gradients_follow_the_ramp() {
    // Ends close in colour, so that mixing sRGB values or light is the same.
    let ramp = vec![
        stop(0.0, Color(40, 90, 160, 255)),
        stop(0.5, Color(60, 120, 180, 255)),
        stop(1.0, Color(90, 150, 200, 255)),
    ];
    agree(
        "gradients",
        vec![
            Primitive::Gradient {
                rect: rect(4.0, 4.0, 60.0, 40.0),
                radius: 0.0,
                from: point(0.0, 0.0),
                to: point(1.0, 1.0),
                stops: ramp.clone(),
                radial: false,
            },
            Primitive::Gradient {
                rect: rect(68.0, 4.0, 50.0, 50.0),
                radius: 10.0,
                from: point(0.5, 0.5),
                to: point(1.0, 0.5),
                stops: ramp.clone(),
                radial: true,
            },
            Primitive::Gradient {
                rect: rect(4.0, 56.0, 100.0, 30.0),
                radius: 0.0,
                from: point(1.0, 0.0),
                to: point(0.0, 0.0),
                stops: ramp,
                radial: false,
            },
        ],
        // The corners of the rounded one are where blending shows.
        RenderOptions::default(),
        20,
    );
}

#[test]
fn shadows_are_close_to_the_gaussian() {
    let shadow = |rect, inset, spread: f32, offset| Primitive::Shadow {
        rect,
        radius: 6.0,
        color: Color(0, 0, 0, 160),
        blur: 4.0,
        spread,
        offset,
        inset,
    };
    agree(
        "shadows",
        vec![
            shadow(rect(10.0, 10.0, 40.0, 30.0), false, 0.0, point(2.0, 3.0)),
            shadow(rect(64.0, 8.0, 50.0, 34.0), false, 3.0, point(0.0, 0.0)),
            shadow(rect(10.0, 54.0, 50.0, 34.0), true, 0.0, point(0.0, 0.0)),
        ],
        RenderOptions::default(),
        // Black over blue is far apart in sRGB and in light; the profile is
        // the same, so the distance stays small next to the 160 of the colour.
        20,
    );
}

#[test]
fn shadows_can_be_left_out() {
    let primitives = vec![Primitive::Shadow {
        rect: rect(10.0, 10.0, 40.0, 30.0),
        radius: 6.0,
        color: Color(0, 0, 0, 255),
        blur: 4.0,
        spread: 0.0,
        offset: point(0.0, 0.0),
        inset: false,
    }];
    let off = embedded(
        &scene(primitives.clone()),
        RenderOptions {
            shadows: false,
            ..RenderOptions::default()
        },
    );
    assert!(off.iter().all(|pixel| *pixel == [30, 40, 70]));
    let on = embedded(&scene(primitives), RenderOptions::default());
    assert!(on.iter().any(|pixel| *pixel != [30, 40, 70]));
}

fn checker() -> Image {
    // 4x4: opaque colours with a translucent diagonal.
    let mut rgba = Vec::new();
    for y in 0..4u8 {
        for x in 0..4u8 {
            let alpha = if x == y { 128 } else { 255 };
            rgba.extend([x * 80, y * 80, 255 - x * 40 - y * 40, alpha]);
        }
    }
    Image::from_rgba8(4, 4, rgba).unwrap()
}

#[test]
fn images_agree_at_their_own_size_and_scaled() {
    let image = checker();
    let whole = rect(0.0, 0.0, 4.0, 4.0);
    let draw = |at: Rect, source: Rect, tint, radius| Primitive::Image {
        rect: at,
        image: image.clone(),
        source,
        tint,
        radius,
    };
    agree(
        "images",
        vec![
            // At its own size: the same texels, a translucent one blended.
            draw(rect(4.0, 4.0, 4.0, 4.0), whole, None, 0.0),
            // Scaled up, with a part of it, and mirrored.
            draw(rect(16.0, 4.0, 32.0, 32.0), whole, None, 0.0),
            draw(
                rect(56.0, 4.0, 32.0, 16.0),
                rect(1.0, 1.0, 2.0, 1.0),
                None,
                0.0,
            ),
            draw(
                rect(96.0, 4.0, 24.0, 24.0),
                rect(4.0, 0.0, -4.0, 4.0),
                None,
                0.0,
            ),
            // Tinted, rounded.
            draw(
                rect(16.0, 48.0, 32.0, 32.0),
                whole,
                Some(Color(250, 200, 60, 255)),
                8.0,
            ),
        ],
        RenderOptions::default(),
        // Linear light against sRGB on a translucent texel, and a bilinear
        // mix of strongly different texels, are the largest gaps.
        64,
    );
    // At its own size and square, the pixels are the image's.
    let ours = embedded(
        &scene(vec![draw(rect(4.0, 4.0, 4.0, 4.0), whole, None, 0.0)]),
        RenderOptions::default(),
    );
    assert_eq!(
        ours[(5 * WIDTH + 6) as usize],
        [160, 80, 135],
        "opaque texel (2, 1)"
    );
}

#[test]
fn nearest_sampling_keeps_texels_whole() {
    let primitives = vec![Primitive::Image {
        rect: rect(0.0, 0.0, 32.0, 32.0),
        image: checker(),
        source: rect(0.0, 0.0, 4.0, 4.0),
        tint: None,
        radius: 0.0,
    }];
    let nearest = embedded(
        &scene(primitives.clone()),
        RenderOptions {
            image_filter: ImageFilter::Nearest,
            ..RenderOptions::default()
        },
    );
    // Each texel is an 8x8 block of one colour.
    for block in 0..16u32 {
        let (bx, by) = (block % 4 * 8, block / 4 * 8);
        let first = nearest[(by * WIDTH + bx) as usize];
        for y in by..by + 8 {
            for x in bx..bx + 8 {
                assert_eq!(nearest[(y * WIDTH + x) as usize], first, "block {block}");
            }
        }
    }
    let smooth = embedded(&scene(primitives), RenderOptions::default());
    assert_ne!(nearest, smooth);
}

#[test]
fn groups_fade_and_move() {
    let content = || {
        vec![
            quad(rect(0.0, 0.0, 30.0, 30.0), Color(240, 240, 240, 255), 0.0),
            quad(rect(10.0, 10.0, 30.0, 30.0), Color(240, 80, 80, 255), 0.0),
        ]
    };
    let group = |bounds, opacity, transform| {
        let mut group = vec![Primitive::PushLayer {
            bounds,
            opacity,
            transform,
        }];
        group.extend(content());
        group.push(Primitive::PopLayer);
        group
    };
    let mut primitives = group(rect(0.0, 0.0, 40.0, 40.0), 1.0, Affine::translate(8.0, 8.0));
    primitives.extend(group(
        rect(0.0, 0.0, 40.0, 40.0),
        0.5,
        Affine::translate(60.0, 20.0),
    ));
    // A group cuts its content to its bounds.
    primitives.extend(group(
        rect(0.0, 0.0, 20.0, 20.0),
        1.0,
        Affine::translate(8.0, 52.0),
    ));
    // Faded groups are painted a primitive at a time, so where its parts
    // overlap the opacity shows twice; opaque ones are identical.
    agree(
        "groups",
        primitives,
        RenderOptions::default(),
        // Half of a light square over blue: the sRGB and light mixes are far apart.
        50,
    );
}

#[test]
fn clips_nest_and_cut_to_whole_pixels() {
    agree(
        "clips",
        vec![
            Primitive::PushClip(rect(10.0, 10.0, 80.0, 60.0)),
            Primitive::PushClip(rect(30.0, 5.0, 100.0, 40.0)),
            quad(rect(0.0, 0.0, 128.0, 96.0), Color(200, 100, 50, 255), 0.0),
            Primitive::PopClip,
            quad(rect(12.0, 50.0, 20.0, 40.0), Color(50, 100, 200, 255), 0.0),
            Primitive::PopClip,
        ],
        RenderOptions::default(),
        0,
    );
}

fn star() -> Path {
    let mut path = Path::new();
    let (cx, cy, outer, inner) = (32.0f32, 32.0f32, 28.0f32, 11.0f32);
    for step in 0..10 {
        let radius = if step % 2 == 0 { outer } else { inner };
        let angle = (step as f32 * 36.0 - 90.0).to_radians();
        let at = point(cx + radius * angle.cos(), cy + radius * angle.sin());
        if step == 0 {
            path.move_to(at);
        } else {
            path.line_to(at);
        }
    }
    path.close();
    path
}

#[test]
fn paths_fill_and_stroke_like_the_cpu_renderer() {
    let mut circle = Path::new();
    let k = 0.552_284_8 * 18.0;
    circle
        .move_to(point(108.0, 30.0))
        .cubic_to(
            point(108.0, 30.0 + k),
            point(90.0 + k, 48.0),
            point(90.0, 48.0),
        )
        .cubic_to(
            point(90.0 - k, 48.0),
            point(72.0, 30.0 + k),
            point(72.0, 30.0),
        )
        .cubic_to(
            point(72.0, 30.0 - k),
            point(90.0 - k, 12.0),
            point(90.0, 12.0),
        )
        .cubic_to(
            point(90.0 + k, 12.0),
            point(108.0, 30.0 - k),
            point(108.0, 30.0),
        )
        .close();
    let mut zigzag = Path::new();
    zigzag
        .move_to(point(70.0, 80.0))
        .line_to(point(85.0, 56.0))
        .line_to(point(100.0, 80.0))
        .quad_to(point(110.0, 56.0), point(122.0, 80.0));
    agree(
        "paths",
        vec![
            Primitive::Path {
                path: star(),
                fill: Some(Color(240, 200, 60, 255)),
                stroke: Some(Stroke {
                    width: 3.0,
                    color: Color(200, 60, 60, 255),
                }),
            },
            Primitive::Path {
                path: circle,
                fill: None,
                stroke: Some(Stroke {
                    width: 4.0,
                    color: Color(100, 230, 120, 255),
                }),
            },
            Primitive::Path {
                path: zigzag,
                fill: None,
                stroke: Some(Stroke {
                    width: 2.0,
                    color: Color(230, 230, 250, 255),
                }),
            },
        ],
        // Edges only, and thin lines are all edge.
        RenderOptions::default(),
        90,
    );
}

#[test]
fn text_lands_in_the_cells_the_measurer_gave() {
    use glassmere_scene::{TextMeasurer, TextStyle};
    let style = TextStyle {
        font_size: 13.0,
        ..TextStyle::default()
    };
    let mut measurer = EmbeddedMeasurer::default();
    let size = measurer.measure_styled("Hello", &style, None);
    assert_eq!(size, Size::new(30.0, 13.0));
    let primitive = Primitive::StyledText {
        rect: rect(10.0, 10.0, size.width, size.height),
        text: "Hello".into(),
        style,
        color: Color::WHITE,
    };
    let frame = embedded(&scene(vec![primitive]), RenderOptions::default());
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let inside = (10..40).contains(&x) && (10..23).contains(&y);
            if !inside {
                assert_eq!(frame[(y * WIDTH + x) as usize], [30, 40, 70], "({x}, {y})");
            }
        }
    }
    let ink = (10..23)
        .flat_map(|y| (10..40).map(move |x| (x, y)))
        .filter(|&(x, y)| frame[(y * WIDTH + x) as usize] == [255, 255, 255])
        .count();
    assert!(ink > 30, "{ink}");
}
