//! Painting a scene into a framebuffer.
use crate::font::{FontFace, FontSet};
use crate::framebuffer::Framebuffer;
use crate::math::FloatMath;
use crate::path::PathScratch;
use crate::pixel::Pixel;
use crate::raster::{IRect, Shape, blend, div255};
use crate::shader::{Gradient, Shader, Shadow, Solid, Texture};
use crate::text::{EmbeddedMeasurer, LineSpan, max_cols, place, span_chars};
use alloc::vec::Vec;
use glassmere_scene::{
    Affine, Color, GradientStop, Image, Path, Point, Primitive, Rect, Scene, Stroke, TextSpan,
    TextStyle,
};

/// How a raster image is sampled when it is drawn at another size than its
/// own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFilter {
    /// The nearest texel: cheapest, and exact for an image drawn at its own
    /// size, but blocky when scaled.
    Nearest,
    /// A mix of the four nearest texels.
    #[default]
    Bilinear,
}

/// What the renderer may spend time on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Draw shadows. They are the costliest primitive per pixel, so a target
    /// short of time turns them off.
    pub shadows: bool,
    pub image_filter: ImageFilter,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            shadows: true,
            image_filter: ImageFilter::default(),
        }
    }
}

/// A group that is open: what to restore when it ends.
#[derive(Clone, Copy)]
struct Layer {
    /// Length of the clip stack once the group's own saved clip is on it.
    clip_depth: usize,
    offset: [f32; 2],
    opacity: u32,
}

/// Paints [`Scene`]s into [`Framebuffer`]s.
///
/// Coordinates are device pixels: a scene laid out at scale 1 is drawn as it
/// is laid out. Colours are blended as the sRGB values they are stored in,
/// which is cheap and a little darker than blending light; areas that are
/// neither fully covered nor translucent, such as the edge of a round corner,
/// are where it shows. See the crate documentation for what each primitive
/// does here.
///
/// The renderer keeps its scratch memory between frames, so keep one around.
#[derive(Default)]
pub struct EmbeddedRenderer {
    fonts: FontSet,
    pub options: RenderOptions,
    coverage: Vec<u8>,
    lines: Vec<LineSpan>,
    path: PathScratch,
    clips: Vec<IRect>,
    layers: Vec<Layer>,
}

impl EmbeddedRenderer {
    /// A renderer that sets text in `fonts`.
    pub fn new(fonts: FontSet, options: RenderOptions) -> Self {
        Self {
            fonts,
            options,
            coverage: Vec::new(),
            lines: Vec::new(),
            path: PathScratch::default(),
            clips: Vec::new(),
            layers: Vec::new(),
        }
    }

    /// A measurer for the fonts this renderer draws with. Lay a scene out with
    /// it and the text fits where it is painted.
    pub fn measurer(&self) -> EmbeddedMeasurer {
        EmbeddedMeasurer::new(self.fonts)
    }

    /// Paints `scene` over everything `target` covers. `clear` first fills
    /// the buffer with a colour, laid over what is there by its alpha; without
    /// it the scene is painted over the buffer's current pixels.
    pub fn render<P: Pixel>(
        &mut self,
        scene: &Scene,
        target: &mut Framebuffer<'_, P>,
        clear: Option<Color>,
    ) {
        let region = target.screen();
        self.paint(scene, target, region, clear);
    }

    /// Like [`render`](Self::render), for the part of the screen inside
    /// `damage` only. Every other pixel of `target` is left as it is, and the
    /// pixels inside are what a full render gives there.
    pub fn render_damage<P: Pixel>(
        &mut self,
        scene: &Scene,
        target: &mut Framebuffer<'_, P>,
        damage: Rect,
        clear: Option<Color>,
    ) {
        let region = target.screen().intersect(IRect::outward(damage));
        self.paint(scene, target, region, clear);
    }

    fn paint<P: Pixel>(
        &mut self,
        scene: &Scene,
        target: &mut Framebuffer<'_, P>,
        region: IRect,
        clear: Option<Color>,
    ) {
        if region.is_empty() {
            return;
        }
        if let Some(Color(red, green, blue, alpha)) = clear {
            for y in region.y0..region.y1 {
                blend_span(
                    target.span_mut(y, region.x0, region.x1),
                    [red, green, blue],
                    u32::from(alpha),
                );
            }
        }
        self.clips.clear();
        self.layers.clear();
        let mut painter = Painter {
            r: self,
            target,
            clip: region,
            offset: [0.0; 2],
            opacity: 255,
        };
        for primitive in &scene.primitives {
            painter.primitive(primitive);
        }
    }
}

/// How much of each pixel of a span a shape covers.
#[derive(Clone, Copy)]
enum Cover<'c> {
    Flat(u32),
    Each(&'c [u8]),
}

fn blend_span<P: Pixel>(pixels: &mut [P], rgb: [u8; 3], alpha: u32) {
    match alpha {
        0 => {}
        255.. => pixels.fill(P::from_rgb(rgb)),
        _ => {
            for pixel in pixels {
                blend(pixel, rgb, alpha);
            }
        }
    }
}

/// Lays the shader's colour over `pixels`, which start at screen column `x`.
/// `opacity` is the group's, `0..=255`.
fn paint_span<P: Pixel>(
    pixels: &mut [P],
    x: i32,
    cover: Cover<'_>,
    shader: &mut impl Shader,
    opacity: u32,
) {
    if let Some((rgb, alpha)) = shader.solid() {
        let alpha = div255(u32::from(alpha) * opacity);
        match cover {
            Cover::Flat(cover) => blend_span(pixels, rgb, div255(alpha * cover)),
            Cover::Each(covers) => {
                for (pixel, cover) in pixels.iter_mut().zip(covers) {
                    blend(pixel, rgb, div255(alpha * u32::from(*cover)));
                }
            }
        }
        return;
    }
    for (at, pixel) in pixels.iter_mut().enumerate() {
        let cover = match cover {
            Cover::Flat(cover) => cover,
            Cover::Each(covers) => u32::from(covers[at]),
        };
        if cover == 0 {
            continue;
        }
        let (rgb, alpha) = shader.shade(x + at as i32);
        blend(
            pixel,
            rgb,
            div255(div255(u32::from(alpha) * cover) * opacity),
        );
    }
}

fn to_alpha(opacity: f32) -> u32 {
    (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32
}

/// The pixels whose centres are inside `rect`: what a rectangle with square
/// corners covers without anti-aliasing.
fn centred(rect: Rect) -> IRect {
    let edge = |value: f32| (value - 0.5).clamp(-1.0e9, 1.0e9).ceilf() as i32;
    IRect {
        x0: edge(rect.x),
        y0: edge(rect.y),
        x1: edge(rect.x + rect.width),
        y1: edge(rect.y + rect.height),
    }
}

/// Whether `transform` only moves, and by how much.
fn translation(transform: &Affine) -> Option<[f32; 2]> {
    const EPSILON: f32 = 1e-5;
    ((transform.a - 1.0).abs() < EPSILON
        && transform.b.abs() < EPSILON
        && transform.c.abs() < EPSILON
        && (transform.d - 1.0).abs() < EPSILON)
        .then_some([transform.e, transform.f])
}

/// One pass over a scene: the renderer's scratch, the buffer and the state
/// the clips and groups so far have set.
struct Painter<'a, 'b, P: Pixel> {
    r: &'a mut EmbeddedRenderer,
    target: &'a mut Framebuffer<'b, P>,
    /// Pixels that may change. Always inside the buffer.
    clip: IRect,
    /// Where the groups open so far have moved what is drawn in them.
    offset: [f32; 2],
    opacity: u32,
}

impl<P: Pixel> Painter<'_, '_, P> {
    fn moved(&self, rect: Rect) -> Rect {
        rect.translated(Point {
            x: self.offset[0],
            y: self.offset[1],
        })
    }

    fn primitive(&mut self, primitive: &Primitive) {
        match primitive {
            Primitive::Quad {
                rect,
                color: Color(red, green, blue, alpha),
                radius,
            } => {
                if *alpha > 0
                    && let Some(shape) = Shape::new(self.moved(*rect), *radius)
                {
                    let mut solid = Solid {
                        rgb: [*red, *green, *blue],
                        alpha: *alpha,
                    };
                    self.fill_shape(&shape, &mut solid);
                }
            }
            Primitive::Text {
                rect,
                text,
                font_size,
                color,
            } => {
                let style = TextStyle {
                    font_size: *font_size,
                    ..TextStyle::default()
                };
                self.text(*rect, text.char_indices(), &style, |_| (false, *color));
            }
            Primitive::StyledText {
                rect,
                text,
                style,
                color,
            } => {
                let bold = style.weight >= 600;
                self.text(*rect, text.char_indices(), style, |_| (bold, *color));
            }
            Primitive::RichText {
                rect,
                spans,
                style,
                color,
            } => {
                let mut cursor = SpanCursor::new(spans);
                self.text(*rect, span_chars(spans), style, |byte| {
                    let span = cursor.at(byte);
                    (
                        span.and_then(|span| span.weight).unwrap_or(style.weight) >= 600,
                        span.and_then(|span| span.color).unwrap_or(*color),
                    )
                });
            }
            Primitive::Image {
                rect,
                image,
                source,
                tint,
                radius,
            } => self.image(*rect, image, *source, *tint, *radius),
            Primitive::PushClip(rect) => {
                let clip = self.clip.intersect(IRect::outward(self.moved(*rect)));
                self.r.clips.push(self.clip);
                self.clip = clip;
            }
            Primitive::PopClip => {
                let floor = self.r.layers.last().map_or(0, |layer| layer.clip_depth);
                if self.r.clips.len() > floor
                    && let Some(clip) = self.r.clips.pop()
                {
                    self.clip = clip;
                }
            }
            Primitive::PushLayer {
                bounds,
                opacity,
                transform,
            } => self.push_layer(*bounds, *opacity, transform),
            Primitive::PopLayer => {
                if let Some(layer) = self.r.layers.pop() {
                    self.r.clips.truncate(layer.clip_depth);
                    if let Some(clip) = self.r.clips.pop() {
                        self.clip = clip;
                    }
                    self.offset = layer.offset;
                    self.opacity = layer.opacity;
                }
            }
            Primitive::Gradient {
                rect,
                radius,
                from,
                to,
                stops,
                radial,
            } => self.gradient(*rect, *radius, *from, *to, stops, *radial),
            Primitive::Shadow {
                rect,
                radius,
                color,
                blur,
                spread,
                offset,
                inset,
            } => {
                if self.r.options.shadows {
                    self.shadow(*rect, *radius, *color, *blur, *spread, *offset, *inset);
                }
            }
            // The application draws these itself, in the buffer, after this.
            Primitive::Custom { .. } => {}
            Primitive::Path { path, fill, stroke } => self.path(path, *fill, *stroke),
        }
    }

    /// Groups are drawn straight into the buffer: a move is applied to
    /// everything inside, an opacity multiplies into everything inside, and
    /// any other transform is ignored.
    fn push_layer(&mut self, bounds: Rect, opacity: f32, transform: &Affine) {
        self.r.clips.push(self.clip);
        self.r.layers.push(Layer {
            clip_depth: self.r.clips.len(),
            offset: self.offset,
            opacity: self.opacity,
        });
        if let Some([x, y]) = translation(transform) {
            self.offset = [self.offset[0] + x, self.offset[1] + y];
        }
        self.opacity = div255(self.opacity * to_alpha(opacity));
        self.clip = self.clip.intersect(IRect::outward(self.moved(bounds)));
    }

    /// Lays `shader` over the shape, by the shape's coverage.
    fn fill_shape(&mut self, shape: &Shape, shader: &mut impl Shader) {
        let bounds = shape.bounds().intersect(self.clip);
        if bounds.is_empty() {
            return;
        }
        let width = (bounds.x1 - bounds.x0) as usize;
        if self.r.coverage.len() < width {
            self.r.coverage.resize(width, 0);
        }
        for y in bounds.y0..bounds.y1 {
            let run = shape.row(y, bounds.x0, bounds.x1, &mut self.r.coverage);
            shader.begin_row(y);
            let (start, end) = (
                (run.start - bounds.x0) as usize,
                (run.end - bounds.x0) as usize,
            );
            let pixels = self.target.span_mut(y, bounds.x0, bounds.x1);
            let (left, rest) = pixels.split_at_mut(start);
            let (middle, right) = rest.split_at_mut(end - start);
            let coverage = &self.r.coverage;
            let opacity = self.opacity;
            paint_span(
                left,
                bounds.x0,
                Cover::Each(&coverage[..start]),
                shader,
                opacity,
            );
            paint_span(
                middle,
                run.start,
                Cover::Flat(u32::from(run.cover)),
                shader,
                opacity,
            );
            paint_span(
                right,
                run.end,
                Cover::Each(&coverage[end..width]),
                shader,
                opacity,
            );
        }
    }

    /// Lays `shader` over every pixel of `bounds`.
    fn fill_rect(&mut self, bounds: IRect, shader: &mut impl Shader) {
        let bounds = bounds.intersect(self.clip);
        if bounds.is_empty() {
            return;
        }
        for y in bounds.y0..bounds.y1 {
            shader.begin_row(y);
            paint_span(
                self.target.span_mut(y, bounds.x0, bounds.x1),
                bounds.x0,
                Cover::Flat(255),
                shader,
                self.opacity,
            );
        }
    }

    fn gradient(
        &mut self,
        rect: Rect,
        radius: f32,
        from: Point,
        to: Point,
        stops: &[GradientStop],
        radial: bool,
    ) {
        let rect = self.moved(rect);
        if stops.len() < 2 {
            return;
        }
        let Some(shape) = Shape::new(rect, radius) else {
            return;
        };
        let mut ramp = Gradient::new(rect, [from.x, from.y], [to.x, to.y], stops, radial);
        self.fill_shape(&shape, &mut ramp);
    }

    #[allow(clippy::too_many_arguments)]
    fn shadow(
        &mut self,
        rect: Rect,
        radius: f32,
        color: Color,
        blur: f32,
        spread: f32,
        offset: Point,
        inset: bool,
    ) {
        if color.3 == 0 {
            return;
        }
        let rect = self.moved(rect);
        // The shape that is blurred: the rectangle grown by the spread and
        // moved by the offset.
        let half = [
            (rect.width / 2.0 + spread).max(0.0),
            (rect.height / 2.0 + spread).max(0.0),
        ];
        let center = [
            rect.x + rect.width / 2.0 + offset.x,
            rect.y + rect.height / 2.0 + offset.y,
        ];
        let sigma = blur.max(0.1);
        let finite = [
            half[0], half[1], center[0], center[1], sigma, radius, spread,
        ];
        if !(half[0] > 0.0 && half[1] > 0.0 && finite.iter().all(|value| value.is_finite())) {
            return;
        }
        let corner = (radius + spread).clamp(0.0, half[0].min(half[1]));
        let mut shadow = Shadow::new(color, sigma, center, half, corner, inset);
        if inset {
            // An inset shadow darkens the inside of the unmoved rectangle.
            if let Some(shape) = Shape::new(rect, radius) {
                self.fill_shape(&shape, &mut shadow);
            }
        } else {
            // A Gaussian is spent three standard deviations out.
            let reach = 3.0 * sigma;
            self.fill_rect(
                IRect::outward(Rect {
                    x: center[0] - half[0] - reach,
                    y: center[1] - half[1] - reach,
                    width: (half[0] + reach) * 2.0,
                    height: (half[1] + reach) * 2.0,
                }),
                &mut shadow,
            );
        }
    }

    fn image(&mut self, rect: Rect, image: &Image, source: Rect, tint: Option<Color>, radius: f32) {
        // Vector images need a rasterizer, which this renderer does not have.
        let Some((width, height, pixels)) = image.pixels() else {
            return;
        };
        let rect = self.moved(rect);
        let step = [source.width / rect.width, source.height / rect.height];
        let start = [
            source.x + (0.5 - rect.x) * step[0],
            source.y + (0.5 - rect.y) * step[1],
        ];
        if !(rect.width > 0.0
            && rect.height > 0.0
            && source.width != 0.0
            && source.height != 0.0
            && step.iter().chain(&start).all(|value| value.is_finite()))
        {
            return;
        }
        // The texels the source rectangle covers: nothing outside may be
        // sampled, or the neighbours of a nine-slice piece would bleed in.
        let span = |at: f32, length: f32, side: u32| {
            let (from, to) = (at.min(at + length), at.max(at + length));
            let low = from.floorf().clamp(0.0, side as f32 - 1.0);
            let high = to.ceilf().clamp(low + 1.0, side as f32);
            (low as i32, high as i32 - 1)
        };
        let mut texture = Texture {
            pixels,
            width: width as usize,
            columns: span(source.x, source.width, width),
            rows: span(source.y, source.height, height),
            start,
            step,
            filter: self.r.options.image_filter,
            tint,
            v: 0.0,
        };
        if radius > 0.0 {
            if let Some(shape) = Shape::new(rect, radius) {
                self.fill_shape(&shape, &mut texture);
            }
        } else {
            // Square corners are not anti-aliased, so that nine-slice pieces
            // and sprites meet without a seam.
            self.fill_rect(centred(rect), &mut texture);
        }
    }

    fn path(&mut self, path: &Path, fill: Option<Color>, stroke: Option<Stroke>) {
        let offset = self.offset;
        if let Some(color) = fill {
            self.r.path.fill(path, offset);
            self.cover_path(color);
        }
        if let Some(Stroke { width, color }) = stroke {
            self.r.path.stroke(path, offset, width);
            self.cover_path(color);
        }
    }

    /// Lays `color` over the shape the path scratch holds.
    fn cover_path(&mut self, color: Color) {
        let bounds = self.r.path.bounds().intersect(self.clip);
        if color.3 == 0 || bounds.is_empty() {
            return;
        }
        let mut solid = Solid {
            rgb: [color.0, color.1, color.2],
            alpha: color.3,
        };
        let (target, opacity) = (&mut *self.target, self.opacity);
        self.r.path.rows(bounds, |y, cover| {
            paint_span(
                target.span_mut(y, bounds.x0, bounds.x1),
                bounds.x0,
                Cover::Each(cover),
                &mut solid,
                opacity,
            );
        });
    }

    /// Sets text in the face nearest the style's size, a cell for each
    /// cluster. `ink` says, for the cluster that starts at a byte, whether it
    /// is bold and what colour it is.
    fn text<I>(
        &mut self,
        rect: Rect,
        chars: I,
        style: &TextStyle,
        mut ink: impl FnMut(usize) -> (bool, Color),
    ) where
        I: Iterator<Item = (usize, char)> + Clone,
    {
        let rect = self.moved(rect);
        let clip = self.clip.intersect(IRect::outward(rect));
        if clip.is_empty() {
            return;
        }
        let face = self.r.fonts.face_for(style.font_size);
        let (cell, line) = (face.width() as i32, face.height() as i32);
        // Glyphs sit on whole pixels.
        let snap = |value: f32| value.clamp(-1.0e9, 1.0e9).snapf() as i32;
        let (left, top) = (snap(rect.x), snap(rect.y));
        let (target, opacity) = (&mut *self.target, self.opacity);
        place(
            chars,
            max_cols(style, Some(rect.width), face.width()),
            &mut self.r.lines,
            |placed| {
                let (bold, Color(red, green, blue, alpha)) = ink(placed.cluster.start);
                let alpha = div255(u32::from(alpha) * opacity);
                if alpha > 0 {
                    glyph(
                        target,
                        clip,
                        face,
                        placed.cluster.ch,
                        bold,
                        [
                            left.saturating_add((placed.col as i32).saturating_mul(cell)),
                            top.saturating_add((placed.line as i32).saturating_mul(line)),
                        ],
                        [red, green, blue],
                        alpha,
                    );
                }
            },
        );
    }
}

/// Which span of rich text a byte is in, for bytes asked for in order.
struct SpanCursor<'s> {
    spans: &'s [TextSpan],
    index: usize,
    /// The byte where the current span ends.
    end: usize,
}

impl<'s> SpanCursor<'s> {
    fn new(spans: &'s [TextSpan]) -> Self {
        Self {
            spans,
            index: 0,
            end: spans.first().map_or(0, |span| span.text.len()),
        }
    }

    fn at(&mut self, byte: usize) -> Option<&'s TextSpan> {
        while byte >= self.end && self.index + 1 < self.spans.len() {
            self.index += 1;
            self.end += self.spans[self.index].text.len();
        }
        self.spans.get(self.index)
    }
}

/// Whether the box that stands for a character the font lacks has a pixel at
/// `(row, column)`: a hollow rectangle that stops a column short of the cell,
/// from a quarter of the way up to the baseline.
fn missing(face: &FontFace, row: i32, column: i32) -> bool {
    let (top, bottom) = (face.ascent() as i32 / 4, face.ascent() as i32 - 1);
    let right = face.width() as i32 - 2;
    (top..=bottom).contains(&row)
        && (0..=right).contains(&column)
        && (row == top || row == bottom || column == 0 || column == right)
}

/// Draws the glyph of `ch` with its cell's top left pixel at `at`, inside
/// `clip`. Spaces, other control characters and the marks that join a
/// character draw nothing; a character the font lacks draws a box.
#[allow(clippy::too_many_arguments)]
fn glyph<P: Pixel>(
    target: &mut Framebuffer<'_, P>,
    clip: IRect,
    face: &FontFace,
    ch: char,
    bold: bool,
    at: [i32; 2],
    rgb: [u8; 3],
    alpha: u32,
) {
    if ch == ' ' || ch.is_control() {
        return;
    }
    let (width, height) = (face.width() as i32, face.height() as i32);
    let columns =
        (clip.x0.saturating_sub(at[0])).max(0)..(clip.x1.saturating_sub(at[0])).min(width);
    let rows = (clip.y0.saturating_sub(at[1])).max(0)..(clip.y1.saturating_sub(at[1])).min(height);
    if columns.is_empty() || rows.is_empty() {
        return;
    }
    let bitmap = face.glyph(ch, bold);
    let row_bytes = (width as usize).div_ceil(8);
    for row in rows {
        let pixels = target.span_mut(at[1] + row, at[0] + columns.start, at[0] + columns.end);
        for (pixel, column) in pixels.iter_mut().zip(columns.clone()) {
            let ink = match bitmap {
                Some(bits) => {
                    bits[row as usize * row_bytes + column as usize / 8] & (0x80 >> (column % 8))
                        != 0
                }
                None => missing(face, row, column),
            };
            if ink {
                blend(pixel, rgb, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Gray8, Rgb565, Rgb888};
    use alloc::vec;
    use glassmere_scene::{Size, TextWrap};

    fn scene(primitives: Vec<Primitive>) -> Scene {
        Scene {
            size: Size::new(40.0, 20.0),
            primitives,
        }
    }

    fn quad(x: f32, y: f32, width: f32, height: f32, color: Color, radius: f32) -> Primitive {
        Primitive::Quad {
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            color,
            radius,
        }
    }

    fn draw(primitives: Vec<Primitive>) -> Vec<Rgb888> {
        let mut pixels = vec![Rgb888::default(); 40 * 20];
        let mut target = Framebuffer::new(&mut pixels, 40, 20);
        EmbeddedRenderer::default().render(&scene(primitives), &mut target, Some(Color::WHITE));
        pixels
    }

    fn at(pixels: &[Rgb888], x: usize, y: usize) -> [u8; 3] {
        pixels[y * 40 + x].0
    }

    const RED: Color = Color(255, 0, 0, 255);

    #[test]
    fn an_opaque_quad_is_exact_and_stays_inside_its_clip() {
        let pixels = draw(vec![
            Primitive::PushClip(Rect {
                x: 5.0,
                y: 5.0,
                width: 10.0,
                height: 5.0,
            }),
            quad(0.0, 0.0, 40.0, 20.0, RED, 0.0),
            Primitive::PopClip,
        ]);
        for y in 0..20 {
            for x in 0..40 {
                let inside = (5..15).contains(&x) && (5..10).contains(&y);
                assert_eq!(
                    at(&pixels, x, y),
                    if inside { [255, 0, 0] } else { [255; 3] },
                    "({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn a_fractional_edge_blends_by_its_coverage() {
        let pixels = draw(vec![quad(2.5, 2.0, 4.0, 2.0, Color(0, 0, 0, 255), 0.0)]);
        assert_eq!(at(&pixels, 2, 2), [127, 127, 127]);
        assert_eq!(at(&pixels, 3, 2), [0, 0, 0]);
        assert_eq!(at(&pixels, 6, 2), [127, 127, 127]);
        assert_eq!(at(&pixels, 7, 2), [255; 3]);
        // A translucent colour blends the same way.
        let pixels = draw(vec![quad(0.0, 0.0, 4.0, 4.0, Color(0, 0, 0, 128), 0.0)]);
        assert_eq!(at(&pixels, 1, 1), [127, 127, 127]);
    }

    #[test]
    fn rounded_corners_are_cut_and_the_sides_are_not() {
        let pixels = draw(vec![quad(0.0, 0.0, 20.0, 20.0, RED, 8.0)]);
        assert_eq!(at(&pixels, 0, 0), [255; 3], "the very corner stays clear");
        let soft = at(&pixels, 2, 2);
        assert!(soft[1] > 0 && soft[1] < 255, "{soft:?}");
        assert_eq!(at(&pixels, 10, 0), [255, 0, 0]);
        assert_eq!(at(&pixels, 0, 10), [255, 0, 0]);
        assert_eq!(at(&pixels, 10, 10), [255, 0, 0]);
    }

    #[test]
    fn a_group_fades_and_moves_what_is_inside_it() {
        let pixels = draw(vec![
            Primitive::PushLayer {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                opacity: 0.5,
                transform: Affine::translate(20.0, 5.0),
            },
            quad(0.0, 0.0, 10.0, 10.0, Color(0, 0, 0, 255), 0.0),
            Primitive::PopLayer,
            quad(0.0, 15.0, 4.0, 4.0, RED, 0.0),
        ]);
        assert_eq!(at(&pixels, 0, 0), [255; 3], "nothing is left where it was");
        assert_eq!(at(&pixels, 20, 5), [127, 127, 127]);
        assert_eq!(at(&pixels, 29, 14), [127, 127, 127]);
        assert_eq!(at(&pixels, 30, 5), [255; 3], "the group's bounds clip it");
        assert_eq!(
            at(&pixels, 1, 16),
            [255, 0, 0],
            "opacity ends with the group"
        );
    }

    #[test]
    fn groups_and_clips_nest_and_unbalanced_pops_are_harmless() {
        let layer = |opacity| Primitive::PushLayer {
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
            opacity,
            transform: Affine::IDENTITY,
        };
        let pixels = draw(vec![
            Primitive::PopClip,
            Primitive::PopLayer,
            layer(0.5),
            layer(0.5),
            Primitive::PopClip,
            quad(0.0, 0.0, 4.0, 4.0, Color(0, 0, 0, 255), 0.0),
            Primitive::PopLayer,
            quad(10.0, 0.0, 4.0, 4.0, Color(0, 0, 0, 255), 0.0),
            Primitive::PopLayer,
            quad(20.0, 0.0, 4.0, 4.0, Color(0, 0, 0, 255), 0.0),
        ]);
        // A quarter, a half, then everything.
        assert_eq!(at(&pixels, 1, 1), [191; 3]);
        assert_eq!(at(&pixels, 11, 1), [127; 3]);
        assert_eq!(at(&pixels, 21, 1), [0; 3]);
    }

    #[test]
    fn a_turned_group_is_drawn_untransformed() {
        let pixels = draw(vec![
            Primitive::PushLayer {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 40.0,
                    height: 20.0,
                },
                opacity: 1.0,
                transform: Affine::rotate(45.0),
            },
            quad(0.0, 0.0, 4.0, 4.0, RED, 0.0),
            Primitive::PopLayer,
        ]);
        assert_eq!(at(&pixels, 1, 1), [255, 0, 0]);
    }

    #[test]
    fn text_paints_ink_in_its_rectangle_and_nowhere_else() {
        let style = TextStyle {
            font_size: 13.0,
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let rect = Rect {
            x: 3.0,
            y: 0.0,
            width: 18.0,
            height: 26.0,
        };
        let pixels = draw(vec![Primitive::StyledText {
            rect,
            text: "ab cd".into(),
            style,
            color: Color(0, 0, 255, 255),
        }]);
        let inked: Vec<(usize, usize)> = (0..20)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .filter(|&(x, y)| at(&pixels, x, y) != [255; 3])
            .collect();
        assert!(inked.len() > 10);
        // Three cells wide, so "ab " and "cd" are two lines of the 13 pixel face.
        assert!(
            inked
                .iter()
                .all(|&(x, y)| (3..21).contains(&x) && (0..20).contains(&y))
        );
        assert!(inked.iter().any(|&(_, y)| y < 13), "the first line");
        assert!(inked.iter().any(|&(_, y)| y >= 13), "the second line");
        assert!(inked.iter().all(|&(x, y)| at(&pixels, x, y) == [0, 0, 255]));
    }

    #[test]
    fn a_character_the_font_lacks_is_a_box_and_a_space_is_nothing() {
        let draw_text = |text: &str| {
            let mut pixels = vec![Gray8(255); 40 * 20];
            let mut target = Framebuffer::new(&mut pixels, 40, 20);
            let primitive = Primitive::Text {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 40.0,
                    height: 20.0,
                },
                text: text.into(),
                font_size: 13.0,
                color: Color(0, 0, 0, 255),
            };
            EmbeddedRenderer::default().render(&scene(vec![primitive]), &mut target, None);
            pixels.iter().filter(|pixel| pixel.0 != 255).count()
        };
        assert_eq!(draw_text("   "), 0);
        assert_eq!(draw_text("\t"), 0);
        // 5 wide and 9 tall: 2 * 5 + 2 * 7 pixels.
        assert_eq!(draw_text("\u{4e2d}"), 24);
    }

    #[test]
    fn rich_text_takes_colours_and_weights_from_its_spans() {
        let mut pixels = vec![Rgb565::default(); 40 * 20];
        let mut target = Framebuffer::new(&mut pixels, 40, 20);
        let spans = vec![
            TextSpan::new("I").color(Color(255, 0, 0, 255)),
            TextSpan::new("I").weight(700).color(Color(0, 255, 0, 255)),
        ];
        let primitive = Primitive::RichText {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            },
            spans,
            style: TextStyle {
                font_size: 13.0,
                ..TextStyle::default()
            },
            color: Color::WHITE,
        };
        EmbeddedRenderer::default().render(
            &scene(vec![primitive]),
            &mut target,
            Some(Color(0, 0, 0, 255)),
        );
        let ink = |from: i32, to: i32, wanted: [u8; 3]| {
            (0..20)
                .flat_map(|y| (from..to).map(move |x| (x, y)))
                .filter(|&(x, y)| target.pixel(x, y).unwrap().to_rgb() == wanted)
                .count()
        };
        let (red, green) = (ink(0, 6, [255, 0, 0]), ink(6, 12, [0, 255, 0]));
        assert!(red > 10, "{red}");
        assert!(green > red, "bold has more ink: {green} against {red}");
        assert_eq!(ink(0, 6, [0, 255, 0]) + ink(6, 12, [255, 0, 0]), 0);
    }

    #[test]
    fn shadows_darken_outside_a_box_and_can_be_turned_off() {
        let shadow = |inset| Primitive::Shadow {
            rect: Rect {
                x: 12.0,
                y: 6.0,
                width: 16.0,
                height: 8.0,
            },
            radius: 2.0,
            color: Color(0, 0, 0, 255),
            blur: 2.0,
            spread: 0.0,
            offset: Point { x: 0.0, y: 0.0 },
            inset,
        };
        let pixels = draw(vec![shadow(false)]);
        // Inside it is the colour, at the edge half of it and far out nothing.
        assert!(at(&pixels, 20, 10)[0] < 16, "{:?}", at(&pixels, 20, 10));
        let edge = at(&pixels, 12, 10)[0];
        assert!((80..160).contains(&edge), "{edge}");
        assert_eq!(at(&pixels, 2, 10), [255; 3]);
        let mut pixels = vec![Rgb888::default(); 40 * 20];
        let mut target = Framebuffer::new(&mut pixels, 40, 20);
        let mut renderer = EmbeddedRenderer::default();
        renderer.options.shadows = false;
        renderer.render(&scene(vec![shadow(false)]), &mut target, Some(Color::WHITE));
        assert!(pixels.iter().all(|pixel| pixel.0 == [255; 3]));
        // An inset shadow darkens the inside's edge and nothing outside it.
        let pixels = draw(vec![shadow(true)]);
        assert!(at(&pixels, 20, 10)[0] > 240);
        assert!(at(&pixels, 12, 10)[0] < 200);
        assert_eq!(at(&pixels, 6, 10), [255; 3]);
    }

    #[test]
    fn a_gradient_runs_from_one_colour_to_the_other() {
        let stop = |at, color| GradientStop { at, color };
        let pixels = draw(vec![Primitive::Gradient {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 10.0,
            },
            radius: 0.0,
            from: Point { x: 0.0, y: 0.0 },
            to: Point { x: 1.0, y: 0.0 },
            stops: vec![
                stop(0.0, Color(0, 0, 0, 255)),
                stop(1.0, Color(255, 255, 255, 255)),
            ],
            radial: false,
        }]);
        let shades: Vec<u8> = (0..40).map(|x| at(&pixels, x, 4)[0]).collect();
        assert!(shades.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(shades[0] < 10 && shades[39] > 245, "{shades:?}");
        assert!((i32::from(shades[20]) - 130).abs() < 8);
        assert_eq!(at(&pixels, 5, 15), [255; 3], "below the rectangle");
    }

    #[test]
    fn an_image_is_drawn_at_its_size_tinted_and_scaled() {
        // 2x1: red, then translucent blue.
        let image = Image::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        let source = Rect {
            x: 0.0,
            y: 0.0,
            width: 2.0,
            height: 1.0,
        };
        let draw_image = |width: f32, tint, filter| {
            let mut pixels = vec![Rgb888::default(); 40 * 20];
            let mut target = Framebuffer::new(&mut pixels, 40, 20);
            let mut renderer = EmbeddedRenderer::default();
            renderer.options.image_filter = filter;
            renderer.render(
                &scene(vec![Primitive::Image {
                    rect: Rect {
                        x: 4.0,
                        y: 3.0,
                        width,
                        height: 1.0 * width / 2.0,
                    },
                    image: image.clone(),
                    source,
                    tint,
                    radius: 0.0,
                }]),
                &mut target,
                Some(Color::WHITE),
            );
            pixels
        };
        let plain = draw_image(2.0, None, ImageFilter::Bilinear);
        assert_eq!(at(&plain, 4, 3), [255, 0, 0]);
        assert_eq!(at(&plain, 5, 3), [127, 127, 255]);
        assert_eq!(at(&plain, 6, 3), [255; 3]);
        // Four times larger and nearest: each texel becomes a block.
        let blocky = draw_image(8.0, None, ImageFilter::Nearest);
        assert_eq!(at(&blocky, 7, 6), [255, 0, 0]);
        assert_eq!(at(&blocky, 8, 6), [127, 127, 255]);
        // Bilinear is between at the seam.
        let smooth = draw_image(8.0, None, ImageFilter::Bilinear);
        let seam = at(&smooth, 8, 5);
        assert!(seam[0] > 127 && seam[0] < 255 && seam[1] < 200, "{seam:?}");
        // A tint keeps the alpha and replaces the colour.
        let tinted = draw_image(2.0, Some(Color(0, 128, 0, 255)), ImageFilter::Nearest);
        assert_eq!(at(&tinted, 4, 3), [0, 128, 0]);
        assert_eq!(at(&tinted, 5, 3), [127, 191, 127]);
    }

    #[test]
    fn filled_and_stroked_paths_are_painted_in_that_order() {
        let mut path = Path::new();
        path.move_to(Point { x: 5.0, y: 5.0 })
            .line_to(Point { x: 15.0, y: 5.0 })
            .line_to(Point { x: 15.0, y: 15.0 })
            .line_to(Point { x: 5.0, y: 15.0 })
            .close();
        let pixels = draw(vec![Primitive::Path {
            path,
            fill: Some(Color(0, 255, 0, 255)),
            stroke: Some(Stroke {
                width: 2.0,
                color: RED,
            }),
        }]);
        assert_eq!(at(&pixels, 10, 10), [0, 255, 0]);
        assert_eq!(
            at(&pixels, 5, 10),
            [255, 0, 0],
            "the stroke is over the fill"
        );
        assert_eq!(at(&pixels, 14, 10), [255, 0, 0]);
        assert_eq!(at(&pixels, 2, 10), [255; 3]);
        assert_eq!(at(&pixels, 10, 17), [255; 3]);
    }

    #[test]
    fn a_damage_rectangle_leaves_everything_else_alone() {
        let mut pixels = vec![Rgb888([7, 7, 7]); 40 * 20];
        let mut target = Framebuffer::new(&mut pixels, 40, 20);
        let damage = Rect {
            x: 10.0,
            y: 5.0,
            width: 10.5,
            height: 4.0,
        };
        EmbeddedRenderer::default().render_damage(
            &scene(vec![quad(0.0, 0.0, 40.0, 20.0, RED, 0.0)]),
            &mut target,
            damage,
            Some(Color::WHITE),
        );
        for y in 0..20 {
            for x in 0..40 {
                // Whole pixels: 10..21 by 5..9.
                let inside = (10..21).contains(&x) && (5..9).contains(&y);
                assert_eq!(
                    at(&pixels, x, y),
                    if inside { [255, 0, 0] } else { [7, 7, 7] },
                    "({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn a_buffer_placed_elsewhere_gets_the_part_of_the_scene_that_falls_in_it() {
        let mut pixels = vec![Rgb888::default(); 10 * 5];
        let mut target = Framebuffer::new(&mut pixels, 10, 5).at(8, 3);
        EmbeddedRenderer::default().render(
            &scene(vec![quad(0.0, 0.0, 40.0, 6.0, RED, 0.0)]),
            &mut target,
            Some(Color::WHITE),
        );
        // The quad ends at screen row 6, which is row 3 of the buffer.
        assert_eq!(target.pixel(8, 3), Some(Rgb888([255, 0, 0])));
        assert_eq!(target.pixel(17, 5), Some(Rgb888([255, 0, 0])));
        assert_eq!(target.pixel(8, 6), Some(Rgb888([255; 3])));
        assert_eq!(target.pixel(17, 7), Some(Rgb888([255; 3])));
        assert_eq!(target.pixel(7, 3), None);
    }

    #[test]
    fn things_that_are_not_numbers_are_skipped() {
        let nan = f32::NAN;
        let pixels = draw(vec![
            quad(nan, 0.0, 10.0, 10.0, RED, 0.0),
            quad(0.0, 0.0, 10.0, 10.0, RED, nan),
            Primitive::PushClip(Rect {
                x: nan,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }),
            quad(30.0, 10.0, 10.0, 10.0, RED, 0.0),
            Primitive::PopClip,
            Primitive::Shadow {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: nan,
                    height: 5.0,
                },
                radius: 0.0,
                color: RED,
                blur: nan,
                spread: 0.0,
                offset: Point { x: 0.0, y: 0.0 },
                inset: false,
            },
        ]);
        // The second quad is drawn with square corners; nothing else is.
        assert_eq!(at(&pixels, 5, 5), [255, 0, 0]);
        assert_eq!(at(&pixels, 35, 15), [255; 3]);
    }
}
