//! GPU scene backend. This crate knows paint primitives, never widgets.
mod image;
mod path;
mod profile;
pub use glassmere_text::TextSystem;
pub use profile::{DriverMemory, GpuPassTime, GpuProfile, MAX_TIMED_PASSES, MemoryStats};
pub use wgpu;

use glassmere_scene::{Brush, Color, GradientStop, Point, Primitive, Rect, Scene};
use glyphon::{
    Cache, Resolution, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
};

pub type RenderResult<T> = Result<T, Box<dyn Error>>;

/// Gradient stops one ramp can hold. A ramp given more keeps the first eight.
pub const MAX_GRADIENT_STOPS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    local: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    radius: f32,
}

// The stops ride along in every vertex, which is wide but keeps the
// ramp to one buffer and no bind group. A uniform buffer if scenes ever hold
// thousands of gradients.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GradientVertex {
    position: [f32; 2],
    /// Position inside the rectangle, then its size, in logical pixels.
    local: [f32; 4],
    /// Where the ramp starts and ends, in the rectangle's own units.
    ends: [f32; 4],
    /// Where the stops sit; slots past the last stop repeat it.
    offsets: [f32; MAX_GRADIENT_STOPS],
    /// Corner radius, 1 for a radial ramp, then 1 for a path's triangles and
    /// how much of the pixel the path covers at this corner.
    shape: [f32; 4],
    /// The stop colours, in linear light.
    colors: [[f32; 4]; MAX_GRADIENT_STOPS],
}

impl GradientVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 14] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4,
        5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4,
        10 => Float32x4, 11 => Float32x4, 12 => Float32x4, 13 => Float32x4
    ];
}

/// The offsets and linear colours of `stops`, of which there is at least one.
/// Stops past the eighth are dropped; the slots left over repeat the last one
/// and mix into nothing.
fn ramp(stops: &[GradientStop]) -> ([f32; MAX_GRADIENT_STOPS], [[f32; 4]; MAX_GRADIENT_STOPS]) {
    let kept = stops.len().min(MAX_GRADIENT_STOPS);
    let mut offsets = [0.0; MAX_GRADIENT_STOPS];
    let mut colors = [[0.0; 4]; MAX_GRADIENT_STOPS];
    for (index, (offset, color)) in offsets.iter_mut().zip(&mut colors).enumerate() {
        let stop = stops[index.min(kept - 1)];
        *offset = stop.at;
        *color = stop.color.linear();
    }
    (offsets, colors)
}

/// A corner of a path's triangles; the alpha is the colour's times how much
/// of the pixel the shape covers there.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PathVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl PathVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowVertex {
    position: [f32; 2],
    /// Position relative to the blurred rectangle's centre, then its half extents.
    outer: [f32; 4],
    /// The same for the rectangle an inset shadow darkens the inside of.
    inner: [f32; 4],
    /// Blur standard deviation, 1 for an inset shadow, then the corner radius
    /// of the blurred rectangle and of the original one.
    params: [f32; 4],
    color: [f32; 4],
}

impl ShadowVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4
    ];
}

/// An application's own drawing for one `Primitive::Custom` id.
pub type CustomDrawFn = Box<dyn FnMut(CustomDraw<'_>)>;

/// What an application's own wgpu code is handed for a `Primitive::Custom`
/// (see `Renderer::set_custom_draw`). The pass is scissored to the primitive's
/// clipped rectangle and the renderer restores its own state afterwards.
#[non_exhaustive]
pub struct CustomDraw<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub pass: &'a mut wgpu::RenderPass<'static>,
    /// The primitive's rectangle, in the logical pixels of the surface being
    /// drawn: the scene's own coordinates, unless this is inside a layer that
    /// renders offscreen.
    pub rect: Rect,
    /// Physical pixels per logical one, and the size of what is drawn into.
    pub scale: f32,
    pub target_size: [u32; 2],
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
    /// A rectangle's two triangles, or a path's, as a vertex range of the
    /// gradient buffer.
    Gradient {
        start: u32,
        count: u32,
        clip: [u32; 4],
    },
    Shadow {
        start: u32,
        clip: [u32; 4],
    },
    /// One path's triangles, as a vertex range of the path buffer.
    Path {
        start: u32,
        count: u32,
        clip: [u32; 4],
    },
    /// The application's own drawing. Nothing happens while no callback is
    /// registered for `id`, this frame or a later replay of it.
    Custom {
        id: u64,
        rect: Rect,
        clip: [u32; 4],
    },
}

struct PreparedText {
    key: glassmere_text::TextKey,
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

/// The two triangles of a unit square, as corner offsets.
const UNIT_QUAD: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [0.0, 1.0],
    [0.0, 1.0],
    [1.0, 0.0],
    [1.0, 1.0],
];

/// What one preparation collects while it walks the scene: the clip and layer
/// stacks, the surfaces with their draw lists, the vertices and the text.
struct Recording {
    clips: Vec<Rect>,
    frames: Vec<LayerFrame>,
    /// The surface primitives are drawn into.
    current: usize,
    surfaces: Vec<Surface>,
    /// Largest size a layer may render at.
    cap: [u32; 2],
    vertices: Vec<Vertex>,
    image_vertices: Vec<image::ImageVertex>,
    gradient_vertices: Vec<GradientVertex>,
    shadow_vertices: Vec<ShadowVertex>,
    path_vertices: Vec<PathVertex>,
    /// Tessellation output, kept between paths.
    corners: Vec<path::Corner>,
    text_items: Vec<PreparedText>,
    /// Per text batch: its surface and its span of `text_items`.
    text_ranges: Vec<(usize, std::ops::Range<usize>)>,
    active_text: HashSet<glassmere_text::TextKey>,
}

impl Recording {
    fn quad(&mut self, clip: Rect, rect: Rect, color: Color, radius: f32) {
        let surface = &mut self.surfaces[self.current];
        let scissor = scissor(clip.intersect(rect), surface);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
        let start = self.vertices.len() as u32;
        for [x, y] in UNIT_QUAD {
            self.vertices.push(Vertex {
                position: [
                    ((rect.x - origin[0]) + x * rect.width) * scale / size[0] as f32 * 2.0 - 1.0,
                    1.0 - ((rect.y - origin[1]) + y * rect.height) * scale / size[1] as f32 * 2.0,
                ],
                local: [x * rect.width, y * rect.height],
                size: [rect.width, rect.height],
                color: color.linear(),
                radius,
            });
        }
        surface.draws.push(Draw::Quad {
            start,
            clip: scissor,
        });
    }

    fn path(&mut self, clip: Rect, primitive: &Primitive) {
        let Primitive::Path { path, fill, stroke } = primitive else {
            return;
        };
        let surface = &mut self.surfaces[self.current];
        let reach = stroke.as_ref().map_or(0.0, path::reach) + 1.0 / surface.scale;
        let bounds = path.bounds();
        let bounds = Rect {
            x: bounds.x - reach,
            y: bounds.y - reach,
            width: bounds.width + reach * 2.0,
            height: bounds.height + reach * 2.0,
        };
        let visible = clip.intersect(bounds);
        let scissor = scissor(visible, surface);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let pixel = 1.0 / surface.scale;
        if let Some(fill) = fill {
            match &fill.brush {
                Brush::Solid(color) if color.3 > 0 => {
                    self.corners.clear();
                    path::fill(path, fill.rule, pixel, &mut self.corners);
                    self.push_path(*color, 1.0, scissor);
                }
                Brush::Linear { .. } | Brush::Radial { .. } => {
                    self.corners.clear();
                    path::fill(path, fill.rule, pixel, &mut self.corners);
                    self.push_path_ramp(path.bounds(), &fill.brush, scissor);
                }
                _ => {}
            }
        }
        if let Some(line) = stroke
            .as_ref()
            .filter(|line| line.color.3 > 0 && line.width > 0.0)
        {
            self.corners.clear();
            let faint = path::stroke(path, line, visible, pixel, &mut self.corners);
            self.push_path(line.color, faint, scissor);
        }
    }

    /// Draws the triangles in `corners`.
    fn push_path(&mut self, color: Color, faint: f32, scissor: [u32; 4]) {
        if self.corners.is_empty() {
            return;
        }
        let surface = &mut self.surfaces[self.current];
        let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
        let linear = color.linear();
        let start = self.path_vertices.len() as u32;
        self.path_vertices
            .extend(self.corners.iter().map(|(point, coverage)| PathVertex {
                position: [
                    (point.x - origin[0]) * scale / size[0] as f32 * 2.0 - 1.0,
                    1.0 - (point.y - origin[1]) * scale / size[1] as f32 * 2.0,
                ],
                color: [
                    linear[0],
                    linear[1],
                    linear[2],
                    linear[3] * coverage * faint,
                ],
            }));
        surface.draws.push(Draw::Path {
            start,
            count: self.corners.len() as u32,
            clip: scissor,
        });
    }

    fn gradient(&mut self, clip: Rect, primitive: &Primitive) {
        let Primitive::Gradient {
            rect,
            radius,
            from,
            to,
            stops,
            radial,
        } = primitive
        else {
            unreachable!()
        };
        let surface = &mut self.surfaces[self.current];
        let scissor = scissor(clip.intersect(*rect), surface);
        // A ramp needs two ends; one colour is a quad.
        if scissor[2] == 0 || scissor[3] == 0 || stops.len() < 2 {
            return;
        }
        let (offsets, colors) = ramp(stops);
        let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
        let start = self.gradient_vertices.len() as u32;
        for [x, y] in UNIT_QUAD {
            self.gradient_vertices.push(GradientVertex {
                position: [
                    ((rect.x - origin[0]) + x * rect.width) * scale / size[0] as f32 * 2.0 - 1.0,
                    1.0 - ((rect.y - origin[1]) + y * rect.height) * scale / size[1] as f32 * 2.0,
                ],
                local: [x * rect.width, y * rect.height, rect.width, rect.height],
                ends: [from.x, from.y, to.x, to.y],
                offsets,
                shape: [*radius, f32::from(u8::from(*radial)), 0.0, 1.0],
                colors,
            });
        }
        surface.draws.push(Draw::Gradient {
            start,
            count: 6,
            clip: scissor,
        });
    }

    /// Draws the triangles in `corners` with a ramp laid out over `bounds`.
    fn push_path_ramp(&mut self, bounds: Rect, brush: &Brush, scissor: [u32; 4]) {
        let (Brush::Linear { from, to, stops } | Brush::Radial { from, to, stops }) = brush else {
            return;
        };
        // A ramp needs two ends, and a shape needs an area to lay it over.
        if self.corners.is_empty() || stops.len() < 2 || bounds.width <= 0.0 || bounds.height <= 0.0
        {
            return;
        }
        let (offsets, colors) = ramp(stops);
        let radial = f32::from(u8::from(matches!(brush, Brush::Radial { .. })));
        let surface = &mut self.surfaces[self.current];
        let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
        let start = self.gradient_vertices.len() as u32;
        self.gradient_vertices
            .extend(self.corners.iter().map(|(point, coverage)| GradientVertex {
                position: [
                    (point.x - origin[0]) * scale / size[0] as f32 * 2.0 - 1.0,
                    1.0 - (point.y - origin[1]) * scale / size[1] as f32 * 2.0,
                ],
                local: [
                    point.x - bounds.x,
                    point.y - bounds.y,
                    bounds.width,
                    bounds.height,
                ],
                ends: [from.x, from.y, to.x, to.y],
                offsets,
                shape: [0.0, radial, 1.0, *coverage],
                colors,
            }));
        surface.draws.push(Draw::Gradient {
            start,
            count: self.corners.len() as u32,
            clip: scissor,
        });
    }

    fn shadow(&mut self, clip: Rect, primitive: &Primitive) {
        let Primitive::Shadow {
            rect,
            radius,
            color,
            blur,
            spread,
            offset,
            inset,
        } = primitive
        else {
            unreachable!()
        };
        let blur = blur.max(0.0);
        // The blurred rectangle: grown by the spread, moved by the
        // offset. A spread that eats it leaves nothing to blur.
        let half = [
            (rect.width / 2.0 + spread).max(0.0),
            (rect.height / 2.0 + spread).max(0.0),
        ];
        let centre = [
            rect.x + rect.width / 2.0 + offset.x,
            rect.y + rect.height / 2.0 + offset.y,
        ];
        let reach = 3.0 * blur;
        // A Gaussian is spent three standard deviations out, and an
        // inset shadow never leaves the rectangle it darkens.
        let drawn = if *inset {
            *rect
        } else {
            Rect {
                x: centre[0] - half[0] - reach,
                y: centre[1] - half[1] - reach,
                width: (half[0] + reach) * 2.0,
                height: (half[1] + reach) * 2.0,
            }
        };
        let surface = &mut self.surfaces[self.current];
        let scissor = scissor(clip.intersect(drawn), surface);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let inner = [rect.width / 2.0, rect.height / 2.0];
        let middle = [rect.x + inner[0], rect.y + inner[1]];
        let (origin, scale, size) = (surface.origin, surface.scale, surface.size);
        let start = self.shadow_vertices.len() as u32;
        for [x, y] in UNIT_QUAD {
            let point = [drawn.x + x * drawn.width, drawn.y + y * drawn.height];
            self.shadow_vertices.push(ShadowVertex {
                position: [
                    (point[0] - origin[0]) * scale / size[0] as f32 * 2.0 - 1.0,
                    1.0 - (point[1] - origin[1]) * scale / size[1] as f32 * 2.0,
                ],
                outer: [point[0] - centre[0], point[1] - centre[1], half[0], half[1]],
                inner: [
                    point[0] - middle[0],
                    point[1] - middle[1],
                    inner[0],
                    inner[1],
                ],
                params: [blur, f32::from(u8::from(*inset)), radius + spread, *radius],
                color: color.linear(),
            });
        }
        surface.draws.push(Draw::Shadow {
            start,
            clip: scissor,
        });
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
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
    /// Gradient and shadow draw calls in the prepared frame, one each.
    pub gradients: usize,
    pub shadows: usize,
    /// Path draw calls in the prepared frame: one for a fill, one for a stroke.
    /// A fill with a gradient counts as a gradient.
    pub paths: usize,
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
    /// not visible from here. Layer composites, image batches, gradients,
    /// shadows and custom draws are one each, a custom draw included while its
    /// id has no callback: those come and go between frames.
    pub passes: usize,
    pub draw_calls: usize,
    pub quads: usize,
    /// Physical pixels the last frame redrew: the whole target, or the damage
    /// the host asked for when the renderer could keep the rest.
    pub redrawn: u64,
    /// GPU memory the renderer allocated, by category.
    pub memory: MemoryStats,
}

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    layer_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    gradient_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    path_pipeline: wgpu::RenderPipeline,
    path_buffer: Option<wgpu::Buffer>,
    path_buffer_bytes: u64,
    path_vertices: Vec<PathVertex>,
    corners: Vec<path::Corner>,
    gradient_buffer: Option<wgpu::Buffer>,
    gradient_buffer_bytes: u64,
    gradient_vertices: Vec<GradientVertex>,
    shadow_buffer: Option<wgpu::Buffer>,
    shadow_buffer_bytes: u64,
    shadow_vertices: Vec<ShadowVertex>,
    custom: HashMap<u64, CustomDrawFn>,
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
    /// Holds the last frame at the size of the target, so a damaged frame can
    /// redraw part of it and keep the rest.
    canvas: Option<LayerTexture>,
    /// The target the canvas was drawn for. While it differs from the one at
    /// hand the canvas holds nothing usable and the frame must be redrawn whole.
    canvas_holds: Option<([u32; 2], u32)>,
    /// Start of the six vertices that put the canvas on the target.
    blit: u32,
    /// Paints the clear colour over a damaged rectangle, which a loaded
    /// attachment has no other way of getting.
    clear_pipeline: wgpu::RenderPipeline,
    clear_quad: Option<wgpu::Buffer>,
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
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: stride as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes,
                        })],
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
        // Replaces rather than blends: this is a clear, so what was there
        // has to go, whatever its alpha.
        let replace = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::Zero,
            operation: wgpu::BlendOperation::Add,
        };
        let clear_pipeline = pipeline(
            "glassmere damage clear",
            include_str!("quad.wgsl"),
            wgpu::BlendState {
                color: replace,
                alpha: replace,
            },
            quad_stride,
            &quad_attributes,
        );
        let cache = Cache::new(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glassmere layers"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Image textures hold premultiplied colour, like layers.
        let image_pipeline = pipeline(
            "glassmere images",
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
            // Gradients and shadows leave straight alpha, as quads do.
            gradient_pipeline: pipeline(
                "glassmere gradients",
                include_str!("gradient.wgsl"),
                wgpu::BlendState::ALPHA_BLENDING,
                std::mem::size_of::<GradientVertex>(),
                &GradientVertex::ATTRIBUTES,
            ),
            shadow_pipeline: pipeline(
                "glassmere shadows",
                include_str!("shadow.wgsl"),
                wgpu::BlendState::ALPHA_BLENDING,
                std::mem::size_of::<ShadowVertex>(),
                &ShadowVertex::ATTRIBUTES,
            ),
            // Coverage rides in the alpha, which stays straight as quads' does.
            path_pipeline: pipeline(
                "glassmere paths",
                include_str!("path.wgsl"),
                wgpu::BlendState::ALPHA_BLENDING,
                std::mem::size_of::<PathVertex>(),
                &PathVertex::ATTRIBUTES,
            ),
            path_buffer: None,
            path_buffer_bytes: 0,
            path_vertices: Vec::new(),
            corners: Vec::new(),
            gradient_buffer: None,
            gradient_buffer_bytes: 0,
            gradient_vertices: Vec::new(),
            shadow_buffer: None,
            shadow_buffer_bytes: 0,
            shadow_vertices: Vec::new(),
            custom: HashMap::new(),
            pipeline: pipeline(
                "glassmere quads",
                include_str!("quad.wgsl"),
                wgpu::BlendState::ALPHA_BLENDING,
                quad_stride,
                &quad_attributes,
            ),
            // Alpha blending into a transparent layer leaves premultiplied color.
            layer_pipeline: pipeline(
                "glassmere layers",
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
            canvas: None,
            canvas_holds: None,
            blit: 0,
            clear_pipeline,
            clear_quad: None,
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

    /// What draws a `Primitive::Custom` carrying `id`: the application's own
    /// wgpu code, inside the renderer's pass, in display-list order. `None`
    /// forgets the id again, and a primitive whose id is not registered draws
    /// nothing. Callbacks are not part of the prepared frame, so registering
    /// one takes effect on the next frame, replay or not.
    pub fn set_custom_draw(&mut self, id: u64, draw: Option<CustomDrawFn>) {
        match draw {
            Some(draw) => self.custom.insert(id, draw),
            None => self.custom.remove(&id),
        };
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
            &[],
        )
    }

    /// Hosts may set `scene_changed` to false only if no scene primitive changed
    /// since the previous successful call. Target size/scale changes force preparation.
    /// Exposure redraws then reuse glyph and quad buffers without uploading again.
    ///
    /// `damage` lists the logical rectangles that changed since the previous
    /// frame; empty means all of it. Where the renderer can honour damage it
    /// redraws their union alone and takes the rest of the frame from an
    /// offscreen canvas, so a host may only name damage that is the truth.
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
        damage: &[Rect],
    ) -> RenderResult<()> {
        if physical_size.contains(&0) || !scale.is_finite() || scale <= 0.0 {
            return Ok(());
        }
        let target_key = (physical_size, scale.to_bits());
        let damage = self.prepare_canvas(device, queue, target_key, damage);
        if !scene_changed && self.prepared_target == Some(target_key) {
            return self.draw(device, queue, target, clear, damage);
        }
        self.prepared_target = None;
        let mut recording = self.begin_recording(device, queue, physical_size, scale);
        self.record_scene(device, queue, &mut recording, scene);
        self.prepare_text(device, queue, &recording)?;
        self.upload_vertices(device, queue, &mut recording);
        self.finish_preparation(recording, target_key);
        self.draw(device, queue, target, clear, damage)
    }

    /// Sets up the buffers and the target's own surface for walking a scene,
    /// and ages the layer pool.
    fn begin_recording(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        physical_size: [u32; 2],
        scale: f32,
    ) -> Recording {
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
        let mut vertices = std::mem::take(&mut self.vertices);
        vertices.clear();
        let mut image_vertices = std::mem::take(&mut self.image_vertices);
        image_vertices.clear();
        let mut gradient_vertices = std::mem::take(&mut self.gradient_vertices);
        gradient_vertices.clear();
        let mut shadow_vertices = std::mem::take(&mut self.shadow_vertices);
        shadow_vertices.clear();
        let mut path_vertices = std::mem::take(&mut self.path_vertices);
        path_vertices.clear();
        self.images.begin();
        let mut draws = self
            .surfaces
            .first_mut()
            .map(|root| std::mem::take(&mut root.draws))
            .unwrap_or_default();
        draws.clear();
        // Layer textures survive two preparations without use, then are released.
        self.pool.retain(|texture| texture.idle < 2);
        for texture in &mut self.pool {
            texture.idle += 1;
        }
        let limit = device.limits().max_texture_dimension_2d;
        Recording {
            clips: vec![viewport],
            frames: Vec::new(),
            current: 0,
            surfaces: vec![Surface {
                size: physical_size,
                origin: [0.0; 2],
                scale,
                texture: None,
                draws,
            }],
            cap: physical_size.map(|side| side.saturating_mul(2).min(limit)),
            vertices,
            image_vertices,
            gradient_vertices,
            shadow_vertices,
            path_vertices,
            corners: std::mem::take(&mut self.corners),
            text_items: Vec::new(),
            text_ranges: Vec::new(),
            active_text: HashSet::new(),
        }
    }

    /// Turns the scene's primitives into vertices and per-surface draw lists.
    fn record_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rec: &mut Recording,
        scene: &Scene,
    ) {
        let close = Primitive::PopLayer;
        let mut primitives = scene.primitives.iter();
        loop {
            let primitive = match primitives.next() {
                Some(primitive) => primitive,
                None if rec.frames.is_empty() => break,
                // Layers left open still composite.
                None => &close,
            };
            let clip = *rec.clips.last().unwrap();
            match primitive {
                Primitive::PushClip(rect) => rec.clips.push(clip.intersect(*rect)),
                Primitive::PopClip => {
                    if rec.clips.len() > rec.frames.last().map_or(1, |frame| frame.clip_depth + 1) {
                        rec.clips.pop();
                    }
                }
                Primitive::PushLayer { .. } => {
                    self.record_push_layer(device, queue, rec, clip, primitive)
                }
                Primitive::PopLayer => {
                    if let Some(frame) = rec.frames.pop() {
                        rec.clips.truncate(frame.clip_depth);
                        rec.current = frame.parent;
                        rec.surfaces[rec.current].draws.extend(frame.composite);
                    }
                }
                Primitive::Quad {
                    rect,
                    color,
                    radius,
                } => rec.quad(clip, *rect, *color, *radius),
                Primitive::Image { .. } => self.record_image(device, queue, rec, clip, primitive),
                Primitive::Gradient { .. } => rec.gradient(clip, primitive),
                Primitive::Shadow { .. } => rec.shadow(clip, primitive),
                Primitive::Path { .. } => rec.path(clip, primitive),
                Primitive::Custom { rect, id } => {
                    let surface = &mut rec.surfaces[rec.current];
                    let scissor = scissor(clip.intersect(*rect), surface);
                    if scissor[2] == 0 || scissor[3] == 0 {
                        continue;
                    }
                    surface.draws.push(Draw::Custom {
                        id: *id,
                        rect: Rect {
                            x: rect.x - surface.origin[0],
                            y: rect.y - surface.origin[1],
                            ..*rect
                        },
                        clip: scissor,
                    });
                }
                Primitive::Text { .. }
                | Primitive::StyledText { .. }
                | Primitive::RichText { .. } => self.record_text(rec, clip, primitive),
                _ => debug_assert!(false, "unhandled primitive {primitive:?}"),
            }
        }
    }

    /// Opens a layer: inline as a clip where it is opaque and untransformed,
    /// otherwise as an offscreen surface composited at its pop.
    fn record_push_layer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rec: &mut Recording,
        clip: Rect,
        primitive: &Primitive,
    ) {
        let Primitive::PushLayer {
            bounds,
            opacity,
            transform,
        } = primitive
        else {
            unreachable!()
        };
        let mut frame = LayerFrame {
            parent: rec.current,
            clip_depth: rec.clips.len(),
            composite: None,
        };
        if *opacity >= 1.0 && transform.is_identity() {
            rec.clips.push(clip.intersect(*bounds));
            rec.frames.push(frame);
            return;
        }
        let parent = &rec.surfaces[rec.current];
        let (origin, scale) = (parent.origin, parent.scale);
        // Snapped to the parent's pixel grid, so an untransformed
        // composite maps texels onto pixels one to one.
        let x = ((bounds.x - origin[0]) * scale).floor();
        let y = ((bounds.y - origin[1]) * scale).floor();
        let width = (((bounds.x + bounds.width - origin[0]) * scale).ceil() - x).max(1.0);
        let height = (((bounds.y + bounds.height - origin[1]) * scale).ceil() - y).max(1.0);
        // Oversized layers render at a lower resolution instead.
        let fit = (rec.cap[0] as f32 / width)
            .min(rec.cap[1] as f32 / height)
            .min(1.0);
        let size = [
            ((width * fit).ceil() as u32).clamp(1, rec.cap[0]),
            ((height * fit).ceil() as u32).clamp(1, rec.cap[1]),
        ];
        let child_origin = [origin[0] + x / scale, origin[1] + y / scale];
        let child_scale = scale * fit;
        let corners = UNIT_QUAD.map(|[u, v]| {
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
            rec.clips.push(Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            });
            rec.frames.push(frame);
            return;
        }
        let start = rec.vertices.len() as u32;
        let parent_size = parent.size;
        rec.vertices.extend(corners.map(|(u, v, point)| Vertex {
            position: [
                (point.x - origin[0]) * scale / parent_size[0] as f32 * 2.0 - 1.0,
                1.0 - (point.y - origin[1]) * scale / parent_size[1] as f32 * 2.0,
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
            surface: rec.surfaces.len(),
            start,
            clip: composite_clip,
        });
        rec.frames.push(frame);
        rec.clips.push(*bounds);
        rec.current = rec.surfaces.len();
        rec.surfaces.push(Surface {
            size,
            origin: child_origin,
            scale: child_scale,
            texture: Some(texture),
            draws: Vec::new(),
        });
    }

    fn record_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rec: &mut Recording,
        clip: Rect,
        primitive: &Primitive,
    ) {
        let Primitive::Image {
            rect,
            image,
            source,
            tint,
            radius,
        } = primitive
        else {
            unreachable!()
        };
        let surface = &mut rec.surfaces[rec.current];
        let visible = scissor(clip.intersect(*rect), surface);
        if visible[2] == 0 || visible[3] == 0 || source.width == 0.0 || source.height == 0.0 {
            return;
        }
        let raster = self
            .images
            .raster_size(image, *rect, *source, surface.scale);
        let Some(place) = self.images.ensure(device, queue, image, raster) else {
            return;
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
        let start = rec.image_vertices.len() as u32;
        for [x, y] in UNIT_QUAD {
            rec.image_vertices.push(image::ImageVertex {
                position: [
                    ((rect.x - origin[0]) + x * rect.width) * scale / size[0] as f32 * 2.0 - 1.0,
                    1.0 - ((rect.y - origin[1]) + y * rect.height) * scale / size[1] as f32 * 2.0,
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
            }) if *texture == place.texture && *last == clip && *first + *count == start => {
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

    fn record_text(&mut self, rec: &mut Recording, clip: Rect, primitive: &Primitive) {
        let (Primitive::Text { rect, color, .. }
        | Primitive::StyledText { rect, color, .. }
        | Primitive::RichText { rect, color, .. }) = primitive
        else {
            unreachable!()
        };
        let key = match primitive {
            Primitive::Text {
                text, font_size, ..
            } => self.text_system.ensure(text, *font_size),
            Primitive::StyledText { text, style, .. } => {
                self.text_system
                    .ensure_styled(text, style, Some(rect.width))
            }
            Primitive::RichText { spans, style, .. } => {
                self.text_system.ensure_rich(spans, style, Some(rect.width))
            }
            _ => unreachable!(),
        };
        rec.active_text.insert(key.clone());
        let surface = &mut rec.surfaces[rec.current];
        let scissor = scissor(clip.intersect(*rect), surface);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let index = rec.text_items.len();
        rec.text_items.push(PreparedText {
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
            rec.text_ranges[*batch].1.end = index + 1;
        } else {
            surface.draws.push(Draw::Text(rec.text_ranges.len()));
            rec.text_ranges.push((rec.current, index..index + 1));
        }
    }

    /// Lays out and rasterises the recorded text, one batch per renderer.
    fn prepare_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rec: &Recording,
    ) -> RenderResult<()> {
        for (index, (surface, range)) in rec.text_ranges.iter().enumerate() {
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
            let areas = rec.text_items[range.clone()].iter().map(|item| TextArea {
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
            let viewport = match rec.surfaces[*surface].texture {
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
        self.text_renderers.truncate(rec.text_ranges.len().max(16));
        self.text_system.retain(&rec.active_text);
        Ok(())
    }

    /// Writes the recorded vertices to their retained buffers, growing them
    /// where they no longer fit, and takes the vertex vectors back.
    fn upload_vertices(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, rec: &mut Recording) {
        // Last in the buffer, at the corners of the target: the canvas blit.
        self.blit = rec.vertices.len() as u32;
        rec.vertices.extend(UNIT_QUAD.map(|[u, v]| Vertex {
            position: [u * 2.0 - 1.0, 1.0 - v * 2.0],
            local: [u, v],
            size: [0.0; 2],
            color: [0.0, 0.0, 0.0, 1.0],
            radius: 0.0,
        }));
        let bytes = bytemuck::cast_slice(&rec.vertices);
        if !bytes.is_empty() {
            if bytes.len() as u64 > self.stats.quad_buffer_bytes {
                let capacity = (bytes.len() as u64).next_power_of_two().max(256);
                self.quad_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("glassmere retained quad vertices"),
                    size: capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                self.stats.quad_buffer_bytes = capacity;
                self.stats.quad_buffer_allocations += 1;
            }
            queue.write_buffer(self.quad_buffer.as_ref().unwrap(), 0, bytes);
        }
        let bytes = bytemuck::cast_slice(&rec.image_vertices);
        if !bytes.is_empty() {
            if bytes.len() as u64 > self.image_buffer_bytes {
                self.image_buffer_bytes = (bytes.len() as u64).next_power_of_two();
                self.image_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("glassmere retained image vertices"),
                    size: self.image_buffer_bytes,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
            queue.write_buffer(self.image_buffer.as_ref().unwrap(), 0, bytes);
        }
        upload(
            device,
            queue,
            "glassmere retained gradient vertices",
            &mut self.gradient_buffer,
            &mut self.gradient_buffer_bytes,
            bytemuck::cast_slice(&rec.gradient_vertices),
        );
        upload(
            device,
            queue,
            "glassmere retained shadow vertices",
            &mut self.shadow_buffer,
            &mut self.shadow_buffer_bytes,
            bytemuck::cast_slice(&rec.shadow_vertices),
        );
        upload(
            device,
            queue,
            "glassmere retained path vertices",
            &mut self.path_buffer,
            &mut self.path_buffer_bytes,
            bytemuck::cast_slice(&rec.path_vertices),
        );
        self.vertices = std::mem::take(&mut rec.vertices);
        self.image_vertices = std::mem::take(&mut rec.image_vertices);
        self.gradient_vertices = std::mem::take(&mut rec.gradient_vertices);
        self.shadow_vertices = std::mem::take(&mut rec.shadow_vertices);
        self.path_vertices = std::mem::take(&mut rec.path_vertices);
        self.corners = std::mem::take(&mut rec.corners);
    }

    /// Keeps the recorded surfaces for drawing and refreshes the statistics.
    fn finish_preparation(&mut self, rec: Recording, target_key: ([u32; 2], u32)) {
        let surfaces = rec.surfaces;
        let count = |wanted: fn(&Draw) -> bool| {
            surfaces
                .iter()
                .flat_map(|surface| &surface.draws)
                .filter(|draw| wanted(draw))
                .count()
        };
        self.stats.image_draws = count(|draw| matches!(draw, Draw::Image { .. }));
        self.stats.gradients = count(|draw| matches!(draw, Draw::Gradient { .. }));
        self.stats.shadows = count(|draw| matches!(draw, Draw::Shadow { .. }));
        self.stats.paths = count(|draw| matches!(draw, Draw::Path { .. }));
        self.stats.quads = count(|draw| matches!(draw, Draw::Quad { .. }));
        self.stats.draw_calls = count(|_| true);
        self.stats.passes = surfaces.len();
        self.stats.image_uploads = self.images.uploads;
        (self.stats.atlas_pages, self.stats.image_textures) = self.images.counts();
        self.stats.image_bytes = self.images.bytes();
        self.surfaces = surfaces;
        self.prepared_target = Some(target_key);
        self.stats.preparations += 1;
        self.stats.text_batches = rec.text_ranges.len();
        self.stats.layer_textures = self.pool.len();
        let texel = u64::from(self.format.block_copy_size(None).unwrap_or(4));
        self.stats.layer_texture_bytes = self
            .pool
            .iter()
            .map(|texture| u64::from(texture.size[0]) * u64::from(texture.size[1]) * texel)
            .sum();
        let memory = &mut self.stats.memory;
        memory.buffers = self.stats.quad_buffer_bytes
            + self.image_buffer_bytes
            + self.gradient_buffer_bytes
            + self.shadow_buffer_bytes
            + self.path_buffer_bytes;
        (memory.image_atlas, memory.image_textures) = self.images.bytes_by_kind();
        memory.layers = self.stats.layer_texture_bytes;
        memory.settle();
    }

    /// Brings the canvas to the target at hand and works out how much of this
    /// frame may be kept. `None` means redraw it whole: there is no damage to
    /// go by, or the canvas is new, holds another target or does not fit.
    fn prepare_canvas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_key: ([u32; 2], u32),
        damage: &[Rect],
    ) -> Option<[u32; 4]> {
        let (size, scale) = (target_key.0, f32::from_bits(target_key.1));
        // Hosts that never name damage never pay for a canvas.
        if damage.is_empty() && self.canvas.is_none() {
            return None;
        }
        if self
            .canvas
            .as_ref()
            .is_none_or(|canvas| canvas.size != size)
        {
            // A target past what the device can hold as a texture leaves no
            // canvas at all, and those frames go to the target whole.
            let limit = device.limits().max_texture_dimension_2d;
            let canvas = size
                .iter()
                .all(|side| *side <= limit)
                .then(|| self.layer_texture(device, queue, size));
            let texel = u64::from(self.format.block_copy_size(None).unwrap_or(4));
            self.stats.memory.canvas = canvas
                .as_ref()
                .map_or(0, |_| u64::from(size[0]) * u64::from(size[1]) * texel);
            self.stats.memory.settle();
            self.canvas = canvas;
            self.canvas_holds = None;
        }
        let union = damage.iter().copied().reduce(Rect::union)?;
        if self.canvas_holds != Some(target_key) {
            return None;
        }
        Some(scissor(
            union,
            &Surface {
                size,
                origin: [0.0; 2],
                scale,
                texture: None,
                draws: Vec::new(),
            },
        ))
    }

    fn layer_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
    ) -> LayerTexture {
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("glassmere layer"),
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
            label: Some("glassmere layer"),
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
        damage: Option<[u32; 4]>,
    ) -> RenderResult<()> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("glassmere frame"),
        });
        self.stats.frames += 1;
        let target_size = self.surfaces[0].size;
        self.stats.redrawn = match damage {
            Some(clip) => u64::from(clip[2]) * u64::from(clip[3]),
            None => u64::from(target_size[0]) * u64::from(target_size[1]),
        };
        if self.profiling && self.timestamps_supported {
            self.timestamps
                .get_or_insert_with(|| profile::Timestamps::new(device, queue))
                .collect(device);
        }
        // A pass that loads the last frame is never cleared, so whatever the
        // damaged rectangle held stays under what is drawn over it. Anything
        // that was there and is not drawn again, a popup above all, would
        // show through. The clear colour goes down first.
        if damage.is_some() {
            let size = target_size.map(|side| side as f32);
            let colour = clear.linear();
            // A pixel wider than the surface on every side, so that the
            // shape's own antialiasing falls outside it.
            let vertices: Vec<Vertex> = UNIT_QUAD
                .into_iter()
                .map(|[u, v]| Vertex {
                    position: [u * 2.0 - 1.0, 1.0 - v * 2.0],
                    local: [u * (size[0] + 2.0) - 1.0, v * (size[1] + 2.0) - 1.0],
                    size: [size[0] + 2.0, size[1] + 2.0],
                    color: colour,
                    radius: 0.0,
                })
                .collect();
            let bytes = bytemuck::cast_slice(&vertices);
            let buffer = self.clear_quad.get_or_insert_with(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("glassmere damage clear"),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            });
            queue.write_buffer(buffer, 0, bytes);
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
                // The target's own pass goes to the canvas while there is one.
                None => match &self.canvas {
                    Some(canvas) => (&canvas.view, &self.viewport, clear.linear()),
                    None => (target, &self.viewport, clear.linear()),
                },
            };
            // Only the target's pass may keep anything; layers are drawn afresh.
            let confine = damage.filter(|_| surface.texture.is_none());
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
            // The lifetime is dropped so custom draws can be handed the pass;
            // the encoder stays locked until the pass ends either way.
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glassmere scene"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: match confine {
                                // Outside the scissor the canvas keeps the last frame.
                                Some(_) => wgpu::LoadOp::Load,
                                None => wgpu::LoadOp::Clear(wgpu::Color {
                                    r: f64::from(clear[0]),
                                    g: f64::from(clear[1]),
                                    b: f64::from(clear[2]),
                                    a: f64::from(clear[3]),
                                }),
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            if let Some(clip) = confine
                && let Some(quad) = &self.clear_quad
            {
                pass.set_pipeline(&self.clear_pipeline);
                pass.set_vertex_buffer(0, quad.slice(..));
                pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                pass.draw(0..6, 0..1);
            }
            for draw in &surface.draws {
                let clip = match draw {
                    Draw::Quad { clip, .. }
                    | Draw::Image { clip, .. }
                    | Draw::Layer { clip, .. }
                    | Draw::Gradient { clip, .. }
                    | Draw::Shadow { clip, .. }
                    | Draw::Path { clip, .. }
                    | Draw::Custom { clip, .. } => *clip,
                    // Each text area carries its own bounds into glyphon.
                    Draw::Text(_) => [0, 0, surface.size[0], surface.size[1]],
                };
                let clip = confine.map_or(clip, |damage| intersect(clip, damage));
                if clip[2] == 0 || clip[3] == 0 {
                    continue;
                }
                pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                match draw {
                    Draw::Quad { start, .. } => {
                        pass.set_pipeline(&self.pipeline);
                        pass.set_vertex_buffer(0, self.quad_buffer.as_ref().unwrap().slice(..));
                        pass.draw(*start..*start + 6, 0..1);
                    }
                    Draw::Text(index) => {
                        self.text_renderers[*index].render(&self.atlas, viewport, &mut pass)?;
                    }
                    Draw::Image {
                        texture,
                        start,
                        count,
                        ..
                    } => {
                        pass.set_pipeline(&self.image_pipeline);
                        pass.set_bind_group(0, self.images.bind_group(*texture), &[]);
                        pass.set_vertex_buffer(0, self.image_buffer.as_ref().unwrap().slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Draw::Layer {
                        surface: child,
                        start,
                        ..
                    } => {
                        let texture = self.surfaces[*child].texture.unwrap();
                        pass.set_pipeline(&self.layer_pipeline);
                        pass.set_bind_group(0, &self.pool[texture].bind_group, &[]);
                        pass.set_vertex_buffer(0, self.quad_buffer.as_ref().unwrap().slice(..));
                        pass.draw(*start..*start + 6, 0..1);
                    }
                    Draw::Gradient { start, count, .. } => {
                        pass.set_pipeline(&self.gradient_pipeline);
                        pass.set_vertex_buffer(0, self.gradient_buffer.as_ref().unwrap().slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Draw::Shadow { start, .. } => {
                        pass.set_pipeline(&self.shadow_pipeline);
                        pass.set_vertex_buffer(0, self.shadow_buffer.as_ref().unwrap().slice(..));
                        pass.draw(*start..*start + 6, 0..1);
                    }
                    Draw::Path { start, count, .. } => {
                        pass.set_pipeline(&self.path_pipeline);
                        pass.set_vertex_buffer(0, self.path_buffer.as_ref().unwrap().slice(..));
                        pass.draw(*start..*start + *count, 0..1);
                    }
                    Draw::Custom { id, rect, .. } => {
                        if let Some(draw) = self.custom.get_mut(id) {
                            draw(CustomDraw {
                                device,
                                queue,
                                pass: &mut pass,
                                rect: *rect,
                                scale: surface.scale,
                                target_size: surface.size,
                            });
                            // Pipeline, vertex buffer and scissor are set again
                            // by every draw that follows, but nothing else is:
                            // the viewport a callback left would stay.
                            let size = surface.size.map(|side| side as f32);
                            pass.set_viewport(0.0, 0.0, size[0], size[1], 0.0, 1.0);
                        }
                    }
                }
            }
        }
        // A swapchain texture keeps nothing between frames, so the canvas goes
        // on it whole. Premultiplied blending over nothing copies it exactly.
        if let Some(canvas) = &self.canvas {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glassmere canvas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.layer_pipeline);
            pass.set_bind_group(0, &canvas.bind_group, &[]);
            pass.set_vertex_buffer(0, self.quad_buffer.as_ref().unwrap().slice(..));
            pass.draw(self.blit..self.blit + 6, 0..1);
        }
        if self.canvas.is_some() {
            self.canvas_holds = self.prepared_target;
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

/// Puts `bytes` in `buffer`, growing it to the next power of two first where
/// they do not fit. `capacity` is what the buffer holds now.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    buffer: &mut Option<wgpu::Buffer>,
    capacity: &mut u64,
    bytes: &[u8],
) {
    if bytes.is_empty() {
        return;
    }
    if bytes.len() as u64 > *capacity {
        *capacity = (bytes.len() as u64).next_power_of_two();
        *buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: *capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    queue.write_buffer(buffer.as_ref().unwrap(), 0, bytes);
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

/// Two scissors of one surface, as one. Empty where they do not meet.
fn intersect(clip: [u32; 4], other: [u32; 4]) -> [u32; 4] {
    let x = clip[0].max(other[0]);
    let y = clip[1].max(other[1]);
    [
        x,
        y,
        (clip[0] + clip[2])
            .min(other[0] + other[2])
            .saturating_sub(x),
        (clip[1] + clip[3])
            .min(other[1] + other[3])
            .saturating_sub(y),
    ]
}
