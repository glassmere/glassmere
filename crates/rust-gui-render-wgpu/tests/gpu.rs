use rust_gui_render_wgpu::{Renderer, TextSystem, wgpu};
use rust_gui_scene::{
    Affine, Color, FontFamily, Point, Primitive, Rect, Scene, Size, TextMeasurer, TextSpan,
    TextStyle, TextWrap,
};

/// Requires a hardware or software graphics adapter. Run explicitly on a machine
/// with graphics drivers: cargo test -p rust-gui-render-wgpu --test gpu -- --ignored
#[test]
#[ignore = "requires a graphics adapter"]
fn renders_ordered_clipped_quads_and_shaped_text_at_two_x_scale() {
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
        let data = output.slice(..).get_mapped_range();
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
        let data = output.slice(..).get_mapped_range();
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
    let data = output.slice(..).get_mapped_range();
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
}

const SIDE: usize = 128;
const RED: Color = Color(255, 0, 0, 255);
const BLUE: Color = Color(0, 0, 255, 255);

impl Gpu {
    fn new() -> Self {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
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
            }
        })
    }

    fn render(&mut self, scale: f32, primitives: &[Primitive], changed: bool) -> Vec<u8> {
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
