//! The two renderers on the same scene. They are not meant to agree to the
//! bit: the GPU feathers the edges of quads and filters resampled images in
//! linear light, the CPU covers edges exactly and filters on sRGB-encoded
//! values. They are meant to draw the same picture, and this states how close
//! that is.
use glassmere_render_cpu::CpuRenderer;
use glassmere_render_wgpu::Renderer;
use glassmere_scene::{
    Affine, Brush, Cap, Color, Dash, Fill, FillRule, GradientStop, Image, Join, Path, Point,
    Primitive, Rect, Scene, Size, Stroke, TextStyle, VectorImage,
};
use glassmere_text::TextSystem;

const SIDE: u32 = 256;

fn fonts() -> TextSystem {
    TextSystem::with_fonts([include_bytes!(
        "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()])
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

struct Rings;
impl VectorImage for Rings {
    fn size(&self) -> Size {
        Size::new(32.0, 32.0)
    }
    fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let (dx, dy) = (
                    x as f32 / width as f32 - 0.5,
                    y as f32 / height as f32 - 0.5,
                );
                let ring = ((dx.hypot(dy) * 16.0) as u32).is_multiple_of(2);
                pixels.extend(if ring { [200, 60, 20, 255] } else { [0; 4] });
            }
        }
        pixels
    }
}

/// A dark page with everything a scene can hold.
fn scene(scale: f32) -> Scene {
    // Opaque at the top and nearly clear at the bottom, so that translucent
    // pixels are laid over the page as well as over the card.
    let mut gradient = Vec::new();
    for y in 0..16u32 {
        for x in 0..16u32 {
            gradient.extend([(x * 16) as u8, (y * 16) as u8, 160, (255 - y * 16) as u8]);
        }
    }
    let photo = Image::from_rgba8(16, 16, gradient).unwrap();
    let style = TextStyle {
        font_size: 15.0,
        ..TextStyle::default()
    };
    let side = SIDE as f32 / scale;
    Scene {
        size: Size::new(side, side),
        primitives: vec![
            Primitive::Quad {
                rect: rect(8.0, 8.0, 120.0, 60.0),
                color: Color::rgb(24, 35, 49),
                radius: 10.0,
            },
            Primitive::StyledText {
                rect: rect(16.0, 14.0, 110.0, 20.0),
                text: "Renderers".into(),
                style: style.clone(),
                color: Color::rgb(230, 237, 243),
            },
            Primitive::Text {
                rect: rect(16.0, 38.0, 110.0, 20.0),
                text: "agree 0123".into(),
                font_size: 13.0,
                color: Color::rgb(103, 232, 206),
            },
            Primitive::Quad {
                rect: rect(100.0, 40.0, 60.0, 60.0),
                color: Color(103, 232, 206, 140),
                radius: 30.0,
            },
            Primitive::Image {
                rect: rect(140.0, 8.0, 56.0, 40.0),
                image: photo.clone(),
                source: rect(0.0, 0.0, 16.0, 16.0),
                tint: None,
                radius: 8.0,
            },
            Primitive::Image {
                rect: rect(8.0, 80.0, 48.0, 48.0),
                image: Image::from_vector(Rings),
                source: rect(0.0, 0.0, 32.0, 32.0),
                tint: None,
                radius: 0.0,
            },
            Primitive::PushClip(rect(64.0, 110.0, 80.0, 50.0)),
            Primitive::Quad {
                rect: rect(50.0, 100.0, 140.0, 40.0),
                color: Color::rgb(22, 105, 99),
                radius: 12.0,
            },
            Primitive::StyledText {
                rect: rect(70.0, 112.0, 200.0, 20.0),
                text: "clipped text runs off".into(),
                style,
                color: Color::WHITE,
            },
            Primitive::PopClip,
            Primitive::PushLayer {
                bounds: rect(110.0, 110.0, 70.0, 50.0),
                opacity: 0.6,
                transform: Affine::translate(-145.0, -135.0)
                    .then(Affine::rotate(17.0))
                    .then(Affine::translate(150.0, 150.0)),
            },
            Primitive::Quad {
                rect: rect(110.0, 110.0, 70.0, 50.0),
                color: Color::rgb(200, 80, 60),
                radius: 6.0,
            },
            Primitive::Image {
                rect: rect(118.0, 118.0, 30.0, 30.0),
                image: photo,
                source: rect(4.0, 4.0, 8.0, 8.0),
                tint: Some(Color::WHITE),
                radius: 0.0,
            },
            Primitive::PopLayer,
            // A ramp and a shadow, which both renderers approximate the same
            // way and which are the easiest thing to get subtly wrong.
            Primitive::Shadow {
                rect: rect(196.0, 24.0, 40.0, 30.0),
                radius: 8.0,
                color: Color(0, 0, 0, 200),
                blur: 6.0,
                spread: 2.0,
                offset: Point { x: 3.0, y: 4.0 },
                inset: false,
            },
            Primitive::Gradient {
                rect: rect(196.0, 24.0, 40.0, 30.0),
                radius: 8.0,
                from: Point { x: 0.0, y: 0.0 },
                to: Point { x: 1.0, y: 1.0 },
                stops: vec![
                    GradientStop {
                        at: 0.0,
                        color: Color::rgb(240, 200, 90),
                    },
                    GradientStop {
                        at: 0.45,
                        color: Color(120, 40, 200, 180),
                    },
                    GradientStop {
                        at: 1.0,
                        color: Color::rgb(30, 160, 210),
                    },
                ],
                radial: false,
            },
            Primitive::Gradient {
                rect: rect(196.0, 150.0, 48.0, 48.0),
                radius: 24.0,
                from: Point { x: 0.5, y: 0.5 },
                to: Point { x: 1.0, y: 0.5 },
                stops: vec![
                    GradientStop {
                        at: 0.0,
                        color: Color::rgb(255, 255, 255),
                    },
                    GradientStop {
                        at: 1.0,
                        color: Color(0, 0, 0, 0),
                    },
                ],
                radial: true,
            },
            Primitive::Shadow {
                rect: rect(20.0, 190.0, 60.0, 40.0),
                radius: 10.0,
                color: Color(0, 0, 0, 160),
                blur: 5.0,
                spread: 0.0,
                offset: Point { x: 0.0, y: 0.0 },
                inset: true,
            },
        ],
    }
}

fn gpu(scene: &Scene, scale: f32, clear: Color) -> Vec<u8> {
    pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("graphics adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("comparison target"),
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut renderer = Renderer::with_text_system(&device, &queue, format, fonts());
        renderer
            .render(
                &device,
                &queue,
                &texture.create_view(&Default::default()),
                [SIDE; 2],
                scale,
                scene,
                clear,
            )
            .unwrap();
        // SIDE * 4 is a multiple of 256, so rows need no padding.
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("comparison readback"),
            size: u64::from(SIDE * SIDE * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIDE * 4),
                    rows_per_image: Some(SIDE),
                },
            },
            texture.size(),
        );
        queue.submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::channel();
        output
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap()
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let pixels = output
            .slice(..)
            .get_mapped_range()
            .expect("the readback buffer is mapped")
            .to_vec();
        output.unmap();
        pixels
    })
}

#[test]
#[ignore = "requires a graphics adapter"]
fn the_cpu_and_the_gpu_draw_the_same_picture_within_a_stated_tolerance() {
    let clear = Color::rgb(16, 23, 34);
    for scale in [1.0, 1.25, 2.0] {
        let scene = scene(scale);
        let reference = gpu(&scene, scale, clear);
        let mut renderer = CpuRenderer::with_text_system(fonts());
        let first = renderer.render([SIDE; 2], scale, &scene, clear).to_vec();
        // The same scene into the same reused target gives the same bytes.
        let pixels = renderer.render([SIDE; 2], scale, &scene, clear);
        assert_eq!(pixels, first, "not deterministic");
        assert_eq!(pixels.len(), reference.len());
        // COMPARE_DUMP=<directory> keeps both pictures as raw RGBA for a look.
        if let Some(directory) = std::env::var_os("COMPARE_DUMP") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::write(directory.join(format!("cpu-{scale}.rgba")), pixels).unwrap();
            std::fs::write(directory.join(format!("gpu-{scale}.rgba")), &reference).unwrap();
        }

        agree(pixels, &reference, scale);
    }
}

fn agree(pixels: &[u8], reference: &[u8], scale: f32) {
    let count = (SIDE * SIDE) as f64;
    let worst = |at: usize| {
        (0..3)
            .map(|channel| pixels[at * 4 + channel].abs_diff(reference[at * 4 + channel]))
            .max()
            .unwrap()
    };
    let differences: Vec<u8> = (0..(SIDE * SIDE) as usize).map(worst).collect();
    let mean = differences.iter().map(|d| f64::from(*d)).sum::<f64>() / count;
    let share = |over: u8| differences.iter().filter(|d| **d > over).count() as f64 / count;
    println!(
        "scale {scale}: mean {mean:.3}, over 8: {:.2}%, over 32: {:.2}%, over 96: {:.3}%, max {}",
        share(8) * 100.0,
        share(32) * 100.0,
        share(96) * 100.0,
        differences.iter().max().unwrap()
    );
    // What differs is edges: a line of pixels around shapes and glyphs.
    // Flat areas agree, large errors are rare, and nothing is misplaced,
    // which would show as whole regions far apart. Measured on this
    // machine: a mean of 0.16 to 0.60, at most 0.44% of pixels over 32.
    // The bounds leave room for another adapter's filtering.
    assert!(mean < 1.2, "mean difference {mean}");
    assert!(
        share(8) < 0.04,
        "{:.2}% of pixels differ by more than 8",
        share(8) * 100.0
    );
    assert!(
        share(32) < 0.015,
        "{:.2}% of pixels differ by more than 32",
        share(32) * 100.0
    );
    assert!(
        share(96) < 0.004,
        "{:.3}% of pixels differ by more than 96",
        share(96) * 100.0
    );
}

/// Fills with curves, a hole and sharp corners; strokes open and closed,
/// mitred and bevelled, translucent and thinner than a pixel.
fn paths() -> Scene {
    let at = |x: f32, y: f32| Point { x, y };
    let mut star = Path::new();
    for index in 0..10 {
        let angle = std::f32::consts::PI * index as f32 / 5.0;
        let reach = if index % 2 == 0 { 50.0 } else { 20.0 };
        let point = at(64.0 + reach * angle.sin(), 64.0 - reach * angle.cos());
        if index == 0 {
            star.move_to(point);
        } else {
            star.line_to(point);
        }
    }
    star.close();
    let mut ring = Path::new();
    ring.move_to(at(140.0, 64.0))
        .cubic_to(at(140.0, 30.0), at(240.0, 30.0), at(240.0, 64.0))
        .cubic_to(at(240.0, 98.0), at(140.0, 98.0), at(140.0, 64.0))
        .close()
        .move_to(at(170.0, 64.0))
        .quad_to(at(190.0, 84.0), at(210.0, 64.0))
        .quad_to(at(190.0, 44.0), at(170.0, 64.0))
        .close();
    let mut wave = Path::new();
    wave.move_to(at(20.0, 160.0))
        .cubic_to(at(60.0, 110.0), at(100.0, 210.0), at(140.0, 160.0))
        .quad_to(at(180.0, 120.0), at(236.0, 150.0));
    let mut zigzag = Path::new();
    zigzag
        .move_to(at(20.0, 240.0))
        .line_to(at(60.0, 200.0))
        .line_to(at(70.0, 240.0))
        .line_to(at(80.0, 200.0))
        .close();
    let mut hair = Path::new();
    hair.move_to(at(100.0, 205.0)).line_to(at(236.0, 245.0));
    Scene {
        size: Size::new(SIDE as f32, SIDE as f32),
        primitives: vec![
            Primitive::Path {
                path: star,
                fill: Some(Color::rgb(250, 204, 21).into()),
                stroke: Some(Stroke {
                    width: 3.0,
                    color: Color::rgb(180, 83, 9),
                    ..Stroke::default()
                }),
            },
            Primitive::Path {
                path: ring,
                fill: Some(Color(56, 189, 248, 200).into()),
                stroke: None,
            },
            Primitive::Path {
                path: wave,
                fill: None,
                stroke: Some(Stroke {
                    width: 6.0,
                    color: Color(244, 114, 182, 180),
                    ..Stroke::default()
                }),
            },
            Primitive::Path {
                path: zigzag,
                fill: None,
                stroke: Some(Stroke {
                    width: 4.0,
                    color: Color::rgb(255, 255, 255),
                    ..Stroke::default()
                }),
            },
            Primitive::Path {
                path: hair,
                fill: None,
                stroke: Some(Stroke {
                    width: 0.5,
                    color: Color::rgb(255, 255, 255),
                    ..Stroke::default()
                }),
            },
        ],
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn paths_look_the_same_on_both_renderers() {
    let clear = Color::rgb(16, 23, 34);
    for scale in [1.0, 1.25, 2.0] {
        let scene = paths();
        let reference = gpu(&scene, scale, clear);
        let mut renderer = CpuRenderer::new();
        let pixels = renderer.render([SIDE; 2], scale, &scene, clear).to_vec();
        if let Some(directory) = std::env::var_os("COMPARE_DUMP") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::write(directory.join(format!("cpu-paths-{scale}.rgba")), &pixels).unwrap();
            std::fs::write(
                directory.join(format!("gpu-paths-{scale}.rgba")),
                &reference,
            )
            .unwrap();
        }
        agree(&pixels, &reference, scale);
    }
}

/// Joins and caps of every kind, a dashed curve and a dotted ring, an
/// even-odd fill and linear and radial gradient fills.
fn path_styles() -> Scene {
    let at = |x: f32, y: f32| Point { x, y };
    let corner = |x: f32, y: f32| {
        let mut path = Path::new();
        path.move_to(at(x, y + 40.0))
            .line_to(at(x + 16.0, y))
            .line_to(at(x + 36.0, y + 40.0));
        path
    };
    let mut primitives = Vec::new();
    for (index, (join, cap)) in [
        (Join::Miter, Cap::Butt),
        (Join::Round, Cap::Round),
        (Join::Bevel, Cap::Square),
    ]
    .into_iter()
    .enumerate()
    {
        primitives.push(Primitive::Path {
            path: corner(14.0 + 56.0 * index as f32, 12.0),
            fill: None,
            stroke: Some(Stroke {
                width: 10.0,
                color: Color::rgb(250, 204, 21),
                join,
                cap,
                ..Stroke::default()
            }),
        });
    }
    let mut wave = Path::new();
    wave.move_to(at(14.0, 100.0))
        .cubic_to(at(60.0, 60.0), at(100.0, 140.0), at(150.0, 100.0))
        .quad_to(at(180.0, 80.0), at(240.0, 110.0));
    primitives.push(Primitive::Path {
        path: wave,
        fill: None,
        stroke: Some(Stroke {
            width: 5.0,
            color: Color::rgb(244, 114, 182),
            cap: Cap::Round,
            dash: Some(Dash {
                lengths: vec![12.0, 8.0],
                offset: 3.0,
            }),
            ..Stroke::default()
        }),
    });
    let mut dotted = Path::new();
    dotted
        .move_to(at(60.0, 130.0))
        .line_to(at(108.0, 130.0))
        .line_to(at(108.0, 178.0))
        .line_to(at(60.0, 178.0))
        .close();
    primitives.push(Primitive::Path {
        path: dotted,
        fill: None,
        stroke: Some(Stroke {
            width: 6.0,
            color: Color::rgb(255, 255, 255),
            join: Join::Round,
            cap: Cap::Round,
            dash: Some(Dash::new([0.0, 12.0])),
            ..Stroke::default()
        }),
    });
    let mut star = Path::new();
    for index in 0..5 {
        let angle = std::f32::consts::PI * 4.0 * index as f32 / 5.0;
        let point = at(190.0 + 40.0 * angle.sin(), 160.0 - 40.0 * angle.cos());
        if index == 0 {
            star.move_to(point);
        } else {
            star.line_to(point);
        }
    }
    star.close();
    primitives.push(Primitive::Path {
        path: star,
        fill: Some(Fill {
            rule: FillRule::EvenOdd,
            ..Color::rgb(56, 189, 248).into()
        }),
        stroke: None,
    });
    let stops = vec![
        GradientStop {
            at: 0.0,
            color: Color::rgb(239, 68, 68),
        },
        GradientStop {
            at: 0.5,
            color: Color::rgb(250, 204, 21),
        },
        GradientStop {
            at: 1.0,
            color: Color::rgb(37, 99, 235),
        },
    ];
    let mut leaf = Path::new();
    leaf.move_to(at(14.0, 240.0))
        .cubic_to(at(14.0, 190.0), at(70.0, 190.0), at(120.0, 190.0))
        .cubic_to(at(120.0, 240.0), at(70.0, 250.0), at(14.0, 240.0))
        .close();
    primitives.push(Primitive::Path {
        path: leaf,
        fill: Some(Fill {
            brush: Brush::Linear {
                from: at(0.0, 0.0),
                to: at(1.0, 1.0),
                stops: stops.clone(),
            },
            rule: FillRule::NonZero,
        }),
        stroke: Some(Stroke {
            width: 2.0,
            color: Color::rgb(255, 255, 255),
            ..Stroke::default()
        }),
    });
    let mut disc = Path::new();
    disc.move_to(at(160.0, 215.0))
        .cubic_to(at(160.0, 190.0), at(240.0, 190.0), at(240.0, 215.0))
        .cubic_to(at(240.0, 245.0), at(160.0, 245.0), at(160.0, 215.0))
        .close();
    primitives.push(Primitive::Path {
        path: disc,
        fill: Some(Fill {
            brush: Brush::Radial {
                from: at(0.5, 0.5),
                to: at(1.0, 0.5),
                stops,
            },
            rule: FillRule::NonZero,
        }),
        stroke: None,
    });
    Scene {
        size: Size::new(SIDE as f32, SIDE as f32),
        primitives,
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn path_styles_look_the_same_on_both_renderers() {
    let clear = Color::rgb(16, 23, 34);
    for scale in [1.0, 1.25, 2.0] {
        let scene = path_styles();
        let reference = gpu(&scene, scale, clear);
        let mut renderer = CpuRenderer::new();
        let pixels = renderer.render([SIDE; 2], scale, &scene, clear).to_vec();
        if let Some(directory) = std::env::var_os("COMPARE_DUMP") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::write(
                directory.join(format!("cpu-path-styles-{scale}.rgba")),
                &pixels,
            )
            .unwrap();
            std::fs::write(
                directory.join(format!("gpu-path-styles-{scale}.rgba")),
                &reference,
            )
            .unwrap();
        }
        agree(&pixels, &reference, scale);
    }
}

/// Where a stroke is nearly nothing: dashes of no length and under a pixel,
/// strokes that go nowhere, a line closed on itself, and a pattern too fine
/// to cut.
fn path_edges() -> Scene {
    let at = |x: f32, y: f32| Point { x, y };
    let line = |y: f32| {
        let mut path = Path::new();
        path.move_to(at(16.0, y)).line_to(at(240.0, y));
        path
    };
    let white = Color::rgb(255, 255, 255);
    let styled = |path: Path, width: f32, cap, dash: Option<Dash>| Primitive::Path {
        path,
        fill: None,
        stroke: Some(Stroke {
            width,
            color: white,
            cap,
            dash,
            ..Stroke::default()
        }),
    };
    let mut primitives = vec![
        styled(line(20.0), 6.0, Cap::Butt, Some(Dash::new([0.0, 14.0]))),
        styled(line(40.0), 6.0, Cap::Round, Some(Dash::new([0.0, 14.0]))),
        styled(line(60.0), 6.0, Cap::Square, Some(Dash::new([0.0, 14.0]))),
        styled(line(80.0), 4.0, Cap::Butt, Some(Dash::new([0.4, 0.6]))),
        styled(line(100.0), 4.0, Cap::Butt, Some(Dash::new([1.5, 1.5]))),
        styled(
            line(120.0),
            4.0,
            Cap::Butt,
            Some(Dash {
                lengths: vec![9.0, 5.0],
                offset: -23.0,
            }),
        ),
        styled(line(140.0), 4.0, Cap::Butt, Some(Dash::new([0.01, 0.01]))),
    ];
    for (index, cap) in [Cap::Butt, Cap::Round, Cap::Square].into_iter().enumerate() {
        let x = 30.0 + 40.0 * index as f32;
        let mut point = Path::new();
        point.move_to(at(x, 170.0)).line_to(at(x, 170.0));
        primitives.push(styled(point, 12.0, cap, None));
        let mut closed = Path::new();
        closed.move_to(at(x, 200.0)).close();
        primitives.push(styled(closed, 12.0, cap, None));
    }
    for (index, join) in [Join::Miter, Join::Round, Join::Bevel]
        .into_iter()
        .enumerate()
    {
        let x = 150.0 + 30.0 * index as f32;
        let mut closed = Path::new();
        closed.move_to(at(x, 160.0)).line_to(at(x, 200.0)).close();
        primitives.push(Primitive::Path {
            path: closed,
            fill: None,
            stroke: Some(Stroke {
                width: 10.0,
                color: white,
                join,
                cap: Cap::Square,
                ..Stroke::default()
            }),
        });
    }
    Scene {
        size: Size::new(SIDE as f32, SIDE as f32),
        primitives,
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn path_edges_look_the_same_on_both_renderers() {
    let clear = Color::rgb(16, 23, 34);
    for scale in [1.0, 1.25, 2.0] {
        let scene = path_edges();
        let reference = gpu(&scene, scale, clear);
        let mut renderer = CpuRenderer::new();
        let pixels = renderer.render([SIDE; 2], scale, &scene, clear).to_vec();
        if let Some(directory) = std::env::var_os("COMPARE_DUMP") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::write(
                directory.join(format!("cpu-path-edges-{scale}.rgba")),
                &pixels,
            )
            .unwrap();
            std::fs::write(
                directory.join(format!("gpu-path-edges-{scale}.rgba")),
                &reference,
            )
            .unwrap();
        }
        agree(&pixels, &reference, scale);
    }
}
