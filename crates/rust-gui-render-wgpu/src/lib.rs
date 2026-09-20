//! GPU scene backend. This crate knows paint primitives, never widgets.
mod text;
pub use text::TextSystem;
pub use wgpu;

use glyphon::{
    Cache, Resolution, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use rust_gui_scene::{Color, Primitive, Rect, Scene};
use std::{collections::HashSet, error::Error};

pub type RenderResult<T> = Result<T, Box<dyn Error>>;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    local: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    radius: f32,
}

enum Draw {
    Quad { start: u32, clip: [u32; 4] },
    Text(usize),
}

struct PreparedText {
    key: text::TextKey,
    rect: Rect,
    clip: [u32; 4],
    color: Color,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub preparations: u64,
    pub quad_buffer_allocations: u64,
    pub quad_buffer_bytes: u64,
    pub text_batches: usize,
    pub text_renderer_allocations: u64,
}

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    text_system: TextSystem,
    swash: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderers: Vec<TextRenderer>,
    quad_buffer: Option<wgpu::Buffer>,
    vertices: Vec<Vertex>,
    draws: Vec<Draw>,
    prepared_target: Option<([u32; 2], u32)>,
    stats: RenderStats,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self::with_text_system(device, queue, format, TextSystem::default())
    }

    /// Like `new`, with a caller-provided font system (see `TextSystem::with_fonts`).
    pub fn with_text_system(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        text_system: TextSystem,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rust-gui quads"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rust-gui quads"), layout: None,
            vertex: wgpu::VertexState {
                module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4, 4 => Float32],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs_main"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(), depth_stencil: None,
            multisample: wgpu::MultisampleState::default(), multiview_mask: None, cache: None,
        });
        let cache = Cache::new(device);
        Self {
            pipeline,
            text_system,
            swash: SwashCache::new(),
            viewport: Viewport::new(device, &cache),
            atlas: TextAtlas::new(device, queue, &cache, format),
            text_renderers: Vec::new(),
            quad_buffer: None,
            vertices: Vec::new(),
            draws: Vec::new(),
            prepared_target: None,
            stats: RenderStats::default(),
        }
    }

    pub fn text_system(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Paint in display-list order. `physical_size` and `scale` describe the target;
    /// scene geometry remains in logical pixels. Targets must be nonzero and sRGB.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        physical_size: [u32; 2],
        scale: f32,
        scene: &Scene,
        clear: Color,
    ) -> RenderResult<()> {
        self.render_cached(
            device,
            queue,
            target,
            physical_size,
            scale,
            scene,
            clear,
            true,
        )
    }

    /// Hosts may set `scene_changed` to false only if no scene primitive changed
    /// since the previous successful call. Target size/scale changes force preparation.
    /// Exposure redraws then reuse glyph and quad buffers without uploading again.
    #[allow(clippy::too_many_arguments)]
    pub fn render_cached(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        physical_size: [u32; 2],
        scale: f32,
        scene: &Scene,
        clear: Color,
        scene_changed: bool,
    ) -> RenderResult<()> {
        if physical_size.contains(&0) || !scale.is_finite() || scale <= 0.0 {
            return Ok(());
        }
        let target_key = (physical_size, scale.to_bits());
        if !scene_changed && self.prepared_target == Some(target_key) {
            return self.draw(device, queue, target, physical_size, clear);
        }
        self.prepared_target = None;
        self.viewport.update(
            queue,
            Resolution {
                width: physical_size[0],
                height: physical_size[1],
            },
        );
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            width: physical_size[0] as f32 / scale,
            height: physical_size[1] as f32 / scale,
        };
        let mut clips = vec![viewport];
        let mut vertices = std::mem::take(&mut self.vertices);
        vertices.clear();
        let mut draws = std::mem::take(&mut self.draws);
        draws.clear();
        let mut text_items = Vec::new();
        let mut text_ranges: Vec<std::ops::Range<usize>> = Vec::new();
        let mut active_text = HashSet::new();
        for primitive in &scene.primitives {
            let clip = *clips.last().unwrap();
            match primitive {
                Primitive::PushClip(rect) => clips.push(clip.intersect(*rect)),
                Primitive::PopClip => {
                    if clips.len() > 1 {
                        clips.pop();
                    }
                }
                Primitive::Quad {
                    rect,
                    color,
                    radius,
                } => {
                    let scissor = scissor(clip.intersect(*rect), scale, physical_size);
                    if scissor[2] == 0 || scissor[3] == 0 {
                        continue;
                    }
                    let start = vertices.len() as u32;
                    for [x, y] in [
                        [0.0, 0.0],
                        [1.0, 0.0],
                        [0.0, 1.0],
                        [0.0, 1.0],
                        [1.0, 0.0],
                        [1.0, 1.0],
                    ] {
                        vertices.push(Vertex {
                            position: [
                                (rect.x + x * rect.width) * scale / physical_size[0] as f32 * 2.0
                                    - 1.0,
                                1.0 - (rect.y + y * rect.height) * scale / physical_size[1] as f32
                                    * 2.0,
                            ],
                            local: [x * rect.width, y * rect.height],
                            size: [rect.width, rect.height],
                            color: color.linear(),
                            radius: *radius,
                        });
                    }
                    draws.push(Draw::Quad {
                        start,
                        clip: scissor,
                    });
                }
                Primitive::Text {
                    rect, text, color, ..
                }
                | Primitive::StyledText {
                    rect, text, color, ..
                } => {
                    let key = match primitive {
                        Primitive::Text { font_size, .. } => {
                            self.text_system.ensure(text, *font_size)
                        }
                        Primitive::StyledText { style, .. } => {
                            self.text_system
                                .ensure_styled(text, style, Some(rect.width))
                        }
                        _ => unreachable!(),
                    };
                    active_text.insert(key.clone());
                    let scissor = scissor(clip.intersect(*rect), scale, physical_size);
                    if scissor[2] == 0 || scissor[3] == 0 {
                        continue;
                    }
                    let index = text_items.len();
                    text_items.push(PreparedText {
                        key,
                        rect: *rect,
                        clip: scissor,
                        color: *color,
                    });
                    // Clips are attached to each text area. Adjacent text may be
                    // batched across clip commands, but never across a painted quad.
                    if let Some(Draw::Text(batch)) = draws.last() {
                        text_ranges[*batch].end = index + 1;
                    } else {
                        draws.push(Draw::Text(text_ranges.len()));
                        text_ranges.push(index..index + 1);
                    }
                }
            }
        }
        let text_count = text_ranges.len();
        for (index, range) in text_ranges.into_iter().enumerate() {
            if index == self.text_renderers.len() {
                self.text_renderers.push(TextRenderer::new(
                    &mut self.atlas,
                    device,
                    wgpu::MultisampleState::default(),
                    None,
                ));
                self.stats.text_renderer_allocations += 1;
            }
            let buffers = &self.text_system.buffers;
            let areas = text_items[range].iter().map(|item| TextArea {
                buffer: &buffers[&item.key],
                left: item.rect.x * scale,
                top: item.rect.y * scale,
                scale,
                bounds: TextBounds {
                    left: item.clip[0] as i32,
                    top: item.clip[1] as i32,
                    right: (item.clip[0] + item.clip[2]) as i32,
                    bottom: (item.clip[1] + item.clip[3]) as i32,
                },
                default_color: glyphon::Color::rgba(
                    item.color.0,
                    item.color.1,
                    item.color.2,
                    item.color.3,
                ),
                custom_glyphs: &[],
            });
            self.text_renderers[index].prepare(
                device,
                queue,
                &mut self.text_system.fonts,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash,
            )?;
        }
        // A bounded spare pool avoids recreating GPU buffers when small sections
        // appear/disappear; exceptionally large past scenes do not remain retained.
        self.text_renderers.truncate(text_count.max(16));
        self.text_system.retain(&active_text);
        let bytes = bytemuck::cast_slice(&vertices);
        if !bytes.is_empty() {
            if bytes.len() as u64 > self.stats.quad_buffer_bytes {
                let capacity = (bytes.len() as u64).next_power_of_two().max(256);
                self.quad_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rust-gui retained quad vertices"),
                    size: capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                self.stats.quad_buffer_bytes = capacity;
                self.stats.quad_buffer_allocations += 1;
            }
            queue.write_buffer(self.quad_buffer.as_ref().unwrap(), 0, bytes);
        }
        self.vertices = vertices;
        self.draws = draws;
        self.prepared_target = Some(target_key);
        self.stats.preparations += 1;
        self.stats.text_batches = text_count;
        self.draw(device, queue, target, physical_size, clear)
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        physical_size: [u32; 2],
        clear: Color,
    ) -> RenderResult<()> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rust-gui frame"),
        });
        {
            let clear = clear.linear();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rust-gui scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(clear[0]),
                            g: f64::from(clear[1]),
                            b: f64::from(clear[2]),
                            a: f64::from(clear[3]),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for draw in &self.draws {
                match draw {
                    Draw::Quad { start, clip } => {
                        pass.set_pipeline(&self.pipeline);
                        pass.set_vertex_buffer(0, self.quad_buffer.as_ref().unwrap().slice(..));
                        pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                        pass.draw(*start..*start + 6, 0..1);
                    }
                    Draw::Text(index) => {
                        pass.set_scissor_rect(0, 0, physical_size[0], physical_size[1]);
                        self.text_renderers[*index].render(
                            &self.atlas,
                            &self.viewport,
                            &mut pass,
                        )?;
                    }
                }
            }
        }
        queue.submit(Some(encoder.finish()));
        self.atlas.trim();
        Ok(())
    }
}

fn scissor(rect: Rect, scale: f32, size: [u32; 2]) -> [u32; 4] {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return [0; 4];
    }
    let x = (rect.x * scale).floor().max(0.0).min(size[0] as f32) as u32;
    let y = (rect.y * scale).floor().max(0.0).min(size[1] as f32) as u32;
    let right = ((rect.x + rect.width) * scale)
        .ceil()
        .max(0.0)
        .min(size[0] as f32) as u32;
    let bottom = ((rect.y + rect.height) * scale)
        .ceil()
        .max(0.0)
        .min(size[1] as f32) as u32;
    [x, y, right.saturating_sub(x), bottom.saturating_sub(y)]
}
