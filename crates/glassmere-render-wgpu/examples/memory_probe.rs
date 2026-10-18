//! Startup checkpoints for scripts/measure-memory.ps1. Each checkpoint waits for
//! a newline on stdin so the parent measures this process, not its own memory.
use glassmere_render_wgpu::{Renderer, TextSystem, wgpu};
use glassmere_scene::{Color, Primitive, Rect, Scene, Size, TextMeasurer};
use std::io::{self, Write};

fn checkpoint(stage: &str) {
    println!("MEMORY_STAGE {stage}");
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    checkpoint("process");
    if mode == "fonts" {
        let mut text = TextSystem::default();
        checkpoint("system_fonts");
        text.measure("Counter 0 / A small API. A retained engine.", 30.0);
        checkpoint("shaped_text");
        std::hint::black_box(text);
        return Ok(());
    }
    pollster::block_on(async {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = match mode.as_str() {
            "vulkan" => wgpu::Backends::VULKAN,
            "dx12" => wgpu::Backends::DX12,
            "gl" => wgpu::Backends::GL,
            "all" => wgpu::Backends::all(),
            _ => return Err("expected fonts, all, vulkan, dx12, or gl".into()),
        };
        let instance = wgpu::Instance::new(descriptor);
        checkpoint("gpu_instance");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                ..Default::default()
            })
            .await?;
        eprintln!("Adapter: {:?}", adapter.get_info());
        checkpoint("gpu_adapter");
        let memory_hints = if std::env::args().nth(2).as_deref() == Some("memory") {
            wgpu::MemoryHints::MemoryUsage
        } else {
            wgpu::MemoryHints::Performance
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                memory_hints,
                ..Default::default()
            })
            .await?;
        checkpoint("gpu_device");
        let mut renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb);
        checkpoint("renderer_and_fonts");
        let text_size = renderer.text_system().measure("Counter 0", 30.0);
        checkpoint("shaped_text");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("memory probe target"),
            size: wgpu::Extent3d {
                width: 900,
                height: 650,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let scene = Scene {
            size: Size::new(900.0, 650.0),
            primitives: vec![Primitive::Text {
                rect: Rect {
                    x: 20.0,
                    y: 20.0,
                    width: text_size.width,
                    height: text_size.height,
                },
                text: "Counter 0".into(),
                font_size: 30.0,
                color: Color::WHITE,
            }],
        };
        renderer.render(
            &device,
            &queue,
            &texture.create_view(&Default::default()),
            [900, 650],
            1.0,
            &scene,
            Color::rgb(15, 23, 34),
        )?;
        device.poll(wgpu::PollType::wait_indefinitely())?;
        checkpoint("rendered_frame");
        Ok(())
    })
}
