//! Renders a scene to pixels without a GPU, for screenshots in any CI, for
//! export, and as the desktop host's fallback on machines where no graphics
//! adapter works.
//!
//! ```ignore
//! let mut renderer = CpuRenderer::new();
//! context.run(size, renderer.text_system(), view);
//! let pixels = renderer.render([800, 600], 1.0, context.scene(), Color::rgb(16, 23, 34));
//! ```
//!
//! It follows the display list as `glassmere-render-wgpu` does: the same clip
//! stack, the same layers snapped to the pixel grid, the same shaped text from
//! `glassmere-text`, the same premultiplied linear texels. Like an sRGB render
//! target it blends in linear light, so translucent fills, faded layers and
//! the soft edges of glyphs come out the same. tiny-skia supplies geometry:
//! the coverage of rounded corners and the resampling of images and
//! transformed layers.
//!
//! It is close to the GPU's output, not identical. A quad's edge is covered by
//! its exact area here and feathered by the shader there, and what is resampled
//! is filtered on sRGB-encoded values here and in linear light there. Both show
//! at edges, never across a flat area.
mod photo;
mod text_memo;

use glassmere_scene::{
    Affine, Color, GradientStop, Image, Path, Point, Primitive, Rect, Scene, Segment,
};
use glassmere_text::cosmic_text::{SwashCache, SwashContent};
use glassmere_text::{TextKey, TextSystem};
use photo::{PHOTO_BYTES, Premultiply, Raster};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use text_memo::{TextMemo, ensure_text, text_bytes};
use tiny_skia::{
    FillRule, FilterQuality, Mask, PathBuilder, Pixmap, PixmapPaint, PixmapRef, Transform,
};

/// The longest side a vector image or a layer is rasterized at.
const MAX_SIDE: u32 = 8192;
/// A damaged frame is painted as at most this many regions, one pass each.
const DAMAGE_REGIONS: usize = 8;
/// Damaged rectangles this close, in logical pixels, are painted as one: a
/// second pass over a thin gap costs more than painting the gap.
const DAMAGE_GAP: f32 = 8.0;
/// Bytes of text that frames painted in part may shape, beyond the text the
/// last whole frame held, before the next frame is painted whole.
const STALE_TEXT_BYTES: usize = 64 * 1024;
/// Stops of a gradient past this are dropped, as on the GPU, where the shader
/// holds a fixed number of them.
pub const MAX_GRADIENT_STOPS: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
pub struct CpuStats {
    pub frames: u64,
    /// Pixels of the target the last frame painted: all of them, or the
    /// damaged part when the caller gave one.
    pub redrawn: u64,
    /// Time the last `render` took.
    pub render: Duration,
    /// Offscreen layers of the last frame.
    pub layers: usize,
    pub glyphs: usize,
}

pub struct CpuRenderer {
    text: TextSystem,
    swash: SwashCache,
    /// The finished frame, kept so that a frame of the same size allocates
    /// nothing and so that a damaged frame has something to paint over.
    target: Option<Pixmap>,
    /// The size and scale the kept frame was painted at. Anything else, and
    /// what is in the target is not the previous picture.
    painted: Option<([u32; 2], u32)>,
    /// How many shaped texts the last whole frame kept.
    text_kept: usize,
    /// Bytes of text the last whole frame held.
    text_kept_bytes: usize,
    /// How many shaped texts the cache held, and how many bytes of text
    /// they came to, when the last frame ended or, for a frame painted in
    /// part, when it last grew. The count of blocks alone does not bound the
    /// memory, for a block of text can be large.
    text_seen: usize,
    text_seen_bytes: usize,
    /// What the last whole frame asked of the text cache, to be checked
    /// against rather than asked again.
    text_memo: TextMemo,
    /// Texels ready to sample, by image and pixel size, with the frame that
    /// used them last: vector images rasterized, translucent ones converted,
    /// photos filtered down. A filtered photo is kept for `PHOTO_IDLE_FRAMES`
    /// frames and up to `photo_budget` bytes, anything else for two frames.
    rasters: HashMap<(u64, [u32; 2]), Raster>,
    photo_budget: usize,
    gamma: Gamma,
    premultiply: Premultiply,
    /// How much of each visible column the square quad being filled covers,
    /// 0..=255. Every quad writes it afresh; it is kept only so that filling
    /// one allocates nothing.
    columns: Vec<u8>,
    stats: CpuStats,
}

/// Somewhere to paint. Its pixels are premultiplied in linear light and stored
/// sRGB-encoded, which is what an `Rgba8UnormSrgb` target on the GPU holds; on
/// an opaque surface that is plain sRGB.
struct Surface {
    pixmap: Pixmap,
    /// Logical position of the pixmap's corner, and pixels per logical pixel.
    origin: [f32; 2],
    scale: f32,
}

struct LayerFrame {
    clip_depth: usize,
    /// `None` draws inline: full opacity, no transform, or nothing visible.
    composite: Option<Composite>,
}

struct Composite {
    /// 0..=255.
    opacity: u32,
    /// Child pixels to parent pixels.
    transform: Transform,
    /// Where the child's corner lands when nothing turns or scales it, in
    /// whole pixels: the child is then copied rather than resampled.
    offset: Option<[i32; 2]>,
    /// The parent's pixels the composite may touch: x, y, width, height.
    clip: [u32; 4],
}

/// sRGB bytes to linear light and back. Blending happens in between, because
/// that is where the GPU does it: mixed in sRGB space, a translucent fill over
/// a dark page comes out visibly darker.
struct Gamma {
    /// 0..=255 to 0..=4095.
    linear: [u16; 256],
    back: [u8; 4096],
}

impl Gamma {
    fn new() -> Self {
        let mut gamma = Self {
            linear: [0; 256],
            back: [0; 4096],
        };
        for (byte, linear) in gamma.linear.iter_mut().enumerate() {
            let value = byte as f32 / 255.0;
            let light = if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            };
            *linear = (light * 4095.0).round() as u16;
        }
        for (light, byte) in gamma.back.iter_mut().enumerate() {
            let light = light as f32 / 4095.0;
            let value = if light <= 0.003_130_8 {
                light * 12.92
            } else {
                1.055 * light.powf(1.0 / 2.4) - 0.055
            };
            *byte = (value * 255.0).round() as u8;
        }
        gamma
    }

    /// A straight sRGB colour under `alpha` (0..=255), premultiplied in linear
    /// light: what `over` lays down.
    #[inline]
    fn light(&self, rgb: [u8; 3], alpha: u32) -> [u32; 3] {
        rgb.map(|channel| (u32::from(self.linear[usize::from(channel)]) * alpha + 127) / 255)
    }

    /// A colour channel of a texel with `alpha`, neither 0 nor 255, as it is
    /// stored once premultiplied: straight, in linear light under the alpha,
    /// sRGB-encoded again.
    fn premultiplied(&self, channel: u8, alpha: u8) -> u8 {
        let straight = (u32::from(channel) * 255 / u32::from(alpha)).min(255) as usize;
        let light = f32::from(self.linear[straight]) * f32::from(alpha) / 255.0;
        self.back[light.round().clamp(0.0, 4095.0) as usize]
    }

    /// Source over destination, in linear light.
    #[inline]
    fn over(&self, pixel: &mut [u8], source: [u32; 3], alpha: u32) {
        let keep = 255 - alpha;
        for (channel, light) in source.iter().enumerate() {
            let under = u32::from(self.linear[usize::from(pixel[channel])]);
            let mixed = light + (under * keep + 127) / 255;
            pixel[channel] = self.back[mixed.min(4095) as usize];
        }
        pixel[3] = (alpha + (u32::from(pixel[3]) * keep + 127) / 255).min(255) as u8;
    }
}

/// `side` halved, rounding up, for as long as the result still covers `drawn`
/// pixels: what is left is under twice what is drawn.
fn halved_side(mut side: u32, drawn: f32) -> u32 {
    while side > 1 && side.div_ceil(2) as f32 >= drawn {
        side = side.div_ceil(2);
    }
    side
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Surface {
    fn size(&self) -> [u32; 2] {
        [self.pixmap.width(), self.pixmap.height()]
    }

    /// Logical coordinates to this surface's pixels.
    fn transform(&self) -> Transform {
        Transform::from_row(
            self.scale,
            0.0,
            0.0,
            self.scale,
            -self.origin[0] * self.scale,
            -self.origin[1] * self.scale,
        )
    }

    /// The pixels a logical rectangle touches: x, y, width, height. The same
    /// rounding as the GPU's scissor rectangles.
    fn pixels(&self, rect: Rect) -> [u32; 4] {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return [0; 4];
        }
        let size = self.size();
        let edge = |value: f32, axis: usize, up: bool| {
            let pixel = (value - self.origin[axis]) * self.scale;
            let pixel = if up { pixel.ceil() } else { pixel.floor() };
            pixel.max(0.0).min(size[axis] as f32) as u32
        };
        let (x, y) = (edge(rect.x, 0, false), edge(rect.y, 1, false));
        let right = edge(rect.x + rect.width, 0, true);
        let bottom = edge(rect.y + rect.height, 1, true);
        [x, y, right.saturating_sub(x), bottom.saturating_sub(y)]
    }
}

/// A rectangle with corners of `radius`, in logical coordinates.
fn rounded(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let bounds = tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.width, rect.height)?;
    let radius = radius.clamp(0.0, rect.width.min(rect.height) / 2.0);
    if radius <= 0.0 {
        return Some(PathBuilder::from_rect(bounds));
    }
    // The cubic that stands in for a quarter circle.
    let bend = radius * 0.447_715_2;
    let (left, top, right, bottom) = (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height);
    let mut path = PathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(right - radius, top);
    path.cubic_to(right - bend, top, right, top + bend, right, top + radius);
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - bend,
        right - bend,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(left + radius, bottom);
    path.cubic_to(
        left + bend,
        bottom,
        left,
        bottom - bend,
        left,
        bottom - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(left, top + bend, left + bend, top, left + radius, top);
    path.close();
    path.finish()
}

fn skia_path(path: &Path) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    for segment in path.segments() {
        match *segment {
            Segment::Move(a) => builder.move_to(a.x, a.y),
            Segment::Line(a) => builder.line_to(a.x, a.y),
            Segment::Quad(a, b) => builder.quad_to(a.x, a.y, b.x, b.y),
            Segment::Cubic(a, b, c) => builder.cubic_to(a.x, a.y, b.x, b.y, c.x, c.y),
            Segment::Close => builder.close(),
        }
    }
    builder.finish()
}

/// Abramowitz and Stegun 7.1.26, the approximation shader code uses.
fn erf(x: f32) -> f32 {
    let (sign, magnitude) = (x.signum(), x.abs());
    let mut value =
        1.0 + (0.278_393 + (0.230_389 + 0.078_108 * magnitude * magnitude) * magnitude) * magnitude;
    value *= value;
    sign - sign / (value * value)
}

fn gaussian(x: f32, sigma: f32) -> f32 {
    (-(x * x) / (2.0 * sigma * sigma)).exp() / ((2.0 * std::f32::consts::PI).sqrt() * sigma)
}

/// How much of a rounded rectangle covers a point, with a soft pixel edge.
fn rounded_coverage(point: [f32; 2], half: [f32; 2], corner: f32) -> f32 {
    let q = [
        point[0].abs() - half[0] + corner,
        point[1].abs() - half[1] + corner,
    ];
    let outside = [q[0].max(0.0), q[1].max(0.0)];
    let distance = outside[0].hypot(outside[1]) + q[0].max(q[1]).min(0.0) - corner;
    (0.5 - distance).clamp(0.0, 1.0)
}

/// The Gaussian integral over a rounded box: an erf along one axis, sampled
/// four times along the other.
fn rounded_box_shadow(half: [f32; 2], point: [f32; 2], sigma: f32, corner: f32) -> f32 {
    let shadow_x = |x: f32, y: f32| {
        let delta = (half[1] - corner - y.abs()).min(0.0);
        let curved = half[0] - corner + (corner * corner - delta * delta).max(0.0).sqrt();
        let scale = (0.5f32).sqrt() / sigma;
        let low = 0.5 + 0.5 * erf((x - curved) * scale);
        let high = 0.5 + 0.5 * erf((x + curved) * scale);
        high - low
    };
    let (low, high) = (point[1] - half[1], point[1] + half[1]);
    let start = (-3.0 * sigma).clamp(low, high);
    let end = (3.0 * sigma).clamp(low, high);
    let step = (end - start) / 4.0;
    let mut y = start + step * 0.5;
    let mut value = 0.0;
    for _ in 0..4 {
        value += shadow_x(point[0], point[1] - y) * gaussian(y, sigma) * step;
        y += step;
    }
    value
}

fn affine(affine: Affine) -> Transform {
    Transform::from_row(affine.a, affine.b, affine.c, affine.d, affine.e, affine.f)
}

/// Moves what a transform draws so that pixel `corner` of the surface becomes
/// the origin: everything is painted into a buffer covering `corner` alone,
/// which is how clipping is enforced here.
fn toward(transform: Transform, corner: [u32; 4]) -> Transform {
    transform.post_translate(-(corner[0] as f32), -(corner[1] as f32))
}

/// The rectangles to paint for `damage`: those that overlap or nearly touch
/// are one, and past `DAMAGE_REGIONS` the pair that costs the least extra
/// area is joined. Each is cleared and painted over on its own, so what lies
/// between them is left alone.
fn coalesce(damage: &[Rect]) -> Vec<Rect> {
    let near = |a: Rect, b: Rect| {
        a.x - DAMAGE_GAP <= b.x + b.width
            && b.x - DAMAGE_GAP <= a.x + a.width
            && a.y - DAMAGE_GAP <= b.y + b.height
            && b.y - DAMAGE_GAP <= a.y + a.height
    };
    let area = |rect: Rect| rect.width * rect.height;
    let mut regions: Vec<Rect> = Vec::new();
    for mut rect in damage
        .iter()
        .copied()
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
    {
        while let Some(at) = regions.iter().position(|region| near(*region, rect)) {
            rect = rect.union(regions.swap_remove(at));
        }
        regions.push(rect);
        while regions.len() > DAMAGE_REGIONS {
            let waste = |a: usize, b: usize| {
                area(regions[a].union(regions[b])) - area(regions[a]) - area(regions[b])
            };
            let (a, b) = (0..regions.len())
                .flat_map(|a| (a + 1..regions.len()).map(move |b| (a, b)))
                .min_by(|x, y| waste(x.0, x.1).total_cmp(&waste(y.0, y.1)))
                .unwrap();
            let joined = regions[a].union(regions.swap_remove(b));
            regions[a] = joined;
        }
    }
    regions
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self::with_text_system(TextSystem::default())
    }

    /// Like `new`, with a caller-provided font system (see `TextSystem::with_fonts`).
    pub fn with_text_system(text: TextSystem) -> Self {
        Self {
            text,
            swash: SwashCache::new(),
            target: None,
            painted: None,
            text_kept: 0,
            text_kept_bytes: 0,
            text_seen: 0,
            text_seen_bytes: 0,
            text_memo: TextMemo::default(),
            rasters: HashMap::new(),
            photo_budget: PHOTO_BYTES,
            gamma: Gamma::new(),
            premultiply: Premultiply::new(),
            columns: Vec::new(),
            stats: CpuStats::default(),
        }
    }

    /// The measurer to lay the scene out with, so that text fits what is drawn.
    pub fn text_system(&mut self) -> &mut TextSystem {
        &mut self.text
    }

    pub fn stats(&self) -> CpuStats {
        self.stats
    }

    /// Paints the whole scene. See `render_damaged`, which this calls with
    /// nothing to reuse.
    pub fn render(
        &mut self,
        physical_size: [u32; 2],
        scale: f32,
        scene: &Scene,
        clear: Color,
    ) -> &[u8] {
        self.render_damaged(physical_size, scale, scene, clear, &[])
    }

    /// Paints the scene in display-list order and returns RGBA8,
    /// `physical_size[0] * physical_size[1] * 4` bytes in row order. `clear`
    /// should be opaque; over a translucent one the result is premultiplied.
    /// Scene geometry is in logical pixels and `scale` is physical pixels per
    /// logical pixel. An empty or unusable target gives an empty slice.
    ///
    /// `damage` is the logical rectangles that changed since the frame this
    /// renderer painted last. Everything else keeps the pixels it had, so a
    /// caret that blinks in a large window costs the caret. Give `&[]` when
    /// anything else drew over the window, or when its previous picture is
    /// not this renderer's last frame, and the whole scene is painted.
    pub fn render_damaged(
        &mut self,
        physical_size: [u32; 2],
        scale: f32,
        scene: &Scene,
        clear: Color,
        damage: &[Rect],
    ) -> &[u8] {
        let started = Instant::now();
        let usable = !physical_size.contains(&0) && scale.is_finite() && scale > 0.0;
        let pixmap = match self.target.take() {
            Some(pixmap) if [pixmap.width(), pixmap.height()] == physical_size => Some(pixmap),
            _ if usable => Pixmap::new(physical_size[0], physical_size[1]),
            _ => None,
        };
        let Some(pixmap) = pixmap.filter(|_| usable) else {
            self.painted = None;
            return &[];
        };
        // Only a frame this renderer painted at the same size and scale can
        // be painted over rather than replaced.
        let kept = self.painted == Some((physical_size, scale.to_bits()));
        // Painting over the last frame: only what the damage shows is drawn.
        let partial = kept && !damage.is_empty();
        let regions = match (kept, damage.is_empty()) {
            (true, false) => coalesce(damage),
            _ => vec![Rect {
                x: 0.0,
                y: 0.0,
                width: physical_size[0] as f32 / scale,
                height: physical_size[1] as f32 / scale,
            }],
        };
        self.painted = Some((physical_size, scale.to_bits()));
        self.stats.frames += 1;
        self.stats.layers = 0;
        self.stats.glyphs = 0;
        let mut root = Surface {
            pixmap,
            origin: [0.0; 2],
            scale,
        };
        self.stats.redrawn = 0;
        let stride = root.pixmap.width() as usize * 4;
        let clear = [clear.0, clear.1, clear.2, clear.3];
        for region in regions {
            // Clearing only the damaged pixels is what keeps the rest.
            let cleared = root.pixels(region);
            self.stats.redrawn += u64::from(cleared[2]) * u64::from(cleared[3]);
            let data = root.pixmap.data_mut();
            for row in 0..cleared[3] {
                let start = (cleared[1] + row) as usize * stride + cleared[0] as usize * 4;
                for pixel in data[start..].as_chunks_mut::<4>().0[..cleared[2] as usize].iter_mut()
                {
                    *pixel = clear;
                }
            }
            root = self.paint(scene, region, root, partial);
        }
        // A frame painted in part leaves the text cache alone, so text
        // that has gone is still in it. When that has let it grow well past
        // what the last whole frame needed, the next frame is painted whole,
        // which clears it out. So is the frame after one that has left the
        // cache with more bytes of text than the last whole frame held, by a
        // margin. Text is shaped for the layout before the frame is painted,
        // so the cache is looked at rather than the text the frame painted.
        if partial && self.text.buffers.len() > self.text_seen {
            self.text_seen = self.text.buffers.len();
            self.text_seen_bytes = self
                .text
                .buffers
                .values()
                .flat_map(|buffer| buffer.lines.iter())
                .map(|line| line.text().len())
                .sum();
        }
        if partial
            && (self.text.buffers.len() > self.text_kept * 2 + 256
                || self.text_seen_bytes > self.text_kept_bytes + STALE_TEXT_BYTES)
        {
            self.painted = None;
        }
        let frame = self.stats.frames;
        photo::retire(&mut self.rasters, frame, self.photo_budget);
        self.stats.render = started.elapsed();
        self.target.insert(root.pixmap).data()
    }

    /// `region` is what may be painted: the whole viewport, or the damage.
    ///
    /// A `partial` frame is painted over the last one. Text and paths
    /// outside the region are then not looked at, where a whole frame looks
    /// at every text to know which shaped text it can let go of.
    fn paint(&mut self, scene: &Scene, region: Rect, root: Surface, partial: bool) -> Surface {
        // Oversized layers render at a lower resolution, as on the GPU.
        let cap = root.size().map(|side| side.saturating_mul(2).min(MAX_SIDE));
        // Every draw is clipped, so bounding the stack bounds the frame.
        let mut clips = vec![region];
        let mut surfaces = vec![root];
        let mut frames: Vec<LayerFrame> = Vec::new();
        // Out of the renderer for the length of a whole frame and back at its
        // end, so that a frame that unwinds leaves a memo that knows nothing
        // about what it left in the cache.
        let mut memo = (!partial).then(|| {
            let mut memo = std::mem::take(&mut self.text_memo);
            memo.begin();
            memo
        });
        let mut held = 0;
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
            let surface = surfaces.last_mut().unwrap();
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
                        clip_depth: clips.len(),
                        composite: None,
                    };
                    if *opacity >= 1.0 && transform.is_identity() {
                        clips.push(clip.intersect(*bounds));
                        frames.push(frame);
                        continue;
                    }
                    let (origin, scale) = (surface.origin, surface.scale);
                    // Snapped to the parent's pixel grid, so an untransformed
                    // composite maps pixels onto pixels one to one.
                    let x = ((bounds.x - origin[0]) * scale).floor();
                    let y = ((bounds.y - origin[1]) * scale).floor();
                    let width =
                        (((bounds.x + bounds.width - origin[0]) * scale).ceil() - x).max(1.0);
                    let height =
                        (((bounds.y + bounds.height - origin[1]) * scale).ceil() - y).max(1.0);
                    let fit = (cap[0] as f32 / width).min(cap[1] as f32 / height).min(1.0);
                    let size = [
                        ((width * fit).ceil() as u32).clamp(1, cap[0]),
                        ((height * fit).ceil() as u32).clamp(1, cap[1]),
                    ];
                    let child_origin = [origin[0] + x / scale, origin[1] + y / scale];
                    let child_scale = scale * fit;
                    let corners =
                        [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]].map(|[u, v]: [f32; 2]| {
                            transform.apply(Point {
                                x: child_origin[0] + u * size[0] as f32 / child_scale,
                                y: child_origin[1] + v * size[1] as f32 / child_scale,
                            })
                        });
                    let fold = |pick: fn(Point) -> f32, by: fn(f32, f32) -> f32, from: f32| {
                        corners.iter().map(|corner| pick(*corner)).fold(from, by)
                    };
                    let (left, top) = (
                        fold(|p| p.x, f32::min, f32::INFINITY),
                        fold(|p| p.y, f32::min, f32::INFINITY),
                    );
                    let covered = Rect {
                        x: left,
                        y: top,
                        width: fold(|p| p.x, f32::max, f32::NEG_INFINITY) - left,
                        height: fold(|p| p.y, f32::max, f32::NEG_INFINITY) - top,
                    };
                    // The clip at the push bounds the composite; turned groups
                    // are bounded by their bounding box.
                    let composite_clip = surface.pixels(clip.intersect(covered));
                    let visible = *opacity > 0.0
                        && bounds.width > 0.0
                        && bounds.height > 0.0
                        && composite_clip[2] > 0
                        && composite_clip[3] > 0;
                    let Some(child) = visible.then(|| Pixmap::new(size[0], size[1])).flatten()
                    else {
                        // Invisible: an empty clip culls the content.
                        clips.push(Rect {
                            x: 0.0,
                            y: 0.0,
                            width: 0.0,
                            height: 0.0,
                        });
                        frames.push(frame);
                        continue;
                    };
                    let moved = fit == 1.0
                        && transform.a == 1.0
                        && transform.b == 0.0
                        && transform.c == 0.0
                        && transform.d == 1.0;
                    let landing = [x + transform.e * scale, y + transform.f * scale];
                    frame.composite = Some(Composite {
                        opacity: (opacity.min(1.0) * 255.0).round() as u32,
                        transform: Transform::from_scale(1.0 / child_scale, 1.0 / child_scale)
                            .post_translate(child_origin[0], child_origin[1])
                            .post_concat(affine(*transform))
                            .post_concat(surface.transform()),
                        offset: (moved && landing.iter().all(|at| at.fract() == 0.0))
                            .then(|| landing.map(|at| at as i32)),
                        clip: composite_clip,
                    });
                    frames.push(frame);
                    clips.push(*bounds);
                    self.stats.layers += 1;
                    // A pixmap per layer per frame. Pool them by size,
                    // as the GPU renderer does, if animations show it in a profile.
                    surfaces.push(Surface {
                        pixmap: child,
                        origin: child_origin,
                        scale: child_scale,
                    });
                }
                Primitive::PopLayer => {
                    let Some(frame) = frames.pop() else {
                        continue;
                    };
                    clips.truncate(frame.clip_depth);
                    let Some(composite) = frame.composite else {
                        continue;
                    };
                    let child = surfaces.pop().unwrap();
                    let parent = surfaces.last_mut().unwrap();
                    let clip = composite.clip;
                    match composite.offset {
                        Some(offset) => self.picture(
                            parent,
                            child.pixmap.as_ref(),
                            offset,
                            clip,
                            composite.opacity,
                        ),
                        None => {
                            // Turned or scaled: tiny-skia resamples the child
                            // into the clipped pixels, and those are laid over.
                            let Some(mut placed) = Pixmap::new(clip[2], clip[3]) else {
                                continue;
                            };
                            placed.draw_pixmap(
                                0,
                                0,
                                child.pixmap.as_ref(),
                                &PixmapPaint {
                                    quality: FilterQuality::Bilinear,
                                    ..PixmapPaint::default()
                                },
                                toward(composite.transform, clip),
                                None,
                            );
                            let corner = [clip[0] as i32, clip[1] as i32];
                            self.picture(parent, placed.as_ref(), corner, clip, composite.opacity);
                        }
                    }
                }
                Primitive::Quad {
                    rect,
                    color,
                    radius,
                } => {
                    let visible = surface.pixels(clip.intersect(*rect));
                    if visible[2] > 0 && visible[3] > 0 && color.3 > 0 {
                        self.quad(surface, visible, *rect, *color, *radius);
                    }
                }
                Primitive::Image {
                    rect,
                    image,
                    source,
                    tint,
                    radius,
                } => {
                    let visible = surface.pixels(clip.intersect(*rect));
                    if visible[2] > 0
                        && visible[3] > 0
                        && source.width != 0.0
                        && source.height != 0.0
                    {
                        self.image(surface, visible, *rect, image, *source, *tint, *radius);
                    }
                }
                Primitive::Gradient {
                    rect,
                    radius,
                    from,
                    to,
                    stops,
                    radial,
                } => {
                    let visible = surface.pixels(clip.intersect(*rect));
                    if visible[2] > 0 && visible[3] > 0 && stops.len() >= 2 {
                        self.gradient(surface, visible, *rect, *radius, *from, *to, stops, *radial);
                    }
                }
                Primitive::Shadow {
                    rect,
                    radius,
                    color,
                    blur,
                    spread,
                    offset,
                    inset,
                } => {
                    let cast = Rect {
                        x: rect.x + offset.x,
                        y: rect.y + offset.y,
                        ..*rect
                    };
                    // A blur reaches about three standard deviations out.
                    let reach = if *inset { 0.0 } else { spread + 3.0 * blur };
                    let touched = Rect {
                        x: cast.x - reach,
                        y: cast.y - reach,
                        width: cast.width + 2.0 * reach,
                        height: cast.height + 2.0 * reach,
                    };
                    let visible = surface.pixels(clip.intersect(touched));
                    if visible[2] > 0 && visible[3] > 0 && color.3 > 0 {
                        self.shadow(
                            surface, visible, cast, *radius, *color, *blur, *spread, *inset,
                        );
                    }
                }
                // Whatever draws these knows its own renderer; this one does
                // not, and leaves the rectangle as it is.
                Primitive::Custom { .. } => {}
                Primitive::Path { path, fill, stroke } => {
                    // Outside the clip nothing of it shows, and stroking it
                    // to find that out is most of what drawing it costs. A
                    // mitred corner reaches up to twice the width out.
                    let bounds = path.bounds();
                    let reach = stroke
                        .as_ref()
                        .map_or(0.0, |stroke| stroke.width * 2.0 + 1.0);
                    let touched = surface.pixels(clip.intersect(Rect {
                        x: bounds.x - reach,
                        y: bounds.y - reach,
                        width: bounds.width + 2.0 * reach,
                        height: bounds.height + 2.0 * reach,
                    }));
                    if touched[2] == 0 || touched[3] == 0 {
                        continue;
                    }
                    let Some(shape) = skia_path(path) else {
                        continue;
                    };
                    if let Some(color) = fill {
                        let visible = surface.pixels(clip.intersect(path.bounds()));
                        self.cover(surface, visible, &shape, *color);
                    }
                    if let Some(stroke) = stroke
                        && let Some(outline) = shape.stroke(
                            &tiny_skia::Stroke {
                                width: stroke.width,
                                miter_limit: 4.0,
                                line_cap: match path.cap() {
                                    glassmere_scene::LineCap::Butt => tiny_skia::LineCap::Butt,
                                    glassmere_scene::LineCap::Round => tiny_skia::LineCap::Round,
                                    glassmere_scene::LineCap::Square => tiny_skia::LineCap::Square,
                                },
                                line_join: match path.join() {
                                    glassmere_scene::LineJoin::Miter => tiny_skia::LineJoin::Miter,
                                    glassmere_scene::LineJoin::Round => tiny_skia::LineJoin::Round,
                                    glassmere_scene::LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                                },
                                ..tiny_skia::Stroke::default()
                            },
                            surface.scale,
                        )
                    {
                        let bounds = outline.bounds();
                        let visible = surface.pixels(clip.intersect(Rect {
                            x: bounds.x(),
                            y: bounds.y(),
                            width: bounds.width(),
                            height: bounds.height(),
                        }));
                        self.cover(surface, visible, &outline, stroke.color);
                    }
                }
                Primitive::Text { rect, color, .. }
                | Primitive::StyledText { rect, color, .. }
                | Primitive::RichText { rect, color, .. } => {
                    let visible = surface.pixels(clip.intersect(*rect));
                    if partial && (visible[2] == 0 || visible[3] == 0) {
                        continue;
                    }
                    let owned;
                    let key: &TextKey = match memo.as_mut() {
                        Some(memo) => {
                            held += text_bytes(primitive);
                            memo.key(&mut self.text, primitive)
                        }
                        None => {
                            owned = ensure_text(&mut self.text, primitive);
                            &owned
                        }
                    };
                    if visible[2] > 0 && visible[3] > 0 {
                        self.glyphs(surface, key, *rect, visible, *color);
                    }
                }
            }
        }
        if let Some(mut memo) = memo {
            memo.finish(&mut self.text);
            self.text_kept = self.text.buffers.len();
            self.text_kept_bytes = held;
            self.text_seen = self.text_kept;
            self.text_seen_bytes = held;
            self.text_memo = memo;
        }
        surfaces.swap_remove(0)
    }

    /// Fills `rect` over the surface's `visible` pixels, which are already clipped.
    fn quad(
        &mut self,
        surface: &mut Surface,
        visible: [u32; 4],
        rect: Rect,
        color: Color,
        radius: f32,
    ) {
        let stride = surface.pixmap.width() as usize * 4;
        let start = |row: u32| (visible[1] + row) as usize * stride + visible[0] as usize * 4;
        let opaque = [color.0, color.1, color.2, 255];
        let rgb = [color.0, color.1, color.2];
        if radius <= 0.0 {
            // A rectangle's coverage is its area, without a rasterizer.
            let scale = surface.scale;
            let (left, top) = (
                (rect.x - surface.origin[0]) * scale,
                (rect.y - surface.origin[1]) * scale,
            );
            let (right, bottom) = (left + rect.width * scale, top + rect.height * scale);
            let cover = |at: u32, from: f32, to: f32| {
                ((to.min(at as f32 + 1.0) - from.max(at as f32)).clamp(0.0, 1.0) * 255.0).round()
                    as u32
            };
            // A column is covered alike on every row, so each is worked out
            // once for the quad and not again for each of its pixels.
            self.columns.clear();
            self.columns.extend(
                (0..visible[2]).map(|column| cover(visible[0] + column, left, right) as u8),
            );
            let columns = &self.columns[..];
            // The first run of columns covered whole: along it a row has one
            // alpha. The columns beside it are mixed one by one.
            let whole = columns
                .iter()
                .position(|&coverage| coverage == 255)
                .unwrap_or(columns.len());
            let end = whole
                + columns[whole..]
                    .iter()
                    .take_while(|&&coverage| coverage == 255)
                    .count();
            let data = surface.pixmap.data_mut();
            for row in 0..visible[3] {
                let rows = cover(visible[1] + row, top, bottom);
                let line = data[start(row)..].as_chunks_mut::<4>().0;
                // No row is taken past the end of the surface, whatever `visible` says.
                let shown = line.len().min(columns.len());
                let line = &mut line[..shown];
                let (from, to) = (whole.min(shown), end.min(shown));
                let mix = |pixel: &mut [u8; 4], coverage: u8| {
                    let alpha = u32::from(coverage) * rows * u32::from(color.3) / (255 * 255);
                    if alpha == 255 {
                        *pixel = opaque;
                    } else if alpha > 0 {
                        self.gamma.over(pixel, self.gamma.light(rgb, alpha), alpha);
                    }
                };
                for (pixel, coverage) in line[..from].iter_mut().zip(&columns[..from]) {
                    mix(pixel, *coverage);
                }
                for (pixel, coverage) in line[to..].iter_mut().zip(&columns[to..]) {
                    mix(pixel, *coverage);
                }
                let alpha = 255 * rows * u32::from(color.3) / (255 * 255);
                if alpha == 255 {
                    line[from..to].fill(opaque);
                } else if alpha > 0 {
                    let source = self.gamma.light(rgb, alpha);
                    for pixel in &mut line[from..to] {
                        self.gamma.over(pixel, source, alpha);
                    }
                }
            }
            return;
        }
        if let Some(path) = rounded(rect, radius) {
            self.cover(surface, visible, &path, color);
        }
    }

    /// Fills a shape in logical coordinates over the `visible` pixels.
    /// tiny-skia says how much of each one is covered.
    fn cover(
        &self,
        surface: &mut Surface,
        visible: [u32; 4],
        path: &tiny_skia::Path,
        color: Color,
    ) {
        // A mask per shape. Reuse one buffer if a profile of a real page
        // ever shows the allocations.
        if color.3 == 0 {
            return;
        }
        let Some(mut mask) = Mask::new(visible[2], visible[3]) else {
            return;
        };
        let stride = surface.pixmap.width() as usize * 4;
        let start = |row: u32| (visible[1] + row) as usize * stride + visible[0] as usize * 4;
        let opaque = [color.0, color.1, color.2, 255];
        let rgb = [color.0, color.1, color.2];
        mask.fill_path(
            path,
            FillRule::Winding,
            true,
            toward(surface.transform(), visible),
        );
        let coverage = mask.data();
        let data = surface.pixmap.data_mut();
        for (row, coverage) in coverage.chunks_exact(visible[2] as usize).enumerate() {
            let pixels = data[start(row as u32)..].as_chunks_mut::<4>().0;
            for (pixel, coverage) in pixels.iter_mut().zip(coverage) {
                let alpha = (u32::from(*coverage) * u32::from(color.3) + 127) / 255;
                if alpha == 255 {
                    *pixel = opaque;
                } else if alpha > 0 {
                    self.gamma.over(pixel, self.gamma.light(rgb, alpha), alpha);
                }
            }
        }
    }

    /// Fills `rect` with a ramp of colours, mixed in linear light as the GPU
    /// mixes them.
    #[allow(clippy::too_many_arguments)]
    fn gradient(
        &self,
        surface: &mut Surface,
        visible: [u32; 4],
        rect: Rect,
        radius: f32,
        from: Point,
        to: Point,
        stops: &[GradientStop],
        radial: bool,
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        // Stops in linear light, in order, with anything out of order or out
        // of range pushed into place so that the walk below is monotone.
        let mut ramp: Vec<(f32, [f32; 3], f32)> = Vec::with_capacity(stops.len());
        for stop in stops.iter().take(MAX_GRADIENT_STOPS) {
            let at = stop.at.clamp(0.0, 1.0);
            let at = ramp
                .last()
                .map_or(at, |(last, _, _): &(f32, _, _)| at.max(*last));
            ramp.push((
                at,
                [stop.color.0, stop.color.1, stop.color.2]
                    .map(|channel| f32::from(self.gamma.linear[usize::from(channel)])),
                f32::from(stop.color.3),
            ));
        }
        let Some(mut mask) = Mask::new(visible[2], visible[3]) else {
            return;
        };
        let Some(path) = rounded(rect, radius) else {
            return;
        };
        mask.fill_path(
            &path,
            FillRule::Winding,
            true,
            toward(surface.transform(), visible),
        );
        let (axis_x, axis_y) = (to.x - from.x, to.y - from.y);
        let length = axis_x * axis_x + axis_y * axis_y;
        let stride = surface.pixmap.width() as usize * 4;
        let (origin, scale) = (surface.origin, surface.scale);
        let coverage = mask.data();
        let data = surface.pixmap.data_mut();
        for row in 0..visible[3] {
            // The rectangle's own units, where its corners are 0 and 1.
            let y =
                ((origin[1] + ((visible[1] + row) as f32 + 0.5) / scale) - rect.y) / rect.height;
            let start = (visible[1] + row) as usize * stride + visible[0] as usize * 4;
            let line = &coverage[row as usize * visible[2] as usize..][..visible[2] as usize];
            for (column, cover) in line.iter().enumerate() {
                if *cover == 0 {
                    continue;
                }
                let x = ((origin[0] + ((visible[0] + column as u32) as f32 + 0.5) / scale)
                    - rect.x)
                    / rect.width;
                let (dx, dy) = (x - from.x, y - from.y);
                let at = if length <= f32::EPSILON {
                    1.0
                } else if radial {
                    (dx * dx + dy * dy).sqrt() / length.sqrt()
                } else {
                    (dx * axis_x + dy * axis_y) / length
                }
                .clamp(0.0, 1.0);
                // The pair the point falls between, or the end it is past.
                let next = ramp.iter().position(|(stop, _, _)| *stop >= at);
                let (light, alpha) = match next {
                    None => (ramp[ramp.len() - 1].1, ramp[ramp.len() - 1].2),
                    Some(0) => (ramp[0].1, ramp[0].2),
                    Some(index) => {
                        let (low, high) = (&ramp[index - 1], &ramp[index]);
                        let span = high.0 - low.0;
                        let mixed = if span <= f32::EPSILON {
                            0.0
                        } else {
                            (at - low.0) / span
                        };
                        (
                            std::array::from_fn(|channel| {
                                low.1[channel] + (high.1[channel] - low.1[channel]) * mixed
                            }),
                            low.2 + (high.2 - low.2) * mixed,
                        )
                    }
                };
                let alpha = alpha * f32::from(*cover) / 255.0;
                let scaled = (alpha.round().clamp(0.0, 255.0)) as u32;
                if scaled == 0 {
                    continue;
                }
                let pixel = &mut data[start + column * 4..][..4];
                let premultiplied = light
                    .map(|channel| (channel * alpha / 255.0).round().clamp(0.0, 4095.0) as u32);
                self.gamma.over(pixel, premultiplied, scaled);
            }
        }
    }

    /// A blurred rounded rectangle, by the same approximation the GPU uses:
    /// four Gaussian-weighted samples of an erf-integrated rounded profile.
    #[allow(clippy::too_many_arguments)]
    fn shadow(
        &self,
        surface: &mut Surface,
        visible: [u32; 4],
        rect: Rect,
        radius: f32,
        color: Color,
        blur: f32,
        spread: f32,
        inset: bool,
    ) {
        let half = [
            rect.width / 2.0 + if inset { 0.0 } else { spread },
            rect.height / 2.0 + if inset { 0.0 } else { spread },
        ];
        if half[0] <= 0.0 || half[1] <= 0.0 {
            return;
        }
        let centre = [rect.x + rect.width / 2.0, rect.y + rect.height / 2.0];
        let corner = (radius + if inset { 0.0 } else { spread }).clamp(0.0, half[0].min(half[1]));
        let sigma = blur.max(0.0);
        let rgb = [color.0, color.1, color.2];
        let stride = surface.pixmap.width() as usize * 4;
        let (origin, scale) = (surface.origin, surface.scale);
        // Inside a rectangle the blur only darkens its own edge, so the plain
        // coverage of the rectangle bounds it.
        let inner = inset.then(|| {
            let mut mask = Mask::new(visible[2], visible[3])?;
            let path = rounded(rect, radius)?;
            mask.fill_path(
                &path,
                FillRule::Winding,
                true,
                toward(surface.transform(), visible),
            );
            Some(mask)
        });
        if inset && inner.as_ref().is_none_or(Option::is_none) {
            return;
        }
        let inner = inner.flatten();
        let data = surface.pixmap.data_mut();
        for row in 0..visible[3] {
            let y = origin[1] + ((visible[1] + row) as f32 + 0.5) / scale - centre[1];
            let start = (visible[1] + row) as usize * stride + visible[0] as usize * 4;
            for column in 0..visible[2] {
                let x = origin[0] + ((visible[0] + column) as f32 + 0.5) / scale - centre[0];
                let mut coverage = if sigma <= 0.0 {
                    rounded_coverage([x, y], half, corner)
                } else {
                    rounded_box_shadow(half, [x, y], sigma, corner)
                };
                if inset {
                    let at = (row * visible[2] + column) as usize;
                    let within = inner
                        .as_ref()
                        .map_or(0.0, |mask| f32::from(mask.data()[at]) / 255.0);
                    coverage = (1.0 - coverage) * within;
                }
                let alpha = (coverage.clamp(0.0, 1.0) * f32::from(color.3)).round() as u32;
                if alpha == 0 {
                    continue;
                }
                let pixel = &mut data[start + column as usize * 4..][..4];
                self.gamma.over(pixel, self.gamma.light(rgb, alpha), alpha);
            }
        }
    }

    /// Lays premultiplied pixels over the surface with their corner at
    /// `corner`, within `clip`, faded by `opacity` (0..=255).
    fn picture(
        &self,
        surface: &mut Surface,
        picture: PixmapRef<'_>,
        corner: [i32; 2],
        clip: [u32; 4],
        opacity: u32,
    ) {
        let stride = surface.pixmap.width() as usize * 4;
        let (width, height) = (picture.width() as i32, picture.height() as i32);
        let min_x = (clip[0] as i32).max(corner[0]);
        let min_y = (clip[1] as i32).max(corner[1]);
        let max_x = ((clip[0] + clip[2]) as i32).min(corner[0] + width);
        let max_y = ((clip[1] + clip[3]) as i32).min(corner[1] + height);
        let source = picture.data();
        let data = surface.pixmap.data_mut();
        for y in min_y..max_y {
            for x in min_x..max_x {
                let at = ((y - corner[1]) * width + (x - corner[0])) as usize * 4;
                let texel: [u8; 4] = source[at..at + 4].try_into().unwrap();
                let alpha = (u32::from(texel[3]) * opacity + 127) / 255;
                if alpha == 0 {
                    continue;
                }
                let pixel = &mut data[y as usize * stride + x as usize * 4..][..4];
                if alpha == 255 {
                    pixel.copy_from_slice(&texel);
                    continue;
                }
                // These are premultiplied already, as a GPU texture of the
                // same bytes is: they only need the fade.
                let light = [texel[0], texel[1], texel[2]].map(|channel| {
                    (u32::from(self.gamma.linear[usize::from(channel)]) * opacity + 127) / 255
                });
                self.gamma.over(pixel, light, alpha);
            }
        }
    }

    /// Draws part of an image into `rect`, over the surface's `visible`
    /// pixels, which are already clipped. Sampled in linear light, as the
    /// GPU's sRGB textures are, so that scaling a translucent image gives the
    /// same colours.
    #[allow(clippy::too_many_arguments)]
    fn image(
        &mut self,
        surface: &mut Surface,
        visible: [u32; 4],
        rect: Rect,
        image: &Image,
        source: Rect,
        tint: Option<Color>,
        radius: f32,
    ) {
        let natural = image.size();
        if natural.width <= 0.0 || natural.height <= 0.0 {
            return;
        }
        // The pixels to draw from: a raster image's own, filtered down when it
        // is drawn at under half its size so that sampling never shrinks it by
        // more than two, or a vector image rasterized for the device pixels
        // `rect` covers.
        let drawn =
            |natural: f32, rect: f32, source: f32| natural * rect / source.abs() * surface.scale;
        let size = match image.pixels() {
            Some((width, height, _)) => [
                halved_side(width, drawn(natural.width, rect.width, source.width)),
                halved_side(height, drawn(natural.height, rect.height, source.height)),
            ],
            None => {
                let side = |natural: f32, rect: f32, source: f32| {
                    let pixels = drawn(natural, rect, source).ceil();
                    if pixels.is_finite() {
                        (pixels.min(MAX_SIDE as f32) as u32).max(1)
                    } else {
                        1
                    }
                };
                [
                    side(natural.width, rect.width, source.width),
                    side(natural.height, rect.height, source.height),
                ]
            }
        };
        let (width, height) = (size[0], size[1]);
        let prepared = self.prepare(image, size);
        let pixels = match image.pixels() {
            Some((width, height, pixels)) if !prepared && [width, height] == size => pixels,
            _ => match self.rasters.get(&(image.id(), size)) {
                Some(raster) => raster.texels.data(),
                None => return,
            },
        };
        // Image units per pixel; 1 for a raster image.
        let (unit_x, unit_y) = (natural.width / width as f32, natural.height / height as f32);
        // The texels the source rectangle covers. Nothing outside may be
        // sampled, or the neighbours of a nine-slice piece would bleed in.
        let span = |at: f32, length: f32, unit: f32, side: u32| {
            let (from, to) = (at / unit, (at + length) / unit);
            let low = from.min(to).floor().clamp(0.0, side as f32 - 1.0);
            let high = from.max(to).ceil().clamp(low + 1.0, side as f32);
            (low as i32, high as i32 - 1)
        };
        let horizontal = span(source.x, source.width, unit_x, width);
        let vertical = span(source.y, source.height, unit_y, height);
        // A surface pixel's centre as a texel coordinate, one axis at a time:
        // the centre is a logical point, the logical point is a point of the
        // source rectangle, and that is a texel. A mirrored source slopes down.
        let map = |origin: f32, edge: f32, at: f32, extent: f32, length: f32, unit: f32| {
            let slope = extent / (length * unit * surface.scale);
            let start = (at + (origin - edge) * extent / length) / unit - 0.5 + slope * 0.5;
            (slope, start)
        };
        let (slope_x, start_x) = map(
            surface.origin[0],
            rect.x,
            source.x,
            source.width,
            rect.width,
            unit_x,
        );
        let (slope_y, start_y) = map(
            surface.origin[1],
            rect.y,
            source.y,
            source.height,
            rect.height,
            unit_y,
        );
        if ![slope_x, start_x, slope_y, start_y]
            .iter()
            .all(|value| value.is_finite())
        {
            return;
        }
        // Rounded corners are anti-aliased and square ones are not, so that
        // nine-slice pieces and sprites meet without a seam, as on the GPU.
        let (Some(path), Some(mut mask)) =
            (rounded(rect, radius), Mask::new(visible[2], visible[3]))
        else {
            return;
        };
        mask.fill_path(
            &path,
            FillRule::Winding,
            radius > 0.0,
            toward(surface.transform(), visible),
        );
        // A tint replaces the image's colours and keeps its alpha.
        let tinted = tint.map(|Color(red, green, blue, alpha)| {
            (
                [red, green, blue]
                    .map(|channel| f32::from(self.gamma.linear[usize::from(channel)])),
                f32::from(alpha) / 255.0,
            )
        });
        let texel = |x: i32, y: i32| {
            let x = x.clamp(horizontal.0, horizontal.1) as usize;
            let y = y.clamp(vertical.0, vertical.1) as usize;
            let at = (y * width as usize + x) * 4;
            [
                f32::from(self.gamma.linear[usize::from(pixels[at])]),
                f32::from(self.gamma.linear[usize::from(pixels[at + 1])]),
                f32::from(self.gamma.linear[usize::from(pixels[at + 2])]),
                f32::from(pixels[at + 3]) * (4095.0 / 255.0),
            ]
        };
        let between = |low: [f32; 4], high: [f32; 4], at: f32| {
            std::array::from_fn(|channel| low[channel] + (high[channel] - low[channel]) * at)
        };
        let stride = surface.pixmap.width() as usize * 4;
        let coverage = mask.data();
        let data = surface.pixmap.data_mut();
        for row in 0..visible[3] {
            let y = start_y + slope_y * (visible[1] + row) as f32;
            let (top, down) = (y.floor(), y - y.floor());
            let start = (visible[1] + row) as usize * stride + visible[0] as usize * 4;
            let line = &coverage[row as usize * visible[2] as usize..][..visible[2] as usize];
            for (column, cover) in line.iter().enumerate() {
                if *cover == 0 {
                    continue;
                }
                let x = start_x + slope_x * (visible[0] + column as u32) as f32;
                let (left, across) = (x.floor(), x - x.floor());
                let (left, top) = (left as i32, top as i32);
                let sampled = between(
                    between(texel(left, top), texel(left + 1, top), across),
                    between(texel(left, top + 1), texel(left + 1, top + 1), across),
                    down,
                );
                let cover = f32::from(*cover) / 255.0;
                let (light, alpha) = match tinted {
                    Some((tint, opacity)) => (
                        tint.map(|channel| channel * sampled[3] / 4095.0 * opacity * cover),
                        sampled[3] * opacity * cover,
                    ),
                    None => (
                        [sampled[0] * cover, sampled[1] * cover, sampled[2] * cover],
                        sampled[3] * cover,
                    ),
                };
                let alpha = (alpha / 4095.0 * 255.0).round().clamp(0.0, 255.0) as u32;
                if alpha == 0 {
                    continue;
                }
                let pixel = &mut data[start + column * 4..][..4];
                self.gamma.over(
                    pixel,
                    light.map(|channel| channel.round().clamp(0.0, 4095.0) as u32),
                    alpha,
                );
            }
        }
    }

    /// Makes the image's texels ready to sample and marks them used: a vector
    /// image rasterized at `size`, a translucent image premultiplied in linear
    /// light, and a photo filtered down to `size`. True when they are in the
    /// cache rather than in the image.
    fn prepare(&mut self, image: &Image, size: [u32; 2]) -> bool {
        let frame = self.stats.frames;
        if let Some(entry) = self.rasters.get_mut(&(image.id(), size)) {
            entry.used = frame;
            return true;
        }
        let settled = |pixel: &[u8; 4]| matches!(pixel[3], 0 | 255);
        let mut texels = match image.pixels() {
            // Filtered from the image's own texels, with translucent ones
            // converted as they are read, so that the photo is not copied: the
            // copy would be the largest allocation here and only the first
            // step reads it.
            Some((width, height, pixels)) if [width, height] != size => {
                let axes = [width > size[0], height > size[1]];
                let Some(mut texels) = photo::halve(
                    pixels,
                    width,
                    height,
                    axes,
                    Some(&mut self.premultiply),
                    &self.gamma,
                ) else {
                    return false;
                };
                while texels.width() > size[0] || texels.height() > size[1] {
                    let axes = [texels.width() > size[0], texels.height() > size[1]];
                    let Some(smaller) = photo::halve(
                        texels.data(),
                        texels.width(),
                        texels.height(),
                        axes,
                        None,
                        &self.gamma,
                    ) else {
                        return false;
                    };
                    texels = smaller;
                }
                let raster = Raster {
                    texels,
                    used: frame,
                    photo: true,
                };
                self.rasters.insert((image.id(), size), raster);
                return true;
            }
            // Texels that are opaque or clear need no conversion, which is
            // most images: only their alpha would have been divided out.
            Some((_, _, pixels)) if pixels.as_chunks::<4>().0.iter().all(settled) => {
                return false;
            }
            Some((width, height, pixels)) => {
                let Some(mut texels) = Pixmap::new(width, height) else {
                    return false;
                };
                texels.data_mut().copy_from_slice(pixels);
                texels
            }
            None => {
                let (Some(vector), Some(mut texels)) =
                    (image.vector(), Pixmap::new(size[0], size[1]))
                else {
                    return false;
                };
                let data = vector.rasterize(size[0], size[1]);
                // A vector image that returns the wrong amount is not drawn.
                if data.len() != texels.data().len() {
                    return false;
                }
                texels.data_mut().copy_from_slice(&data);
                texels
            }
        };
        // Scene images are premultiplied in sRGB space and blending is linear,
        // so a translucent texel is stored as the sRGB encoding of its
        // premultiplied linear colour, exactly as the GPU's textures hold it.
        for pixel in texels.data_mut().as_chunks_mut::<4>().0 {
            let alpha = pixel[3];
            if settled(pixel) {
                continue;
            }
            for channel in &mut pixel[..3] {
                *channel = self.premultiply.convert(*channel, alpha, &self.gamma);
            }
        }
        let raster = Raster {
            texels,
            used: frame,
            photo: false,
        };
        self.rasters.insert((image.id(), size), raster);
        true
    }

    /// Lays a shaped block's glyph images over the surface, within `visible`.
    fn glyphs(
        &mut self,
        surface: &mut Surface,
        key: &TextKey,
        rect: Rect,
        visible: [u32; 4],
        color: Color,
    ) {
        let Some(buffer) = self.text.buffers.get(key) else {
            return;
        };
        let scale = surface.scale;
        let left = (rect.x - surface.origin[0]) * scale;
        let top = (rect.y - surface.origin[1]) * scale;
        let stride = surface.pixmap.width() as usize * 4;
        let (min_x, min_y) = (visible[0] as i32, visible[1] as i32);
        let (max_x, max_y) = (min_x + visible[2] as i32, min_y + visible[3] as i32);
        let data = surface.pixmap.data_mut();
        for run in buffer.layout_runs() {
            let line_y = (run.line_y * scale).round() as i32;
            for glyph in run.glyphs {
                let physical = glyph.physical((left, top), scale);
                let Some(image) = self
                    .swash
                    .get_image(&mut self.text.fonts, physical.cache_key)
                else {
                    continue;
                };
                let ink = match glyph.color_opt {
                    Some(color) => [color.r(), color.g(), color.b(), color.a()],
                    None => [color.0, color.1, color.2, color.3],
                };
                let (width, height) = (image.placement.width as i32, image.placement.height as i32);
                let x0 = physical.x + image.placement.left;
                let y0 = line_y + physical.y - image.placement.top;
                self.stats.glyphs += 1;
                for y in y0.max(min_y)..(y0 + height).min(max_y) {
                    for x in x0.max(min_x)..(x0 + width).min(max_x) {
                        let at = ((y - y0) * width + (x - x0)) as usize;
                        // A straight colour and the alpha to lay it on with.
                        let (rgb, alpha) = match image.content {
                            SwashContent::Mask => (
                                [ink[0], ink[1], ink[2]],
                                u32::from(image.data[at]) * u32::from(ink[3]) / 255,
                            ),
                            // Colour glyphs, such as emoji, keep their own colours.
                            SwashContent::Color => (
                                [
                                    image.data[at * 4],
                                    image.data[at * 4 + 1],
                                    image.data[at * 4 + 2],
                                ],
                                u32::from(image.data[at * 4 + 3]),
                            ),
                            // cosmic-text does not produce these.
                            SwashContent::SubpixelMask => continue,
                        };
                        if alpha > 0 {
                            let pixel = &mut data[y as usize * stride + x as usize * 4..][..4];
                            self.gamma.over(pixel, self.gamma.light(rgb, alpha), alpha);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glassmere_scene::{Size, VectorImage};

    fn renderer() -> CpuRenderer {
        CpuRenderer::with_text_system(TextSystem::with_fonts([include_bytes!(
            "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
        )
        .to_vec()]))
    }

    /// White where `x + y` is even, black elsewhere.
    fn checker(side: u32) -> Image {
        let mut rgba = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let shade = if (x + y) % 2 == 0 { 255 } else { 0 };
                rgba.extend([shade, shade, shade, 255]);
            }
        }
        Image::from_rgba8(side, side, rgba).unwrap()
    }

    /// Opaque white at whatever size it is asked for.
    struct Flat;

    impl VectorImage for Flat {
        fn size(&self) -> Size {
            Size::new(8.0, 8.0)
        }
        fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
            [255; 4].repeat((width * height) as usize)
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

    fn draw(image: &Image, rect: Rect) -> Primitive {
        let size = image.size();
        Primitive::Image {
            rect,
            image: image.clone(),
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: size.width,
                height: size.height,
            },
            tint: None,
            radius: 0.0,
        }
    }

    fn scene(primitives: Vec<Primitive>) -> Scene {
        Scene {
            size: Size::new(64.0, 64.0),
            primitives,
        }
    }

    const SQUARE: Rect = Rect {
        x: 40.0,
        y: 40.0,
        width: 16.0,
        height: 16.0,
    };

    fn quad() -> Primitive {
        Primitive::Quad {
            rect: SQUARE,
            color: Color(200, 0, 0, 255),
            radius: 0.0,
        }
    }

    #[test]
    fn a_filtered_photo_survives_frames_that_do_not_draw_it() {
        let photo = checker(256);
        let thumbnail = rect(0.0, 0.0, 32.0, 32.0);
        let key = (photo.id(), [32, 32]);
        let page = scene(vec![draw(&photo, thumbnail), quad()]);
        let elsewhere = scene(vec![quad()]);

        let mut renderer = renderer();
        let first = renderer.render([64, 64], 1.0, &page, Color::WHITE).to_vec();
        assert!(renderer.rasters.contains_key(&key));
        for _ in 0..10 {
            renderer.render([64, 64], 1.0, &elsewhere, Color::WHITE);
        }
        assert!(renderer.rasters.contains_key(&key));
        let again = renderer.render([64, 64], 1.0, &page, Color::WHITE).to_vec();
        assert_eq!(again, first);
        assert_eq!(renderer.rasters.len(), 1);

        // What an application does: whole frame, then frames of damage on
        // something else, then damage over the photo.
        let mut renderer = self::renderer();
        renderer.render([64, 64], 1.0, &page, Color::WHITE);
        for _ in 0..5 {
            renderer.render_damaged([64, 64], 1.0, &page, Color::WHITE, &[SQUARE]);
        }
        assert!(renderer.rasters.contains_key(&key));
        let hovered = renderer
            .render_damaged([64, 64], 1.0, &page, Color::WHITE, &[thumbnail])
            .to_vec();
        assert_eq!(hovered, first);
        assert_eq!(renderer.rasters.len(), 1);

        // A vector image is rasterized again after two frames without it.
        let flat = Image::from_vector(Flat);
        let flat_key = (flat.id(), [8, 8]);
        let mut renderer = self::renderer();
        renderer.render(
            [64, 64],
            1.0,
            &scene(vec![draw(&flat, rect(0.0, 0.0, 8.0, 8.0))]),
            Color::WHITE,
        );
        assert!(renderer.rasters.contains_key(&flat_key));
        renderer.render([64, 64], 1.0, &elsewhere, Color::WHITE);
        assert!(renderer.rasters.contains_key(&flat_key));
        renderer.render([64, 64], 1.0, &elsewhere, Color::WHITE);
        assert!(!renderer.rasters.contains_key(&flat_key));
    }

    #[test]
    fn a_photo_drawn_on_every_frame_is_kept() {
        let photo = checker(256);
        let key = (photo.id(), [32, 32]);
        let page = scene(vec![draw(&photo, rect(0.0, 0.0, 32.0, 32.0))]);
        let mut renderer = renderer();
        // Past the 120 frames a photo may go without being drawn.
        for _ in 0..130 {
            renderer.render([64, 64], 1.0, &page, Color::WHITE);
            let frame = renderer.stats().frames;
            assert!(renderer.rasters.contains_key(&key), "frame {frame}");
            assert_eq!(renderer.rasters[&key].used, frame);
        }
    }

    #[test]
    fn idle_limit_and_budget_drop_the_least_recently_drawn_photos() {
        assert_eq!(renderer().photo_budget, 64 << 20);
        let nothing = scene(Vec::new());
        // 64 texels in 16 pixels: 16 by 16 texels of four bytes, 1,024 bytes.
        let thumbnail = rect(0.0, 0.0, 16.0, 16.0);
        let photo = checker(64);
        let flat = Image::from_vector(Flat);
        let photo_key = (photo.id(), [16, 16]);
        let flat_key = (flat.id(), [8, 8]);

        // Whole frames, and frames painted over the last one in the damage
        // alone, which is what an application mostly renders.
        for partial in [false, true] {
            let render = |renderer: &mut CpuRenderer, page: &Scene, damage: Rect| {
                let damage = if partial { vec![damage] } else { Vec::new() };
                renderer.render_damaged([64, 64], 1.0, page, Color::WHITE, &damage);
            };

            // A photo goes on the 120th frame without it, a vector image on
            // the second.
            let mut renderer = renderer();
            let both = scene(vec![
                draw(&photo, thumbnail),
                draw(&flat, rect(32.0, 0.0, 8.0, 8.0)),
            ]);
            render(&mut renderer, &both, thumbnail);
            for idle in 1..=119 {
                render(&mut renderer, &nothing, SQUARE);
                let flat_kept = renderer.rasters.contains_key(&flat_key);
                assert_eq!(flat_kept, idle < 2, "{partial} {idle}");
                let photo_kept = renderer.rasters.contains_key(&photo_key);
                assert!(photo_kept, "{partial} {idle}");
            }
            render(&mut renderer, &nothing, SQUARE);
            assert!(!renderer.rasters.contains_key(&photo_key), "{partial}");

            // Four photos drawn in turn do not fit three entries: the first
            // goes.
            let photos: Vec<Image> = (0..4).map(|_| checker(64)).collect();
            let mut renderer = self::renderer();
            renderer.photo_budget = 3 * 1024;
            for photo in &photos {
                let page = scene(vec![draw(photo, thumbnail)]);
                render(&mut renderer, &page, thumbnail);
            }
            let alive: Vec<bool> = photos
                .iter()
                .map(|photo| renderer.rasters.contains_key(&(photo.id(), [16, 16])))
                .collect();
            assert_eq!(alive, [false, true, true, true], "{partial}");

            // What a frame drew is kept whatever the budget, whether it
            // filtered the photo or found it, and goes after.
            let mut renderer = self::renderer();
            renderer.photo_budget = 1;
            let page = scene(vec![draw(&photo, thumbnail)]);
            for _ in 0..2 {
                render(&mut renderer, &page, thumbnail);
                assert!(renderer.rasters.contains_key(&photo_key), "{partial}");
            }
            render(&mut renderer, &nothing, SQUARE);
            assert!(!renderer.rasters.contains_key(&photo_key), "{partial}");
        }
    }
}
