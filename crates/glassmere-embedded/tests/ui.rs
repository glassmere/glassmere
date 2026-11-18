//! A real UI, declared with glassmere-core and measured with the embedded
//! fonts, painted into a 320x240 RGB565 buffer.
//!
//! `EMBEDDED_DUMP=<dir> cargo test -p glassmere-embedded --test ui` also
//! writes the frame as `<dir>/ui.ppm`.
mod support;

use glassmere_core::{Context, Progress, Style, Ui};
use glassmere_embedded::{
    EmbeddedMeasurer, EmbeddedRenderer, Framebuffer, Gray8, Mono, Pixel, Rgb565, Rgb888, Xrgb8888,
};
use glassmere_scene::{Color, Primitive, Rect, Scene, Size};

const SIZE: Size = Size::new(320.0, 240.0);
const WIDTH: usize = 320;
const HEIGHT: usize = 240;
const BACKGROUND: Color = Color(15, 23, 42, 255);
const SENTINEL: Rgb565 = Rgb565(0xF81F);

fn app(ui: &mut Ui<'_>) {
    ui.column(Style::default().gap(6.0), |ui| {
        ui.label("Glassmere on a microcontroller");
        ui.button("Press me");
        ui.checkbox("Enabled", true, |_| ());
        ui.slider("Volume", 0.4, 0.0..=1.0, 0.01, |_| ());
        ui.progress(Progress::new("Loading"), Some(0.6));
    });
}

fn scene() -> Scene {
    let mut context = Context::new();
    context.run(SIZE, &mut EmbeddedMeasurer::default(), app);
    context.scene().clone()
}

fn background() -> Rgb565 {
    Rgb565::from_rgb([BACKGROUND.0, BACKGROUND.1, BACKGROUND.2])
}

fn render(scene: &Scene) -> Vec<Rgb565> {
    let mut pixels = vec![SENTINEL; WIDTH * HEIGHT];
    let mut frame = Framebuffer::new(&mut pixels, WIDTH as u32, HEIGHT as u32);
    EmbeddedRenderer::default().render(scene, &mut frame, Some(BACKGROUND));
    pixels
}

fn at(pixels: &[Rgb565], x: usize, y: usize) -> Rgb565 {
    pixels[y * WIDTH + x]
}

/// The first pixel where two frames differ.
fn first_difference(a: &[Rgb565], b: &[Rgb565]) -> Option<(usize, usize, Rgb565, Rgb565)> {
    a.iter()
        .zip(b)
        .position(|(a, b)| a != b)
        .map(|at| (at % WIDTH, at / WIDTH, a[at], b[at]))
}

fn texts(scene: &Scene) -> Vec<(Rect, &str)> {
    scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text { rect, text, .. } => Some((*rect, text.as_str())),
            _ => None,
        })
        .collect()
}

#[test]
fn the_ui_paints_its_text_and_its_button() {
    let scene = scene();
    let frame = render(&scene);
    support::dump("ui", WIDTH as u32, HEIGHT as u32, |x, y| {
        at(&frame, x as usize, y as usize).to_rgb()
    });

    // Every piece of text leaves ink: its rectangle differs from the same
    // frame painted without any text.
    let mut bare = scene.clone();
    bare.primitives
        .retain(|primitive| !matches!(primitive, Primitive::Text { .. }));
    let without = render(&bare);
    let texts = texts(&scene);
    assert_eq!(texts.len(), 4);
    for (rect, text) in &texts {
        let inked = (rect.y as usize..(rect.y + rect.height) as usize)
            .flat_map(|y| (rect.x as usize..(rect.x + rect.width) as usize).map(move |x| (x, y)))
            .filter(|&(x, y)| at(&frame, x, y) != at(&without, x, y))
            .count();
        assert!(
            inked >= text.chars().count() * 3,
            "{text:?} has {inked} pixels of ink"
        );
    }

    // The button is the quad drawn before its label, in the colour it was given.
    let label = scene
        .primitives
        .iter()
        .position(
            |primitive| matches!(primitive, Primitive::Text { text, .. } if text == "Press me"),
        )
        .unwrap();
    let (rect, color) = scene.primitives[..label]
        .iter()
        .rev()
        .find_map(|primitive| match primitive {
            Primitive::Quad { rect, color, .. } => Some((*rect, *color)),
            _ => None,
        })
        .unwrap();
    let expected = Rgb565::from_rgb([color.0, color.1, color.2]);
    assert_ne!(expected, background());
    let (x, y) = ((rect.x + rect.width / 2.0) as usize, rect.y as usize + 4);
    assert_eq!(at(&frame, x, y), expected);
    // Its corner is rounded: the very corner pixel is still the background.
    assert_eq!(at(&frame, rect.x as usize, rect.y as usize), background());
    // The track of the slider is filled up to its value and no further.
    let (slider_y, fill, track) = (169, 60, 200);
    assert_ne!(at(&frame, fill, slider_y), at(&frame, track, slider_y));
}

#[test]
fn nothing_is_painted_outside_a_clip() {
    let full = render(&scene());
    let clip = Rect {
        x: 20.0,
        y: 20.0,
        width: 150.5,
        height: 100.0,
    };
    let mut clipped = scene();
    clipped.primitives.insert(0, Primitive::PushClip(clip));
    clipped.primitives.push(Primitive::PopClip);
    // Over a frame that already holds the background, nothing else may change.
    let mut pixels = vec![background(); WIDTH * HEIGHT];
    let mut frame = Framebuffer::new(&mut pixels, WIDTH as u32, HEIGHT as u32);
    EmbeddedRenderer::default().render(&clipped, &mut frame, None);
    let (mut inside, mut outside_ink) = (0, 0);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let in_clip = (20..171).contains(&x) && (20..120).contains(&y);
            if in_clip {
                assert_eq!(at(&pixels, x, y), at(&full, x, y), "({x}, {y}) inside");
                inside += usize::from(at(&full, x, y) != background());
            } else {
                assert_eq!(at(&pixels, x, y), background(), "({x}, {y}) outside");
                outside_ink += usize::from(at(&full, x, y) != background());
            }
        }
    }
    // The clip hides things, and leaves things: the test says something.
    assert!(inside > 100, "{inside}");
    assert!(outside_ink > 100, "{outside_ink}");
}

#[test]
fn bands_are_the_pixels_of_the_whole_frame() {
    let scene = scene();
    let full = render(&scene);
    let mut assembled = vec![SENTINEL; WIDTH * HEIGHT];
    let mut band = vec![SENTINEL; WIDTH * 30];
    let mut renderer = EmbeddedRenderer::default();
    let mut frame = Framebuffer::new(&mut band, WIDTH as u32, 30);
    for index in 0..8 {
        frame.set_origin(0, 30 * index);
        renderer.render(&scene, &mut frame, Some(BACKGROUND));
        let start = WIDTH * 30 * index as usize;
        assembled[start..start + WIDTH * 30].copy_from_slice(frame.pixels());
    }
    assert_eq!(first_difference(&assembled, &full), None);
}

#[test]
fn a_damage_rectangle_repaints_only_itself() {
    let scene = scene();
    let full = render(&scene);
    let damage = Rect {
        x: 50.4,
        y: 30.2,
        width: 130.0,
        height: 90.0,
    };
    let mut pixels = vec![SENTINEL; WIDTH * HEIGHT];
    let mut frame = Framebuffer::new(&mut pixels, WIDTH as u32, HEIGHT as u32);
    EmbeddedRenderer::default().render_damage(&scene, &mut frame, damage, Some(BACKGROUND));
    // The pixels the rectangle touches: 50..181 by 30..121.
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let inside = (50..181).contains(&x) && (30..121).contains(&y);
            let expected = if inside { at(&full, x, y) } else { SENTINEL };
            assert_eq!(at(&pixels, x, y), expected, "({x}, {y})");
        }
    }
}

#[test]
fn a_buffer_for_one_rectangle_matches_the_frame() {
    // A buffer that holds just the damaged rectangle, placed where it belongs.
    let scene = scene();
    let full = render(&scene);
    let mut pixels = vec![SENTINEL; 100 * 40];
    let mut frame = Framebuffer::new(&mut pixels, 100, 40).at(30, 120);
    let mut renderer = EmbeddedRenderer::default();
    renderer.render(&scene, &mut frame, Some(BACKGROUND));
    for y in 0..40 {
        for x in 0..100 {
            assert_eq!(
                frame.pixel(30 + x, 120 + y),
                Some(at(&full, 30 + x as usize, 120 + y as usize)),
                "({x}, {y})"
            );
        }
    }
}

/// How many pixels of the UI, painted in format `P`, are not the background.
fn painted<P: Pixel + PartialEq>(scene: &Scene) -> usize {
    let mut pixels = vec![P::from_rgb([0; 3]); WIDTH * HEIGHT];
    let mut frame = Framebuffer::new(&mut pixels, WIDTH as u32, HEIGHT as u32);
    EmbeddedRenderer::default().render(scene, &mut frame, Some(BACKGROUND));
    let background = P::from_rgb([BACKGROUND.0, BACKGROUND.1, BACKGROUND.2]);
    pixels.iter().filter(|pixel| **pixel != background).count()
}

#[test]
fn every_pixel_format_gets_the_ui() {
    let scene = scene();
    let rgb565 = painted::<Rgb565>(&scene);
    assert!(rgb565 > 2000, "{rgb565}");
    // The formats that keep the colours agree on what is not background.
    assert_eq!(painted::<Rgb888>(&scene), painted::<Xrgb8888>(&scene));
    for (name, count) in [
        ("Gray8", painted::<Gray8>(&scene)),
        ("Mono", painted::<Mono>(&scene)),
    ] {
        assert!(count > 300, "{name} has {count} pixels of ink");
    }
}
