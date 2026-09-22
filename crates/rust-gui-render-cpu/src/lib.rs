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
//! It follows the display list exactly as `rust-gui-render-wgpu` does: the
//! same clip stack, the same layers snapped to the pixel grid, the same shaped
//! text from `rust-gui-text`, the same premultiplied linear texels, and, like
//! an sRGB render target, it blends in
//! linear light, so translucent fills, faded layers and the soft edges of
//! glyphs come out the same. tiny-skia supplies geometry: the coverage of
//! rounded corners and the resampling of images and transformed layers.
//!
//! It is close to the GPU's output, not identical. A quad's edge is covered by
//! its exact area here and feathered by the shader there, and what is resampled
//! is filtered on sRGB-encoded values here and in linear light there. Both show
//! at edges, never across a flat area.
use rust_gui_scene::{Affine, Color, Image, Point, Primitive, Rect, Scene};
use rust_gui_text::cosmic_text::{SwashCache, SwashContent};
use rust_gui_text::{TextKey, TextSystem};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tiny_skia::{
    FillRule, FilterQuality, Mask, PathBuilder, Pixmap, PixmapPaint, PixmapRef, Transform,
};

/// The longest side a vector image or a layer is rasterized at.
const MAX_SIDE: u32 = 8192;

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
    /// Texels ready to sample, by image and pixel size, with the frame that
    /// used them last: vector images rasterized, translucent ones converted.
    rasters: HashMap<(u64, [u32; 2]), (Pixmap, u64)>,
    gamma: Gamma,
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

fn affine(affine: Affine) -> Transform {
    Transform::from_row(affine.a, affine.b, affine.c, affine.d, affine.e, affine.f)
}

/// Moves what a transform draws so that pixel `corner` of the surface becomes
/// the origin: everything is painted into a buffer covering `corner` alone,
/// which is how clipping is enforced here.
fn toward(transform: Transform, corner: [u32; 4]) -> Transform {
    transform.post_translate(-(corner[0] as f32), -(corner[1] as f32))
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
            rasters: HashMap::new(),
            gamma: Gamma::new(),
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
        let region = match (kept, damage.is_empty()) {
            (true, false) => damage
                .iter()
                .copied()
                .reduce(|region, rect| region.union(rect))
                .unwrap_or_default(),
            _ => Rect {
                x: 0.0,
                y: 0.0,
                width: physical_size[0] as f32 / scale,
                height: physical_size[1] as f32 / scale,
            },
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
        // Clearing only the damaged pixels is what keeps the rest.
        let cleared = root.pixels(region);
        self.stats.redrawn = u64::from(cleared[2]) * u64::from(cleared[3]);
        let stride = root.pixmap.width() as usize * 4;
        let clear = [clear.0, clear.1, clear.2, clear.3];
        let data = root.pixmap.data_mut();
        for row in 0..cleared[3] {
            let start = (cleared[1] + row) as usize * stride + cleared[0] as usize * 4;
            for pixel in data[start..].as_chunks_mut::<4>().0[..cleared[2] as usize].iter_mut() {
                *pixel = clear;
            }
        }
        let root = self.paint(scene, region, root);
        let frame = self.stats.frames;
        self.rasters.retain(|_, (_, used)| frame - *used < 2);
        self.stats.render = started.elapsed();
        self.target.insert(root.pixmap).data()
    }

    /// `region` is what may be painted: the whole viewport, or the damage.
    fn paint(&mut self, scene: &Scene, region: Rect, root: Surface) -> Surface {
        // Oversized layers render at a lower resolution, as on the GPU.
        let cap = root.size().map(|side| side.saturating_mul(2).min(MAX_SIDE));
        // Every draw is clipped, so bounding the stack bounds the frame.
        let mut clips = vec![region];
        let mut surfaces = vec![root];
        let mut frames: Vec<LayerFrame> = Vec::new();
        let mut active_text: HashSet<TextKey> = HashSet::new();
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
                    // ponytail: a pixmap per layer per frame. Pool them by size,
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
                Primitive::Text { rect, color, .. }
                | Primitive::StyledText { rect, color, .. }
                | Primitive::RichText { rect, color, .. } => {
                    let key = match primitive {
                        Primitive::Text {
                            text, font_size, ..
                        } => self.text.ensure(text, *font_size),
                        Primitive::StyledText { text, style, .. } => {
                            self.text.ensure_styled(text, style, Some(rect.width))
                        }
                        Primitive::RichText { spans, style, .. } => {
                            self.text.ensure_rich(spans, style, Some(rect.width))
                        }
                        _ => unreachable!(),
                    };
                    active_text.insert(key.clone());
                    let visible = surface.pixels(clip.intersect(*rect));
                    if visible[2] > 0 && visible[3] > 0 {
                        self.glyphs(surface, &key, *rect, visible, *color);
                    }
                }
            }
        }
        self.text.retain(&active_text);
        surfaces.swap_remove(0)
    }

    /// Fills `rect` over the surface's `visible` pixels, which are already clipped.
    fn quad(
        &self,
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
            let data = surface.pixmap.data_mut();
            for row in 0..visible[3] {
                let rows = cover(visible[1] + row, top, bottom);
                for (column, pixel) in data[start(row)..]
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .take(visible[2] as usize)
                    .enumerate()
                {
                    let alpha =
                        cover(visible[0] + column as u32, left, right) * rows * u32::from(color.3)
                            / (255 * 255);
                    if alpha == 255 {
                        *pixel = opaque;
                    } else if alpha > 0 {
                        self.gamma.over(pixel, self.gamma.light(rgb, alpha), alpha);
                    }
                }
            }
            return;
        }
        // Rounded: tiny-skia says how much of each visible pixel is covered.
        // ponytail: a mask per rounded quad. Reuse one buffer if a profile of
        // a real page ever shows the allocations.
        let (Some(path), Some(mut mask)) =
            (rounded(rect, radius), Mask::new(visible[2], visible[3]))
        else {
            return;
        };
        mask.fill_path(
            &path,
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
        // The pixels to draw from: a raster image's own, or a vector image
        // rasterized for the device pixels `rect` covers.
        let size = match image.pixels() {
            Some((width, height, _)) => [width, height],
            None => {
                let side = |natural: f32, rect: f32, source: f32| {
                    let pixels = (natural * rect / source.abs() * surface.scale).ceil();
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
            Some((_, _, pixels)) if !prepared => pixels,
            _ => match self.rasters.get(&(image.id(), size)) {
                Some((texels, _)) => texels.data(),
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
    /// image rasterized at `size`, and a translucent image premultiplied in
    /// linear light. True when they are in the cache rather than in the image.
    fn prepare(&mut self, image: &Image, size: [u32; 2]) -> bool {
        let frame = self.stats.frames;
        if let Some(entry) = self.rasters.get_mut(&(image.id(), size)) {
            entry.1 = frame;
            return true;
        }
        let settled = |pixel: &[u8; 4]| matches!(pixel[3], 0 | 255);
        let mut texels = match image.pixels() {
            // Texels that are opaque or clear need no conversion, which is
            // most images: only their alpha would have been divided out.
            Some((_, _, pixels)) if pixels.as_chunks::<4>().0.iter().all(settled) => return false,
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
            let alpha = u32::from(pixel[3]);
            if settled(pixel) {
                continue;
            }
            for channel in &mut pixel[..3] {
                let straight = (u32::from(*channel) * 255 / alpha).min(255) as usize;
                let light = f32::from(self.gamma.linear[straight]) * alpha as f32 / 255.0;
                *channel = self.gamma.back[light.round().clamp(0.0, 4095.0) as usize];
            }
        }
        self.rasters.insert((image.id(), size), (texels, frame));
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
