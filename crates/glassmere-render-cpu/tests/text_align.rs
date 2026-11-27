//! The lines of wrapped text are drawn where their alignment puts them, by
//! the real shaper and the CPU renderer.
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{Color, Primitive, Rect, Scene, Size, TextAlign, TextStyle, TextWrap};
use glassmere_text::TextSystem;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 90;

fn renderer() -> CpuRenderer {
    CpuRenderer::with_text_system(TextSystem::with_fonts([include_bytes!(
        "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()]))
}

/// The columns that have ink in them, and where the ink's weight sits on average.
fn ink(align: TextAlign) -> (u32, u32, f32) {
    let scene = Scene {
        size: Size::new(WIDTH as f32, HEIGHT as f32),
        primitives: vec![Primitive::StyledText {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
            },
            text: "one two three four five six seven eight".into(),
            style: TextStyle {
                wrap: TextWrap::Word,
                align,
                ..TextStyle::default()
            },
            color: Color(0, 0, 0, 255),
        }],
    };
    let pixels = renderer()
        .render([WIDTH, HEIGHT], 1.0, &scene, Color::WHITE)
        .to_vec();
    let (mut first, mut last, mut weight, mut total) = (WIDTH, 0, 0.0_f64, 0.0_f64);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let at = ((y * WIDTH + x) * 4) as usize;
            let dark = 255.0 - f64::from(pixels[at]);
            if dark > 64.0 {
                first = first.min(x);
                last = last.max(x);
            }
            weight += dark * f64::from(x);
            total += dark;
        }
    }
    (first, last, (weight / total) as f32)
}

#[test]
fn start_centre_and_end_move_the_ink_across_the_box() {
    let start = ink(TextAlign::Start);
    let center = ink(TextAlign::Center);
    let end = ink(TextAlign::End);
    assert!(start.0 <= 3, "starts at the left edge: {start:?}");
    assert!(center.2 > start.2 + 4.0, "{center:?} against {start:?}");
    assert!(end.2 > center.2 + 4.0, "{end:?} against {center:?}");
    assert!(end.1 >= WIDTH - 4, "reaches the right edge: {end:?}");
}

#[test]
fn justified_lines_reach_both_edges() {
    let (first, last, _) = ink(TextAlign::Justify);
    assert!(first <= 3, "{first}");
    assert!(last >= WIDTH - 4, "{last}");
}
