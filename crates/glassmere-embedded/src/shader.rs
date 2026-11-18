//! What fills a shape: one colour, a ramp, an image, or the falloff of a
//! shadow. A shader gives the straight colour and alpha of a pixel; the
//! shape's coverage and the layer's opacity are applied by the caller.
use crate::ImageFilter;
use crate::math::FloatMath;
use crate::raster::{div255, rounded_sdf};
use glassmere_scene::{Color, GradientStop, Rect};

pub(crate) trait Shader {
    /// The colour and alpha when they are the same for every pixel.
    fn solid(&self) -> Option<([u8; 3], u8)> {
        None
    }

    /// Called before the pixels of row `y`, left to right, are asked for.
    fn begin_row(&mut self, _y: i32) {}

    fn shade(&mut self, x: i32) -> ([u8; 3], u8);
}

pub(crate) struct Solid {
    pub rgb: [u8; 3],
    pub alpha: u8,
}

impl Shader for Solid {
    fn solid(&self) -> Option<([u8; 3], u8)> {
        Some((self.rgb, self.alpha))
    }

    fn shade(&mut self, _: i32) -> ([u8; 3], u8) {
        (self.rgb, self.alpha)
    }
}

/// The most stops a gradient has, as on the GPU.
const MAX_STOPS: usize = 8;

/// A ramp as a table of 256 colours, looked up by position.
pub(crate) struct Gradient {
    table: [[u8; 4]; 256],
    radial: bool,
    /// Whether the two ends are the same point, which makes the ramp its end colour.
    flat: bool,
    /// Pixel to rectangle units, and the rectangle's own start and size.
    scale: [f32; 2],
    offset: [f32; 2],
    from: [f32; 2],
    axis: [f32; 2],
    /// Linear: 1 over the squared length of the axis. Radial: 1 over its length.
    inverse: f32,
    // The current row.
    row_dy: f32,
    row_base: f32,
}

impl Gradient {
    /// `rect` is where the ramp is laid, in screen pixels; `from` and `to` are
    /// in the rectangle's own units. Stops after the eighth are ignored.
    pub fn new(
        rect: Rect,
        from: [f32; 2],
        to: [f32; 2],
        stops: &[GradientStop],
        radial: bool,
    ) -> Self {
        let mut ramp = [(0.0f32, [0u8; 4]); MAX_STOPS];
        let count = stops.len().min(MAX_STOPS);
        let mut last = 0.0f32;
        for (slot, stop) in ramp.iter_mut().zip(stops) {
            last = stop.at.clamp(0.0, 1.0).max(last);
            *slot = (
                last,
                [stop.color.0, stop.color.1, stop.color.2, stop.color.3],
            );
        }
        let ramp = &ramp[..count];
        let mut table = [[0u8; 4]; 256];
        for (index, entry) in table.iter_mut().enumerate() {
            let at = index as f32 / 255.0;
            *entry = match ramp.iter().position(|(stop, _)| *stop >= at) {
                None => ramp[count - 1].1,
                Some(0) => ramp[0].1,
                Some(next) => {
                    let ((low_at, low), (high_at, high)) = (ramp[next - 1], ramp[next]);
                    let span = high_at - low_at;
                    let mixed = if span > f32::EPSILON {
                        (at - low_at) / span
                    } else {
                        0.0
                    };
                    let channel = |at: usize| {
                        (f32::from(low[at])
                            + (f32::from(high[at]) - f32::from(low[at])) * mixed
                            + 0.5) as u8
                    };
                    [channel(0), channel(1), channel(2), channel(3)]
                }
            };
        }
        let axis = [to[0] - from[0], to[1] - from[1]];
        let squared = axis[0] * axis[0] + axis[1] * axis[1];
        let scale = [1.0 / rect.width, 1.0 / rect.height];
        Self {
            table,
            radial,
            flat: squared <= f32::EPSILON,
            scale,
            offset: [(0.5 - rect.x) * scale[0], (0.5 - rect.y) * scale[1]],
            from,
            axis,
            inverse: if radial {
                1.0 / squared.sqrtf().max(f32::EPSILON)
            } else {
                1.0 / squared.max(f32::EPSILON)
            },
            row_dy: 0.0,
            row_base: 0.0,
        }
    }
}

impl Shader for Gradient {
    fn begin_row(&mut self, y: i32) {
        self.row_dy = y as f32 * self.scale[1] + self.offset[1] - self.from[1];
        // Linear ramps are a line in x: its value at x = 0.
        self.row_base = ((self.offset[0] - self.from[0]) * self.axis[0]
            + self.row_dy * self.axis[1])
            * self.inverse;
    }

    fn shade(&mut self, x: i32) -> ([u8; 3], u8) {
        let at = if self.flat {
            1.0
        } else if self.radial {
            let dx = x as f32 * self.scale[0] + self.offset[0] - self.from[0];
            (dx * dx + self.row_dy * self.row_dy).sqrtf() * self.inverse
        } else {
            self.row_base + x as f32 * self.scale[0] * self.axis[0] * self.inverse
        };
        let [r, g, b, a] = self.table[(at.clamp(0.0, 1.0) * 255.0 + 0.5) as usize];
        ([r, g, b], a)
    }
}

/// Part of a raster image, sampled at the centre of each pixel.
pub(crate) struct Texture<'a> {
    pub pixels: &'a [u8],
    pub width: usize,
    /// The texels a source rectangle covers, inclusive. Nothing outside is
    /// sampled, so that the neighbours of a nine-slice piece do not bleed in.
    pub columns: (i32, i32),
    pub rows: (i32, i32),
    /// Texel coordinates of pixel `(0, 0)`'s centre, and the step per pixel.
    pub start: [f32; 2],
    pub step: [f32; 2],
    pub filter: ImageFilter,
    pub tint: Option<Color>,
    /// The texel row of the current pixel row.
    pub v: f32,
}

impl Texture<'_> {
    fn texel(&self, x: i32, y: i32) -> [u32; 4] {
        let x = x.clamp(self.columns.0, self.columns.1) as usize;
        let y = y.clamp(self.rows.0, self.rows.1) as usize;
        let at = (y * self.width + x) * 4;
        let t = &self.pixels[at..at + 4];
        [
            u32::from(t[0]),
            u32::from(t[1]),
            u32::from(t[2]),
            u32::from(t[3]),
        ]
    }
}

impl Shader for Texture<'_> {
    fn begin_row(&mut self, y: i32) {
        self.v = self.start[1] + self.step[1] * y as f32;
    }

    fn shade(&mut self, x: i32) -> ([u8; 3], u8) {
        let u = self.start[0] + self.step[0] * x as f32;
        // Premultiplied texels, which mix correctly as they are.
        let texel = match self.filter {
            ImageFilter::Nearest => self.texel(u.floorf() as i32, self.v.floorf() as i32),
            ImageFilter::Bilinear => {
                let (u, v) = (u - 0.5, self.v - 0.5);
                let (left, top) = (u.floorf(), v.floorf());
                let fx = ((u - left) * 256.0) as u32;
                let fy = ((v - top) * 256.0) as u32;
                let (left, top) = (left as i32, top as i32);
                let weights = [
                    (256 - fx) * (256 - fy),
                    fx * (256 - fy),
                    (256 - fx) * fy,
                    fx * fy,
                ];
                let taps = [
                    self.texel(left, top),
                    self.texel(left + 1, top),
                    self.texel(left, top + 1),
                    self.texel(left + 1, top + 1),
                ];
                let mut mixed = [0u32; 4];
                for (tap, weight) in taps.iter().zip(weights) {
                    for (sum, value) in mixed.iter_mut().zip(tap) {
                        *sum += value * weight;
                    }
                }
                mixed.map(|sum| (sum + 32768) >> 16)
            }
        };
        let alpha = texel[3];
        if let Some(Color(r, g, b, tint)) = self.tint {
            return ([r, g, b], div255(alpha * u32::from(tint)) as u8);
        }
        match alpha {
            0 => ([0; 3], 0),
            255 => ([texel[0] as u8, texel[1] as u8, texel[2] as u8], 255),
            _ => {
                let straight = |value: u32| ((value * 255 + alpha / 2) / alpha).min(255) as u8;
                (
                    [straight(texel[0]), straight(texel[1]), straight(texel[2])],
                    alpha as u8,
                )
            }
        }
    }
}

/// Steps in the shadow's falloff table: eight per standard deviation, from
/// three inside the edge to three outside it.
const FALLOFF: usize = 49;

/// How much shadow is `t` standard deviations outside the edge of a shape,
/// `0..=255`: the Gaussian's tail, read from a table.
fn falloff(table: &[u8; FALLOFF], t: f32) -> u32 {
    let at = (t + 3.0) * 8.0;
    if at <= 0.0 {
        u32::from(table[0])
    } else if at >= (FALLOFF - 1) as f32 {
        u32::from(table[FALLOFF - 1])
    } else {
        let index = at as usize;
        let mixed = ((at - index as f32) * 256.0) as u32;
        (u32::from(table[index]) * (256 - mixed) + u32::from(table[index + 1]) * mixed + 128) >> 8
    }
}

/// Abramowitz and Stegun 7.1.27, good to 5e-4.
fn erf(x: f32) -> f32 {
    let m = x.abs();
    let p = 1.0 + m * (0.278_393 + m * (0.230_389 + m * (0.000_972 + m * 0.078_108)));
    let p2 = p * p;
    (1.0 - 1.0 / (p2 * p2)).copysign(x)
}

/// A blurred rounded rectangle: the Gaussian's falloff by the distance from
/// the rectangle's edge, which is exact along a straight edge and close at a
/// corner. Rectangles much smaller than the blur come out darker than they
/// should.
pub(crate) struct Shadow {
    rgb: [u8; 3],
    alpha: u32,
    table: [u8; FALLOFF],
    inverse_sigma: f32,
    /// Centre, half sides and corner radius of the shape that casts it.
    center: [f32; 2],
    half: [f32; 2],
    radius: f32,
    /// Shadow inside the shape's edge, where it is not.
    inset: bool,
    row: f32,
}

impl Shadow {
    pub fn new(
        color: Color,
        sigma: f32,
        center: [f32; 2],
        half: [f32; 2],
        radius: f32,
        inset: bool,
    ) -> Self {
        let mut table = [0u8; FALLOFF];
        for (step, value) in table.iter_mut().enumerate() {
            let t = step as f32 / 8.0 - 3.0;
            *value = (255.0 * 0.5 * (1.0 - erf(t * core::f32::consts::FRAC_1_SQRT_2)) + 0.5) as u8;
        }
        Self {
            rgb: [color.0, color.1, color.2],
            alpha: u32::from(color.3),
            table,
            inverse_sigma: 1.0 / sigma,
            center,
            half,
            radius,
            inset,
            row: 0.0,
        }
    }
}

impl Shader for Shadow {
    fn begin_row(&mut self, y: i32) {
        self.row = y as f32 + 0.5;
    }

    fn shade(&mut self, x: i32) -> ([u8; 3], u8) {
        let distance = rounded_sdf(
            x as f32 + 0.5,
            self.row,
            self.center[0],
            self.center[1],
            self.half[0],
            self.half[1],
            self.radius,
        );
        let shade = falloff(&self.table, distance * self.inverse_sigma);
        let shade = if self.inset { 255 - shade } else { shade };
        (self.rgb, div255(self.alpha * shade) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_falloff_is_a_gaussian_tail() {
        let mut table = [0u8; FALLOFF];
        for (step, value) in table.iter_mut().enumerate() {
            let t = step as f32 / 8.0 - 3.0;
            *value = (255.0 * 0.5 * (1.0 - erf(t * core::f32::consts::FRAC_1_SQRT_2)) + 0.5) as u8;
        }
        assert_eq!(table[0], 255);
        assert_eq!(table[FALLOFF - 1], 0);
        // Half at the edge, a sixth one standard deviation out.
        assert!((i32::from(table[24]) - 128).abs() <= 1, "{}", table[24]);
        assert!((i32::from(table[32]) - 40).abs() <= 1, "{}", table[32]);
        assert!(table.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_eq!(falloff(&table, -10.0), 255);
        assert_eq!(falloff(&table, 10.0), 0);
        assert!((erf(1.0) - 0.842_700_8).abs() < 1e-3);
        assert!((erf(-0.5) + 0.520_499_9).abs() < 1e-3);
    }

    #[test]
    fn gradients_run_between_their_stops() {
        let stops = [
            GradientStop {
                at: 0.0,
                color: Color(0, 0, 0, 255),
            },
            GradientStop {
                at: 1.0,
                color: Color(200, 100, 0, 255),
            },
        ];
        let rect = Rect {
            x: 10.0,
            y: 0.0,
            width: 100.0,
            height: 10.0,
        };
        let mut ramp = Gradient::new(rect, [0.0, 0.0], [1.0, 0.0], &stops, false);
        ramp.begin_row(3);
        assert_eq!(ramp.shade(10), ([1, 0, 0], 255));
        let (middle, _) = ramp.shade(59);
        assert!((i32::from(middle[0]) - 100).abs() <= 2, "{middle:?}");
        assert_eq!(ramp.shade(109).0, [199, 100, 0]);
        assert_eq!(ramp.shade(400).0, [200, 100, 0]);
        assert_eq!(ramp.shade(-50).0, [0, 0, 0]);
        let mut radial = Gradient::new(rect, [0.5, 0.5], [1.0, 0.5], &stops, true);
        radial.begin_row(5);
        // The row is half a pixel off the centre line, a tenth of the way out.
        assert!(radial.shade(60).0[0] < 30);
        assert!(radial.shade(110).0[0] > 190);
    }
}
