//! Shapes of straight and curved segments.

use crate::{Color, GradientStop, Point, Rect};

/// A shape of straight and curved segments, in the same logical coordinates
/// as the rest of the scene. Each `move_to` starts a subpath; `close` joins a
/// subpath back to where it started. A fill says by `FillRule` which points
/// are inside: by the non-zero rule a subpath wound the other way inside
/// another cuts a hole.
///
/// ```
/// use glassmere_scene::{Path, Point};
///
/// let mut triangle = Path::new();
/// triangle
///     .move_to(Point { x: 0.0, y: 0.0 })
///     .line_to(Point { x: 10.0, y: 0.0 })
///     .line_to(Point { x: 5.0, y: 8.0 })
///     .close();
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    segments: Vec<Segment>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Segment {
    Move(Point),
    Line(Point),
    /// A quadratic curve: its control point, then where it ends.
    Quad(Point, Point),
    /// A cubic curve: its two control points, then where it ends.
    Cubic(Point, Point, Point),
    Close,
}

/// How a stroke turns a corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Join {
    /// The outer edges run on to meet in a point, unless that point is more
    /// than `Stroke::miter_limit` widths from the corner: then it is bevelled.
    #[default]
    Miter,
    /// A circular arc around the corner.
    Round,
    /// The outer corners are joined by a straight edge.
    Bevel,
}

/// How a stroke ends, at the ends of an open subpath and of each dash.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Cap {
    /// Cut square at the end point.
    #[default]
    Butt,
    /// A half circle around the end point.
    Round,
    /// Cut square, half a width past the end point.
    Square,
}

/// Which points a fill covers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum FillRule {
    /// A point is inside when the subpaths wind around it a net number of
    /// times: one wound the other way inside another cuts a hole.
    #[default]
    NonZero,
    /// A point is inside when a ray from it crosses an odd number of edges,
    /// whichever way they run.
    EvenOdd,
}

/// What a fill is painted with.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Brush {
    Solid(Color),
    /// A ramp from `from` to `to`. Both are in the path's own units, where
    /// (0, 0) is the top left corner of `Path::bounds` and (1, 1) the bottom
    /// right one, as in `Primitive::Gradient`; so are the units of a radial
    /// ramp, which is an ellipse where the bounds are not square.
    Linear {
        from: Point,
        to: Point,
        /// Sorted by position, which runs from 0 to 1. Fewer than two stops
        /// draws nothing.
        stops: Vec<GradientStop>,
    },
    /// A ramp running outward from `from` to the distance of `to`, in the
    /// same units.
    Radial {
        from: Point,
        to: Point,
        /// Sorted by position, which runs from 0 to 1. Fewer than two stops
        /// draws nothing.
        stops: Vec<GradientStop>,
    },
}

/// The inside of a path, and how it is painted.
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub brush: Brush,
    pub rule: FillRule,
}

/// Opaque black, by the non-zero rule.
impl Default for Fill {
    fn default() -> Self {
        Color(0, 0, 0, 255).into()
    }
}

impl From<Color> for Fill {
    fn from(color: Color) -> Self {
        Self {
            brush: Brush::Solid(color),
            rule: FillRule::default(),
        }
    }
}

/// A pattern of dashes along a stroke.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dash {
    /// Lengths in logical pixels, alternately drawn and skipped, starting
    /// with a drawn one. An odd count is repeated, so `[3.0]` is three on and
    /// three off. A pattern that is empty, has a negative or non-finite
    /// length, or sums to nothing draws the stroke solid.
    pub lengths: Vec<f32>,
    /// How far into the pattern the stroke starts. It restarts at every
    /// subpath.
    pub offset: f32,
}

impl Dash {
    pub fn new(lengths: impl Into<Vec<f32>>) -> Self {
        Self {
            lengths: lengths.into(),
            offset: 0.0,
        }
    }

    /// The pattern as pairs of drawn and skipped lengths, or `None` where it
    /// draws the stroke solid.
    pub fn pattern(&self) -> Option<Vec<f32>> {
        let sum: f32 = self.lengths.iter().sum();
        let valid = !self.lengths.is_empty()
            && self.offset.is_finite()
            && self
                .lengths
                .iter()
                .all(|length| length.is_finite() && *length >= 0.0)
            && sum.is_finite()
            && sum > 0.0;
        valid.then(|| {
            let mut lengths = self.lengths.clone();
            if lengths.len() % 2 == 1 {
                lengths.extend_from_within(..);
            }
            lengths
        })
    }
}

/// The outline drawn along a path. The default is one pixel of black with
/// mitred corners, butt ends and no dashes.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
    pub join: Join,
    /// The longest a mitred corner may be, in stroke widths, from the inside
    /// of the corner to its tip; past it the corner is bevelled. A limit of 1
    /// or less bevels every corner.
    pub miter_limit: f32,
    pub cap: Cap,
    pub dash: Option<Dash>,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: Color(0, 0, 0, 255),
            join: Join::Miter,
            miter_limit: 4.0,
            cap: Cap::Butt,
            dash: None,
        }
    }
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, to: Point) -> &mut Self {
        self.segments.push(Segment::Move(to));
        self
    }

    pub fn line_to(&mut self, to: Point) -> &mut Self {
        self.segments.push(Segment::Line(to));
        self
    }

    pub fn quad_to(&mut self, control: Point, to: Point) -> &mut Self {
        self.segments.push(Segment::Quad(control, to));
        self
    }

    pub fn cubic_to(&mut self, first: Point, second: Point, to: Point) -> &mut Self {
        self.segments.push(Segment::Cubic(first, second, to));
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.segments.push(Segment::Close);
        self
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// A rectangle holding every point and control point, so the whole shape.
    pub fn bounds(&self) -> Rect {
        let mut points = self.segments.iter().flat_map(|segment| match *segment {
            Segment::Move(a) | Segment::Line(a) => [Some(a), None, None],
            Segment::Quad(a, b) => [Some(a), Some(b), None],
            Segment::Cubic(a, b, c) => [Some(a), Some(b), Some(c)],
            Segment::Close => [None; 3],
        });
        let Some(first) = points.find_map(|point| point) else {
            return Rect::default();
        };
        let (mut low, mut high) = (first, first);
        for point in points.flatten() {
            low = Point {
                x: low.x.min(point.x),
                y: low.y.min(point.y),
            };
            high = Point {
                x: high.x.max(point.x),
                y: high.y.max(point.y),
            };
        }
        Rect {
            x: low.x,
            y: low.y,
            width: high.x - low.x,
            height: high.y - low.y,
        }
    }

    pub fn translated(&self, offset: Point) -> Self {
        let moved = |point: Point| Point {
            x: point.x + offset.x,
            y: point.y + offset.y,
        };
        Self {
            segments: self
                .segments
                .iter()
                .map(|segment| match *segment {
                    Segment::Move(a) => Segment::Move(moved(a)),
                    Segment::Line(a) => Segment::Line(moved(a)),
                    Segment::Quad(a, b) => Segment::Quad(moved(a), moved(b)),
                    Segment::Cubic(a, b, c) => Segment::Cubic(moved(a), moved(b), moved(c)),
                    Segment::Close => Segment::Close,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_hold_the_control_points_too() {
        let mut path = Path::new();
        path.move_to(Point { x: 2.0, y: 3.0 })
            .quad_to(Point { x: 6.0, y: -1.0 }, Point { x: 10.0, y: 3.0 })
            .close();
        let bounds = path.bounds();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (2.0, -1.0, 8.0, 4.0)
        );
        assert_eq!(path.translated(Point { x: 1.0, y: 1.0 }).bounds().x, 3.0);
        assert_eq!(Path::new().bounds(), Rect::default());
    }

    #[test]
    fn a_dash_pattern_pairs_up_and_rejects_what_draws_nothing() {
        assert_eq!(Dash::new([3.0]).pattern(), Some(vec![3.0, 3.0]));
        assert_eq!(Dash::new([2.0, 1.0]).pattern(), Some(vec![2.0, 1.0]));
        assert_eq!(Dash::new([0.0, 4.0]).pattern(), Some(vec![0.0, 4.0]));
        for bad in [vec![], vec![0.0, 0.0], vec![-1.0, 2.0], vec![f32::NAN, 1.0]] {
            assert_eq!(Dash::new(bad).pattern(), None);
        }
        let offset = Dash {
            offset: f32::INFINITY,
            ..Dash::new([1.0, 1.0])
        };
        assert_eq!(offset.pattern(), None);
    }
}
