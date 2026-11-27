//! Shapes of straight and curved segments.

use crate::{Color, Point, Rect};
use alloc::vec::Vec;

mod dash;

/// A shape of straight and curved segments, in the same logical coordinates
/// as the rest of the scene. Each `move_to` starts a subpath; `close` joins a
/// subpath back to where it started. Fills use the non-zero rule, so a
/// subpath wound the other way inside another cuts a hole.
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
pub enum Segment {
    Move(Point),
    Line(Point),
    /// A quadratic curve: its control point, then where it ends.
    Quad(Point, Point),
    /// A cubic curve: its two control points, then where it ends.
    Cubic(Point, Point, Point),
    Close,
}

/// The outline drawn along a path. Corners are mitred up to four times the
/// width and bevelled beyond it; open ends are cut square at their points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    /// A rectangle with rounded corners, wound clockwise from the top edge.
    /// The radius is kept to half the shorter side, so a radius larger than
    /// the rectangle gives a stadium or a circle; a radius of zero or less
    /// gives square corners.
    pub fn rounded_rect(rect: Rect, radius: f32) -> Self {
        // The control-point distance that makes a cubic follow a quarter
        // circle to within a fraction of a percent.
        const ARC: f32 = 0.552_284_7;
        let radius = radius.max(0.0).min(rect.width / 2.0).min(rect.height / 2.0);
        let (left, top) = (rect.x, rect.y);
        let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
        let at = |x, y| Point { x, y };
        // Ten segments with round corners, five without: one allocation.
        let mut path = Self {
            segments: Vec::with_capacity(if radius > 0.0 { 10 } else { 5 }),
        };
        if radius <= 0.0 {
            path.move_to(at(left, top))
                .line_to(at(right, top))
                .line_to(at(right, bottom))
                .line_to(at(left, bottom))
                .close();
            return path;
        }
        let k = radius * ARC;
        path.move_to(at(left + radius, top))
            .line_to(at(right - radius, top))
            .cubic_to(
                at(right - radius + k, top),
                at(right, top + radius - k),
                at(right, top + radius),
            )
            .line_to(at(right, bottom - radius))
            .cubic_to(
                at(right, bottom - radius + k),
                at(right - radius + k, bottom),
                at(right - radius, bottom),
            )
            .line_to(at(left + radius, bottom))
            .cubic_to(
                at(left + radius - k, bottom),
                at(left, bottom - radius + k),
                at(left, bottom - radius),
            )
            .line_to(at(left, top + radius))
            .cubic_to(
                at(left, top + radius - k),
                at(left + radius - k, top),
                at(left + radius, top),
            )
            .close();
        path
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
    #[test]
    fn a_rounded_rect_stays_inside_its_rectangle_and_clamps_the_radius() {
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            width: 40.0,
            height: 12.0,
        };
        let path = Path::rounded_rect(rect, 100.0);
        assert_eq!(path.bounds(), rect);
        // Clamped to a stadium: the straight runs along the top and bottom
        // are as long as the difference of the sides.
        assert_eq!(path.segments().len(), 10);
        let square = Path::rounded_rect(rect, 0.0);
        assert_eq!(square.segments().len(), 5);
        assert_eq!(square.bounds(), rect);
    }

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
}
