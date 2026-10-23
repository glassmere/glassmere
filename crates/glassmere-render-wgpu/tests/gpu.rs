use glassmere_render_wgpu::{CustomDraw, Renderer, TextSystem, wgpu};
use glassmere_scene::{
    Affine, Color, FontFamily, GradientStop, Image, Point, Primitive, Rect, Scene, Size,
    TextMeasurer, TextSpan, TextStyle, TextWrap, VectorImage,
};

/// GPU tests take turns. The test harness starts one thread per core, and many
/// devices created at the same moment in one process stall some drivers: with
/// eighteen tests on a 24-core machine the Direct3D 12 runs hung about half the
/// time, and never with four threads. An app creates one device.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn turn() -> std::sync::MutexGuard<'static, ()> {
    // A failed test must not take the others down with a poisoned lock.
    TURN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Requires a hardware or software graphics adapter. Run explicitly on a machine
/// with graphics drivers: cargo test -p glassmere-render-wgpu --test gpu -- --ignored
#[test]
#[ignore = "requires a graphics adapter"]
fn renders_ordered_clipped_quads_and_shaped_text_at_two_x_scale() {
    let _turn = turn();
    pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("graphics adapter");
        eprintln!("GPU smoke adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                memory_hints: wgpu::MemoryHints::Manual {
                    suballocated_device_memory_block_size: 4 * 1024 * 1024..16 * 1024 * 1024,
                },
                required_limits: wgpu::Limits {
                    max_non_sampler_bindings: 4096,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await
            .unwrap();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("smoke target"),
            size: wgpu::Extent3d {
                width: 128,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb);
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        let scene = Scene {
            size: Size::new(64.0, 64.0),
            primitives: vec![
                Primitive::Quad {
                    rect: rect(0.0, 0.0, 64.0, 64.0),
                    color: Color::rgb(200, 0, 0),
                    radius: 0.0,
                },
                Primitive::PushClip(rect(4.0, 4.0, 16.0, 16.0)),
                Primitive::Quad {
                    rect: rect(0.0, 0.0, 64.0, 64.0),
                    color: Color::rgb(0, 180, 0),
                    radius: 0.0,
                },
                Primitive::PopClip,
                Primitive::PushClip(rect(0.0, 32.0, 16.0, 32.0)),
                Primitive::Text {
                    rect: rect(0.0, 32.0, 64.0, 32.0),
                    text: "Hello".into(),
                    font_size: 16.0,
                    color: Color::WHITE,
                },
                Primitive::PopClip,
                Primitive::Text {
                    rect: rect(32.0, 0.0, 32.0, 24.0),
                    text: "X".into(),
                    font_size: 16.0,
                    color: Color::WHITE,
                },
                // This quad must cover the previous X: renderers cannot batch all text last.
                Primitive::Quad {
                    rect: rect(32.0, 0.0, 32.0, 24.0),
                    color: Color::rgb(0, 0, 220),
                    radius: 0.0,
                },
            ],
        };
        renderer
            .render(
                &device,
                &queue,
                &texture.create_view(&Default::default()),
                [128, 128],
                2.0,
                &scene,
                Color::rgb(0, 0, 0),
            )
            .unwrap();
        for _ in 0..4 {
            renderer
                .render_cached(
                    &device,
                    &queue,
                    &texture.create_view(&Default::default()),
                    [128, 128],
                    2.0,
                    &scene,
                    Color::rgb(0, 0, 0),
                    false,
                    &[],
                )
                .unwrap();
        }
        assert_eq!(
            renderer.stats().preparations,
            1,
            "exposure frames reuse prepared geometry and glyphs"
        );
        assert_eq!(renderer.stats().quad_buffer_allocations, 1);
        assert_eq!(renderer.stats().text_batches, 1);
        assert_eq!(renderer.stats().text_renderer_allocations, 1);
        // A changed scene and a DPI change each invalidate prepared data, while
        // updates that fit the existing quad buffer do not allocate another one.
        for (scale, changed) in [(2.0, true), (1.0, false), (2.0, false)] {
            renderer
                .render_cached(
                    &device,
                    &queue,
                    &texture.create_view(&Default::default()),
                    [128, 128],
                    scale,
                    &scene,
                    Color::rgb(0, 0, 0),
                    changed,
                    &[],
                )
                .unwrap();
        }
        assert_eq!(renderer.stats().preparations, 4);
        assert_eq!(renderer.stats().quad_buffer_allocations, 1);
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("smoke readback"),
            size: 128 * 128 * 4,
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
                    bytes_per_row: Some(512),
                    rows_per_image: Some(128),
                },
            },
            texture.size(),
        );
        queue.submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::channel();
        output
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let data = output
            .slice(..)
            .get_mapped_range()
            .expect("the readback buffer is mapped");
        let pixel = |x: usize, y: usize| &data[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4];
        assert_eq!(pixel(2, 2), [200, 0, 0, 255]);
        assert_eq!(pixel(16, 16), [0, 180, 0, 255]);
        assert_eq!(pixel(44, 16), [200, 0, 0, 255]);
        for y in 2..46 {
            for x in 66..126 {
                assert_eq!(pixel(x, y), [0, 0, 220, 255]);
            }
        }
        for y in 64..128 {
            for x in 32..128 {
                assert_eq!(&pixel(x, y)[1..3], [0, 0], "text escaped its clip");
            }
        }
        let ink = (66..126)
            .flat_map(|y| (2..126).map(move |x| (x, y)))
            .filter(|(x, y)| pixel(*x, *y)[1] > 100)
            .count();
        assert!(
            ink > 100,
            "Expected visible shaped glyphs, found {ink} ink pixels"
        );
        drop(data);
        output.unmap();

        let style = TextStyle {
            font_size: 10.0,
            family: FontFamily::Monospace,
            weight: 700,
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let text = "AA AA AA AA";
        let size = renderer
            .text_system()
            .measure_styled(text, &style, Some(26.0));
        assert!(
            size.height >= 56.0,
            "four wrapped lines should share the measured line height"
        );
        let wrapped = Scene {
            size: Size::new(64.0, 64.0),
            primitives: vec![
                Primitive::PushClip(rect(8.0, 2.0, 26.0, 62.0)),
                Primitive::StyledText {
                    rect: rect(8.0, 2.0, 26.0, 62.0),
                    text: text.into(),
                    style,
                    color: Color::WHITE,
                },
                Primitive::PopClip,
            ],
        };
        renderer
            .render(
                &device,
                &queue,
                &texture.create_view(&Default::default()),
                [128, 128],
                2.0,
                &wrapped,
                Color::rgb(0, 0, 0),
            )
            .unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(512),
                    rows_per_image: Some(128),
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
        let data = output
            .slice(..)
            .get_mapped_range()
            .expect("the readback buffer is mapped");
        let pixel = |x: usize, y: usize| &data[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4];
        for line in 0..4 {
            let ink = (4 + line * 28..32 + line * 28)
                .flat_map(|y| (16..68).map(move |x| (x, y)))
                .filter(|(x, y)| pixel(*x, *y)[0] > 100)
                .count();
            assert!(ink > 40, "missing wrapped line {line}: {ink} pixels");
        }
        for y in 0..128 {
            for x in 0..128 {
                if !(16..68).contains(&x) || y < 4 {
                    assert_eq!(
                        pixel(x, y),
                        [0, 0, 0, 255],
                        "wrapped text escaped its bounds"
                    );
                }
            }
        }
    });
}

fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let padded = (width * 4).div_ceil(256) * 256;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: (padded * height) as u64,
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
                bytes_per_row: Some(padded as u32),
                rows_per_image: Some(height as u32),
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
    let data = output
        .slice(..)
        .get_mapped_range()
        .expect("the readback buffer is mapped");
    let pixels = data
        .chunks(padded)
        .flat_map(|row| row[..width * 4].iter().copied())
        .collect();
    drop(data);
    output.unmap();
    pixels
}

/// Uses only the bundled font, so the reference is stable across machines.
/// Regenerate with UPDATE_SNAPSHOTS=1 after intentional rendering changes.
#[test]
#[ignore = "requires a graphics adapter"]
fn pixel_snapshot_matches_reference_with_bundled_font() {
    const WIDTH: usize = 96;
    const HEIGHT: usize = 64;
    let _turn = turn();
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
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snapshot target"),
            size: wgpu::Extent3d {
                width: WIDTH as u32,
                height: HEIGHT as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let fonts =
            TextSystem::with_fonts([include_bytes!("../fonts/FiraMono-Regular.ttf").to_vec()]);
        let mut renderer =
            Renderer::with_text_system(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb, fonts);
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        let scene = Scene {
            size: Size::new(WIDTH as f32, HEIGHT as f32),
            primitives: vec![
                Primitive::Quad {
                    rect: rect(4.0, 4.0, 40.0, 24.0),
                    color: Color::rgb(22, 105, 99),
                    radius: 6.0,
                },
                Primitive::Quad {
                    rect: rect(52.0, 8.0, 36.0, 48.0),
                    color: Color::rgb(64, 48, 96),
                    radius: 10.0,
                },
                Primitive::PushClip(rect(0.0, 30.0, 48.0, 20.0)),
                Primitive::Text {
                    rect: rect(4.0, 28.0, 60.0, 30.0),
                    text: "Ag 1".into(),
                    font_size: 16.0,
                    color: Color::WHITE,
                },
                Primitive::PopClip,
            ],
        };
        renderer
            .render(
                &device,
                &queue,
                &texture.create_view(&Default::default()),
                [WIDTH as u32, HEIGHT as u32],
                1.0,
                &scene,
                Color::rgb(15, 23, 34),
            )
            .unwrap();
        let actual = readback(&device, &queue, &texture, WIDTH, HEIGHT);
        let ink = actual
            .chunks(4)
            .filter(|p| p[0] > 128 && p[1] > 128)
            .count();
        assert!(
            ink > 20,
            "expected shaped glyphs from the bundled font, got {ink} ink pixels"
        );

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
        let reference = dir.join("reference.rgba");
        let write = || {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&reference, &actual).unwrap();
            let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
            ppm.extend(actual.chunks(4).flat_map(|p| p[..3].iter().copied()));
            std::fs::write(dir.join("reference.ppm"), ppm).unwrap();
        };
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            write();
            return;
        }
        let Ok(expected) = std::fs::read(&reference) else {
            write();
            panic!(
                "pixel snapshot created at {}; review reference.ppm and rerun",
                reference.display()
            );
        };
        assert_eq!(expected.len(), actual.len(), "reference size changed");
        let close = expected
            .iter()
            .zip(&actual)
            .filter(|(a, b)| a.abs_diff(**b) <= 2)
            .count();
        let ratio = close as f64 / actual.len() as f64;
        assert!(
            ratio >= 0.995,
            "{:.2}% of channels within tolerance; set UPDATE_SNAPSHOTS=1 if the change is intended",
            ratio * 100.0
        );
    });
}

/// A 128×128 sRGB target and a bundled-font renderer for the layer tests.
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    renderer: Renderer,
    /// Released last, after the device is gone.
    _turn: std::sync::MutexGuard<'static, ()>,
}

const SIDE: usize = 128;
const RED: Color = Color(255, 0, 0, 255);
const BLUE: Color = Color(0, 0, 255, 255);

impl Gpu {
    fn new() -> Self {
        Self::with_features(wgpu::Features::empty())
    }

    /// Requests those of `wanted` the adapter has, as a host would.
    fn with_features(wanted: wgpu::Features) -> Self {
        let turn = turn();
        pollster::block_on(async {
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("graphics adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_features: adapter.features() & wanted,
                    ..Default::default()
                })
                .await
                .unwrap();
            let format = wgpu::TextureFormat::Rgba8UnormSrgb;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("layer target"),
                size: wgpu::Extent3d {
                    width: SIDE as u32,
                    height: SIDE as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let fonts =
                TextSystem::with_fonts([include_bytes!("../fonts/FiraMono-Regular.ttf").to_vec()]);
            let renderer = Renderer::with_text_system(&device, &queue, format, fonts);
            Self {
                device,
                queue,
                texture,
                renderer,
                _turn: turn,
            }
        })
    }

    fn render(&mut self, scale: f32, primitives: &[Primitive], changed: bool) -> Vec<u8> {
        self.damaged(scale, primitives, changed, &[])
    }

    /// A frame with the damage a host would name for it; empty means all of it.
    fn damaged(
        &mut self,
        scale: f32,
        primitives: &[Primitive],
        changed: bool,
        damage: &[Rect],
    ) -> Vec<u8> {
        let scene = Scene {
            size: Size::new(SIDE as f32 / scale, SIDE as f32 / scale),
            primitives: primitives.to_vec(),
        };
        self.renderer
            .render_cached(
                &self.device,
                &self.queue,
                &self.texture.create_view(&Default::default()),
                [SIDE as u32; 2],
                scale,
                &scene,
                Color::WHITE,
                changed,
                damage,
            )
            .unwrap();
        readback(&self.device, &self.queue, &self.texture, SIDE, SIDE)
    }
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn quad(rect: Rect, color: Color) -> Primitive {
    Primitive::Quad {
        rect,
        color,
        radius: 0.0,
    }
}

fn layer(bounds: Rect, opacity: f32, transform: Affine) -> Primitive {
    Primitive::PushLayer {
        bounds,
        opacity,
        transform,
    }
}

#[track_caller]
fn assert_pixel(pixels: &[u8], x: usize, y: usize, expected: [u8; 3]) {
    let actual = &pixels[(y * SIDE + x) * 4..][..3];
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 3),
        "pixel ({x}, {y}) is {actual:?}, expected {expected:?} ±3"
    );
}

/// sRGB encoding of a linear value, as the target stores it.
fn srgb(linear: f32) -> u8 {
    ((1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0).round() as u8
}

#[test]
#[ignore = "requires a graphics adapter"]
fn layer_opacity_applies_to_the_group_not_each_child() {
    let mut gpu = Gpu::new();
    let pixels = gpu.render(
        1.0,
        &[
            layer(rect(10.0, 10.0, 100.0, 100.0), 0.5, Affine::IDENTITY),
            quad(rect(20.0, 20.0, 60.0, 60.0), RED),
            quad(rect(50.0, 50.0, 50.0, 50.0), BLUE),
            Primitive::PopLayer,
        ],
        true,
    );
    let half = srgb(0.5);
    // Blending is linear: half of an opaque color over white.
    assert_pixel(&pixels, 30, 30, [255, half, half]);
    // Red must not show through the blue that covers it inside the group.
    assert_pixel(&pixels, 65, 65, [half, half, 255]);
    assert_pixel(&pixels, 90, 90, [half, half, 255]);
    assert_pixel(&pixels, 5, 5, [255; 3]);
    assert_pixel(&pixels, 105, 30, [255; 3]);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn layer_transform_moves_scales_and_rotates_the_group() {
    let mut gpu = Gpu::new();
    let (red, white) = ([255, 0, 0], [255; 3]);

    let bounds = rect(10.0, 10.0, 20.0, 20.0);
    let pixels = gpu.render(
        1.0,
        &[
            layer(bounds, 1.0, Affine::translate(30.0, 15.0)),
            quad(bounds, RED),
            Primitive::PopLayer,
        ],
        true,
    );
    for (x, y, expected) in [
        (50, 35, red),
        (41, 35, red),
        (20, 20, white),
        (38, 35, white),
    ] {
        assert_pixel(&pixels, x, y, expected);
    }

    // The same translation is twice as many physical pixels at 2x.
    let bounds = rect(5.0, 5.0, 10.0, 10.0);
    let pixels = gpu.render(
        2.0,
        &[
            layer(bounds, 1.0, Affine::translate(20.0, 10.0)),
            quad(bounds, RED),
            Primitive::PopLayer,
        ],
        true,
    );
    for (x, y, expected) in [
        (60, 40, red),
        (52, 40, red),
        (20, 20, white),
        (46, 40, white),
    ] {
        assert_pixel(&pixels, x, y, expected);
    }

    let center = Point { x: 64.0, y: 64.0 };
    let bounds = rect(24.0, 24.0, 80.0, 80.0);
    let pixels = gpu.render(
        1.0,
        &[
            layer(bounds, 1.0, Affine::scale(0.5, 0.5).about(center)),
            quad(bounds, RED),
            Primitive::PopLayer,
        ],
        true,
    );
    for (x, y, expected) in [
        (64, 64, red),
        (47, 47, red),
        (81, 81, red),
        (40, 40, white),
        (88, 88, white),
    ] {
        assert_pixel(&pixels, x, y, expected);
    }

    // A wide bar becomes a tall one.
    let bounds = rect(14.0, 54.0, 100.0, 20.0);
    let pixels = gpu.render(
        1.0,
        &[
            layer(bounds, 1.0, Affine::rotate(90.0).about(center)),
            quad(bounds, RED),
            Primitive::PopLayer,
        ],
        true,
    );
    for (x, y, expected) in [
        (64, 64, red),
        (64, 20, red),
        (64, 108, red),
        (20, 64, white),
        (108, 64, white),
    ] {
        assert_pixel(&pixels, x, y, expected);
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn nested_layers_multiply_their_opacity() {
    let mut gpu = Gpu::new();
    let pixels = gpu.render(
        1.0,
        &[
            layer(rect(10.0, 10.0, 100.0, 100.0), 0.5, Affine::IDENTITY),
            layer(rect(20.0, 20.0, 80.0, 80.0), 0.5, Affine::IDENTITY),
            quad(rect(30.0, 30.0, 40.0, 40.0), RED),
            Primitive::PopLayer,
            Primitive::PopLayer,
        ],
        true,
    );
    // A quarter of opaque red over white.
    assert_pixel(&pixels, 50, 50, [255, srgb(0.75), srgb(0.75)]);
    assert_pixel(&pixels, 25, 25, [255; 3]);
    assert_eq!(gpu.renderer.stats().layer_passes, 2);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn opaque_identity_layer_renders_inline() {
    let mut gpu = Gpu::new();
    let content = [
        Primitive::Quad {
            rect: rect(10.0, 10.0, 70.0, 50.0),
            color: RED,
            radius: 8.0,
        },
        Primitive::Quad {
            rect: rect(40.0, 40.0, 60.0, 60.0),
            color: Color(0, 0, 255, 160),
            radius: 12.0,
        },
    ];
    let plain = gpu.render(1.0, &content, true);
    let wrapped = |opacity| {
        let mut primitives = vec![layer(
            rect(0.0, 0.0, 128.0, 128.0),
            opacity,
            Affine::IDENTITY,
        )];
        primitives.extend(content.iter().cloned());
        primitives.push(Primitive::PopLayer);
        primitives
    };
    let inline = gpu.render(1.0, &wrapped(1.0), true);
    assert!(plain == inline, "an opaque identity layer changed pixels");
    assert_eq!(gpu.renderer.stats().layer_passes, 0);
    assert_eq!(gpu.renderer.stats().layer_textures, 0);

    gpu.render(1.0, &wrapped(0.5), true);
    assert_eq!(gpu.renderer.stats().layer_passes, 1);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn text_moves_with_a_translated_layer() {
    let mut gpu = Gpu::new();
    let bounds = rect(4.0, 4.0, 60.0, 24.0);
    let pixels = gpu.render(
        1.0,
        &[
            layer(bounds, 1.0, Affine::translate(40.0, 60.0)),
            Primitive::Text {
                rect: bounds,
                text: "Ag 1".into(),
                font_size: 16.0,
                color: Color::rgb(0, 0, 0),
            },
            Primitive::PopLayer,
        ],
        true,
    );
    let ink = |inside: bool| {
        (0..SIDE * SIDE)
            .filter(|i| {
                ((44..104).contains(&(i % SIDE)) && (64..88).contains(&(i / SIDE))) == inside
            })
            .filter(|i| pixels[i * 4] < 128)
            .count()
    };
    assert!(ink(true) > 20, "expected glyphs in the translated layer");
    assert_eq!(ink(false), 0, "text stayed at its untranslated position");
}

#[test]
#[ignore = "requires a graphics adapter"]
fn cached_replay_and_pooled_textures_work_with_layers() {
    let mut gpu = Gpu::new();
    let scene = [
        quad(rect(0.0, 0.0, 64.0, 128.0), BLUE),
        layer(
            rect(10.0, 10.0, 40.0, 40.0),
            0.5,
            Affine::translate(30.0, 30.0),
        ),
        quad(rect(10.0, 10.0, 40.0, 40.0), RED),
        Primitive::Text {
            rect: rect(12.0, 12.0, 36.0, 20.0),
            text: "Ag".into(),
            font_size: 16.0,
            color: Color::WHITE,
        },
        Primitive::PopLayer,
    ];
    let first = gpu.render(1.0, &scene, true);
    assert_pixel(&first, 100, 100, [255; 3]);
    assert_pixel(&first, 75, 75, [255, srgb(0.5), srgb(0.5)]);
    let stats = gpu.renderer.stats();
    assert_eq!((stats.preparations, stats.layer_passes), (1, 1));
    assert_eq!((stats.layer_textures, stats.layer_texture_bytes), (1, 6400));

    let replayed = gpu.render(1.0, &scene, false);
    assert!(first == replayed, "cached replay changed pixels");
    let stats = gpu.renderer.stats();
    assert_eq!((stats.preparations, stats.layer_passes), (1, 2));

    let second = gpu.render(1.0, &scene, true);
    assert!(first == second, "a reused layer texture changed pixels");
    assert_eq!(gpu.renderer.stats().layer_textures, 1);

    // Unused textures are released after a few frames.
    for _ in 0..3 {
        gpu.render(1.0, &scene[..1], true);
    }
    assert_eq!(gpu.renderer.stats().layer_textures, 0);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn oversized_layers_render_at_a_capped_resolution() {
    let mut gpu = Gpu::new();
    let pixels = gpu.render(
        1.0,
        &[
            layer(rect(0.0, 0.0, 4000.0, 4000.0), 0.5, Affine::IDENTITY),
            quad(rect(0.0, 0.0, 100.0, 100.0), RED),
            Primitive::PopLayer,
        ],
        true,
    );
    assert_pixel(&pixels, 50, 50, [255, srgb(0.5), srgb(0.5)]);
    assert_pixel(&pixels, 120, 120, [255; 3]);
    // Twice the target per side, not 4000×4000.
    assert_eq!(gpu.renderer.stats().layer_texture_bytes, 256 * 256 * 4);
}

/// Four red then four blue 9.6 px FiraMono cells starting at x = 8, over a
/// green default colour that no glyph should use.
fn two_colour_block() -> Primitive {
    Primitive::RichText {
        rect: rect(8.0, 8.0, 112.0, 24.0),
        spans: vec![
            TextSpan::new("MMMM").color(RED),
            TextSpan::new("MMMM").color(BLUE),
        ],
        style: TextStyle {
            family: FontFamily::Monospace,
            ..TextStyle::default()
        },
        color: Color(0, 255, 0, 255),
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn rich_text_spans_keep_their_own_colours() {
    let mut gpu = Gpu::new();
    let pixels = gpu.render(1.0, &[two_colour_block()], true);
    let count = |matches: &dyn Fn(usize, [u8; 3]) -> bool| {
        (0..SIDE * SIDE)
            .filter(|i| {
                matches(
                    i % SIDE,
                    [pixels[i * 4], pixels[i * 4 + 1], pixels[i * 4 + 2]],
                )
            })
            .count()
    };
    let red = |[r, g, b]: [u8; 3]| r > 150 && g < 120 && b < 120;
    let blue = |[r, g, b]: [u8; 3]| b > 150 && r < 120 && g < 120;
    // The colour boundary is at x = 8 + 4 * 9.6 = 46.4.
    assert!(count(&|x, rgb| x < 47 && red(rgb)) > 40, "red span missing");
    assert!(
        count(&|x, rgb| x >= 46 && blue(rgb)) > 40,
        "blue span missing"
    );
    assert_eq!(
        count(&|x, rgb| x >= 48 && red(rgb)),
        0,
        "red ran into the blue span"
    );
    assert_eq!(
        count(&|x, rgb| x < 45 && blue(rgb)),
        0,
        "blue ran into the red span"
    );
    assert_eq!(
        count(&|_, [r, g, b]| g > r.saturating_add(40) && g > b.saturating_add(40)),
        0,
        "a glyph fell back to the default colour"
    );
}

#[test]
#[ignore = "requires a graphics adapter"]
fn cached_replay_of_rich_text_is_identical() {
    let mut gpu = Gpu::new();
    let scene = [
        quad(rect(0.0, 0.0, 128.0, 40.0), Color::rgb(30, 30, 30)),
        two_colour_block(),
    ];
    let first = gpu.render(1.0, &scene, true);
    let replayed = gpu.render(1.0, &scene, false);
    assert!(first == replayed, "cached replay changed pixels");
    assert_eq!(gpu.renderer.stats().preparations, 1);
    // A later preparation still finds the rich buffer (it stays in the active set).
    let again = gpu.render(1.0, &scene, true);
    assert!(
        first == again,
        "re-preparing the same rich block changed pixels"
    );
}

/// A raster image from straight RGBA texels.
fn raster(width: u32, height: u32, texel: impl Fn(u32, u32) -> [u8; 4]) -> Image {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend(texel(x, y));
        }
    }
    Image::from_rgba8(width, height, pixels).unwrap()
}

fn rect_of(image: &Image) -> Rect {
    let size = image.size();
    rect(0.0, 0.0, size.width, size.height)
}

fn picture(rect: Rect, image: &Image) -> Primitive {
    Primitive::Image {
        rect,
        image: image.clone(),
        source: rect_of(image),
        tint: None,
        radius: 0.0,
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn images_show_their_texels_in_the_colours_quads_have() {
    let mut gpu = Gpu::new();
    let colours = [
        Color::rgb(200, 0, 0),
        Color::rgb(0, 180, 0),
        Color::rgb(0, 0, 220),
        Color::rgb(240, 200, 40),
    ];
    let image = raster(2, 2, |x, y| {
        let Color(r, g, b, a) = colours[(y * 2 + x) as usize];
        [r, g, b, a]
    });
    let mut primitives = vec![picture(rect(10.0, 10.0, 40.0, 40.0), &image)];
    for (index, colour) in colours.iter().enumerate() {
        primitives.push(quad(
            rect(70.0, 10.0 + 12.0 * index as f32, 10.0, 10.0),
            *colour,
        ));
    }
    let pixels = gpu.render(1.0, &primitives, true);
    for (index, Color(r, g, b, _)) in colours.into_iter().enumerate() {
        // Outside the texel centres, where filtering has nothing to blend.
        let (x, y) = (15 + 30 * (index % 2), 15 + 30 * (index / 2));
        assert_pixel(&pixels, x, y, [r, g, b]);
        let (qx, qy) = (75, 15 + 12 * index);
        assert_eq!(
            pixels[(y * SIDE + x) * 4..][..4],
            pixels[(qy * SIDE + qx) * 4..][..4],
            "texel {index} differs from a quad of its colour"
        );
    }
    // One texel as the source fills the rectangle edge to edge with its colour.
    let pixels = gpu.render(
        1.0,
        &[Primitive::Image {
            rect: rect(10.0, 10.0, 40.0, 40.0),
            image,
            source: rect(1.0, 0.0, 1.0, 1.0),
            tint: None,
            radius: 0.0,
        }],
        true,
    );
    for (x, y) in [(12, 12), (30, 30), (47, 47), (12, 47)] {
        assert_pixel(&pixels, x, y, [0, 180, 0]);
    }
    assert_eq!(gpu.renderer.stats().image_uploads, 1);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn tint_recolours_opaque_texels_and_radius_rounds_the_rectangle() {
    let mut gpu = Gpu::new();
    // The left half is an opaque white glyph, the right half is empty.
    let icon = raster(4, 4, |x, _| if x < 2 { [255; 4] } else { [0; 4] });
    let tinted = |rect, tint| Primitive::Image {
        rect,
        image: icon.clone(),
        source: rect_of(&icon),
        tint: Some(tint),
        radius: 0.0,
    };
    let photo = raster(2, 2, |_, _| [0, 0, 255, 255]);
    let pixels = gpu.render(
        1.0,
        &[
            quad(rect(0.0, 0.0, 128.0, 60.0), RED),
            tinted(rect(10.0, 10.0, 40.0, 40.0), BLUE),
            tinted(rect(10.0, 70.0, 40.0, 40.0), Color(0, 0, 255, 128)),
            Primitive::Image {
                rect: rect(70.0, 70.0, 40.0, 40.0),
                image: photo.clone(),
                source: rect_of(&photo),
                tint: None,
                radius: 14.0,
            },
        ],
        true,
    );
    assert_pixel(&pixels, 15, 30, [0, 0, 255]);
    assert_pixel(&pixels, 45, 30, [255, 0, 0]);
    // A translucent tint keeps the shape and scales its alpha.
    let rest = srgb(1.0 - 128.0 / 255.0);
    assert_pixel(&pixels, 15, 90, [rest, rest, 255]);
    assert_pixel(&pixels, 45, 90, [255; 3]);
    assert_pixel(&pixels, 90, 90, [0, 0, 255]);
    assert_pixel(&pixels, 90, 72, [0, 0, 255]);
    for (x, y) in [(71, 71), (108, 71), (71, 108), (108, 108)] {
        assert_pixel(&pixels, x, y, [255; 3]);
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn atlas_neighbours_do_not_bleed_and_share_a_draw() {
    let mut gpu = Gpu::new();
    let red = raster(4, 4, |_, _| [255, 0, 0, 255]);
    let blue = raster(4, 4, |_, _| [0, 0, 255, 255]);
    let green = raster(4, 4, |_, _| [0, 255, 0, 255]);
    let pixels = gpu.render(
        1.0,
        &[
            picture(rect(10.0, 10.0, 40.0, 40.0), &red),
            picture(rect(50.0, 10.0, 40.0, 40.0), &blue),
            picture(rect(10.0, 60.0, 40.0, 40.0), &green),
            picture(rect(60.0, 60.0, 40.0, 40.0), &blue),
        ],
        true,
    );
    // Ten pixels per texel: bleeding would tint the outer five.
    for y in [12, 30, 47] {
        assert_pixel(&pixels, 12, y, [255, 0, 0]);
        assert_pixel(&pixels, 47, y, [255, 0, 0]);
        assert_pixel(&pixels, 52, y, [0, 0, 255]);
        assert_pixel(&pixels, 87, y, [0, 0, 255]);
    }
    for (x, y) in [(12, 62), (47, 62), (12, 97), (47, 97), (30, 80)] {
        assert_pixel(&pixels, x, y, [0, 255, 0]);
    }
    for (x, y) in [(62, 62), (97, 97)] {
        assert_pixel(&pixels, x, y, [0, 0, 255]);
    }
    let stats = gpu.renderer.stats();
    assert_eq!((stats.atlas_pages, stats.image_textures), (1, 0));
    assert_eq!(stats.image_uploads, 3, "the second blue picture is cached");
    assert_eq!(stats.image_draws, 1, "one page under one clip is one draw");
}

#[test]
#[ignore = "requires a graphics adapter"]
fn square_images_that_abut_leave_no_seam() {
    let mut gpu = Gpu::new();
    let white = raster(4, 4, |_, _| [255, 255, 255, 255]);
    // Nine-slice pieces meet at whole and at fractional device pixels.
    for scale in [1.0, 1.25] {
        let pixels = gpu.render(
            scale,
            &[
                picture(rect(10.0, 10.0, 12.0, 30.0), &white),
                picture(rect(22.0, 10.0, 21.5, 30.0), &white),
                picture(rect(43.5, 10.0, 20.0, 30.0), &white),
                picture(rect(10.0, 40.0, 53.5, 12.5), &white),
            ],
            true,
        );
        let inside = |logical: f32| (logical * scale) as usize;
        for x in inside(11.0)..inside(62.0) {
            for y in inside(11.0)..inside(51.0) {
                assert_pixel(&pixels, x, y, [255, 255, 255]);
            }
        }
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn large_images_get_their_own_texture_and_the_budget_evicts_idle_ones() {
    let mut gpu = Gpu::new();
    let small = raster(8, 8, |_, _| [255, 0, 0, 255]);
    // A new large image each frame, as a slideshow would produce.
    let frame = |gpu: &mut Gpu| {
        let large = raster(1024, 1024, |x, _| {
            if x < 512 {
                [0, 0, 255, 255]
            } else {
                [0, 255, 0, 255]
            }
        });
        let pixels = gpu.render(
            1.0,
            &[
                picture(rect(0.0, 0.0, 64.0, 64.0), &large),
                picture(rect(70.0, 0.0, 20.0, 20.0), &small),
            ],
            true,
        );
        assert_pixel(&pixels, 16, 32, [0, 0, 255]);
        assert_pixel(&pixels, 48, 32, [0, 255, 0]);
        assert_pixel(&pixels, 80, 10, [255, 0, 0]);
        gpu.renderer.stats()
    };
    let stats = frame(&mut gpu);
    assert_eq!((stats.atlas_pages, stats.image_textures), (1, 1));
    assert_eq!(stats.image_draws, 2);
    assert_eq!(stats.image_bytes, (2048 * 2048 + 1024 * 1024) * 4);
    // A page and two large images fit 24 MiB; idle ones go once it is exceeded.
    gpu.renderer.set_image_budget(24 << 20);
    frame(&mut gpu);
    let stats = frame(&mut gpu);
    assert_eq!(stats.image_textures, 3);
    let stats = frame(&mut gpu);
    assert_eq!((stats.atlas_pages, stats.image_textures), (1, 2));
    assert_eq!(stats.image_uploads, 5, "the small image was uploaded once");
}

struct Solid(std::sync::Arc<std::sync::Mutex<Vec<(u32, u32)>>>);

impl VectorImage for Solid {
    fn size(&self) -> Size {
        Size::new(10.0, 10.0)
    }
    fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
        self.0.lock().unwrap().push((width, height));
        [0, 0, 255, 255].repeat((width * height) as usize)
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn vector_images_rasterize_for_the_pixels_they_cover_and_replay_from_cache() {
    let mut gpu = Gpu::new();
    let sizes = std::sync::Arc::default();
    let vector = Image::from_vector(Solid(std::sync::Arc::clone(&sizes)));
    let small = [picture(rect(5.0, 5.0, 20.0, 20.0), &vector)];
    let pixels = gpu.render(2.0, &small, true);
    assert_pixel(&pixels, 30, 30, [0, 0, 255]);
    assert_pixel(&pixels, 60, 60, [255; 3]);
    gpu.render(2.0, &small, true);
    assert_eq!(*sizes.lock().unwrap(), [(40, 40)]);
    let large = [picture(rect(5.0, 5.0, 30.0, 30.0), &vector)];
    let first = gpu.render(2.0, &large, true);
    assert_eq!(*sizes.lock().unwrap(), [(40, 40), (60, 60)]);
    let stats = gpu.renderer.stats();
    let replay = gpu.render(2.0, &large, false);
    assert_eq!(first, replay);
    assert_pixel(&replay, 60, 60, [0, 0, 255]);
    assert_eq!(gpu.renderer.stats().preparations, stats.preparations);
    assert_eq!(gpu.renderer.stats().image_uploads, 2);
    assert_eq!(sizes.lock().unwrap().len(), 2);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn images_composite_inside_layers_and_keep_painters_order() {
    let mut gpu = Gpu::new();
    let red = raster(2, 2, |_, _| [255, 0, 0, 255]);
    let green = Color::rgb(0, 255, 0);
    let pixels = gpu.render(
        1.0,
        &[
            layer(rect(0.0, 0.0, 60.0, 60.0), 0.5, Affine::IDENTITY),
            picture(rect(10.0, 10.0, 40.0, 40.0), &red),
            Primitive::PopLayer,
            quad(rect(70.0, 10.0, 50.0, 50.0), BLUE),
            picture(rect(80.0, 20.0, 30.0, 30.0), &red),
            quad(rect(95.0, 35.0, 25.0, 25.0), green),
            Primitive::PushClip(rect(10.0, 70.0, 20.0, 20.0)),
            picture(rect(10.0, 70.0, 40.0, 40.0), &red),
            Primitive::PopClip,
        ],
        true,
    );
    let half = srgb(0.5);
    assert_pixel(&pixels, 30, 30, [255, half, half]);
    assert_pixel(&pixels, 55, 55, [255; 3]);
    assert_pixel(&pixels, 75, 15, [0, 0, 255]);
    assert_pixel(&pixels, 85, 25, [255, 0, 0]);
    assert_pixel(&pixels, 100, 40, [0, 255, 0]);
    assert_pixel(&pixels, 115, 55, [0, 255, 0]);
    // The clip scissors the image.
    assert_pixel(&pixels, 20, 80, [255, 0, 0]);
    assert_pixel(&pixels, 40, 80, [255; 3]);
    assert_pixel(&pixels, 20, 100, [255; 3]);
}

/// A quad, and a half-transparent layer holding another: two passes.
fn layered_scene() -> [Primitive; 4] {
    [
        quad(rect(0.0, 0.0, 64.0, 128.0), BLUE),
        layer(rect(10.0, 10.0, 40.0, 40.0), 0.5, Affine::IDENTITY),
        quad(rect(10.0, 10.0, 40.0, 40.0), RED),
        Primitive::PopLayer,
    ]
}

#[test]
#[ignore = "requires a graphics adapter"]
fn profiled_frames_report_gpu_pass_times_where_the_adapter_has_timestamps() {
    let mut gpu = Gpu::with_features(wgpu::Features::TIMESTAMP_QUERY);
    gpu.renderer.set_profiling(true);
    assert!(gpu.renderer.profiling());
    let scene = layered_scene();
    // `render` waits for its own readback, so earlier frames' timestamps are
    // mapped by the time the next frame collects them.
    for frame in 0..8 {
        gpu.render(1.0, &scene, frame == 0);
    }
    if !gpu.renderer.timestamps_supported() {
        println!("timestamp path: adapter has no TIMESTAMP_QUERY, no GPU times");
        assert!(gpu.renderer.gpu_profile().is_none());
        assert_eq!(gpu.renderer.stats().memory.profiling, 0);
        return;
    }
    let profile = gpu.renderer.gpu_profile().expect("a finished profile");
    println!("timestamp path: queries, {profile:?}");
    let labels: Vec<&str> = profile
        .passes
        .iter()
        .map(|pass| pass.label.as_str())
        .collect();
    assert_eq!(labels, ["layer 40x40", "main"]);
    assert!(profile.frame >= 1 && profile.frame < 8);
    let sum: std::time::Duration = profile.passes.iter().map(|pass| pass.duration).sum();
    assert!(!profile.total.is_zero() && profile.total >= sum / 2);
    assert!(profile.total < std::time::Duration::from_secs(1));
    // One resolve buffer and at most four readbacks of 512 bytes.
    let stats = gpu.renderer.stats();
    assert!((1024..=2560).contains(&stats.memory.profiling));

    gpu.renderer.set_profiling(false);
    assert!(gpu.renderer.gpu_profile().is_none());
    assert_eq!(gpu.renderer.stats().memory.profiling, 0);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn profiling_on_a_device_without_timestamps_draws_and_reports_no_times() {
    // What a host gets when the adapter lacks the feature or did not ask.
    let mut gpu = Gpu::new();
    gpu.renderer.set_profiling(true);
    assert!(!gpu.renderer.timestamps_supported());
    let scene = layered_scene();
    let first = gpu.render(1.0, &scene, true);
    for _ in 0..4 {
        assert!(first == gpu.render(1.0, &scene, false), "replay changed");
    }
    assert!(gpu.renderer.gpu_profile().is_none());
    let stats = gpu.renderer.stats();
    assert_eq!(stats.memory.profiling, 0);
    // Counters and memory do not need timestamps.
    assert!(stats.draw_calls > 0 && stats.memory.total > 0);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn without_profiling_there_is_no_profile_and_replays_stay_identical() {
    let mut gpu = Gpu::with_features(wgpu::Features::TIMESTAMP_QUERY);
    assert!(!gpu.renderer.profiling());
    let scene = layered_scene();
    let first = gpu.render(1.0, &scene, true);
    for _ in 0..4 {
        assert!(first == gpu.render(1.0, &scene, false), "replay changed");
        assert!(gpu.renderer.gpu_profile().is_none());
    }
    let stats = gpu.renderer.stats();
    assert_eq!((stats.frames, stats.preparations), (5, 1));
    assert_eq!(stats.memory.profiling, 0);
    // Profiling does not change the picture either.
    gpu.renderer.set_profiling(true);
    assert!(first == gpu.render(1.0, &scene, false), "profiling changed");
}

#[test]
#[ignore = "requires a graphics adapter"]
fn memory_is_counted_by_category_and_the_peak_survives_eviction() {
    let mut gpu = Gpu::new();
    #[track_caller]
    fn consistent(gpu: &Gpu) -> glassmere_render_wgpu::MemoryStats {
        let memory = gpu.renderer.stats().memory;
        assert_eq!(memory.glyph_atlas, None, "glyphon does not say");
        assert_eq!(
            memory.total,
            memory.buffers
                + memory.image_atlas
                + memory.image_textures
                + memory.layers
                + memory.canvas
                + memory.profiling
        );
        assert!(memory.peak >= memory.total);
        memory
    }
    gpu.render(1.0, &[quad(rect(0.0, 0.0, 8.0, 8.0), RED)], true);
    let quads = consistent(&gpu);
    assert!(quads.buffers > 0);
    assert_eq!(
        (quads.image_atlas, quads.image_textures, quads.layers),
        (0, 0, 0)
    );

    gpu.render(1.0, &layered_scene(), true);
    assert_eq!(consistent(&gpu).layers, 40 * 40 * 4);

    let small = raster(8, 8, |_, _| [255, 0, 0, 255]);
    let frame = |gpu: &mut Gpu| {
        let large = raster(1024, 1024, |_, _| [0, 0, 255, 255]);
        gpu.render(
            1.0,
            &[
                picture(rect(0.0, 0.0, 64.0, 64.0), &large),
                picture(rect(70.0, 0.0, 20.0, 20.0), &small),
            ],
            true,
        );
        consistent(gpu)
    };
    let images = frame(&mut gpu);
    assert_eq!(images.image_atlas, 2048 * 2048 * 4);
    assert_eq!(images.image_textures, 1024 * 1024 * 4);
    assert!(images.total > quads.total);

    gpu.renderer.set_image_budget(24 << 20);
    frame(&mut gpu);
    let full = frame(&mut gpu);
    assert_eq!(full.image_textures, 3 * 1024 * 1024 * 4);
    let evicted = frame(&mut gpu);
    assert_eq!(evicted.image_textures, 2 * 1024 * 1024 * 4);
    assert!(evicted.total < full.total);
    assert!(evicted.peak >= full.total && evicted.peak > evicted.total);

    gpu.renderer.reset_peak();
    let memory = gpu.renderer.stats().memory;
    assert_eq!(memory.peak, memory.total);

    // The driver's view of the whole device, where the backend has one.
    match Renderer::driver_memory(&gpu.device) {
        Some(driver) => {
            println!("driver memory: {driver:?}");
            assert!(driver.reserved >= driver.allocated && driver.allocations > 0);
            assert!(driver.allocated >= memory.total);
        }
        None => println!("driver memory: this backend has no allocator report"),
    }
}

#[test]
#[ignore = "requires a graphics adapter"]
fn draw_calls_quads_and_passes_match_a_known_scene() {
    let mut gpu = Gpu::new();
    let image = raster(8, 8, |_, _| [255, 0, 0, 255]);
    let mut scene = layered_scene().to_vec();
    scene.push(Primitive::Text {
        rect: rect(70.0, 40.0, 50.0, 20.0),
        text: "Ag".into(),
        font_size: 16.0,
        color: Color(0, 0, 0, 255),
    });
    scene.push(picture(rect(70.0, 0.0, 20.0, 20.0), &image));
    gpu.render(1.0, &scene, true);
    let stats = gpu.renderer.stats();
    // Two quads, one text batch, one image batch and the layer's composite.
    assert_eq!(
        (stats.quads, stats.text_batches, stats.image_draws),
        (2, 1, 1)
    );
    assert_eq!((stats.draw_calls, stats.passes), (5, 2));
}

#[test]
#[ignore = "requires a graphics adapter"]
fn damage_redraws_only_its_rectangles_and_keeps_the_rest() {
    let mut gpu = Gpu::new();
    let all = [rect(0.0, 0.0, SIDE as f32, SIDE as f32)];
    let a = [quad(rect(8.0, 8.0, 40.0, 40.0), RED)];
    let b = [quad(rect(70.0, 70.0, 40.0, 40.0), BLUE)];
    // Until a host names damage there is no canvas and nothing changes.
    let plain_a = gpu.render(1.0, &a, true);
    let plain_b = gpu.render(1.0, &b, true);
    let stats = gpu.renderer.stats();
    assert_eq!(
        (stats.memory.canvas, stats.redrawn),
        (0, (SIDE * SIDE) as u64)
    );

    // The first damaged frame has nothing to keep, so it draws the lot.
    let canvas_a = gpu.damaged(1.0, &a, true, &all);
    let stats = gpu.renderer.stats();
    assert_eq!(stats.redrawn, (SIDE * SIDE) as u64);
    assert_eq!(stats.memory.canvas, (SIDE * SIDE * 4) as u64);
    assert!(stats.memory.total > stats.memory.canvas);
    assert!(
        canvas_a == plain_a,
        "the canvas lost colour on its way over"
    );

    // Damage over the top left quarter of B's quad, and nowhere else.
    let mixed = gpu.damaged(1.0, &b, true, &[rect(64.0, 64.0, 32.0, 32.0)]);
    assert_eq!(gpu.renderer.stats().redrawn, 32 * 32);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let at = (y * SIDE + x) * 4;
            let damaged = (64..96).contains(&x) && (64..96).contains(&y);
            let (expected, scene) = match damaged {
                true => (&plain_b, 'B'),
                false => (&plain_a, 'A'),
            };
            assert_eq!(
                mixed[at..at + 4],
                expected[at..at + 4],
                "pixel ({x}, {y}) is not scene {scene}"
            );
        }
    }
    // The rest of B is absent, and A survived where B did not reach it.
    assert_pixel(&mixed, 80, 80, [0, 0, 255]);
    assert_pixel(&mixed, 105, 105, [255; 3]);
    assert_pixel(&mixed, 20, 20, [255, 0, 0]);

    // Empty damage means everything changed.
    let full = gpu.damaged(1.0, &b, true, &[]);
    assert_eq!(gpu.renderer.stats().redrawn, (SIDE * SIDE) as u64);
    assert!(full == plain_b, "an undamaged frame is not a full redraw");

    // Several rectangles are redrawn as one scissor over their union, so a
    // pixel between them is redrawn too.
    let mixed = gpu.damaged(
        1.0,
        &a,
        true,
        &[rect(8.0, 8.0, 20.0, 20.0), rect(28.0, 28.0, 20.0, 20.0)],
    );
    assert_eq!(gpu.renderer.stats().redrawn, 40 * 40);
    assert_pixel(&mixed, 40, 10, [255, 0, 0]);
    assert_pixel(&mixed, 80, 80, [0, 0, 255]);
    assert_pixel(&mixed, 60, 60, [255; 3]);
}

#[test]
#[ignore = "requires a graphics adapter"]
fn a_resized_target_redraws_in_full_however_small_the_damage() {
    const SMALL: usize = 64;
    let mut gpu = Gpu::new();
    let all = [rect(0.0, 0.0, SIDE as f32, SIDE as f32)];
    // A canvas holding a red frame at the old size.
    gpu.damaged(1.0, &[quad(all[0], RED)], true, &all);
    assert_eq!(
        gpu.renderer.stats().memory.canvas,
        (SIDE * SIDE * 4) as u64,
        "the canvas is the size of the target"
    );

    let small = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("resized target"),
        size: wgpu::Extent3d {
            width: SMALL as u32,
            height: SMALL as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let scene = Scene {
        size: Size::new(SMALL as f32, SMALL as f32),
        primitives: vec![quad(rect(0.0, 0.0, 10.0, 10.0), BLUE)],
    };
    gpu.renderer
        .render_cached(
            &gpu.device,
            &gpu.queue,
            &small.create_view(&Default::default()),
            [SMALL as u32; 2],
            1.0,
            &scene,
            Color::WHITE,
            true,
            &[rect(0.0, 0.0, 4.0, 4.0)],
        )
        .unwrap();
    let stats = gpu.renderer.stats();
    assert_eq!(stats.redrawn, (SMALL * SMALL) as u64);
    assert_eq!(stats.memory.canvas, (SMALL * SMALL * 4) as u64);
    let pixels = readback(&gpu.device, &gpu.queue, &small, SMALL, SMALL);
    for y in 0..SMALL {
        for x in 0..SMALL {
            let [r, g, b, a] = pixels[(y * SMALL + x) * 4..][..4] else {
                unreachable!()
            };
            // Blue on white: the two dim channels move together and blue stays
            // full. A kept pixel of the red frame would have neither.
            assert_eq!(
                (r == g, b, a),
                (true, 255, 255),
                "({x}, {y}) is {r} {g} {b}"
            );
        }
    }
    // Drawn whole: the quad reaches well past the damage, and where the old
    // frame was red the new one is its own white.
    assert_eq!(pixels[(2 * SMALL + 7) * 4..][..4], [0, 0, 255, 255]);
    assert_eq!(pixels[(40 * SMALL + 40) * 4..][..4], [255; 4]);
}

fn point(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn stop(at: f32, color: Color) -> GradientStop {
    GradientStop { at, color }
}

/// A ramp mixes where it is told to, and it mixes light: half red and half blue
/// is (188, 0, 188), a value an sRGB mix of the two would never reach.
#[test]
#[ignore = "requires a graphics adapter"]
fn linear_gradient_mixes_its_stops_in_linear_light() {
    let mut gpu = Gpu::new();
    let band = rect(24.0, 24.0, 81.0, 40.0);
    let round = rect(24.0, 72.0, 40.0, 40.0);
    let across = |rect, radius| Primitive::Gradient {
        rect,
        radius,
        from: point(0.0, 0.0),
        to: point(1.0, 0.0),
        stops: vec![stop(0.0, RED), stop(1.0, BLUE)],
        radial: false,
    };
    let pixels = gpu.render(1.0, &[across(band, 0.0), across(round, 20.0)], true);
    assert_eq!(gpu.renderer.stats().gradients, 2);
    // Half of each, in linear light: an sRGB mix would give (128, 0, 128).
    assert_pixel(&pixels, 64, 44, [188, 0, 188]);
    // Clear of the boundary pixels, which the rounded-box coverage softens
    // exactly as it does a quad's.
    for x in [26, 44, 64, 84, 102] {
        let at = (x as f32 + 0.5 - band.x) / band.width;
        assert_pixel(&pixels, x, 44, [srgb(1.0 - at), 0, srgb(at)]);
    }
    // The ramp runs along x alone.
    assert_pixel(&pixels, 64, 25, [188, 0, 188]);
    assert_pixel(&pixels, 64, 62, [188, 0, 188]);
    // Outside the rectangle, and outside the rounded corners, the background.
    for (x, y) in [(20, 44), (108, 44), (64, 20), (64, 66), (25, 73), (62, 110)] {
        assert_pixel(&pixels, x, y, [255; 3]);
    }
    // Only the first eight stops are kept: the green ninth and tenth never
    // reach the ramp, which ends in the blue of the eighth.
    let mut many = vec![stop(0.0, RED)];
    many.extend((1..8).map(|index| stop(index as f32 / 9.0, BLUE)));
    many.push(stop(8.0 / 9.0, Color::rgb(0, 255, 0)));
    many.push(stop(1.0, Color::rgb(0, 255, 0)));
    let pixels = gpu.render(
        1.0,
        &[Primitive::Gradient {
            rect: band,
            radius: 0.0,
            from: point(0.0, 0.0),
            to: point(1.0, 0.0),
            stops: many,
            radial: false,
        }],
        true,
    );
    for x in [44, 64, 84, 102] {
        assert_pixel(&pixels, x, 44, [0, 0, 255]);
    }
}

/// `from` is the middle and `to` the far corner, both in the rectangle's own
/// units, so the ramp ends where the corner is.
#[test]
#[ignore = "requires a graphics adapter"]
fn radial_gradient_runs_from_its_centre_to_the_far_corner() {
    let mut gpu = Gpu::new();
    let square = rect(24.0, 24.0, 81.0, 81.0);
    let pixels = gpu.render(
        1.0,
        &[Primitive::Gradient {
            rect: square,
            radius: 0.0,
            from: point(0.5, 0.5),
            to: point(1.0, 1.0),
            stops: vec![stop(0.0, RED), stop(1.0, BLUE)],
            radial: true,
        }],
        true,
    );
    // The middle of the rectangle is a pixel centre here, and the first stop.
    assert_pixel(&pixels, 64, 64, [255, 0, 0]);
    let at = |x: usize, y: usize| {
        let u = (x as f32 + 0.5 - square.x) / square.width - 0.5;
        let v = (y as f32 + 0.5 - square.y) / square.height - 0.5;
        u.hypot(v) / 0.5f32.hypot(0.5)
    };
    // Near the far corners the ramp is all but spent, and it is the same in
    // every direction from the middle. Boundary pixels are left to the
    // coverage, which softens them as it does a quad's.
    for (x, y) in [(102, 102), (26, 26), (102, 26), (64, 30), (95, 64)] {
        let at = at(x, y);
        assert!(at > 0.5, "({x}, {y}) is only {at} along the ramp");
        assert_pixel(&pixels, x, y, [srgb(1.0 - at), 0, srgb(at)]);
    }
    assert!(at(102, 102) > 0.93, "the corner is where the ramp ends");
    assert_pixel(&pixels, 20, 20, [255; 3]);
}

/// A shadow darkens around its rectangle, out to the spread and three standard
/// deviations, and no further.
#[test]
#[ignore = "requires a graphics adapter"]
fn shadow_blurs_around_its_rectangle_and_ends_three_deviations_out() {
    let mut gpu = Gpu::new();
    let cast = |offset, spread| Primitive::Shadow {
        rect: rect(44.0, 44.0, 40.0, 40.0),
        radius: 0.0,
        color: Color(0, 0, 0, 255),
        blur: 6.0,
        spread,
        offset,
        inset: false,
    };
    let grey = |pixels: &[u8], x: usize, y: usize| pixels[(y * SIDE + x) * 4];
    let pixels = gpu.render(1.0, &[cast(point(0.0, 0.0), 0.0)], true);
    assert_eq!(gpu.renderer.stats().shadows, 1);
    // Under the rectangle the shadow is whole, and it fades outward.
    assert!(grey(&pixels, 64, 64) < 20, "{}", grey(&pixels, 64, 64));
    let near = grey(&pixels, 64, 86);
    assert!(near > grey(&pixels, 64, 64) && near < 240, "{near}");
    assert!(near < grey(&pixels, 64, 94));
    assert!(grey(&pixels, 64, 94) <= grey(&pixels, 64, 100));
    // Three standard deviations out the Gaussian is spent; nothing is drawn
    // past it, and the corners of the target never hear about it.
    assert_pixel(&pixels, 64, 104, [255; 3]);
    assert_pixel(&pixels, 10, 10, [255; 3]);

    // The spread grows the rectangle before it is blurred, so the shadow
    // reaches that much further and stops there.
    let spread = gpu.render(1.0, &[cast(point(0.0, 0.0), 4.0)], true);
    assert!(grey(&spread, 64, 90) < grey(&pixels, 64, 90));
    assert_pixel(&spread, 64, 108, [255; 3]);

    // The offset moves the whole shadow, and takes it off the left edge.
    let moved = gpu.render(1.0, &[cast(point(16.0, 0.0), 0.0)], true);
    for (x, y) in [(46, 64), (64, 64), (64, 90), (86, 50)] {
        let (there, here) = (grey(&moved, x + 16, y), grey(&pixels, x, y));
        assert!(
            there.abs_diff(here) <= 1,
            "({x}, {y}) is {there}, was {here}"
        );
    }
    assert!(grey(&moved, 42, 64) > grey(&pixels, 42, 64) + 20);
}

/// An inset shadow is the same blur inverted: the inside edge darkens, the
/// middle stays clear and nothing leaves the rectangle.
#[test]
#[ignore = "requires a graphics adapter"]
fn inset_shadow_darkens_the_inside_edge_and_leaves_the_middle() {
    let mut gpu = Gpu::new();
    let pixels = gpu.render(
        1.0,
        &[Primitive::Shadow {
            rect: rect(24.0, 24.0, 80.0, 80.0),
            radius: 0.0,
            color: Color(0, 0, 0, 255),
            blur: 6.0,
            spread: 0.0,
            offset: point(0.0, 0.0),
            inset: true,
        }],
        true,
    );
    let grey = |x: usize, y: usize| pixels[(y * SIDE + x) * 4];
    assert_pixel(&pixels, 64, 64, [255; 3]);
    for (x, y) in [(25, 64), (102, 64), (64, 25), (64, 102)] {
        assert!(grey(x, y) < 215, "({x}, {y}) is {}", grey(x, y));
    }
    assert!(grey(25, 64) < grey(31, 64) && grey(31, 64) < grey(64, 64));
    // Outside the rectangle an inset shadow draws nothing at all.
    for (x, y) in [(20, 64), (108, 64), (64, 20), (64, 108)] {
        assert_pixel(&pixels, x, y, [255; 3]);
    }
}

/// A pipeline of the test's own, as an application's custom draw would have:
/// one triangle over the whole clip space in solid green, which the renderer's
/// scissor cuts down to the primitive's rectangle.
fn solid_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("custom draw"),
        source: wgpu::ShaderSource::Wgsl(
            "@vertex
             fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
                 let x = f32(i32(index) / 2) * 4.0 - 1.0;
                 let y = f32(i32(index) % 2) * 4.0 - 1.0;
                 return vec4<f32>(x, y, 0.0, 1.0);
             }
             @fragment
             fn fs_main() -> @location(0) vec4<f32> {
                 return vec4<f32>(0.0, 1.0, 0.0, 1.0);
             }"
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("custom draw"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[test]
#[ignore = "requires a graphics adapter"]
fn custom_draws_where_it_is_told_and_an_unknown_id_draws_nothing() {
    let mut gpu = Gpu::new();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = std::sync::Arc::clone(&seen);
    let mut pipeline = None;
    gpu.renderer.set_custom_draw(
        7,
        Some(Box::new(move |draw: CustomDraw<'_>| {
            calls
                .lock()
                .unwrap()
                .push((draw.rect, draw.scale, draw.target_size));
            let pipeline = pipeline.get_or_insert_with(|| solid_pipeline(draw.device));
            draw.pass.set_pipeline(pipeline);
            draw.pass.draw(0..3, 0..1);
        })),
    );
    let mine = rect(32.0, 40.0, 48.0, 24.0);
    let scene = [
        quad(rect(0.0, 0.0, 128.0, 128.0), RED),
        Primitive::Custom { rect: mine, id: 7 },
        Primitive::Custom {
            rect: rect(0.0, 100.0, 40.0, 20.0),
            id: 99,
        },
        quad(rect(64.0, 40.0, 16.0, 24.0), BLUE),
    ];
    let pixels = gpu.render(1.0, &scene, true);
    let (green, red, blue) = ([0, 255, 0], [255, 0, 0], [0, 0, 255]);
    for (x, y) in [(33, 41), (63, 62)] {
        assert_pixel(&pixels, x, y, green);
    }
    // The quad after it still covers it: a custom draw keeps its place in the
    // display list.
    assert_pixel(&pixels, 70, 45, blue);
    assert_pixel(&pixels, 76, 55, blue);
    // The unknown id draws nothing, and nothing else moved.
    for (x, y) in [(10, 105), (30, 110), (31, 41), (33, 39), (10, 10)] {
        assert_pixel(&pixels, x, y, red);
    }

    // The callback is told the scale, and the scissor follows it.
    let pixels = gpu.render(2.0, &scene, true);
    assert_pixel(&pixels, 70, 82, green);
    assert_pixel(&pixels, 62, 82, red);
    assert_eq!(
        *seen.lock().unwrap(),
        vec![(mine, 1.0, [128, 128]), (mine, 2.0, [128, 128])]
    );

    // Forgotten, the id draws nothing and the callback is not called again.
    gpu.renderer.set_custom_draw(7, None);
    let pixels = gpu.render(1.0, &scene, true);
    assert_pixel(&pixels, 33, 41, red);
    assert_pixel(&pixels, 70, 45, blue);
    assert_eq!(seen.lock().unwrap().len(), 2);
}

/// A damaged rectangle is cleared before it is drawn again. Without that,
/// whatever it held stays under the new frame, and anything that was there
/// and is gone, a popup above all, shows through it.
#[test]
#[ignore = "requires a graphics adapter"]
fn what_a_damaged_rectangle_held_is_gone_when_nothing_replaces_it() {
    let mut gpu = Gpu::new();
    let page = quad(rect(0.0, 0.0, 128.0, 128.0), BLUE);
    let popup = quad(rect(20.0, 20.0, 60.0, 40.0), RED);
    let over = rect(20.0, 20.0, 60.0, 40.0);

    // The first frame establishes the canvas: a page with a popup over it.
    gpu.damaged(1.0, &[page.clone(), popup.clone()], true, &[]);
    let with_popup = gpu.damaged(1.0, &[page.clone(), popup], true, &[over]);
    assert_pixel(&with_popup, 40, 30, [255, 0, 0]);

    // The popup goes, and the only news is the rectangle it was in.
    let without = gpu.damaged(1.0, std::slice::from_ref(&page), true, &[over]);
    assert_pixel(&without, 40, 30, [0, 0, 255]);
    assert_pixel(&without, 100, 100, [0, 0, 255]);

    // The same where nothing at all is drawn in the damaged rectangle: what
    // is left has to be the clear colour, not the last frame.
    let bare = gpu.damaged(
        1.0,
        &[quad(rect(0.0, 90.0, 128.0, 38.0), BLUE)],
        true,
        &[over],
    );
    assert_pixel(&bare, 40, 30, [255; 3]);
    assert_pixel(&bare, 40, 100, [0, 0, 255]);
}
