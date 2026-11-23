use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{
    Affine, Color, FontFamily, Image, Path, Point, Primitive, Rect, Scene, Size, Stroke,
    TextMeasurer, TextSpan, TextStyle, TextWrap, VectorImage,
};
use glassmere_text::{TextKey, TextSystem};
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

const SIDE: usize = 64;
const WHITE: [u8; 4] = [255; 4];
const RED: Color = Color(255, 0, 0, 255);
const BLUE: Color = Color(0, 0, 255, 255);

fn renderer() -> CpuRenderer {
    // Only the bundled font, so that the pixels are the same on every machine.
    CpuRenderer::with_text_system(TextSystem::with_fonts([include_bytes!(
        "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()]))
}

fn render(renderer: &mut CpuRenderer, scale: f32, primitives: Vec<Primitive>) -> Vec<u8> {
    let scene = Scene {
        size: Size::new(SIDE as f32 / scale, SIDE as f32 / scale),
        primitives,
    };
    renderer
        .render([SIDE as u32; 2], scale, &scene, Color::WHITE)
        .to_vec()
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn quad(rect: Rect, color: Color, radius: f32) -> Primitive {
    Primitive::Quad {
        rect,
        color,
        radius,
    }
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    pixels[(y * SIDE + x) * 4..][..4].try_into().unwrap()
}

#[track_caller]
fn assert_near(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
        "{actual:?} is not {expected:?} ±2"
    );
}

#[test]
fn quads_fill_exactly_blend_and_round_their_corners() {
    let mut renderer = renderer();
    let pixels = render(
        &mut renderer,
        1.0,
        vec![
            quad(rect(8.0, 8.0, 32.0, 32.0), RED, 0.0),
            quad(rect(24.0, 24.0, 32.0, 32.0), Color(0, 0, 255, 128), 12.0),
        ],
    );
    assert_eq!(pixels.len(), SIDE * SIDE * 4);
    assert_eq!(pixel(&pixels, 7, 20), WHITE, "outside, to the pixel");
    assert_eq!(pixel(&pixels, 8, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 39, 20), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 40, 20), WHITE);
    // Half blue over red, and over white, in linear light as the GPU blends:
    // half of each is 188, not the 128 an sRGB-space mix would give.
    assert_near(pixel(&pixels, 32, 32), [188, 0, 188, 255]);
    assert_near(pixel(&pixels, 50, 50), [188, 188, 255, 255]);
    // The rounded corner leaves what was under it, and its edge is soft.
    assert_eq!(pixel(&pixels, 24, 24), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 55, 55), WHITE);
    let (edge, covered) = (pixel(&pixels, 27, 27), pixel(&pixels, 32, 32));
    assert!(
        edge[2] > 0 && edge[2] < covered[2],
        "{edge:?} against {covered:?}"
    );
    assert_eq!(renderer.stats().frames, 1);
}

#[test]
fn the_scale_factor_and_an_empty_target_are_respected() {
    let mut renderer = renderer();
    let pixels = render(
        &mut renderer,
        2.0,
        vec![quad(rect(4.0, 4.0, 8.0, 8.0), RED, 0.0)],
    );
    assert_eq!(pixel(&pixels, 7, 7), WHITE);
    assert_eq!(pixel(&pixels, 8, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 23, 23), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 24, 24), WHITE);
    let scene = Scene {
        size: Size::new(0.0, 0.0),
        primitives: Vec::new(),
    };
    assert!(
        renderer
            .render([0, 10], 1.0, &scene, Color::WHITE)
            .is_empty()
    );
    assert!(
        renderer
            .render([10, 10], 0.0, &scene, Color::WHITE)
            .is_empty()
    );
    assert_eq!(renderer.render([3, 2], 1.0, &scene, BLUE).len(), 24);
}

#[test]
fn clips_nest_and_a_stray_pop_does_not_unclip() {
    let mut renderer = renderer();
    let pixels = render(
        &mut renderer,
        1.0,
        vec![
            Primitive::PopClip,
            Primitive::PushClip(rect(10.0, 10.0, 30.0, 30.0)),
            Primitive::PushClip(rect(20.0, 0.0, 40.0, 25.0)),
            quad(rect(0.0, 0.0, 64.0, 64.0), RED, 8.0),
            Primitive::PopClip,
            quad(rect(0.0, 30.0, 64.0, 4.0), BLUE, 0.0),
            Primitive::PopClip,
            quad(rect(0.0, 60.0, 64.0, 4.0), BLUE, 0.0),
        ],
    );
    // Red only where both clips agree: x 20..40, y 10..25.
    assert_eq!(pixel(&pixels, 20, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 39, 24), [255, 0, 0, 255]);
    for (x, y) in [(19, 12), (40, 12), (30, 9), (30, 25)] {
        assert_eq!(pixel(&pixels, x, y), WHITE, "({x}, {y})");
    }
    // The outer clip alone, then none.
    assert_eq!(pixel(&pixels, 10, 31), [0, 0, 255, 255]);
    assert_eq!(pixel(&pixels, 9, 31), WHITE);
    assert_eq!(pixel(&pixels, 0, 62), [0, 0, 255, 255]);
}

/// Square quads painted the plain way: every pixel on its own, from the area
/// of it the rectangle covers and the colour's alpha, mixed in linear light
/// through tables built the way the renderer builds its own.
struct PerPixel {
    /// 0..=255 to 0..=4095.
    linear: [u16; 256],
    back: [u8; 4096],
}

impl PerPixel {
    fn new() -> Self {
        let mut tables = Self {
            linear: [0; 256],
            back: [0; 4096],
        };
        for (byte, linear) in tables.linear.iter_mut().enumerate() {
            let value = byte as f32 / 255.0;
            let light = if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            };
            *linear = (light * 4095.0).round() as u16;
        }
        for (light, byte) in tables.back.iter_mut().enumerate() {
            let light = light as f32 / 4095.0;
            let value = if light <= 0.003_130_8 {
                light * 12.92
            } else {
                1.055 * light.powf(1.0 / 2.4) - 0.055
            };
            *byte = (value * 255.0).round() as u8;
        }
        tables
    }

    /// The frame `render` gives for clips and quads without a radius.
    fn paint(&self, scale: f32, primitives: &[Primitive]) -> Vec<u8> {
        let mut pixels = vec![255; SIDE * SIDE * 4];
        let mut clips = vec![rect(0.0, 0.0, SIDE as f32 / scale, SIDE as f32 / scale)];
        for primitive in primitives {
            let clip = *clips.last().unwrap();
            match primitive {
                Primitive::PushClip(inner) => clips.push(clip.intersect(*inner)),
                Primitive::PopClip => {
                    if clips.len() > 1 {
                        clips.pop();
                    }
                }
                Primitive::Quad {
                    rect,
                    color,
                    radius,
                } if *radius <= 0.0 => {
                    self.quad(&mut pixels, scale, clip.intersect(*rect), *rect, *color);
                }
                other => panic!("neither a clip nor a square quad: {other:?}"),
            }
        }
        pixels
    }

    /// Lays `quad` over the pixels that `seen`, the part of it inside the
    /// clip, touches.
    fn quad(&self, pixels: &mut [u8], scale: f32, seen: Rect, quad: Rect, color: Color) {
        if seen.width <= 0.0 || seen.height <= 0.0 {
            return;
        }
        // A clip cuts at whole pixels, outward.
        let first = |edge: f32| (edge * scale).floor().max(0.0).min(SIDE as f32) as usize;
        let past = |edge: f32| (edge * scale).ceil().max(0.0).min(SIDE as f32) as usize;
        let (left, top) = (quad.x * scale, quad.y * scale);
        let (right, bottom) = (left + quad.width * scale, top + quad.height * scale);
        // How much of pixel `at` lies between two edges, 0..=255.
        let cover = |at: usize, from: f32, to: f32| {
            ((to.min(at as f32 + 1.0) - from.max(at as f32)).clamp(0.0, 1.0) * 255.0).round() as u32
        };
        for y in first(seen.y)..past(seen.y + seen.height) {
            for x in first(seen.x)..past(seen.x + seen.width) {
                let alpha = cover(x, left, right) * cover(y, top, bottom) * u32::from(color.3)
                    / (255 * 255);
                let pixel = &mut pixels[(y * SIDE + x) * 4..][..4];
                if alpha == 255 {
                    pixel.copy_from_slice(&[color.0, color.1, color.2, 255]);
                } else if alpha > 0 {
                    let keep = 255 - alpha;
                    for (channel, straight) in [color.0, color.1, color.2].into_iter().enumerate() {
                        let over =
                            (u32::from(self.linear[usize::from(straight)]) * alpha + 127) / 255;
                        let under = u32::from(self.linear[usize::from(pixel[channel])]);
                        let mixed = over + (under * keep + 127) / 255;
                        pixel[channel] = self.back[mixed.min(4095) as usize];
                    }
                    pixel[3] = (alpha + (u32::from(pixel[3]) * keep + 127) / 255).min(255) as u8;
                }
            }
        }
    }
}

/// Fails, naming the first pixel that differs, unless the renderer paints
/// `primitives` exactly as `PerPixel` does.
#[track_caller]
fn assert_per_pixel(
    renderer: &mut CpuRenderer,
    per_pixel: &PerPixel,
    scale: f32,
    primitives: Vec<Primitive>,
    case: &str,
) {
    let expected = per_pixel.paint(scale, &primitives);
    let painted = render(renderer, scale, primitives);
    assert_eq!(painted.len(), expected.len());
    for at in 0..SIDE * SIDE {
        let (x, y) = (at % SIDE, at / SIDE);
        assert_eq!(
            pixel(&painted, x, y),
            pixel(&expected, x, y),
            "{case} at scale {scale}: ({x}, {y})"
        );
    }
}

#[test]
fn square_quads_match_the_per_pixel_formula() {
    let per_pixel = PerPixel::new();
    let mut renderer = renderer();
    // Every edge between pixels: columns and rows covered in part around a
    // middle that is covered whole.
    let between = rect(8.25, 8.5, 20.5, 10.25);
    // Two colours side by side, for a translucent quad to lie across.
    let ground = || {
        vec![
            quad(rect(0.0, 0.0, 14.0, 32.0), RED, 0.0),
            quad(rect(14.0, 0.0, 18.0, 32.0), Color(20, 150, 60, 255), 0.0),
        ]
    };
    for scale in [1.0, 1.5, 2.0] {
        assert_per_pixel(
            &mut renderer,
            &per_pixel,
            scale,
            vec![quad(between, RED, 0.0)],
            "opaque",
        );
        for alpha in [128, 1, 254] {
            let mut primitives = ground();
            primitives.push(quad(between, Color(0, 0, 255, alpha), 0.0));
            assert_per_pixel(
                &mut renderer,
                &per_pixel,
                scale,
                primitives,
                &format!("alpha {alpha}"),
            );
        }
    }
    // What the formula comes to: the middle is the colour itself, and three
    // quarters of a column of red lie over white in linear light.
    let pixels = render(&mut renderer, 1.0, vec![quad(between, RED, 0.0)]);
    assert_eq!(pixel(&pixels, 9, 9), [255, 0, 0, 255]);
    assert_near(pixel(&pixels, 8, 12), [255, 137, 137, 255]);
    assert_eq!(pixel(&pixels, 7, 12), WHITE);
}

#[test]
fn narrow_square_quads_match_the_per_pixel_formula() {
    let per_pixel = PerPixel::new();
    let mut renderer = renderer();
    for scale in [1.0, 1.5, 2.0] {
        for (case, narrow) in [
            ("inside one column", rect(10.3, 4.0, 0.4, 20.0)),
            ("one whole column", rect(10.0, 4.0, 1.0, 20.0)),
            ("across two columns", rect(10.5, 4.0, 1.0, 20.0)),
            ("inside one row", rect(4.0, 20.3, 24.0, 0.4)),
        ] {
            for color in [RED, Color(0, 0, 255, 128)] {
                assert_per_pixel(
                    &mut renderer,
                    &per_pixel,
                    scale,
                    vec![quad(narrow, color, 0.0)],
                    case,
                );
            }
        }
    }
    // No column of it is covered whole, and it still shows, in one column.
    let pixels = render(
        &mut renderer,
        1.0,
        vec![quad(rect(10.3, 4.0, 0.4, 20.0), RED, 0.0)],
    );
    let faint = pixel(&pixels, 10, 12);
    assert!(
        faint[0] == 255 && faint[1] > 0 && faint[1] < 255,
        "{faint:?}"
    );
    assert_eq!(pixel(&pixels, 9, 12), WHITE);
    assert_eq!(pixel(&pixels, 11, 12), WHITE);
}

#[test]
fn clipped_square_quads_match_the_per_pixel_formula() {
    let per_pixel = PerPixel::new();
    let mut renderer = renderer();
    let glass = Color(0, 0, 255, 128);
    // The clips leave one column of pixels, and two quads reach into it: one
    // covers the column whole, the other has its edge inside it.
    let one_column = || {
        vec![
            Primitive::PushClip(rect(10.0, 0.0, 9.0, 64.0)),
            Primitive::PushClip(rect(0.0, 2.0, 10.5, 40.0)),
            quad(rect(4.25, 6.5, 30.0, 8.0), RED, 0.0),
            quad(rect(10.25, 20.5, 30.0, 8.0), glass, 0.0),
        ]
    };
    for scale in [1.0, 1.5, 2.0] {
        assert_per_pixel(
            &mut renderer,
            &per_pixel,
            scale,
            vec![quad(rect(-3.5, -3.5, 80.0, 80.0), glass, 0.0)],
            "past every edge of the target",
        );
        for color in [RED, glass] {
            assert_per_pixel(
                &mut renderer,
                &per_pixel,
                scale,
                vec![
                    Primitive::PushClip(rect(12.5, 9.5, 7.25, 30.0)),
                    quad(rect(8.25, 4.5, 20.5, 20.25), color, 0.0),
                    Primitive::PopClip,
                    quad(rect(2.5, 1.25, 4.0, 2.5), color, 0.0),
                ],
                "under a clip between pixels",
            );
        }
        assert_per_pixel(&mut renderer, &per_pixel, scale, one_column(), "one column");
    }
    let pixels = render(&mut renderer, 1.0, one_column());
    for y in [8, 24] {
        assert_eq!(pixel(&pixels, 9, y), WHITE, "left of the column");
        assert_ne!(pixel(&pixels, 10, y), WHITE, "the column");
        assert_eq!(pixel(&pixels, 11, y), WHITE, "right of the column");
    }
}

#[test]
fn a_wide_square_quad_then_a_narrow_one_and_back() {
    let per_pixel = PerPixel::new();
    let mut renderer = renderer();
    // Each quad's columns come from its own edges: none of a wide quad's are
    // left over for the narrow ones after it, nor any of theirs for the wide
    // one at the end.
    assert_per_pixel(
        &mut renderer,
        &per_pixel,
        1.0,
        vec![
            quad(rect(1.25, 2.0, 60.0, 8.0), RED, 0.0),
            quad(rect(20.6, 14.0, 3.0, 8.0), BLUE, 0.0),
            quad(rect(30.2, 26.0, 3.0, 8.0), BLUE, 0.0),
            quad(rect(40.3, 38.0, 0.5, 8.0), BLUE, 0.0),
            quad(rect(11.75, 50.0, 40.0, 8.0), Color(0, 0, 255, 128), 0.0),
        ],
        "wide, narrow, wide",
    );
}

/// A column covered 254 steps of 255 is not covered whole: opaque red is mixed
/// into it, not stored in its place. Each edge of this quad is 0.004 of a
/// pixel in from a pixel boundary, so the columns and rows along them are
/// covered 254 of 255 and the pixels at the corners come to 253.
#[test]
fn an_edge_one_step_short_of_whole_is_mixed_and_not_stored() {
    let per_pixel = PerPixel::new();
    let mut renderer = renderer();
    let short = rect(10.004, 4.004, 10.992, 7.992);
    assert_per_pixel(
        &mut renderer,
        &per_pixel,
        1.0,
        vec![quad(short, RED, 0.0)],
        "one step short",
    );
    // A step of 255 of the white under the red is left, and shows as 13 in the
    // green and the blue, the mix being in linear light. Two of these pixels
    // are in the columns along the edges, two in the rows.
    let pixels = render(&mut renderer, 1.0, vec![quad(short, RED, 0.0)]);
    for (x, y) in [(10, 8), (20, 8), (15, 4), (15, 11)] {
        assert_near(pixel(&pixels, x, y), [255, 13, 13, 255]);
    }
    assert_eq!(pixel(&pixels, 15, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 9, 8), WHITE);
    assert_eq!(pixel(&pixels, 21, 8), WHITE);
}

/// A sliver 0.0019607842 of a pixel across is 0.49999997 of one step of 255
/// and rounds to none. It is the one fraction of a pixel where adding a half
/// and cutting off the rest comes out a step higher, the sum being 1.0, and a
/// step of white shows on black.
#[test]
fn a_sliver_under_half_a_step_across_paints_nothing() {
    const HALF_A_STEP: f32 = 0.001_960_784_2;
    let mut renderer = renderer();
    let mut over_black = |sliver: Rect| {
        let ground = quad(rect(0.0, 0.0, 64.0, 64.0), Color::rgb(0, 0, 0), 0.0);
        let sliver = quad(sliver, Color::WHITE, 0.0);
        render(&mut renderer, 1.0, vec![ground, sliver])
    };
    // Down the left edge of the target, and along its top edge.
    for sliver in [
        rect(0.0, 4.0, HALF_A_STEP, 20.0),
        rect(4.0, 0.0, 20.0, HALF_A_STEP),
    ] {
        let pixels = over_black(sliver);
        let (frame, _) = pixels.as_chunks::<4>();
        assert!(
            frame.iter().all(|pixel| *pixel == [0, 0, 0, 255]),
            "{sliver:?} marked the ground"
        );
    }
    // One step across, 0.004, does show, faint as it is.
    let pixels = over_black(rect(0.0, 4.0, 0.004, 20.0));
    assert_near(pixel(&pixels, 0, 8), [13, 13, 13, 255]);
}

#[test]
fn layers_fade_move_clip_their_content_and_close_themselves() {
    let mut renderer = renderer();
    let pixels = render(
        &mut renderer,
        1.0,
        vec![
            Primitive::PushLayer {
                bounds: rect(0.0, 0.0, 20.0, 20.0),
                opacity: 0.5,
                transform: Affine::translate(30.0, 10.0),
            },
            // Two overlapping quads fade as one picture, not one by one.
            quad(rect(0.0, 0.0, 16.0, 16.0), RED, 0.0),
            quad(rect(4.0, 4.0, 30.0, 30.0), RED, 0.0),
            Primitive::PopLayer,
            // Full opacity and no transform draws inline, but still clips.
            Primitive::PushLayer {
                bounds: rect(0.0, 40.0, 10.0, 10.0),
                opacity: 1.0,
                transform: Affine::IDENTITY,
            },
            quad(rect(0.0, 40.0, 64.0, 24.0), BLUE, 0.0),
            // Never closed.
        ],
    );
    assert_eq!(pixel(&pixels, 5, 5), WHITE, "moved away from here");
    // Half red over white, in linear light.
    assert_near(pixel(&pixels, 35, 15), [255, 188, 188, 255]);
    assert_near(pixel(&pixels, 40, 20), [255, 188, 188, 255]);
    // The layer's bounds cut the second quad off at 20 by 20.
    assert_near(pixel(&pixels, 49, 29), [255, 188, 188, 255]);
    assert_eq!(pixel(&pixels, 50, 29), WHITE);
    assert_eq!(pixel(&pixels, 49, 30), WHITE);
    assert_eq!(pixel(&pixels, 9, 49), [0, 0, 255, 255]);
    assert_eq!(pixel(&pixels, 10, 49), WHITE);
    assert_eq!(pixel(&pixels, 5, 50), WHITE);
    assert_eq!(
        renderer.stats().layers,
        1,
        "only the first needed a surface"
    );
}

/// Two by two: red, blue over green, white.
fn four() -> Image {
    Image::from_rgba8(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255, 255, 255, 255, 255,
        ],
    )
    .unwrap()
}

#[test]
fn images_scale_take_a_source_without_bleeding_tint_and_mirror() {
    let mut renderer = renderer();
    let image = four();
    let draw = |rect: Rect, source: Rect, tint: Option<Color>| Primitive::Image {
        rect,
        image: image.clone(),
        source,
        tint,
        radius: 0.0,
    };
    let pixels = render(
        &mut renderer,
        1.0,
        vec![
            // The whole image, filtered: corners are pure, the middle is mixed.
            draw(rect(0.0, 0.0, 32.0, 32.0), rect(0.0, 0.0, 2.0, 2.0), None),
            // One texel of it: no neighbour may show, even at its far edge.
            draw(rect(32.0, 0.0, 32.0, 32.0), rect(1.0, 0.0, 1.0, 1.0), None),
            // Tinted: the tint's colour everywhere the image is opaque.
            draw(
                rect(0.0, 32.0, 32.0, 32.0),
                rect(0.0, 0.0, 2.0, 2.0),
                Some(Color(10, 200, 30, 255)),
            ),
            // Mirrored: blue comes first.
            draw(
                rect(32.0, 32.0, 32.0, 32.0),
                rect(2.0, 0.0, -2.0, 2.0),
                None,
            ),
        ],
    );
    assert_near(pixel(&pixels, 1, 1), [255, 0, 0, 255]);
    assert_near(pixel(&pixels, 30, 30), [255, 255, 255, 255]);
    let middle = pixel(&pixels, 16, 1);
    assert!(middle[0] > 60 && middle[2] > 60, "{middle:?}");
    for (x, y) in [(33, 1), (62, 1), (33, 30), (62, 30)] {
        assert_eq!(pixel(&pixels, x, y), [0, 0, 255, 255], "({x}, {y})");
    }
    assert_eq!(pixel(&pixels, 5, 40), [10, 200, 30, 255]);
    assert_eq!(pixel(&pixels, 28, 60), [10, 200, 30, 255]);
    assert_near(pixel(&pixels, 34, 34), [0, 0, 255, 255]);
    assert_near(pixel(&pixels, 61, 34), [255, 0, 0, 255]);
}

/// Opaque, and as red as it is wide, so that the size asked for can be read
/// from the picture.
struct Probe;
impl VectorImage for Probe {
    fn size(&self) -> Size {
        Size::new(10.0, 10.0)
    }
    fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
        [width as u8, height as u8, 0, 255].repeat((width * height) as usize)
    }
}

#[test]
fn vector_images_are_rasterized_for_the_pixels_they_cover() {
    let mut renderer = renderer();
    let image = Image::from_vector(Probe);
    let draw = |rect: Rect| Primitive::Image {
        rect,
        image: image.clone(),
        source: Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        tint: None,
        radius: 0.0,
    };
    let pixels = render(
        &mut renderer,
        2.0,
        vec![
            draw(rect(0.0, 0.0, 10.0, 5.0)),
            draw(rect(16.0, 16.0, 16.0, 16.0)),
        ],
    );
    // Ten by five logical pixels at a scale of two: twenty by ten.
    assert_eq!(pixel(&pixels, 2, 2), [20, 10, 0, 255]);
    assert_eq!(pixel(&pixels, 40, 40), [32, 32, 0, 255]);
}

/// White where `x + y` is even, black elsewhere: detail one pixel wide.
fn checkerboard(side: u32) -> Image {
    let mut rgba = Vec::new();
    for y in 0..side {
        for x in 0..side {
            let shade = if (x + y) % 2 == 0 { 255 } else { 0 };
            rgba.extend([shade, shade, shade, 255]);
        }
    }
    Image::from_rgba8(side, side, rgba).unwrap()
}

#[test]
fn an_image_drawn_far_below_its_size_is_filtered_and_one_to_one_is_not() {
    let mut renderer = renderer();
    let picture = |rect: Rect, image: &Image, side: f32| Primitive::Image {
        rect,
        image: image.clone(),
        source: Rect {
            x: 0.0,
            y: 0.0,
            width: side,
            height: side,
        },
        tint: None,
        radius: 0.0,
    };
    // 512 texels in 60 pixels, an eighth and a bit: the sampling phase drifts
    // from pixel to pixel, so unfiltered one-pixel detail comes out as noise.
    let large = checkerboard(512);
    let pixels = render(
        &mut renderer,
        1.0,
        vec![picture(rect(2.0, 2.0, 60.0, 60.0), &large, 512.0)],
    );
    let inside: Vec<u8> = (4..60)
        .flat_map(|y| (4..60).map(move |x| (x, y)))
        .map(|(x, y)| pixel(&pixels, x, y)[0])
        .collect();
    let mean = inside.iter().map(|v| f32::from(*v)).sum::<f32>() / inside.len() as f32;
    let spread = inside.iter().map(|v| v.abs_diff(mean as u8)).max().unwrap();
    assert!(spread <= 8, "pixels stray {spread} from the mean");
    // Half black and half white, averaged in linear light: sRGB 188.
    assert!((mean - 188.0).abs() <= 3.0, "mean {mean}");
    // Drawn at its own size the same pattern is left alone.
    let small = checkerboard(64);
    let pixels = render(
        &mut renderer,
        1.0,
        vec![picture(rect(0.0, 0.0, 64.0, 64.0), &small, 64.0)],
    );
    assert_eq!(pixel(&pixels, 10, 10), [255, 255, 255, 255]);
    assert_eq!(pixel(&pixels, 11, 10), [0, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 11, 11), [255, 255, 255, 255]);
}

/// A translucent left half over an opaque right half, 512 texels on a side.
fn halves() -> Image {
    let mut rgba = Vec::new();
    for _ in 0..512 {
        for x in 0..512 {
            rgba.extend(if x < 256 {
                [200, 80, 40, 128]
            } else {
                [30, 120, 220, 255]
            });
        }
    }
    Image::from_rgba8(512, 512, rgba).unwrap()
}

#[test]
fn a_translucent_photo_filtered_far_below_its_size_keeps_its_pixels() {
    let mut renderer = renderer();
    let image = halves();
    let picture = |rect: Rect| Primitive::Image {
        rect,
        image: image.clone(),
        source: Rect {
            x: 0.0,
            y: 0.0,
            width: 512.0,
            height: 512.0,
        },
        tint: None,
        radius: 0.0,
    };
    // 512 texels in 41 pixels: three halvings, to 64. The seam falls inside
    // the pixel at x = 28.
    let pixels = render(
        &mut renderer,
        1.0,
        vec![picture(rect(8.0, 8.0, 41.0, 41.0))],
    );
    for (x, expected) in [
        (7, WHITE),
        (10, [229, 194, 189, 255]),
        (27, [229, 194, 189, 255]),
        (28, [170, 163, 206, 255]),
        (29, [30, 120, 220, 255]),
        (46, [30, 120, 220, 255]),
    ] {
        assert_near(pixel(&pixels, x, 28), expected);
    }
    // 512 texels in 26 pixels: four halvings, to 32, the seam in the pixel at
    // x = 21.
    let pixels = render(
        &mut renderer,
        2.0,
        vec![picture(rect(4.25, 4.0, 13.0, 13.0))],
    );
    for (x, expected) in [
        (7, WHITE),
        (12, [229, 194, 189, 255]),
        (20, [229, 194, 189, 255]),
        (21, [169, 162, 205, 255]),
        (22, [30, 120, 220, 255]),
        (30, [30, 120, 220, 255]),
    ] {
        assert_near(pixel(&pixels, x, 20), expected);
    }
}

#[test]
fn text_is_drawn_where_it_was_measured_in_its_colour_and_clipped() {
    let mut renderer = renderer();
    let style = TextStyle {
        font_size: 20.0,
        ..TextStyle::default()
    };
    let measured = renderer.text_system().measure_styled("Hg", &style, None);
    assert!(
        measured.width > 15.0 && measured.height > 15.0,
        "{measured:?}"
    );
    let text = |clip: Rect| {
        vec![
            Primitive::PushClip(clip),
            Primitive::StyledText {
                rect: rect(6.0, 8.0, measured.width, measured.height),
                text: "Hg".into(),
                style: style.clone(),
                color: BLUE,
            },
        ]
    };
    let ink = |pixels: &[u8]| -> Vec<(usize, usize)> {
        (0..SIDE * SIDE)
            .filter(|at| pixels[at * 4..][..4] != WHITE)
            .map(|at| (at % SIDE, at / SIDE))
            .collect()
    };
    let pixels = render(&mut renderer, 1.0, text(rect(0.0, 0.0, 64.0, 64.0)));
    let inked = ink(&pixels);
    assert!(inked.len() > 60, "{}", inked.len());
    // Inside the measured box, and only ever blue mixed with white.
    for (x, y) in &inked {
        assert!((6..=6 + measured.width as usize).contains(x), "x {x}");
        assert!((8..=8 + measured.height as usize).contains(y), "y {y}");
        let [r, g, b, a] = pixel(&pixels, *x, *y);
        assert!(r == g && b == 255 && a == 255, "{:?}", [r, g, b, a]);
    }
    assert!(
        inked.iter().any(|(x, y)| pixel(&pixels, *x, *y)[0] < 40),
        "solid strokes"
    );
    assert!(renderer.stats().glyphs >= 2);

    // The left half only: the H stays, the g goes.
    let half = render(&mut renderer, 1.0, text(rect(0.0, 0.0, 17.0, 64.0)));
    let kept = ink(&half);
    assert!(!kept.is_empty() && kept.len() < inked.len());
    assert!(kept.iter().all(|(x, _)| *x < 17));

    // Twice the pixels, about four times the ink.
    let doubled = ink(&render(
        &mut renderer,
        2.0,
        text(rect(0.0, 0.0, 64.0, 64.0)),
    ));
    assert!(
        doubled.len() > inked.len() * 2,
        "{} vs {}",
        doubled.len(),
        inked.len()
    );
}

#[test]
fn a_damaged_frame_repaints_only_what_changed() {
    let mut renderer = renderer();
    let first = render(
        &mut renderer,
        1.0,
        vec![
            quad(rect(0.0, 0.0, 30.0, 30.0), RED, 0.0),
            quad(rect(34.0, 34.0, 30.0, 30.0), BLUE, 0.0),
        ],
    );
    assert_eq!(renderer.stats().redrawn, (SIDE * SIDE) as u64);

    // The second scene keeps neither quad, but only the second one's corner
    // is declared damaged, so that is all that may change.
    let scene = Scene {
        size: Size::new(SIDE as f32, SIDE as f32),
        primitives: vec![quad(rect(40.0, 40.0, 24.0, 24.0), RED, 0.0)],
    };
    let damage = [rect(40.0, 40.0, 10.0, 10.0)];
    let pixels = renderer
        .render_damaged([SIDE as u32; 2], 1.0, &scene, Color::WHITE, &damage)
        .to_vec();
    assert_eq!(renderer.stats().redrawn, 100);
    for (at, (new, old)) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(first.as_chunks::<4>().0)
        .enumerate()
    {
        let (x, y) = (at % SIDE, at / SIDE);
        let inside = (40..50).contains(&x) && (40..50).contains(&y);
        if inside {
            assert_eq!(*new, [255, 0, 0, 255], "({x}, {y}) is inside the damage");
        } else {
            assert_eq!(new, old, "({x}, {y}) is outside the damage");
        }
    }

    // Nothing to reuse: a different size, and the whole frame is painted.
    let scene = Scene {
        size: Size::new(32.0, 32.0),
        primitives: vec![quad(rect(0.0, 0.0, 8.0, 8.0), BLUE, 0.0)],
    };
    let pixels = renderer
        .render_damaged([32, 32], 1.0, &scene, Color::WHITE, &damage)
        .to_vec();
    assert_eq!(renderer.stats().redrawn, 32 * 32);
    assert_eq!(pixels[..4], [0, 0, 255, 255]);
    assert_eq!(pixels[(31 * 32 + 31) * 4..][..4], WHITE);
}

/// A ramp straight across the rectangle.
fn gradient(rect: Rect, stops: Vec<glassmere_scene::GradientStop>) -> Primitive {
    Primitive::Gradient {
        rect,
        radius: 0.0,
        from: Point { x: 0.0, y: 0.0 },
        to: Point { x: 1.0, y: 0.0 },
        stops,
        radial: false,
    }
}

fn stop(at: f32, color: Color) -> glassmere_scene::GradientStop {
    glassmere_scene::GradientStop { at, color }
}

#[test]
fn a_gradient_mixes_its_stops_in_linear_light_and_stops_at_eight() {
    let mut renderer = renderer();
    let pixels = render(
        &mut renderer,
        1.0,
        vec![gradient(
            rect(0.0, 0.0, 64.0, 16.0),
            vec![stop(0.0, RED), stop(1.0, BLUE)],
        )],
    );
    // Half of each in linear light is 188, not the 128 an sRGB mix gives.
    assert_near(pixel(&pixels, 32, 8), [188, 0, 188, 255]);
    // The ends are all but pure: a pixel's centre is already a little along.
    let (first, last) = (pixel(&pixels, 0, 0), pixel(&pixels, 63, 15));
    assert!(first[0] > 240 && first[2] < 40, "{first:?}");
    assert!(last[2] > 240 && last[0] < 40, "{last:?}");
    // Along the ramp only: every row is the same.
    assert_eq!(pixel(&pixels, 20, 2), pixel(&pixels, 20, 14));

    // A stop in the middle bends the ramp.
    let pixels = render(
        &mut renderer,
        1.0,
        vec![gradient(
            rect(0.0, 0.0, 64.0, 16.0),
            vec![stop(0.0, RED), stop(0.25, BLUE), stop(1.0, RED)],
        )],
    );
    let bend = pixel(&pixels, 16, 8);
    assert!(bend[2] > 240 && bend[0] < 40, "{bend:?}");
    assert!(pixel(&pixels, 40, 8)[0] > 150, "on its way back to red");

    // The ninth stop and beyond are dropped, as the shader has room for eight.
    let mut many = vec![stop(0.0, RED)];
    many.extend((1..8).map(|index| stop(index as f32 / 7.0, BLUE)));
    many.push(stop(1.0, Color::rgb(0, 255, 0)));
    let pixels = render(
        &mut renderer,
        1.0,
        vec![gradient(rect(0.0, 0.0, 64.0, 16.0), many)],
    );
    let end = pixel(&pixels, 63, 8);
    assert!(
        end[2] > 240 && end[1] < 40,
        "the green ninth stop was dropped: {end:?}"
    );

    // A radial one runs outward from its centre.
    let pixels = render(
        &mut renderer,
        1.0,
        vec![Primitive::Gradient {
            rect: rect(0.0, 0.0, 64.0, 64.0),
            radius: 0.0,
            from: Point { x: 0.5, y: 0.5 },
            to: Point { x: 1.0, y: 0.5 },
            stops: vec![stop(0.0, RED), stop(1.0, BLUE)],
            radial: true,
        }],
    );
    // The middle pixel's centre is already a little way out.
    let centre = pixel(&pixels, 32, 32);
    assert!(centre[0] > 240 && centre[2] < 60, "{centre:?}");
    let edge = pixel(&pixels, 63, 32);
    let corner = pixel(&pixels, 60, 60);
    assert!(edge[2] > 200 && corner[2] > 200, "{edge:?} {corner:?}");
    assert!(
        pixel(&pixels, 40, 32)[0] > pixel(&pixels, 56, 32)[0],
        "it fades outward"
    );
}

#[test]
fn a_shadow_falls_around_its_rectangle_and_an_inset_one_stays_inside() {
    let mut renderer = renderer();
    let pixels = render(
        &mut renderer,
        1.0,
        vec![Primitive::Shadow {
            rect: rect(20.0, 20.0, 24.0, 24.0),
            radius: 6.0,
            color: Color(0, 0, 0, 255),
            blur: 4.0,
            spread: 0.0,
            offset: Point { x: 0.0, y: 0.0 },
            inset: false,
        }],
    );
    // Dark under it, fading outward, and gone three deviations away.
    assert!(pixel(&pixels, 32, 32)[0] < 20);
    let near = pixel(&pixels, 32, 17)[0];
    let far = pixel(&pixels, 32, 10)[0];
    assert!(near < far && far > 200, "{near} then {far}");
    assert_eq!(pixel(&pixels, 32, 2), WHITE, "nothing past the blur");
    assert_eq!(pixel(&pixels, 0, 0), WHITE);

    // An offset moves it without moving anything else.
    let moved = render(
        &mut renderer,
        1.0,
        vec![Primitive::Shadow {
            rect: rect(20.0, 20.0, 24.0, 24.0),
            radius: 6.0,
            color: Color(0, 0, 0, 255),
            blur: 4.0,
            spread: 0.0,
            offset: Point { x: 8.0, y: 0.0 },
            inset: false,
        }],
    );
    assert_eq!(pixel(&moved, 40, 32), pixel(&pixels, 32, 32));

    // An inset one darkens the inside edge and leaves the middle alone.
    let inside = render(
        &mut renderer,
        1.0,
        vec![
            quad(rect(8.0, 8.0, 48.0, 48.0), Color::WHITE, 8.0),
            Primitive::Shadow {
                rect: rect(8.0, 8.0, 48.0, 48.0),
                radius: 8.0,
                color: Color(0, 0, 0, 255),
                blur: 5.0,
                spread: 0.0,
                offset: Point { x: 0.0, y: 0.0 },
                inset: true,
            },
        ],
    );
    let (edge, middle) = (
        u32::from(pixel(&inside, 10, 32)[0]),
        u32::from(pixel(&inside, 32, 32)[0]),
    );
    assert!(
        edge + 20 < middle,
        "the edge is darkened: {edge} against {middle}"
    );
    assert_eq!(pixel(&inside, 32, 32), WHITE, "the middle is left alone");
    assert_eq!(pixel(&inside, 2, 32), WHITE, "and nothing spills out");
}

#[test]
fn paths_fill_by_the_non_zero_rule_and_stroke_over_the_fill() {
    let at = |x: f32, y: f32| Point { x, y };
    let mut square = Path::new();
    square
        .move_to(at(8.0, 8.0))
        .line_to(at(56.0, 8.0))
        .line_to(at(56.0, 56.0))
        .line_to(at(8.0, 56.0))
        .close()
        // Wound the other way: a hole.
        .move_to(at(24.0, 24.0))
        .line_to(at(24.0, 40.0))
        .line_to(at(40.0, 40.0))
        .line_to(at(40.0, 24.0))
        .close();
    let mut renderer = renderer();
    let pixels = render(
        &mut renderer,
        1.0,
        vec![Primitive::Path {
            path: square,
            fill: Some(RED),
            stroke: Some(Stroke {
                width: 4.0,
                color: BLUE,
            }),
        }],
    );
    assert_near(pixel(&pixels, 16, 16), [255, 0, 0, 255]);
    assert_near(pixel(&pixels, 32, 32), WHITE);
    assert_near(pixel(&pixels, 2, 2), WHITE);
    // The outline is centred on the edges, outside and around the hole.
    assert_near(pixel(&pixels, 7, 32), [0, 0, 255, 255]);
    assert_near(pixel(&pixels, 9, 32), [0, 0, 255, 255]);
    assert_near(pixel(&pixels, 25, 32), [0, 0, 255, 255]);
    // Mitred: the corner is square, not cut.
    assert_near(pixel(&pixels, 6, 6), [0, 0, 255, 255]);
}

#[test]
fn frames_painted_in_part_draw_what_a_whole_frame_would_and_do_not_hoard_text() {
    // A line of text and a stroked path inside the damage, and one of each
    // outside it, which a frame painted in part has no need to look at.
    let scene = |round: usize| {
        let mut tick = Path::new();
        tick.move_to(Point { x: 4.0, y: 20.0 })
            .line_to(Point { x: 10.0, y: 26.0 })
            .line_to(Point { x: 22.0, y: 12.0 });
        let stroke = Stroke {
            width: 2.0,
            color: BLUE,
        };
        Scene {
            size: Size::new(SIDE as f32, SIDE as f32),
            primitives: vec![
                Primitive::Text {
                    rect: rect(2.0, 0.0, 60.0, 12.0),
                    text: format!("r{round}"),
                    font_size: 10.0,
                    color: RED,
                },
                Primitive::Path {
                    path: tick.clone(),
                    fill: None,
                    stroke: Some(stroke),
                },
                Primitive::Text {
                    rect: rect(2.0, 48.0, 60.0, 12.0),
                    text: "still".into(),
                    font_size: 10.0,
                    color: RED,
                },
                Primitive::Path {
                    path: tick.translated(Point { x: 30.0, y: 20.0 }),
                    fill: None,
                    stroke: Some(stroke),
                },
            ],
        }
    };
    let size = [SIDE as u32; 2];
    // The top half changes every frame; the bottom half never does.
    let damage = [rect(0.0, 0.0, SIDE as f32, 32.0)];
    let mut partial = renderer();
    partial.render(size, 1.0, &scene(0), Color::WHITE);
    let mut most = 0;
    for round in 1..=1200 {
        let painted = partial
            .render_damaged(size, 1.0, &scene(round), Color::WHITE, &damage)
            .to_vec();
        most = most.max(partial.text_system().buffers.len());
        if round % 150 == 0 {
            let whole = renderer()
                .render(size, 1.0, &scene(round), Color::WHITE)
                .to_vec();
            assert!(painted == whole, "round {round} differs from a whole frame");
        }
    }
    // 1,200 different texts went by. The cache was let grow for a while and
    // then cleared by a frame painted whole, more than once.
    assert!(most < 700, "the text cache reached {most}");
    assert!(partial.text_system().buffers.len() < 700);
}

fn text_at(y: f32, text: &str, font_size: f32) -> Primitive {
    Primitive::Text {
        rect: rect(1.0, y, 60.0, 12.0),
        text: text.into(),
        font_size,
        color: RED,
    }
}

fn styled_at(y: f32, text: &str, style: &TextStyle, width: f32) -> Primitive {
    Primitive::StyledText {
        rect: rect(1.0, y, width, 12.0),
        text: text.into(),
        style: style.clone(),
        color: BLUE,
    }
}

fn rich_at(y: f32, spans: &[TextSpan], style: &TextStyle, width: f32) -> Primitive {
    Primitive::RichText {
        rect: rect(1.0, y, width, 12.0),
        spans: spans.to_vec(),
        style: style.clone(),
        color: BLUE,
    }
}

/// A small style, so that a few lines fit the target.
fn small(edit: impl FnOnce(&mut TextStyle)) -> TextStyle {
    let mut style = TextStyle {
        font_size: 8.0,
        ..TextStyle::default()
    };
    edit(&mut style);
    style
}

fn wrapped(edit: impl FnOnce(&mut TextStyle)) -> TextStyle {
    small(|style| {
        style.wrap = TextWrap::Word;
        edit(style);
    })
}

fn keys_of(renderer: &mut CpuRenderer) -> HashSet<TextKey> {
    renderer.text_system().buffers.keys().cloned().collect()
}

/// Shapes the text of every primitive the way painting does.
fn shape_all(system: &mut TextSystem, primitives: &[Primitive]) {
    for primitive in primitives {
        match primitive {
            Primitive::Text {
                text, font_size, ..
            } => {
                system.ensure(text, *font_size);
            }
            Primitive::StyledText {
                rect, text, style, ..
            } => {
                system.ensure_styled(text, style, Some(rect.width));
            }
            Primitive::RichText {
                rect, spans, style, ..
            } => {
                system.ensure_rich(spans, style, Some(rect.width));
            }
            _ => {}
        }
    }
}

/// Paints a whole frame with `kept`, which has painted before, and with a
/// renderer that has not: what is drawn, counted and shaped must be the same.
/// Pixels cannot tell the fonts apart, for there is only one, so it is the
/// set of shaped texts that shows a text asked for wrongly.
#[track_caller]
fn assert_like_fresh(kept: &mut CpuRenderer, scale: f32, primitives: &[Primitive]) {
    let pixels = render(kept, scale, primitives.to_vec());
    let mut fresh = renderer();
    let expected = render(&mut fresh, scale, primitives.to_vec());
    assert!(pixels == expected, "the pixels differ from a fresh frame's");
    let (stats, fresh_stats) = (kept.stats(), fresh.stats());
    assert_eq!(stats.glyphs, fresh_stats.glyphs);
    assert_eq!(stats.redrawn, fresh_stats.redrawn);
    assert_eq!(stats.layers, fresh_stats.layers);
    assert_eq!(keys_of(kept), keys_of(&mut fresh));
    assert_eq!(
        kept.text_system().buffers.len(),
        fresh.text_system().buffers.len()
    );
}

#[test]
fn identical_whole_frames_repeat_exactly() {
    let spans = [
        TextSpan::new("two ").weight(700),
        TextSpan::new("spans").italic(true),
    ];
    let bold = small(|style| style.weight = 700);
    let scene = [
        text_at(0.0, "plain", 8.0),
        styled_at(8.0, "bold", &bold, 60.0),
        styled_at(16.0, "wrapped words that wrap", &wrapped(|_| {}), 30.0),
        styled_at(
            24.0,
            "right to left",
            &wrapped(|style| style.rtl = true),
            50.0,
        ),
        rich_at(32.0, &spans, &wrapped(|_| {}), 50.0),
        // The same texts again, which share their shaped blocks.
        text_at(40.0, "plain", 8.0),
        styled_at(48.0, "bold", &bold, 60.0),
        rich_at(56.0, &spans, &wrapped(|_| {}), 50.0),
    ];
    let mut kept = renderer();
    for _ in 0..4 {
        assert_like_fresh(&mut kept, 1.0, &scene);
    }
    assert_eq!(kept.text_system().buffers.len(), 5);
    assert!(kept.stats().glyphs > 0);
}

#[test]
fn a_changed_input_gets_its_own_buffer() {
    let spans = |edit: fn(&mut [TextSpan])| {
        let mut spans = vec![TextSpan::new("ab"), TextSpan::new("c")];
        edit(&mut spans);
        spans
    };
    let rich = |spans: &[TextSpan]| rich_at(2.0, spans, &wrapped(|_| {}), 40.0);
    let styled = |style: &TextStyle, width: f32| styled_at(2.0, "ab ab ab", style, width);
    let base_style = wrapped(|_| {});
    // Each case is a text, then the same with one input changed.
    let cases: Vec<(&str, Primitive, Primitive)> = vec![
        (
            "plain text",
            text_at(2.0, "ab", 14.0),
            text_at(2.0, "ac", 14.0),
        ),
        (
            "plain size",
            text_at(2.0, "ab", 14.0),
            text_at(2.0, "ab", 15.0),
        ),
        (
            "plain size to the least",
            text_at(2.0, "ab", 1.0),
            text_at(2.0, "ab", 0.5),
        ),
        (
            "styled text",
            styled(&base_style, 40.0),
            styled_at(2.0, "ab ab ac", &base_style, 40.0),
        ),
        (
            "styled size",
            styled(&base_style, 40.0),
            styled(&wrapped(|s| s.font_size = 9.0), 40.0),
        ),
        (
            "family, generic",
            styled(&base_style, 40.0),
            styled(&wrapped(|s| s.family = FontFamily::Serif), 40.0),
        ),
        (
            "family, monospace",
            styled(&base_style, 40.0),
            styled(&wrapped(|s| s.family = FontFamily::Monospace), 40.0),
        ),
        (
            "family, named",
            styled(&wrapped(|s| s.family = FontFamily::Named("a".into())), 40.0),
            styled(&wrapped(|s| s.family = FontFamily::Named("b".into())), 40.0),
        ),
        (
            "weight",
            styled(&base_style, 40.0),
            styled(&wrapped(|s| s.weight = 700), 40.0),
        ),
        (
            "weight past the limit",
            styled(&wrapped(|s| s.weight = 900), 40.0),
            styled(&wrapped(|s| s.weight = 950), 40.0),
        ),
        (
            "italic",
            styled(&base_style, 40.0),
            styled(&wrapped(|s| s.italic = true), 40.0),
        ),
        (
            "wrap",
            styled(&base_style, 40.0),
            styled(&small(|_| {}), 40.0),
        ),
        (
            "right to left",
            styled(&base_style, 60.0),
            styled(&wrapped(|s| s.rtl = true), 60.0),
        ),
        (
            "base direction, none to left",
            styled(&base_style, 40.0),
            styled(&wrapped(|s| s.base = Some(false)), 40.0),
        ),
        (
            "base direction, left to right",
            styled(&wrapped(|s| s.base = Some(false)), 40.0),
            styled(&wrapped(|s| s.base = Some(true)), 40.0),
        ),
        (
            "width of a wrapped text",
            styled(&base_style, 40.0),
            styled(&base_style, 50.0),
        ),
        (
            "width of a text that does not wrap",
            styled(&small(|_| {}), 40.0),
            styled(&small(|_| {}), 50.0),
        ),
        (
            "width of a wrapped text to the least",
            styled(&base_style, 1.0),
            styled(&base_style, 0.25),
        ),
        (
            "rich text, text of a span",
            rich(&spans(|_| {})),
            rich(&spans(|spans| spans[1].text = "d".into())),
        ),
        (
            "rich text, colour",
            rich(&spans(|_| {})),
            rich(&spans(|spans| spans[0].color = Some(RED))),
        ),
        (
            "rich text, weight",
            rich(&spans(|_| {})),
            rich(&spans(|spans| spans[0].weight = Some(700))),
        ),
        (
            "rich text, italic",
            rich(&spans(|_| {})),
            rich(&spans(|spans| spans[1].italic = Some(true))),
        ),
        (
            "rich text, family",
            rich(&spans(|_| {})),
            rich(&spans(|spans| spans[0].family = Some(FontFamily::Serif))),
        ),
        (
            "rich text, a boundary moved",
            rich(&spans(|_| {})),
            rich(&[TextSpan::new("a"), TextSpan::new("bc")]),
        ),
        (
            "rich text, spans joined",
            rich(&spans(|_| {})),
            rich(&[TextSpan::new("abc")]),
        ),
        (
            "rich text, block weight",
            rich(&spans(|_| {})),
            rich_at(2.0, &spans(|_| {}), &wrapped(|s| s.weight = 700), 40.0),
        ),
        (
            "rich text, width",
            rich(&spans(|_| {})),
            rich_at(2.0, &spans(|_| {}), &base_style, 50.0),
        ),
        (
            "plain text to styled text",
            text_at(2.0, "ab", 14.0),
            styled_at(2.0, "ab", &small(|s| s.font_size = 15.0), 60.0),
        ),
        (
            "styled text to rich text",
            styled_at(2.0, "abc", &base_style, 40.0),
            rich(&spans(|_| {})),
        ),
    ];
    for (name, base, changed) in cases {
        let mut kept = renderer();
        for primitive in [&base, &changed, &base, &base, &changed, &changed] {
            assert_like_fresh(&mut kept, 1.0, std::slice::from_ref(primitive));
            assert_eq!(kept.text_system().buffers.len(), 1, "{name}");
        }
    }
}

/// A small generator, so that the walks below are the same on every run.
struct Walk(u64);

impl Walk {
    fn below(&mut self, count: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize % count
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())].clone()
    }

    fn style_field(&mut self, style: &mut TextStyle) {
        match self.below(7) {
            0 => style.font_size = self.pick(&[8.0, 9.0, 1.0, 0.5]),
            1 => {
                style.family = self.pick(&[
                    FontFamily::SansSerif,
                    FontFamily::Serif,
                    FontFamily::Monospace,
                    FontFamily::Named("x".into()),
                ]);
            }
            2 => style.weight = self.pick(&[400, 700]),
            3 => style.italic = self.pick(&[false, true]),
            4 => style.wrap = self.pick(&[TextWrap::None, TextWrap::Word]),
            5 => style.rtl = self.pick(&[false, true]),
            _ => style.base = self.pick(&[None, Some(false), Some(true)]),
        }
    }

    fn width(&mut self) -> f32 {
        self.pick(&[24.0, 40.0, 0.25])
    }

    fn spans(&mut self) -> Vec<TextSpan> {
        self.pick(&[
            vec![TextSpan::new("ab"), TextSpan::new("c")],
            vec![TextSpan::new("a"), TextSpan::new("bc")],
            vec![TextSpan::new("abc")],
            vec![TextSpan::new("ab").weight(700), TextSpan::new("c")],
            vec![TextSpan::new("ab").italic(true), TextSpan::new("c")],
            vec![TextSpan::new("ab").color(BLUE), TextSpan::new("c")],
            vec![
                TextSpan::new("ab").family(FontFamily::Serif),
                TextSpan::new("c"),
            ],
            vec![TextSpan::new("ab"), TextSpan::new("c").color(RED)],
        ])
    }

    fn text(&mut self) -> &'static str {
        self.pick(&["ab", "ac", "a b c d"])
    }

    /// A text of any kind and any inputs.
    fn text_at(&mut self, y: f32) -> Primitive {
        let mut style = small(|_| {});
        for _ in 0..3 {
            self.style_field(&mut style);
        }
        match self.below(3) {
            0 => text_at(y, self.text(), self.pick(&[8.0, 9.0, 1.0, 0.5])),
            1 => styled_at(y, self.text(), &style, self.width()),
            _ => rich_at(y, &self.spans(), &style, self.width()),
        }
    }

    /// `primitive` with one of its inputs changed.
    fn edit(&mut self, primitive: &mut Primitive) {
        match primitive {
            Primitive::Text {
                text, font_size, ..
            } => match self.below(2) {
                0 => *text = self.text().into(),
                _ => *font_size = self.pick(&[8.0, 9.0, 1.0, 0.5]),
            },
            Primitive::StyledText {
                rect, text, style, ..
            } => match self.below(3) {
                0 => *text = self.text().into(),
                1 => rect.width = self.width(),
                _ => self.style_field(style),
            },
            Primitive::RichText {
                rect, spans, style, ..
            } => match self.below(3) {
                0 => *spans = self.spans(),
                1 => rect.width = self.width(),
                _ => self.style_field(style),
            },
            _ => {}
        }
    }
}

#[test]
fn a_walk_over_field_combinations_matches_a_fresh_renderer() {
    let mut walk = Walk(29939);
    let mut kept = renderer();
    let mut history: Vec<Vec<Primitive>> = Vec::new();
    for frame in 0..300 {
        let scene: Vec<Primitive> = (0..6)
            .map(|slot| {
                let y = slot as f32 * 10.0;
                let before = history.last().map(|scene| scene[slot].clone());
                let earlier = history
                    .len()
                    .checked_sub(2)
                    .map(|back| history[back][slot].clone());
                match (walk.below(8), before, earlier) {
                    // As it was, or as it was two frames ago.
                    (0..=2, Some(before), _) => before,
                    (3, _, Some(earlier)) => earlier,
                    // One input changed.
                    (4..=6, Some(mut before), _) => {
                        walk.edit(&mut before);
                        before
                    }
                    _ => walk.text_at(y),
                }
            })
            .collect();
        assert_like_fresh(&mut kept, 1.0, &scene);
        assert!(kept.stats().frames == frame + 1);
        history.push(scene);
    }
}

#[test]
fn texts_that_come_go_and_move() {
    let [a, b, c, d] =
        ["alpha", "beta", "gamma", "delta"].map(|name| (name, text_at(0.0, name, 8.0)));
    let at = |y: f32, text: &(&str, Primitive)| text_at(y, text.0, 8.0);
    let frame = |texts: &[&(&str, Primitive)]| -> Vec<Primitive> {
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| at(index as f32 * 10.0, text))
            .collect()
    };
    let frames = [
        frame(&[&a, &b, &c]),
        frame(&[&a, &b]),
        frame(&[]),
        frame(&[&a, &b, &c, &d]),
        frame(&[&a, &b, &c]),
        // One in front, and every other one a place on.
        frame(&[&d, &a, &b, &c]),
        // Two swapped.
        frame(&[&d, &b, &a, &c]),
        // The last one gone.
        frame(&[&d, &b, &a]),
        frame(&[&d, &b, &a]),
        // The first one gone.
        frame(&[&b, &a]),
        frame(&[&b, &a, &b, &a]),
        frame(&[&c]),
    ];
    let mut kept = renderer();
    for frame in &frames {
        assert_like_fresh(&mut kept, 1.0, frame);
    }
    assert_eq!(kept.text_system().buffers.len(), 1);
}

#[test]
fn a_text_changed_into_a_copy_of_another_text_leaves_no_old_buffer() {
    let [a, b, c] = ["alpha", "beta", "gamma"].map(|name| text_at(0.0, name, 8.0));
    let place = |texts: &[&Primitive]| -> Vec<Primitive> {
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| {
                let mut text = (*text).clone();
                if let Primitive::Text { rect, .. } = &mut text {
                    rect.y = index as f32 * 10.0;
                }
                text
            })
            .collect()
    };
    let mut kept = renderer();
    assert_like_fresh(&mut kept, 1.0, &place(&[&a, &b]));
    // Nothing is shaped that was not, and the count is the one it was.
    assert_like_fresh(&mut kept, 1.0, &place(&[&a, &a]));
    assert_eq!(kept.text_system().buffers.len(), 1);
    assert_like_fresh(&mut kept, 1.0, &place(&[&a, &b, &c]));
    assert_like_fresh(&mut kept, 1.0, &place(&[&a, &c, &c]));
    assert_eq!(kept.text_system().buffers.len(), 2);
    assert_like_fresh(&mut kept, 1.0, &place(&[&c, &c, &c]));
    assert_eq!(kept.text_system().buffers.len(), 1);
}

#[test]
fn a_text_and_a_styled_text_with_one_key_share_it() {
    let plain = text_at(2.0, "x", 14.0);
    let styled = styled_at(2.0, "x", &small(|style| style.font_size = 14.0), 60.0);
    let mut kept = renderer();
    for text in [&plain, &styled, &plain, &plain, &styled, &styled, &plain] {
        assert_like_fresh(&mut kept, 1.0, std::slice::from_ref(text));
        assert_eq!(kept.text_system().buffers.len(), 1);
    }
    // Sizes under the least one are the least one.
    for size in [0.5, 1.0, 0.0, 1.0, 0.5, 0.25] {
        let scene = [
            text_at(2.0, "x", size),
            styled_at(14.0, "x", &small(|style| style.font_size = size), 60.0),
        ];
        assert_like_fresh(&mut kept, 1.0, &scene);
        assert_eq!(kept.text_system().buffers.len(), 1);
    }
}

#[test]
fn a_partial_frame_that_adds_text_is_cleaned_by_the_next_whole_frame() {
    let scene = |first: &str| vec![text_at(2.0, first, 8.0), text_at(40.0, "still", 8.0)];
    let size = [SIDE as u32; 2];
    let damage = [rect(0.0, 0.0, SIDE as f32, 20.0)];
    let mut kept = renderer();
    assert_like_fresh(&mut kept, 1.0, &scene("one"));
    assert_like_fresh(&mut kept, 1.0, &scene("one"));
    for first in ["two", "three"] {
        let changed = Scene {
            size: Size::new(SIDE as f32, SIDE as f32),
            primitives: scene(first),
        };
        kept.render_damaged(size, 1.0, &changed, Color::WHITE, &damage);
    }
    // The texts the frames painted in part shaped are still in the cache.
    assert_eq!(kept.text_system().buffers.len(), 4);
    assert_like_fresh(&mut kept, 1.0, &scene("one"));
    assert_eq!(kept.text_system().buffers.len(), 2);
    assert_like_fresh(&mut kept, 1.0, &scene("one"));
    // A partial frame that shapes nothing new leaves the cache as it was, and
    // the whole frame after it has nothing to clean.
    let same = Scene {
        size: Size::new(SIDE as f32, SIDE as f32),
        primitives: scene("one"),
    };
    kept.render_damaged(size, 1.0, &same, Color::WHITE, &damage);
    assert_eq!(kept.text_system().buffers.len(), 2);
    assert_like_fresh(&mut kept, 1.0, &scene("one"));
}

#[test]
fn the_text_cache_may_be_edited_between_whole_frames() {
    let scene = vec![
        text_at(0.0, "first", 8.0),
        styled_at(10.0, "second words", &wrapped(|_| {}), 40.0),
        rich_at(
            20.0,
            &[TextSpan::new("th").weight(700), TextSpan::new("ird")],
            &wrapped(|_| {}),
            40.0,
        ),
        text_at(30.0, "first", 8.0),
    ];
    type Edit = fn(&mut CpuRenderer, &[Primitive]);
    let edits: [(&str, Edit); 9] = [
        ("cleared", |renderer, _| {
            renderer.text_system().buffers.clear()
        }),
        ("one active text removed", |renderer, _| {
            let key = first_key(renderer);
            renderer.text_system().buffers.remove(&key);
        }),
        ("one active text removed, a stray put in", |renderer, _| {
            let key = first_key(renderer);
            renderer.text_system().buffers.remove(&key);
            stray(renderer.text_system());
        }),
        ("a stray put in", |renderer, _| {
            stray(renderer.text_system())
        }),
        ("replaced by an empty system", |renderer, _| {
            *renderer.text_system() = fira_system();
        }),
        (
            "replaced by one with the active texts",
            |renderer, scene| {
                let mut system = fira_system();
                shape_all(&mut system, scene);
                *renderer.text_system() = system;
            },
        ),
        (
            "replaced by one with the active texts and a stray",
            |renderer, scene| {
                let mut system = fira_system();
                shape_all(&mut system, scene);
                stray(&mut system);
                *renderer.text_system() = system;
            },
        ),
        (
            "replaced by one with some of the active texts and strays",
            |renderer, scene| {
                let mut system = fira_system();
                shape_all(&mut system, &scene[..1]);
                system.ensure("stray", 9.0);
                system.ensure("another stray", 9.0);
                *renderer.text_system() = system;
            },
        ),
        (
            "a text shaped by hand that the frame paints",
            |renderer, _| {
                renderer.text_system().buffers.clear();
                renderer.text_system().ensure("second words", 8.0);
            },
        ),
    ];
    for (name, edit) in edits {
        let mut kept = renderer();
        assert_like_fresh(&mut kept, 1.0, &scene);
        assert_like_fresh(&mut kept, 1.0, &scene);
        edit(&mut kept, &scene);
        assert_like_fresh(&mut kept, 1.0, &scene);
        assert!(kept.stats().glyphs > 0, "{name}");
        assert_like_fresh(&mut kept, 1.0, &scene);
    }
}

fn stray(system: &mut TextSystem) {
    system.ensure("stray", 9.0);
}

/// One of the texts in the cache, the same one each time.
fn first_key(renderer: &mut CpuRenderer) -> TextKey {
    let mut keys: Vec<TextKey> = keys_of(renderer).into_iter().collect();
    keys.sort_by_key(|key| format!("{key:?}"));
    keys.remove(0)
}

fn fira_system() -> TextSystem {
    TextSystem::with_fonts([include_bytes!(
        "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()])
}

#[test]
fn the_first_whole_frame_cleans_a_text_system_that_has_strays() {
    let scene = vec![
        text_at(0.0, "one", 8.0),
        styled_at(10.0, "two", &small(|_| {}), 40.0),
    ];
    let with_strays = || {
        let mut system = fira_system();
        shape_all(&mut system, &scene[..1]);
        system.ensure("stray", 9.0);
        system.ensure("another stray", 9.0);
        CpuRenderer::with_text_system(system)
    };
    let mut kept = with_strays();
    assert_like_fresh(&mut kept, 1.0, &scene);
    assert_like_fresh(&mut kept, 1.0, &scene);
    let mut bare = with_strays();
    assert_like_fresh(&mut bare, 1.0, &[]);
    assert!(bare.text_system().buffers.is_empty());
    // A text that is shaped already stays, and the strays go.
    let mut shaped = with_strays();
    assert_like_fresh(&mut shaped, 1.0, &scene[..1]);
    assert_eq!(shaped.text_system().buffers.len(), 1);
}

/// Cannot be drawn: painting it unwinds.
struct Unpaintable;
impl VectorImage for Unpaintable {
    fn size(&self) -> Size {
        Size::new(10.0, 10.0)
    }
    fn rasterize(&self, _: u32, _: u32) -> Vec<u8> {
        panic!("this image cannot be drawn");
    }
}

#[test]
fn a_whole_frame_that_unwinds_leaves_the_next_one_to_clean_the_cache() {
    let broken = Primitive::Image {
        rect: rect(40.0, 40.0, 10.0, 10.0),
        image: Image::from_vector(Unpaintable),
        source: rect(0.0, 0.0, 10.0, 10.0),
        tint: None,
        radius: 0.0,
    };
    let before = [text_at(0.0, "alpha", 8.0), text_at(10.0, "beta", 8.0)];
    let after = [text_at(0.0, "alpha", 8.0), text_at(10.0, "alpha", 8.0)];
    // The frame unwinds before its first text, between them and after both.
    // After both, the second text has been asked for as it will be again and
    // the cache still holds `beta`, at the size the frame before left it.
    for place in 0..=2 {
        let mut kept = renderer();
        assert_like_fresh(&mut kept, 1.0, &before);
        assert_eq!(kept.text_system().buffers.len(), 2);
        let mut scene = after.to_vec();
        scene.insert(place, broken.clone());
        let result = catch_unwind(AssertUnwindSafe(|| render(&mut kept, 1.0, scene)));
        assert!(result.is_err(), "the frame did not unwind");
        assert_like_fresh(&mut kept, 1.0, &after);
        assert_eq!(kept.text_system().buffers.len(), 1, "{place}");
        assert_like_fresh(&mut kept, 1.0, &after);
    }
}

#[test]
fn odd_floats_behave_like_a_fresh_renderer() {
    let sizes = [
        f32::NAN,
        f32::INFINITY,
        -0.0,
        0.0,
        0.5,
        16.0,
        f32::NEG_INFINITY,
    ];
    let widths = [f32::NAN, 0.0, -1.0, f32::INFINITY, 30.0];
    let mut kept = renderer();
    for round in 0..3 {
        for (at, size) in sizes.iter().enumerate() {
            let y = 4.0 + at as f32 * 8.0;
            let style = wrapped(|style| style.font_size = *size);
            let scene = [
                text_at(0.0, "plain", *size),
                styled_at(
                    y,
                    "styled words",
                    &style,
                    widths[(at + round) % widths.len()],
                ),
                styled_at(
                    y,
                    "styled words",
                    &small(|style| style.font_size = *size),
                    widths[at % widths.len()],
                ),
                rich_at(
                    y,
                    &[TextSpan::new("rich "), TextSpan::new("words")],
                    &style,
                    widths[(at + 2 * round) % widths.len()],
                ),
            ];
            // Twice, which is the case that a text is asked for as it was.
            assert_like_fresh(&mut kept, 1.0, &scene);
            assert_like_fresh(&mut kept, 1.0, &scene);
        }
    }
}

#[test]
fn whole_frames_at_other_sizes_and_scales_match_a_fresh_renderer() {
    let scene = |width: f32| {
        vec![
            styled_at(
                0.0,
                "words that wrap in a narrow block",
                &wrapped(|_| {}),
                width,
            ),
            text_at(36.0, "plain", 8.0),
        ]
    };
    let mut kept = renderer();
    assert_like_fresh(&mut kept, 1.0, &scene(30.0));
    assert_like_fresh(&mut kept, 2.0, &scene(30.0));
    assert_like_fresh(&mut kept, 2.0, &scene(30.0));
    assert_like_fresh(&mut kept, 1.0, &scene(30.0));
    assert_like_fresh(&mut kept, 1.0, &scene(45.0));
    assert_like_fresh(&mut kept, 0.5, &scene(45.0));
    assert_like_fresh(&mut kept, 1.0, &scene(30.0));
}

#[test]
fn frames_in_part_may_grow_the_cache_by_what_the_last_whole_frame_kept() {
    // Three hundred texts below the damage, and one inside it that changes.
    let scene = |round: usize| Scene {
        size: Size::new(SIDE as f32, SIDE as f32),
        primitives: std::iter::once(text_at(2.0, &format!("r{round}"), 8.0))
            .chain((0..300).map(|n| text_at(40.0, &n.to_string(), 8.0)))
            .collect(),
    };
    let size = [SIDE as u32; 2];
    let damage = [rect(0.0, 0.0, SIDE as f32, 20.0)];
    let mut kept = renderer();
    kept.render(size, 1.0, &scene(0), Color::WHITE);
    assert_eq!(kept.text_system().buffers.len(), 301);
    // A few texts more are well within twice what the whole frame kept, so
    // the frames after it are painted in part as well.
    for round in 1..=3 {
        kept.render_damaged(size, 1.0, &scene(round), Color::WHITE, &damage);
        assert_eq!(kept.stats().redrawn, SIDE as u64 * 20, "round {round}");
        assert_eq!(kept.text_system().buffers.len(), 301 + round);
    }
}
