//! Caps and joins of a stroked path, as the CPU renderer draws them.
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{Color, LineCap, LineJoin, Path, Point, Primitive, Scene, Size, Stroke};

const SIDE: usize = 64;

fn ink(path: Path, width: f32) -> f32 {
    let scene = Scene {
        size: Size::new(SIDE as f32, SIDE as f32),
        primitives: vec![Primitive::Path {
            path,
            fill: None,
            stroke: Some(Stroke {
                width,
                color: Color(0, 0, 0, 255),
            }),
        }],
    };
    let pixels = CpuRenderer::new()
        .render([SIDE as u32; 2], 1.0, &scene, Color::WHITE)
        .to_vec();
    pixels
        .chunks_exact(4)
        .map(|pixel| (255.0 - f32::from(pixel[0])) / 255.0)
        .sum()
}

fn line() -> Path {
    let mut line = Path::new();
    line.move_to(Point { x: 14.0, y: 32.0 })
        .line_to(Point { x: 34.0, y: 32.0 });
    line
}

fn square() -> Path {
    let mut path = Path::new();
    path.move_to(Point { x: 20.0, y: 20.0 })
        .line_to(Point { x: 40.0, y: 20.0 })
        .line_to(Point { x: 40.0, y: 40.0 })
        .line_to(Point { x: 20.0, y: 40.0 })
        .close();
    path
}

#[test]
fn caps_finish_an_open_line_by_their_shape() {
    let butt = ink(line(), 6.0);
    let square_cap = ink(line().with_cap(LineCap::Square), 6.0);
    let round = ink(line().with_cap(LineCap::Round), 6.0);
    // 20 by 6; half the width longer at both ends; a half circle at each.
    assert!((butt - 120.0).abs() < 1.5, "{butt}");
    assert!((square_cap - 156.0).abs() < 1.5, "{square_cap}");
    // The curve's edge is anti-aliased, which takes about a quarter of a pixel
    // off its radius: a little under the exact half circles.
    let caps = std::f32::consts::PI * 9.0;
    assert!(
        round > butt + caps * 0.8 && round < butt + caps * 1.02,
        "{round} against {butt} and two half discs of {caps}"
    );
}

#[test]
fn joins_finish_a_corner_by_their_shape() {
    let miter = ink(square(), 4.0);
    let bevel = ink(square().with_join(LineJoin::Bevel), 4.0);
    let round = ink(square().with_join(LineJoin::Round), 4.0);
    // The band between 24 by 24 and 16 by 16; a bevel takes 2 off each
    // corner and a round join what is outside a quarter circle.
    assert!((miter - 320.0).abs() < 2.0, "{miter}");
    assert!((bevel - 312.0).abs() < 2.0, "{bevel}");
    assert!(
        bevel < round && round < miter,
        "{bevel} < {round} < {miter}"
    );
}
