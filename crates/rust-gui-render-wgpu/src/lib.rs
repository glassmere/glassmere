//! GPU scene backend. This crate knows paint primitives, never widgets.
mod image;
mod profile;
mod text;
pub use profile::{DriverMemory, GpuPassTime, GpuProfile, MAX_TIMED_PASSES, MemoryStats};
pub use text::TextSystem;
pub use wgpu;

use glyphon::{
    Cache, Resolution, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use rust_gui_scene::{Color, Point, Primitive, Rect, Scene};
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
    Quad {
        start: u32,
        clip: [u32; 4],
    },
    Text(usize),
    /// Consecutive images from one texture under one clip, as a vertex range
    /// of the image buffer.
    Image {
        texture: u32,
        start: u32,
        count: u32,
        clip: [u32; 4],
    },
    /// Composites a child surface. Its vertices reuse `Vertex`: `local` is the
    /// uv and `color[3]` the group opacity.
    Layer {
        surface: usize,
        start: u32,
        clip: [u32; 4],
    },
}

struct PreparedText {
    key: text::TextKey,
    left: f32,
    top: f32,
    scale: f32,
    clip: [u32; 4],
    color: Color,
}

/// One render pass: the target (index 0) or an offscreen layer. Children always
/// have a larger index than their parent, so passes run in reverse order.
struct Surface {
    size: [u32; 2],
    /// Logical position of the surface's top-left pixel.
    origin: [f32; 2],
    scale: f32,
    /// Index into the layer pool; `None` for the target.
    texture: Option<usize>,
    draws: Vec<Draw>,
}

struct LayerTexture {
    size: [u32; 2],
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    /// glyphon needs the resolution of the surface it draws into.
    viewport: Viewport,
    /// Preparations since this texture was last used.
    idle: u32,
}

struct LayerFrame {
    parent: usize,
    clip_depth: usize,
    /// `None` for layers drawn inline as a clip.
    composite: Option<Draw>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub preparations: u64,
    pub quad_buffer_allocations: u64,
    pub quad_buffer_bytes: u64,
    pub text_batches: usize,
    pub text_renderer_allocations: u64,
    /// Offscreen layer passes executed so far, replays included.
    pub layer_passes: u64,
    /// Pooled layer textures currently alive, and their size.
    pub layer_textures: usize,
    pub layer_texture_bytes: u64,
    /// Batched image draw calls in the prepared frame.
    pub image_draws: usize,
    /// Image uploads so far; a cached image does not upload again.
    pub image_uploads: u64,
    /// Shared atlas pages, standalone textures for large images, and the size
    /// of both.
    pub atlas_pages: usize,
    pub image_textures: usize,
    pub image_bytes: u64,
    /// Frames submitted so far, cached replays included.
    pub frames: u64,
    /// In the prepared frame: render passes (the target plus one per offscreen
    /// layer), draw calls the renderer issues, and how many of those are quads.
    /// A text batch counts as one draw call; glyphon's own draws inside it are
    /// not visible from here. Layer composites and image batches are one each.
    pub passes: usize,
    pub draw_calls: usize,
    pub quads: usize,
    /// GPU memory the renderer allocated, by category.
    pub memory: MemoryStats,
}

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    layer_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    images: image::ImageCache,
    image_buffer: Option<wgpu::Buffer>,
    image_buffer_bytes: u64,
    image_vertices: Vec<image::ImageVertex>,
    sampler: wgpu::Sampler,
    format: wgpu::TextureFormat,
    text_system: TextSystem,
    swash: SwashCache,
    cache: Cache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderers: Vec<TextRenderer>,
    quad_buffer: Option<wgpu::Buffer>,
    vertices: Vec<Vertex>,
    surfaces: Vec<Surface>,
    pool: Vec<LayerTexture>,
    prepared_target: Option<([u32; 2], u32)>,
    stats: RenderStats,
    profiling: bool,
    /// Created by the first profiled frame on a device with timestamp queries.
    timestamps: Option<profile::Timestamps>,
    timestamps_supported: bool,
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
        let pipeline =
            |label, source: &str, blend, stride: usize, attributes: &[wgpu::VertexAttribute]| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl(source.into()),
                });
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: None,
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: stride as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes,
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(blend),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let quad_attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4, 4 => Float32];
        let quad_stride = std::mem::size_of::<Vertex>();
        let cache = Cache::new(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rust-gui layers"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Image textures hold premultiplied colour, like layers.
        let image_pipeline = pipeline(
            "rust-gui images",
            include_str!("image.wgsl"),
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            std::mem::size_of::<image::ImageVertex>(),
            &image::ImageVertex::ATTRIBUTES,
        );
        Self {
            images: image::ImageCache::new(
                device,
                image_pipeline.get_bind_group_layout(0),
                sampler.clone(),
            ),
            image_pipeline,
            image_buffer: None,
            image_buffer_bytes: 0,
            image_vertices: Vec::new(),
            pipeline: pipeline(
                "rust-gui quads",
                include_str!("quad.wgsl"),
                wgpu::BlendState::ALPHA_BLENDING,
                quad_stride,
                &quad_attributes,
            ),
            // Alpha blending into a transparent layer leaves premultiplied color.
            layer_pipeline: pipeline(
                "rust-gui layers",
                include_str!("layer.wgsl"),
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                quad_stride,
                &quad_attributes,
            ),
            sampler,
            format,
            text_system,
            swash: SwashCache::new(),
            viewport: Viewport::new(device, &cache),
            atlas: TextAtlas::new(device, queue, &cache, format),
            cache,
            text_renderers: Vec::new(),
            quad_buffer: None,
            vertices: Vec::new(),
            surfaces: Vec::new(),
            pool: Vec::new(),
            prepared_target: None,
            stats: RenderStats::default(),
            profiling: false,
            timestamps: None,
            timestamps_supported: device.features().contains(wgpu::Features::TIMESTAMP_QUERY),
        }
    }

    /// Switches GPU pass timing on or off; off by default. While off nothing
    /// is written to query sets, nothing is read back and the buffers used for
    /// it are released. Hosts switch it on while an inspector is open.
    pub fn set_profiling(&mut self, enabled: bool) {
        self.profiling = enabled;
        if !enabled {
            self.timestamps = None;
            self.stats.memory.profiling = 0;
            self.stats.memory.settle();
        }
    }

    pub fn profiling(&self) -> bool {
        self.profiling
    }

    /// Whether the device was created with `wgpu::Features::TIMESTAMP_QUERY`,
    /// which the host has to request. Without it there are no GPU times and
    /// the CPU time around `render` is all a host can show.
    pub fn timestamps_supported(&self) -> bool {
        self.timestamps_supported
    }

    /// GPU times of the most recent profiled frame that has come back.
    pub fn gpu_profile(&self) -> Option<&GpuProfile> {
        self.timestamps.as_ref()?.latest.as_ref()
    }

    /// Starts the memory high-water mark again from the current total.
    pub fn reset_peak(&mut self) {
        self.stats.memory.peak = self.stats.memory.total;
    }

    /// The driver allocator's figures for the whole device, where the backend
    /// has such an allocator; show it beside `RenderStats::memory`, which is
    /// this renderer alone.
    pub fn driver_memory(device: &wgpu::Device) -> Option<DriverMemory> {
        let report = device.generate_allocator_report()?;
        Some(DriverMemory {
            allocated: report.total_allocated_bytes,
            reserved: report.total_reserved_bytes,
            allocations: report.allocations.len(),
        })
    }

    pub fn text_system(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// GPU memory images may occupy before textures the last preparation did
    /// not draw are released early. 256 MiB by default. Images on screen are
    /// never dropped, so a frame that needs more than this still renders.
    pub fn set_image_budget(&mut self, bytes: u64) {
        self.images.budget = bytes;
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
            return self.draw(device, queue, target, clear);
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
        let mut image_vertices = std::mem::take(&mut self.image_vertices);
        image_vertices.clear();
        self.images.begin();
        let mut draws = self
            .surfaces
            .first_mut()
            .map(|root| std::mem::take(&mut root.draws))
            .unwrap_or_default();
        draws.clear();
        let mut surfaces = vec![Surface {
            size: physical_size,
            origin: [0.0; 2],
            scale,
            texture: None,
            draws,
        }];
        let mut current = 0;
        let mut frames: Vec<LayerFrame> = Vec::new();
        // Layer textures survive two preparations without use, then are released.
        self.pool.retain(|texture| texture.idle < 2);
        for texture in &mut self.pool {
            texture.idle += 1;
        }
        let limit = device.limits().max_texture_dimension_2d;
        let cap = physical_size.map(|side| side.saturating_mul(2).min(limit));
        let mut text_items = Vec::new();
        let mut text_ranges: Vec<(usize, std::ops::Range<usize>)> = Vec::new();
        let mut active_text = HashSet::new();
        let close = Primitive::PopLayer;
        let mut primitives = scene.primitives.iter();
        loop {
            let primitive = match primitives.next() {
                Some(primitive) => primitive,
                None if frames.is_empty() => break,
                // Layers left open still composite.
                None => &close,
            };
            let clip = *clips.last().unwrap();
            match primitive {
                Primitive::PushClip(rect) => clips.push(clip.intersect(*rect)),
                Primitive::PopClip => {
                    if clips.len() > frames.last().map_or(1, |frame| frame.clip_depth + 1) {
                        clips.pop();
                    }
                }
                Primitive::PushLayer {
                    bounds,
                    opacity,
                    transform,
                } => {
                    let mut frame = LayerFrame {
                        parent: current,
                        clip_depth: clips.len(),
                        composite: None,
                    };
                    if *opacity >= 1.0 && transform.is_identity() {
                        clips.push(clip.intersect(*bounds));
                        frames.push(frame);
                        continue;
                    }
                    let parent = &surfaces[current];
                    let (origin, scale) = (parent.origin, parent.scale);
                    // Snapped to the parent's pixel grid, so an untransformed
                    // composite maps texels onto pixels one to one.
                    let x = ((bounds.x - origin[0]) * scale).floor();
                    let y = ((bounds.y - origin[1]) * scale).floor();
                    let width =
                        (((bounds.x + bounds.width - origin[0]) * scale).ceil() - x).max(1.0);
                    let height =
                        (((bounds.y + bounds.height - origin[1]) * scale).ceil() - y).max(1.0);
                    // Oversized layers render at a lower resolution instead.
                    let fit = (cap[0] as f32 / width).min(cap[1] as f32 / height).min(1.0);
                    let size = [
                        ((width * fit).ceil() as u32).clamp(1, cap[0]),
                        ((height * fit).ceil() as u32).clamp(1, cap[1]),
                    ];
                    let child_origin = [origin[0] + x / scale, origin[1] + y / scale];
                    let child_scale = scale * fit;
                    let corners = [
                        [0.0, 0.0],
                        [1.0, 0.0],
                        [0.0, 1.0],
                        [0.0, 1.0],
                        [1.0, 0.0],
                        [1.0, 1.0],
                    ]
                    .map(|[u, v]| {
                        let point = transform.apply(Point {
                            x: child_origin[0] + u * size[0] as f32 / child_scale,
                            y: child_origin[1] + v * size[1] as f32 / child_scale,
                        });
                        (u, v, point)
                    });
                    let min = |f: fn(Point) -> f32| {
                        corners
                            .iter()
                            .map(|corner| f(corner.2))
                            .fold(f32::INFINITY, f32::min)
                    };
                    let (left, top) = (min(|p| p.x), min(|p| p.y));
                    let covered = Rect {
                        x: left,
                        y: top,
                        width: -min(|p| -p.x) - left,
                        height: -min(|p| -p.y) - top,
                    };
                    // The clip at the push bounds the composite; rotated groups
                    // are scissored by their bounding box.
                    let composite_clip = scissor(clip.intersect(covered), parent);
                    if !(*opacity > 0.0 && bounds.width > 0.0 && bounds.height > 0.0)
                        || composite_clip[2] == 0
                        || composite_clip[3] == 0
                    {
                        // Invisible: an empty clip culls the content.
                        clips.push(Rect {
                            x: 0.0,
                            y: 0.0,
                            width: 0.0,
                            height: 0.0,
                        });
                        frames.push(frame);
                        continue;
                    }
                    let start = vertices.len() as u32;
                    vertices.extend(corners.map(|(u, v, point)| Vertex {
                        position: [
                            (point.x - origin[0]) * scale / parent.size[0] as f32 * 2.0 - 1.0,
                            1.0 - (point.y - origin[1]) * scale / parent.size[1] as f32 * 2.0,
                        ],
                        local: [u, v],
                        size: [0.0; 2],
                        color: [0.0, 0.0, 0.0, opacity.min(1.0)],
                        radius: 0.0,
                    }));
                    let texture = match self
                        .pool
                        .iter()
                        .position(|texture| texture.idle > 0 && texture.size == size)
                    {
                        Some(index) => index,
                        None => {
                            let created = self.layer_texture(device, queue, size);
                            self.pool.push(created);
                            self.pool.len() - 1
                        }
                    };
                    self.pool[texture].idle = 0;
                    frame.composite = Some(Draw::Layer {
                        surface: surfaces.len(),
                        start,
                        clip: composite_clip,
                    });
                    frames.push(frame);
                    clips.push(*bounds);
                    current = surfaces.len();
                    surfaces.push(Surface {
                        size,
                        origin: child_origin,
                        scale: child_scale,
                        texture: Some(texture),
                        draws: Vec::new(),
                    });
                }
                Primitive::PopLayer => {
                    if let Some(frame) = frames.pop() {
                        clips.truncate(frame.clip_depth);
                        current = frame.parent;
                        surfaces[current].draws.extend(frame.composite);
                    }
                }
                Primitive::Quad {
                    rect,
                    color,
                    radius,
                } => {
                    let surface = &mut surfaces[current];
                    let scissor = scissor(clip.intersect(*rect), surface);
                    if scissor[2] == 0 || scissor[3] == 0 {
                        continue;
                    }
                    let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
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
                                ((rect.x - origin[0]) + x * rect.width) * scale / size[0] as f32
                                    * 2.0
                                    - 1.0,
                                1.0 - ((rect.y - origin[1]) + y * rect.height) * scale
                                    / size[1] as f32
                                    * 2.0,
                            ],
                            local: [x * rect.width, y * rect.height],
                            size: [rect.width, rect.height],
                            color: color.linear(),
                            radius: *radius,
                        });
                    }
                    surface.draws.push(Draw::Quad {
                        start,
                        clip: scissor,
                    });
                }
                Primitive::Image {
                    rect,
                    image,
                    source,
                    tint,
                    radius,
                } => {
                    let surface = &mut surfaces[current];
                    let visible = scissor(clip.intersect(*rect), surface);
                    if visible[2] == 0
                        || visible[3] == 0
                        || source.width == 0.0
                        || source.height == 0.0
                    {
                        continue;
                    }
                    let raster = self
                        .images
                        .raster_size(image, *rect, *source, surface.scale);
                    let Some(place) = self.images.ensure(device, queue, image, raster) else {
                        continue;
                    };
                    // `source` in texels of the placement, which differ from the
                    // image's units for vector rasters and downscaled photos.
                    let natural = image.size();
                    let texels = |at: f32, length: f32, natural: f32, axis: usize| {
                        let (start, side) = (place.rect[axis] as f32, place.rect[axis + 2] as f32);
                        let from = start + at * side / natural;
                        let to = from + length * side / natural;
                        // Half a texel inside both the source and the image, so
                        // filtering blends in nothing from beyond either.
                        let low = from.min(to).max(start) + 0.5;
                        let high = from.max(to).min(start + side) - 0.5;
                        let (low, high) = if low > high {
                            ((low + high) / 2.0, (low + high) / 2.0)
                        } else {
                            (low, high)
                        };
                        [from, to, low, high].map(|texel| texel / place.texture_size[axis] as f32)
                    };
                    let u = texels(source.x, source.width, natural.width, 0);
                    let v = texels(source.y, source.height, natural.height, 1);
                    let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
                    let start = image_vertices.len() as u32;
                    for [x, y] in [
                        [0.0, 0.0],
                        [1.0, 0.0],
                        [0.0, 1.0],
                        [0.0, 1.0],
                        [1.0, 0.0],
                        [1.0, 1.0],
                    ] {
                        image_vertices.push(image::ImageVertex {
                            position: [
                                ((rect.x - origin[0]) + x * rect.width) * scale / size[0] as f32
                                    * 2.0
                                    - 1.0,
                                1.0 - ((rect.y - origin[1]) + y * rect.height) * scale
                                    / size[1] as f32
                                    * 2.0,
                            ],
                            uv: [u[0] + x * (u[1] - u[0]), v[0] + y * (v[1] - v[0])],
                            local: [x * rect.width, y * rect.height],
                            size: [rect.width, rect.height],
                            tint: tint.map_or([0.0, 0.0, 0.0, 1.0], Color::linear),
                            params: [*radius, f32::from(u8::from(tint.is_some()))],
                            bounds: [u[2], v[2], u[3], v[3]],
                        });
                    }
                    // The scissor is the clip alone, so neighbours can share a draw.
                    let clip = scissor(clip, surface);
                    match surface.draws.last_mut() {
                        Some(Draw::Image {
                            texture,
                            start: first,
                            count,
                            clip: last,
                        }) if *texture == place.texture
                            && *last == clip
                            && *first + *count == start =>
                        {
                            *count += 6
                        }
                        _ => surface.draws.push(Draw::Image {
                            texture: place.texture,
                            start,
                            count: 6,
                            clip,
                        }),
                    }
                }
                Primitive::Text { rect, color, .. }
                | Primitive::StyledText { rect, color, .. }
                | Primitive::RichText { rect, color, .. } => {
                    let key = match primitive {
                        Primitive::Text {
                            text, font_size, ..
                        } => self.text_system.ensure(text, *font_size),
                        Primitive::StyledText { text, style, .. } => self
                            .text_system
                            .ensure_styled(text, style, Some(rect.width)),
                        Primitive::RichText { spans, style, .. } => {
                            self.text_system.ensure_rich(spans, style, Some(rect.width))
                        }
                        _ => unreachable!(),
                    };
                    active_text.insert(key.clone());
                    let surface = &mut surfaces[current];
                    let scissor = scissor(clip.intersect(*rect), surface);
                    if scissor[2] == 0 || scissor[3] == 0 {
                        continue;
                    }
                    let index = text_items.len();
                    text_items.push(PreparedText {
                        key,
                        left: (rect.x - surface.origin[0]) * surface.scale,
                        top: (rect.y - surface.origin[1]) * surface.scale,
                        scale: surface.scale,
                        clip: scissor,
                        color: *color,
                    });
                    // Clips are attached to each text area. Adjacent text may be
                    // batched across clip commands, but never across a painted quad
                    // or a composited layer.
                    if let Some(Draw::Text(batch)) = surface.draws.last() {
                        text_ranges[*batch].1.end = index + 1;
                    } else {
                        surface.draws.push(Draw::Text(text_ranges.len()));
                        text_ranges.push((current, index..index + 1));
                    }
                }
            }
        }
        let text_count = text_ranges.len();
        for (index, (surface, range)) in text_ranges.into_iter().enumerate() {
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
                left: item.left,
                top: item.top,
                scale: item.scale,
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
            let viewport = match surfaces[surface].texture {
                Some(texture) => &self.pool[texture].viewport,
                None => &self.viewport,
            };
            self.text_renderers[index].prepare(
                device,
                queue,
                &mut self.text_system.fonts,
                &mut self.atlas,
                viewport,
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
        let bytes = bytemuck::cast_slice(&image_vertices);
        if !bytes.is_empty() {
            if bytes.len() as u64 > self.image_buffer_bytes {
                self.image_buffer_bytes = (bytes.len() as u64).next_power_of_two();
                self.image_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rust-gui retained image vertices"),
                    size: self.image_buffer_bytes,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
            queue.write_buffer(self.image_buffer.as_ref().unwrap(), 0, bytes);
        }
        self.image_vertices = image_vertices;
        let count = |wanted: fn(&Draw) -> bool| {
            surfaces
                .iter()
                .flat_map(|surface| &surface.draws)
                .filter(|draw| wanted(draw))
                .count()
        };
        self.stats.image_draws = count(|draw| matches!(draw, Draw::Image { .. }));
        self.stats.quads = count(|draw| matches!(draw, Draw::Quad { .. }));
        self.stats.draw_calls = count(|_| true);
        self.stats.passes = surfaces.len();
        self.stats.image_uploads = self.images.uploads;
        (self.stats.atlas_pages, self.stats.image_textures) = self.images.counts();
        self.stats.image_bytes = self.images.bytes();
        self.vertices = vertices;
        self.surfaces = surfaces;
        self.prepared_target = Some(target_key);
        self.stats.preparations += 1;
        self.stats.text_batches = text_count;
        self.stats.layer_textures = self.pool.len();
        let texel = u64::from(self.format.block_copy_size(None).unwrap_or(4));
        self.stats.layer_texture_bytes = self
            .pool
            .iter()
            .map(|texture| u64::from(texture.size[0]) * u64::from(texture.size[1]) * texel)
            .sum();
        let memory = &mut self.stats.memory;
        memory.buffers = self.stats.quad_buffer_bytes + self.image_buffer_bytes;
        (memory.image_atlas, memory.image_textures) = self.images.bytes_by_kind();
        memory.layers = self.stats.layer_texture_bytes;
        memory.settle();
        self.draw(device, queue, target, clear)
    }

    fn layer_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
    ) -> LayerTexture {
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("rust-gui layer"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rust-gui layer"),
            layout: &self.layer_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut viewport = Viewport::new(device, &self.cache);
        viewport.update(
            queue,
            Resolution {
                width: size[0],
                height: size[1],
            },
        );
        LayerTexture {
            size,
            view,
            bind_group,
            viewport,
            idle: 0,
        }
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        clear: Color,
    ) -> RenderResult<()> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rust-gui frame"),
        });
        self.stats.frames += 1;
        if self.profiling && self.timestamps_supported {
            self.timestamps
                .get_or_insert_with(|| profile::Timestamps::new(device, queue))
                .collect(device);
        }
        let mut timed = Vec::new();
        // Children first: every layer is complete before its parent samples it.
        for surface in self.surfaces.iter().rev() {
            let (view, viewport, clear) = match surface.texture {
                Some(texture) => {
                    self.stats.layer_passes += 1;
                    let texture = &self.pool[texture];
                    (&texture.view, &texture.viewport, [0.0; 4])
                }
                None => (target, &self.viewport, clear.linear()),
            };
            let timestamp_writes = self
                .timestamps
                .as_ref()
                .filter(|_| timed.len() < profile::MAX_TIMED_PASSES)
                .map(|timestamps| {
                    let index = timed.len() as u32 * 2;
                    timed.push(match surface.texture {
                        Some(_) => format!("layer {}x{}", surface.size[0], surface.size[1]),
                        None => "main".to_owned(),
                    });
                    wgpu::RenderPassTimestampWrites {
                        query_set: &timestamps.queries,
                        beginning_of_pass_write_index: Some(index),
                        end_of_pass_write_index: Some(index + 1),
                    }
                });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rust-gui scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for draw in &surface.draws {
                match draw {
                    Draw::Quad { start, clip } => {
                        pass.set_pipeline(&self.pipeline);
                        pass.set_vertex_buffer(0, self.quad_buffer.as_ref().unwrap().slice(..));
                        pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                        pass.draw(*start..*start + 6, 0..1);
                    }
                    Draw::Text(index) => {
                        pass.set_scissor_rect(0, 0, surface.size[0], surface.size[1]);
                        self.text_renderers[*index].render(&self.atlas, viewport, &mut pass)?;
                    }
                    Draw::Image {
                        texture,
                        start,
                        count,
                        clip,
                    } => {
                        pass.set_pipeline(&self.image_pipeline);
                        pass.set_bind_group(0, self.images.bind_group(*texture), &[]);
                        pass.set_vertex_buffer(0, self.image_buffer.as_ref().unwrap().slice(..));
                        pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Draw::Layer {
                        surface: child,
                        start,
                        clip,
                    } => {
                        let texture = self.surfaces[*child].texture.unwrap();
                        pass.set_pipeline(&self.layer_pipeline);
                        pass.set_bind_group(0, &self.pool[texture].bind_group, &[]);
                        pass.set_vertex_buffer(0, self.quad_buffer.as_ref().unwrap().slice(..));
                        pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                        pass.draw(*start..*start + 6, 0..1);
                    }
                }
            }
        }
        let frame = self.stats.frames;
        let slot = self
            .timestamps
            .as_mut()
            .filter(|_| !timed.is_empty())
            .map(|timestamps| timestamps.resolve(device, &mut encoder, frame, timed));
        queue.submit(Some(encoder.finish()));
        if let (Some(timestamps), Some(slot)) = (&mut self.timestamps, slot) {
            // Mapping is asked for after the submission that fills the buffer;
            // a later frame's `collect` picks the result up.
            timestamps.submitted(slot);
            self.stats.memory.profiling = timestamps.bytes();
            self.stats.memory.settle();
        }
        self.atlas.trim();
        Ok(())
    }
}

/// `rect` in the surface's own physical pixels, rounded outward and clamped.
fn scissor(rect: Rect, surface: &Surface) -> [u32; 4] {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return [0; 4];
    }
    let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
    let x = ((rect.x - origin[0]) * scale)
        .floor()
        .max(0.0)
        .min(size[0] as f32) as u32;
    let y = ((rect.y - origin[1]) * scale)
        .floor()
        .max(0.0)
        .min(size[1] as f32) as u32;
    let right = ((rect.x + rect.width - origin[0]) * scale)
        .ceil()
        .max(0.0)
        .min(size[0] as f32) as u32;
    let bottom = ((rect.y + rect.height - origin[1]) * scale)
        .ceil()
        .max(0.0)
        .min(size[1] as f32) as u32;
    [x, y, right.saturating_sub(x), bottom.saturating_sub(y)]
}
