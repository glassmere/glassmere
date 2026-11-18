//! Pixel arithmetic and the coverage of rounded rectangles.
use crate::Pixel;
use crate::math::FloatMath;
use glassmere_scene::Rect;

/// Pixels `x0..x1` by `y0..y1` of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub const EMPTY: Self = Self {
        x0: 0,
        y0: 0,
        x1: 0,
        y1: 0,
    };

    pub fn is_empty(self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    pub fn intersect(self, other: Self) -> Self {
        Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }

    /// The pixels `rect` touches: its edges rounded outward, and nothing for
    /// a rectangle without area or one that is not a number.
    pub fn outward(rect: Rect) -> Self {
        if !(rect.width > 0.0 && rect.height > 0.0) {
            return Self::EMPTY;
        }
        // Far enough out that nothing visible is lost, near enough for `i32`.
        let edge = |value: f32| value.clamp(-1.0e9, 1.0e9);
        let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
        if !(right.is_finite() && bottom.is_finite()) {
            return Self::EMPTY;
        }
        Self {
            x0: edge(rect.x).floorf() as i32,
            y0: edge(rect.y).floorf() as i32,
            x1: edge(right).ceilf() as i32,
            y1: edge(bottom).ceilf() as i32,
        }
    }
}

/// `value / 255`, rounded, for `value` up to 65535.
#[inline]
pub(crate) fn div255(value: u32) -> u32 {
    let value = value + 128;
    (value + (value >> 8)) >> 8
}

/// `source` over `base` at `alpha` of 255, one channel.
#[inline]
pub(crate) fn mix(base: u8, source: u8, alpha: u32) -> u8 {
    div255(u32::from(source) * alpha + u32::from(base) * (255 - alpha)) as u8
}

/// Lays `rgb` over a pixel at `alpha` of 255, mixing the sRGB values as they
/// are.
#[inline]
pub(crate) fn blend<P: Pixel>(pixel: &mut P, rgb: [u8; 3], alpha: u32) {
    match alpha {
        0 => {}
        255.. => *pixel = P::from_rgb(rgb),
        _ => {
            let base = pixel.to_rgb();
            *pixel = P::from_rgb([
                mix(base[0], rgb[0], alpha),
                mix(base[1], rgb[1], alpha),
                mix(base[2], rgb[2], alpha),
            ]);
        }
    }
}

/// The part of the pixel `[p, p + 1]` that `[lo, hi]` covers, `0..=255`.
fn axis(p: f32, lo: f32, hi: f32) -> u32 {
    let (from, to) = (p.max(lo), (p + 1.0).min(hi));
    if to > from {
        ((to - from) * 255.0 + 0.5) as u32
    } else {
        0
    }
}

/// How far a point is outside a rounded rectangle, negative inside: the
/// rectangle has half sides `hw` and `hh` around `(cx, cy)` and corners of
/// radius `r`.
pub(crate) fn rounded_sdf(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - hw + r;
    let qy = (py - cy).abs() - hh + r;
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    let outside = if ox > 0.0 && oy > 0.0 {
        (ox * ox + oy * oy).sqrtf()
    } else {
        ox + oy
    };
    outside + qx.max(qy).min(0.0) - r
}

/// A rounded rectangle in screen coordinates, and the coverage of the pixels
/// it touches.
pub(crate) struct Shape {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
}

/// Pixels `start..end` of a row that the shape covers by one amount.
pub(crate) struct Run {
    pub start: i32,
    pub end: i32,
    pub cover: u8,
}

impl Shape {
    /// `None` for a rectangle without area. The radius is limited to half the
    /// shorter side.
    pub fn new(rect: Rect, radius: f32) -> Option<Self> {
        let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
        if !(rect.width > 0.0 && rect.height > 0.0 && right.is_finite() && bottom.is_finite()) {
            return None;
        }
        let radius = if radius > 0.0 {
            radius.min(rect.width / 2.0).min(rect.height / 2.0)
        } else {
            0.0
        };
        Some(Self {
            left: rect.x,
            top: rect.y,
            right,
            bottom,
            radius,
            cx: rect.x + rect.width / 2.0,
            cy: rect.y + rect.height / 2.0,
            hw: rect.width / 2.0,
            hh: rect.height / 2.0,
        })
    }

    pub fn bounds(&self) -> IRect {
        IRect::outward(Rect {
            x: self.left,
            y: self.top,
            width: self.right - self.left,
            height: self.bottom - self.top,
        })
    }

    /// How much of pixel `(x, y)` the shape covers, from its distance to the
    /// edge at the pixel's centre.
    fn corner_cover(&self, x: i32, y: i32) -> u8 {
        let distance = rounded_sdf(
            x as f32 + 0.5,
            y as f32 + 0.5,
            self.cx,
            self.cy,
            self.hw,
            self.hh,
            self.radius,
        );
        let cover = 0.5 - distance;
        if cover >= 1.0 {
            255
        } else if cover <= 0.0 {
            0
        } else {
            (cover * 255.0 + 0.5) as u8
        }
    }

    /// The coverage of row `y` over the pixels `x0..x1`: written to `out`
    /// (index 0 is `x0`) except for the returned run, which has one coverage
    /// throughout and is left unwritten. Straight edges are covered by their
    /// area, corners by their distance to the pixel centre.
    pub fn row(&self, y: i32, x0: i32, x1: i32, out: &mut [u8]) -> Run {
        let row = y as f32;
        let tall = axis(row, self.top, self.bottom);
        // A row that reaches into the rounded corners.
        let curved = self.radius > 0.0
            && (row < self.top + self.radius || row + 1.0 > self.bottom - self.radius);
        let (from, to) = if curved {
            (
                (self.left + self.radius).ceilf() as i32,
                (self.right - self.radius).floorf() as i32,
            )
        } else {
            (self.left.floorf() as i32 + 1, self.right.ceilf() as i32 - 1)
        };
        let (start, end) = if from < to {
            (from.clamp(x0, x1), to.clamp(x0, x1))
        } else {
            (x1, x1)
        };
        for x in (x0..start).chain(end..x1) {
            out[(x - x0) as usize] = if curved {
                self.corner_cover(x, y)
            } else {
                div255(axis(x as f32, self.left, self.right) * tall) as u8
            };
        }
        Run {
            start,
            end,
            cover: tall as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn div255_rounds_like_division() {
        for value in 0..=65535u32 {
            assert_eq!(div255(value), (value + 127) / 255, "{value}");
        }
        assert_eq!(mix(0, 255, 128), 128);
        assert_eq!(mix(200, 200, 77), 200);
    }

    #[test]
    fn outward_rounding_keeps_every_touched_pixel() {
        let pixels = IRect::outward(rect(1.2, 2.0, 3.0, 0.5));
        assert_eq!(
            pixels,
            IRect {
                x0: 1,
                y0: 2,
                x1: 5,
                y1: 3
            }
        );
        assert!(IRect::outward(rect(0.0, 0.0, 0.0, 5.0)).is_empty());
        assert!(IRect::outward(rect(f32::NAN, 0.0, 5.0, 5.0)).is_empty());
        assert!(IRect::outward(rect(0.0, 0.0, f32::INFINITY, 5.0)).is_empty());
        assert!(!IRect::outward(rect(-1.0e30, 0.0, 2.0e30, 5.0)).is_empty());
    }

    #[test]
    fn a_square_shape_covers_by_area() {
        let shape = Shape::new(rect(1.5, 1.25, 3.0, 2.0), 0.0).unwrap();
        let mut out = vec![0u8; 6];
        // Row 1 is a quarter covered, columns 1 and 4 are half covered.
        let run = shape.row(1, 0, 6, &mut out);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], div255(128 * 191) as u8);
        assert_eq!((run.start, run.end, run.cover), (2, 4, 191));
        assert_eq!(out[4], div255(128 * 191) as u8);
        assert_eq!(out[5], 0);
        // Row 2 is covered all the way down.
        let run = shape.row(2, 0, 6, &mut out);
        assert_eq!(run.cover, 255);
        assert_eq!(out[1], 128);
    }

    #[test]
    fn rounded_corners_are_soft_and_the_middle_is_solid() {
        let shape = Shape::new(rect(0.0, 0.0, 20.0, 20.0), 8.0).unwrap();
        let mut out = vec![0u8; 20];
        let run = shape.row(0, 0, 20, &mut out);
        // The top row starts well inside the arc and is solid between corners.
        assert_eq!((out[0], out[1], out[3]), (0, 0, 0));
        assert!(out[4] > 0 && out[4] < 128, "{}", out[4]);
        assert!(out[7] > 240, "{}", out[7]);
        assert_eq!((run.start, run.end, run.cover), (8, 12, 255));
        assert_eq!(out[19], 0);
        // A row in the middle is covered edge to edge.
        let run = shape.row(10, 0, 20, &mut out);
        assert_eq!((run.start, run.end), (1, 19));
        assert_eq!((out[0], out[19]), (255, 255));
        // The arc is symmetric.
        shape.row(0, 0, 20, &mut out);
        let top: vec::Vec<u8> = out.clone();
        let mut bottom = vec![0u8; 20];
        shape.row(19, 0, 20, &mut bottom);
        assert_eq!(top, bottom);
        for x in 0..10 {
            assert_eq!(top[x], top[19 - x], "{x}");
        }
    }

    #[test]
    fn the_radius_is_limited_to_half_the_shorter_side() {
        let pill = Shape::new(rect(0.0, 0.0, 30.0, 10.0), 100.0).unwrap();
        let mut out = vec![0u8; 30];
        pill.row(5, 0, 30, &mut out);
        assert!(
            out[0] > 0 && out[0] < 255,
            "an end of the pill is curved: {}",
            out[0]
        );
        assert!(Shape::new(rect(0.0, 0.0, 0.0, 10.0), 1.0).is_none());
        assert!(Shape::new(rect(0.0, 0.0, 10.0, f32::NAN), 1.0).is_none());
    }

    #[test]
    fn blending_keeps_the_extremes_exact() {
        use crate::Rgb565;
        let mut pixel = Rgb565::from_rgb([0, 0, 0]);
        blend(&mut pixel, [255, 255, 255], 0);
        assert_eq!(pixel, Rgb565(0));
        blend(&mut pixel, [255, 255, 255], 255);
        assert_eq!(pixel, Rgb565(0xFFFF));
        blend(&mut pixel, [0, 0, 0], 128);
        assert_eq!(pixel.to_rgb(), [123, 125, 123]);
    }
}
