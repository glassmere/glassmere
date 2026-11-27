//! Paths as polygons, and the coverage of polygons by scanline.
//!
//! A path is flattened into straight edges. A stroke becomes a rectangle per
//! segment and a join per corner, all filled together by the non-zero rule.
//! Coverage is taken at four scanlines per pixel row and exactly across each
//! pixel, which looks smooth enough for icons and thin lines.
use crate::math::FloatMath;
use crate::raster::IRect;
use alloc::vec::Vec;
use glassmere_scene::{LineCap, LineJoin, Path, Point, Segment};

type Pt = [f32; 2];

/// A line from its top down to its bottom, and which way the path went.
#[derive(Clone, Copy)]
struct Edge {
    /// Where the line is at `y0`.
    x: f32,
    y0: f32,
    y1: f32,
    /// Change of x per unit of y.
    slope: f32,
    up: bool,
}

/// Scratch space for filling one path at a time, kept between frames so that
/// drawing does not allocate once it has grown.
#[derive(Default)]
pub(crate) struct PathScratch {
    points: Vec<Pt>,
    edges: Vec<Edge>,
    crossings: Vec<(f32, i32)>,
    sums: Vec<u16>,
    cover: Vec<u8>,
    low: Pt,
    high: Pt,
}

/// Longest stretch of a curve that is not split further, in pixels, and the
/// most pieces a curve is split into.
const SEGMENT: f32 = 2.0;
const MAX_SPLITS: f32 = 32.0;
/// Scanlines per pixel row, and how much each is worth: four 64s fill a pixel.
const SUBROWS: usize = 4;
const SUBROW_WEIGHT: f32 = 64.0;
/// A corner is mitred while the tip is at most this many widths from it.
const MITER_LIMIT: f32 = 4.0;

fn finite(point: Pt) -> bool {
    point[0].is_finite() && point[1].is_finite()
}

fn distance(a: Pt, b: Pt) -> f32 {
    ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrtf()
}

fn splits(length: f32) -> usize {
    (length / SEGMENT).ceilf().clamp(2.0, MAX_SPLITS) as usize
}

/// Appends `point` unless the polyline is already there.
fn add(points: &mut Vec<Pt>, point: Pt) {
    if points.last() != Some(&point) {
        points.push(point);
    }
}

/// The point a segment starts from. A polyline that has been closed starts
/// again at the subpath's `start`.
fn current(points: &mut Vec<Pt>, start: Option<Pt>) -> Option<Pt> {
    if points.is_empty() {
        points.push(start?);
    }
    points.last().copied()
}

/// Turns `path`, moved by `offset`, into polylines and hands each to `emit`
/// with whether the subpath was closed. `false` when a coordinate is not a
/// number.
fn flatten(
    path: &Path,
    offset: Pt,
    points: &mut Vec<Pt>,
    emit: &mut impl FnMut(&[Pt], bool),
) -> bool {
    let place = |p: Point| [p.x + offset[0], p.y + offset[1]];
    let mut start: Option<Pt> = None;
    points.clear();
    for segment in path.segments() {
        match *segment {
            Segment::Move(to) => {
                if points.len() > 1 {
                    emit(points, false);
                }
                let to = place(to);
                if !finite(to) {
                    return false;
                }
                start = Some(to);
                points.clear();
                points.push(to);
            }
            Segment::Line(to) => {
                let to = place(to);
                if !finite(to) {
                    return false;
                }
                if current(points, start).is_none() {
                    // A line with nothing before it begins the path.
                    start = Some(to);
                }
                add(points, to);
            }
            Segment::Quad(control, to) => {
                let (control, to) = (place(control), place(to));
                if !(finite(control) && finite(to)) {
                    return false;
                }
                let Some(from) = current(points, start) else {
                    continue;
                };
                let count = splits(distance(from, control) + distance(control, to));
                for step in 1..=count {
                    let t = step as f32 / count as f32;
                    let u = 1.0 - t;
                    add(
                        points,
                        [
                            u * u * from[0] + 2.0 * u * t * control[0] + t * t * to[0],
                            u * u * from[1] + 2.0 * u * t * control[1] + t * t * to[1],
                        ],
                    );
                }
            }
            Segment::Cubic(first, second, to) => {
                let (first, second, to) = (place(first), place(second), place(to));
                if !(finite(first) && finite(second) && finite(to)) {
                    return false;
                }
                let Some(from) = current(points, start) else {
                    continue;
                };
                let count =
                    splits(distance(from, first) + distance(first, second) + distance(second, to));
                for step in 1..=count {
                    let t = step as f32 / count as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    add(
                        points,
                        [
                            a * from[0] + b * first[0] + c * second[0] + d * to[0],
                            a * from[1] + b * first[1] + c * second[1] + d * to[1],
                        ],
                    );
                }
            }
            Segment::Close => {
                if points.len() > 1 {
                    emit(points, true);
                }
                // What follows starts again where this subpath did.
                points.clear();
            }
        }
    }
    if points.len() > 1 {
        emit(points, false);
    }
    points.clear();
    true
}

impl PathScratch {
    pub fn clear(&mut self) {
        self.edges.clear();
        self.low = [f32::INFINITY; 2];
        self.high = [f32::NEG_INFINITY; 2];
    }

    /// The pixels the loaded shape touches.
    pub fn bounds(&self) -> IRect {
        if self.edges.is_empty() {
            return IRect::EMPTY;
        }
        let edge = |value: f32| value.clamp(-1.0e9, 1.0e9);
        IRect {
            x0: edge(self.low[0]).floorf() as i32,
            y0: edge(self.low[1]).floorf() as i32,
            x1: edge(self.high[0]).ceilf() as i32,
            y1: edge(self.high[1]).ceilf() as i32,
        }
    }

    fn edge(&mut self, a: Pt, b: Pt) {
        for point in [a, b] {
            self.low = [self.low[0].min(point[0]), self.low[1].min(point[1])];
            self.high = [self.high[0].max(point[0]), self.high[1].max(point[1])];
        }
        if a[1] == b[1] {
            return;
        }
        let (top, bottom, up) = if a[1] < b[1] {
            (a, b, false)
        } else {
            (b, a, true)
        };
        self.edges.push(Edge {
            x: top[0],
            y0: top[1],
            y1: bottom[1],
            slope: (bottom[0] - top[0]) / (bottom[1] - top[1]),
            up,
        });
    }

    /// Adds the outline of a closed polygon, wound as given.
    fn polygon(&mut self, points: &[Pt]) {
        for (at, &point) in points.iter().enumerate() {
            self.edge(point, points[(at + 1) % points.len()]);
        }
    }

    /// Adds a polygon wound the same way whichever way it was given: strokes
    /// are a union of parts, and parts that wind against each other would cut
    /// each other out.
    fn solid_polygon(&mut self, points: &[Pt]) {
        let doubled_area: f32 = points
            .iter()
            .enumerate()
            .map(|(at, a)| {
                let b = points[(at + 1) % points.len()];
                a[0] * b[1] - b[0] * a[1]
            })
            .sum();
        if doubled_area >= 0.0 {
            self.polygon(points);
        } else {
            for (at, &point) in points.iter().enumerate() {
                self.edge(points[(at + 1) % points.len()], point);
            }
        }
    }

    /// Loads the area of `path` moved by `offset`; every subpath is closed.
    /// Nothing is loaded when a coordinate is not a number.
    pub fn fill(&mut self, path: &Path, offset: Pt) {
        self.clear();
        let mut points = core::mem::take(&mut self.points);
        let valid = flatten(path, offset, &mut points, &mut |points, _| {
            if points.len() >= 3 {
                self.polygon(points);
            }
        });
        self.points = points;
        if !valid {
            self.clear();
        }
    }

    /// Loads the outline of `path` moved by `offset`, `width` across.
    pub fn stroke(&mut self, path: &Path, offset: Pt, width: f32) {
        self.clear();
        if !(width > 0.0 && width.is_finite()) {
            return;
        }
        let mut points = core::mem::take(&mut self.points);
        let (cap, join) = (path.cap(), path.join());
        let valid = flatten(path, offset, &mut points, &mut |points, closed| {
            self.outline(points, closed, width / 2.0, cap, join);
        });
        self.points = points;
        if !valid {
            self.clear();
        }
    }

    /// A rectangle along each segment and a join at each corner. `half` is
    /// half the line's width.
    fn outline(&mut self, points: &[Pt], closed: bool, half: f32, cap: LineCap, join: LineJoin) {
        // A closed line that comes back to its first point has no segment there.
        let line = match points {
            [first, .., last] if closed && first == last => &points[..points.len() - 1],
            _ => points,
        };
        let count = line.len();
        if count < 2 {
            return;
        }
        let segments = if closed { count } else { count - 1 };
        // The unit direction of segment `at`.
        let direction = |at: usize| {
            let (a, b) = (line[at], line[(at + 1) % count]);
            let long = distance(a, b);
            [(b[0] - a[0]) / long, (b[1] - a[1]) / long]
        };
        for at in 0..segments {
            let (a, b, d) = (line[at], line[(at + 1) % count], direction(at));
            let n = [-d[1] * half, d[0] * half];
            self.solid_polygon(&[
                [a[0] + n[0], a[1] + n[1]],
                [b[0] + n[0], b[1] + n[1]],
                [b[0] - n[0], b[1] - n[1]],
                [a[0] - n[0], a[1] - n[1]],
            ]);
            // The corner at the end of this segment, where the line goes on.
            let next = at + 1;
            if next == segments && !closed {
                continue;
            }
            let (v, e) = (line[next % count], direction(next % count));
            let cosine = d[0] * e[0] + d[1] * e[1];
            let turn = d[0] * e[1] - d[1] * e[0];
            if turn == 0.0 && cosine > 0.0 {
                continue;
            }
            // The outer side of a corner is away from where the line turns.
            let side = if turn > 0.0 { -half } else { half };
            let o0 = [-d[1] * side, d[0] * side];
            let o1 = [-e[1] * side, e[0] * side];
            let (p0, p1) = ([v[0] + o0[0], v[1] + o0[1]], [v[0] + o1[0], v[1] + o1[1]]);
            // The tip of the mitre is `half / cos(turn / 2)` from the corner,
            // and a mitre is cut when that is over the limit.
            if join == LineJoin::Miter && 1.0 + cosine >= 2.0 / (MITER_LIMIT * MITER_LIMIT) {
                let reach = 1.0 / (1.0 + cosine);
                let tip = [
                    v[0] + (o0[0] + o1[0]) * reach,
                    v[1] + (o0[1] + o1[1]) * reach,
                ];
                self.solid_polygon(&[v, p0, tip, p1]);
            } else if join == LineJoin::Round {
                // A fan from the corner along the arc between the two edges.
                let turned =
                    libm::atan2f(o0[0] * o1[1] - o0[1] * o1[0], o0[0] * o1[0] + o0[1] * o1[1]);
                let mut fan = alloc::vec![v];
                arc(&mut fan, v, o0, turned);
                self.solid_polygon(&fan);
            } else {
                self.solid_polygon(&[v, p0, p1]);
            }
        }
        // The ends of a line that is not closed. The first segment's end is
        // its start, seen the other way round.
        if !closed && cap != LineCap::Butt {
            let (first, last) = (direction(0), direction(segments - 1));
            let reversed = [-first[0], -first[1]];
            self.cap(line[0], reversed, half, cap);
            self.cap(line[count - 1], last, half, cap);
        }
    }

    /// The cap at `end`, a point a line with unit direction `d` runs out at.
    fn cap(&mut self, end: Pt, d: Pt, half: f32, cap: LineCap) {
        let n = [-d[1] * half, d[0] * half];
        match cap {
            LineCap::Butt => {}
            LineCap::Square => {
                let past = [d[0] * half, d[1] * half];
                self.solid_polygon(&[
                    [end[0] + n[0], end[1] + n[1]],
                    [end[0] + n[0] + past[0], end[1] + n[1] + past[1]],
                    [end[0] - n[0] + past[0], end[1] - n[1] + past[1]],
                    [end[0] - n[0], end[1] - n[1]],
                ]);
            }
            LineCap::Round => {
                // The half disc from one side of the line, round the way it
                // runs, to the other.
                let mut fan = Vec::new();
                arc(&mut fan, end, n, -core::f32::consts::PI);
                self.solid_polygon(&fan);
            }
        }
    }

    /// Calls `row` with the coverage of each pixel row of `bounds`, left to
    /// right from `bounds.x0`, for the loaded shape. `bounds` is not empty.
    pub fn rows(&mut self, bounds: IRect, mut row: impl FnMut(i32, &[u8])) {
        let width = (bounds.x1 - bounds.x0) as usize;
        self.sums.resize(width, 0);
        self.cover.resize(width, 0);
        let (left, right) = (bounds.x0 as f32, bounds.x1 as f32);
        for y in bounds.y0..bounds.y1 {
            self.sums.fill(0);
            for sub in 0..SUBROWS {
                let scan = y as f32 + (sub as f32 + 0.5) / SUBROWS as f32;
                self.crossings.clear();
                for edge in &self.edges {
                    if edge.y0 <= scan && scan < edge.y1 {
                        let x = edge.x + (scan - edge.y0) * edge.slope;
                        self.crossings.push((x, if edge.up { -1 } else { 1 }));
                    }
                }
                self.crossings.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                let (mut winding, mut start) = (0, 0.0);
                for &(x, step) in &self.crossings {
                    let before = winding;
                    winding += step;
                    if before == 0 {
                        start = x;
                    } else if winding == 0 {
                        add_span(&mut self.sums, left, right, start, x);
                    }
                }
            }
            for (cover, sum) in self.cover.iter_mut().zip(&self.sums) {
                *cover = (*sum).min(255) as u8;
            }
            row(y, &self.cover);
        }
    }
}

/// Pushes the points of the arc about `centre` that starts at `centre + from`
/// and turns by `sweep` radians, both ends included.
fn arc(points: &mut Vec<Pt>, centre: Pt, from: Pt, sweep: f32) {
    // Eight steps to a half turn is within a pixel of a circle for anything a
    // small display draws, and costs nothing on a big one.
    let pieces = (libm::fabsf(sweep) / (core::f32::consts::PI / 8.0))
        .ceilf()
        .max(1.0) as usize;
    for piece in 0..=pieces {
        let angle = sweep * piece as f32 / pieces as f32;
        let (sin, cos) = (libm::sinf(angle), libm::cosf(angle));
        points.push([
            centre[0] + from[0] * cos - from[1] * sin,
            centre[1] + from[0] * sin + from[1] * cos,
        ]);
    }
}

/// Adds the part of `start..end` that lies in `left..right`, pixel by pixel,
/// to the sums of the row that begins at `left`.
fn add_span(sums: &mut [u16], left: f32, right: f32, start: f32, end: f32) {
    let (start, end) = (start.max(left), end.min(right));
    if end <= start {
        return;
    }
    let first = (start - left).floorf() as usize;
    let last = ((end - left).ceilf() as usize).min(sums.len());
    for (at, sum) in sums.iter_mut().enumerate().take(last).skip(first) {
        let pixel = left + at as f32;
        let overlap = end.min(pixel + 1.0) - start.max(pixel);
        *sum += (overlap * SUBROW_WEIGHT + 0.5) as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn point(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    /// The coverage of every pixel in `bounds`, row after row.
    fn coverage(scratch: &mut PathScratch) -> (IRect, Vec<Vec<u8>>) {
        let bounds = scratch.bounds();
        let mut rows = Vec::new();
        scratch.rows(bounds, |_, cover| rows.push(cover.to_vec()));
        (bounds, rows)
    }

    fn square(x: f32, y: f32, size: f32) -> Path {
        let mut path = Path::new();
        path.move_to(point(x, y))
            .line_to(point(x + size, y))
            .line_to(point(x + size, y + size))
            .line_to(point(x, y + size))
            .close();
        path
    }

    #[test]
    fn a_square_fills_whole_pixels_and_half_pixels() {
        let mut scratch = PathScratch::default();
        scratch.fill(&square(1.0, 1.0, 2.5), [0.0, 0.0]);
        let (bounds, rows) = coverage(&mut scratch);
        assert_eq!(
            bounds,
            IRect {
                x0: 1,
                y0: 1,
                x1: 4,
                y1: 4
            }
        );
        assert_eq!(rows[0], [255, 255, 128]);
        assert_eq!(rows[1], [255, 255, 128]);
        assert_eq!(rows[2], [128, 128, 64]);
    }

    #[test]
    fn a_contour_wound_the_other_way_cuts_a_hole() {
        let mut path = square(0.0, 0.0, 6.0);
        path.move_to(point(2.0, 2.0))
            .line_to(point(2.0, 4.0))
            .line_to(point(4.0, 4.0))
            .line_to(point(4.0, 2.0))
            .close();
        let mut scratch = PathScratch::default();
        scratch.fill(&path, [0.0, 0.0]);
        let (_, rows) = coverage(&mut scratch);
        assert_eq!(rows[0], [255; 6]);
        assert_eq!(rows[2], [255, 255, 0, 0, 255, 255]);
        assert_eq!(rows[5], [255; 6]);
    }

    #[test]
    fn a_circle_of_curves_has_the_area_of_a_circle() {
        let k = 0.552_284_8 * 10.0;
        let mut path = Path::new();
        path.move_to(point(20.0, 10.0))
            .cubic_to(
                point(20.0, 10.0 + k),
                point(10.0 + k, 20.0),
                point(10.0, 20.0),
            )
            .cubic_to(
                point(10.0 - k, 20.0),
                point(0.0, 10.0 + k),
                point(0.0, 10.0),
            )
            .cubic_to(point(0.0, 10.0 - k), point(10.0 - k, 0.0), point(10.0, 0.0))
            .cubic_to(
                point(10.0 + k, 0.0),
                point(20.0, 10.0 - k),
                point(20.0, 10.0),
            )
            .close();
        let mut scratch = PathScratch::default();
        scratch.fill(&path, [0.0, 0.0]);
        let (_, rows) = coverage(&mut scratch);
        let area: f32 = rows
            .iter()
            .flatten()
            .map(|cover| f32::from(*cover) / 255.0)
            .sum();
        let exact = core::f32::consts::PI * 100.0;
        assert!((area - exact).abs() < 3.0, "{area} against {exact}");
        assert_eq!(rows[0][0], 0, "the corners are empty");
        assert_eq!(rows[10][10], 255, "the middle is full");
    }

    #[test]
    fn a_stroke_is_as_wide_as_asked_and_ends_square() {
        let mut path = Path::new();
        path.move_to(point(2.0, 5.0)).line_to(point(8.0, 5.0));
        let mut scratch = PathScratch::default();
        scratch.stroke(&path, [0.0, 0.0], 2.0);
        let (bounds, rows) = coverage(&mut scratch);
        assert_eq!(
            bounds,
            IRect {
                x0: 2,
                y0: 4,
                x1: 8,
                y1: 6
            }
        );
        assert_eq!(rows, vec![vec![255; 6]; 2]);
    }

    #[test]
    fn a_right_angle_is_mitred() {
        let mut path = Path::new();
        path.move_to(point(2.0, 12.0))
            .line_to(point(12.0, 12.0))
            .line_to(point(12.0, 2.0));
        let mut scratch = PathScratch::default();
        scratch.stroke(&path, [0.0, 0.0], 4.0);
        let (bounds, rows) = coverage(&mut scratch);
        assert_eq!((bounds.x0, bounds.y0, bounds.x1, bounds.y1), (2, 2, 14, 14));
        // The outer corner pixel is filled, which a bevel would leave half empty.
        assert_eq!(rows[11][11], 255);
        // And the inner one is not.
        assert_eq!(rows[0][0], 0);
    }

    /// The area a stroke of `path` covers, in pixels, from its coverage.
    fn stroked_area(path: &Path, width: f32) -> f32 {
        let mut scratch = PathScratch::default();
        scratch.stroke(path, [0.0, 0.0], width);
        let (_, rows) = coverage(&mut scratch);
        rows.iter().flatten().map(|&c| f32::from(c) / 255.0).sum()
    }

    #[test]
    fn caps_finish_an_open_line_by_their_shape() {
        let line = || {
            let mut line = Path::new();
            line.move_to(point(10.0, 10.0)).line_to(point(30.0, 10.0));
            line
        };
        let butt = stroked_area(&line(), 4.0);
        let square = stroked_area(&line().with_cap(LineCap::Square), 4.0);
        let round = stroked_area(&line().with_cap(LineCap::Round), 4.0);
        // 20 by 4; half the width longer at both ends; a half circle at each.
        assert!((butt - 80.0).abs() < 1.0, "{butt}");
        assert!((square - 96.0).abs() < 1.0, "{square}");
        assert!(
            (round - (80.0 + core::f32::consts::PI * 4.0)).abs() < 1.5,
            "{round}"
        );
        // And the same at the start: a line drawn the other way is the same shape.
        let mut back = Path::new();
        back.move_to(point(30.0, 10.0)).line_to(point(10.0, 10.0));
        let back = stroked_area(&back.with_cap(LineCap::Round), 4.0);
        assert!((back - round).abs() < 0.5, "{back} against {round}");
    }

    #[test]
    fn joins_finish_a_corner_by_their_shape() {
        let square = || {
            let mut path = Path::new();
            path.move_to(point(2.0, 2.0))
                .line_to(point(12.0, 2.0))
                .line_to(point(12.0, 12.0))
                .line_to(point(2.0, 12.0))
                .close();
            path
        };
        let miter = stroked_area(&square(), 2.0);
        let bevel = stroked_area(&square().with_join(LineJoin::Bevel), 2.0);
        let round = stroked_area(&square().with_join(LineJoin::Round), 2.0);
        // The band between 12 by 12 and 8 by 8; a bevel takes half a square off
        // each corner and a round join what is outside a quarter circle.
        assert!((miter - 80.0).abs() < 1.0, "{miter}");
        assert!((bevel - 78.0).abs() < 1.0, "{bevel}");
        let expected = 80.0 - 4.0 * (1.0 - core::f32::consts::FRAC_PI_4);
        assert!((round - expected).abs() < 1.0, "{round}");
        assert!(bevel < round && round < miter);
    }

    #[test]
    fn a_round_cap_makes_a_dot_of_a_short_line() {
        // A line as long as it is wide, with round caps, is a stadium: wider
        // than a butt-capped square of the same length, and inside a box of
        // its width plus the caps.
        let mut dash = Path::new();
        dash.move_to(point(10.0, 10.0)).line_to(point(14.0, 10.0));
        let square = stroked_area(&dash, 4.0);
        let stadium = stroked_area(&dash.with_cap(LineCap::Round), 4.0);
        assert!((square - 16.0).abs() < 0.5, "{square}");
        assert!(
            (stadium - (16.0 + core::f32::consts::PI * 4.0)).abs() < 1.5,
            "{stadium}"
        );
    }

    #[test]
    fn a_sharp_turn_is_bevelled_instead_of_spiking_out() {
        let mut path = Path::new();
        path.move_to(point(0.0, 20.0))
            .line_to(point(20.0, 20.0))
            .line_to(point(0.0, 19.0));
        let mut scratch = PathScratch::default();
        scratch.stroke(&path, [0.0, 0.0], 4.0);
        let bounds = scratch.bounds();
        assert!(bounds.x1 <= 23 && bounds.y1 <= 23, "{bounds:?}");
    }

    #[test]
    fn a_closed_stroke_joins_all_the_way_round() {
        let mut scratch = PathScratch::default();
        scratch.stroke(&square(2.0, 2.0, 8.0), [0.0, 0.0], 2.0);
        let (bounds, rows) = coverage(&mut scratch);
        assert_eq!(
            bounds,
            IRect {
                x0: 1,
                y0: 1,
                x1: 11,
                y1: 11
            }
        );
        assert_eq!(rows[0][0], 255, "corner");
        assert_eq!(rows[9][9], 255, "corner");
        assert_eq!(rows[0][5], 255, "edge");
        assert_eq!(rows[5][5], 0, "inside");
    }

    #[test]
    fn paths_that_are_not_numbers_or_not_shapes_draw_nothing() {
        let mut scratch = PathScratch::default();
        let mut bad = Path::new();
        bad.move_to(point(0.0, 0.0))
            .line_to(point(f32::NAN, 4.0))
            .line_to(point(4.0, 4.0));
        scratch.fill(&bad, [0.0, 0.0]);
        assert!(scratch.bounds().is_empty());
        let mut line = Path::new();
        line.move_to(point(0.0, 0.0)).line_to(point(4.0, 4.0));
        scratch.fill(&line, [0.0, 0.0]);
        assert!(scratch.bounds().is_empty(), "a line has no area");
        scratch.stroke(&line, [0.0, 0.0], f32::NAN);
        assert!(scratch.bounds().is_empty());
        scratch.stroke(&Path::new(), [0.0, 0.0], 2.0);
        assert!(scratch.bounds().is_empty());
    }
}
