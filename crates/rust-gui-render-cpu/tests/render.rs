use rust_gui_render_cpu::CpuRenderer;
use rust_gui_scene::{
    Affine, Color, Image, Primitive, Rect, Scene, Size, TextMeasurer, TextStyle, VectorImage,
};
use rust_gui_text::TextSystem;

const SIDE: usize = 64;
const WHITE: [u8; 4] = [255; 4];
const RED: Color = Color(255, 0, 0, 255);
const BLUE: Color = Color(0, 0, 255, 255);

fn renderer() -> CpuRenderer {
    // Only the bundled font, so that the pixels are the same on every machine.
    CpuRenderer::with_text_system(TextSystem::with_fonts([include_bytes!(
        "../../rust-gui-render-wgpu/fonts/FiraMono-Regular.ttf"
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
