use rust_gui_render_wgpu::{Renderer, TextSystem, wgpu};
use rust_gui_scene::{
    Color, FontFamily, Primitive, Rect, Scene, Size, TextMeasurer, TextStyle, TextWrap,
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
