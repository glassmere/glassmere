//! Paths as triangles. lyon flattens the curves and fills the polygons; the
//! edges are anti-aliased by a fringe one device pixel wide whose coverage
//! falls from 1 to 0, half inside the outline and half outside it, as the
//! CPU renderer's coverage does. Strokes become outlines filled the same way.

use glassmere_scene::{Cap, Dash, FillRule, Join, Path, Point, Rect, Segment, Stroke};
use lyon_tessellation::path::Path as LyonPath;
use lyon_tessellation::path::PathEvent;
use lyon_tessellation::path::iterator::PathIterator;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers, math,
};

/// A corner of a triangle in logical coordinates, and how much of a pixel
/// there the shape covers.
pub(crate) type Corner = (Point, f32);

/// How far a flattened curve may stray from the real one, in device pixels.
const TOLERANCE: f32 = 0.2;
/// The most arc pieces one round join or cap is cut into.
const MAX_ARC_PIECES: usize = 1024;

/// Triangles covering `path` filled by `rule`, where a device pixel is
/// `pixel` logical units wide.
pub(crate) fn fill(path: &Path, rule: FillRule, pixel: f32, out: &mut Vec<Corner>) {
    let rings: Vec<Vec<Point>> = polylines(path, pixel)
        .into_iter()
        .map(|(points, _)| points)
        .filter(|points| points.len() >= 3)
        .collect();
    fill_rings(&rings, rule, pixel, out);
}

/// How far a stroke reaches from its path, in the stroke's own units: a mitre
/// at most the limit's worth of widths, the other joins and the caps one.
pub(crate) fn reach(stroke: &Stroke) -> f32 {
    let widths = if stroke.join == Join::Miter && stroke.miter_limit.is_finite() {
        stroke.miter_limit.max(1.0)
    } else {
        1.0
    };
    stroke.width * widths
}

/// Triangles covering the outline `stroke.width` wide drawn along `path`,
/// where `visible` is the part of the plane that is drawn: dashes that cannot
/// reach it are left out. Below a pixel the line is drawn a pixel wide and
/// fainter, the result returned as the factor to scale its alpha by.
pub(crate) fn stroke(
    path: &Path,
    stroke: &Stroke,
    visible: Rect,
    pixel: f32,
    out: &mut Vec<Corner>,
) -> f32 {
    let faint = (stroke.width / pixel).min(1.0);
    let half = stroke.width.max(pixel) / 2.0;
    let mut lines = polylines(path, pixel);
    if let Some(dash) = &stroke.dash
        && let Some(pattern) = dash.pattern()
    {
        lines = dashed(lines, &pattern, dash.offset, stroke.cap, pixel);
        let margin = reach(stroke) + pixel;
        lines.retain(|(points, _)| near(points, margin, visible));
    }
    // A limit that is not a number bevels every corner.
    let joining = Joining {
        join: stroke.join,
        limit: if stroke.miter_limit.is_finite() {
            stroke.miter_limit
        } else {
            0.0
        },
        pixel,
    };
    let open_ring = |points: &[Point]| {
        let mut ring = offset(points, false, half, &joining);
        let end = points.len() - 1;
        cap(
            &mut ring,
            points[end],
            points[end - 1],
            half,
            stroke.cap,
            pixel,
        );
        let mut back = points.to_vec();
        back.reverse();
        ring.extend(offset(&back, false, half, &joining));
        cap(
            &mut ring,
            back[back.len() - 1],
            back[back.len() - 2],
            half,
            stroke.cap,
            pixel,
        );
        ring
    };
    let mut rings = Vec::new();
    for (points, closed) in lines {
        if points.len() < 2 {
            continue;
        }
        if points.iter().all(|point| *point == points[0]) {
            if let Some(dot) = dot(points[0], half, stroke.cap, pixel) {
                rings.push(dot);
            }
        } else if closed && points.len() >= 3 {
            rings.push(offset(&points, true, half, &joining));
            let mut back = points;
            back.reverse();
            rings.push(offset(&back, true, half, &joining));
        } else if closed {
            // Two points, joined at both ends: one outline round the line.
            rings.push(offset(&points, true, half, &joining));
        } else {
            let length: f32 = points
                .windows(2)
                .map(|pair| sub(pair[1], pair[0]).x.hypot(sub(pair[1], pair[0]).y))
                .sum();
            let reached = if stroke.cap == Cap::Butt {
                length
            } else {
                length + 2.0 * half
            };
            if reached >= pixel {
                rings.push(open_ring(&points));
                continue;
            }
            // Shorter than the pixel it is drawn in, which a half-pixel
            // inset at each end would turn inside out: stretch it to one
            // pixel and take the difference from its coverage instead.
            let (first, last) = (points[0], points[points.len() - 1]);
            let middle = Point {
                x: (first.x + last.x) / 2.0,
                y: (first.y + last.y) / 2.0,
            };
            let direction = left(first, last);
            let along_line = Point {
                x: direction.y,
                y: -direction.x,
            };
            let stretch = (pixel - (reached - length)) / 2.0;
            let ring = open_ring(&[
                along(middle, along_line, -stretch),
                along(middle, along_line, stretch),
            ]);
            let from = out.len();
            fill_rings(&[ring], FillRule::NonZero, pixel, out);
            for corner in &mut out[from..] {
                corner.1 *= reached / pixel;
            }
        }
    }
    fill_rings(&rings, FillRule::NonZero, pixel, out);
    faint
}

/// Whether the points come within `margin` of `visible`.
fn near(points: &[Point], margin: f32, visible: Rect) -> bool {
    let (mut low, mut high) = (points[0], points[0]);
    for point in points {
        low = Point {
            x: low.x.min(point.x),
            y: low.y.min(point.y),
        };
        high = Point {
            x: high.x.max(point.x),
            y: high.y.max(point.y),
        };
    }
    // Written so that points that are not numbers are kept.
    !(high.x + margin < visible.x
        || low.x - margin > visible.x + visible.width
        || high.y + margin < visible.y
        || low.y - margin > visible.y + visible.height)
}

/// The outline of what a cap draws at a stroke of no length, if it draws
/// anything: a circle, or a square with its sides along the axes.
fn dot(at: Point, half: f32, cap: Cap, pixel: f32) -> Option<Vec<Point>> {
    match cap {
        Cap::Round => {
            let pieces = arc_pieces(half, 2.0 * std::f32::consts::PI, pixel).max(8);
            Some(
                (0..pieces)
                    .map(|piece| {
                        let angle = 2.0 * std::f32::consts::PI * piece as f32 / pieces as f32;
                        along(
                            at,
                            Point {
                                x: angle.cos(),
                                y: angle.sin(),
                            },
                            half,
                        )
                    })
                    .collect(),
            )
        }
        Cap::Square => Some(
            [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, y)| Point {
                    x: at.x + x * half,
                    y: at.y + y * half,
                })
                .to_vec(),
        ),
        _ => None,
    }
}

/// Each subpath flattened, without repeated points, and whether it was closed.
/// One that was drawn but never left its point has that point twice.
fn polylines(path: &Path, pixel: f32) -> Vec<(Vec<Point>, bool)> {
    // A shape with a point that is not a number has nowhere to be drawn.
    let finite = |point: &Point| point.x.is_finite() && point.y.is_finite();
    let drawable = path.segments().iter().all(|segment| match *segment {
        Segment::Move(a) | Segment::Line(a) => finite(&a),
        Segment::Quad(a, b) => finite(&a) && finite(&b),
        Segment::Cubic(a, b, c) => finite(&a) && finite(&b) && finite(&c),
        _ => true,
    });
    if !drawable {
        return Vec::new();
    }
    let point = |point: Point| math::point(point.x, point.y);
    let mut builder = LyonPath::builder();
    let mut open = false;
    let mut start = Point::default();
    let mut last = Point::default();
    for segment in path.segments() {
        let to = match *segment {
            Segment::Move(to) => {
                if open {
                    builder.end(false);
                }
                builder.begin(point(to));
                (open, start) = (true, to);
                last = to;
                continue;
            }
            Segment::Close => {
                if open {
                    builder.end(true);
                    open = false;
                }
                last = start;
                continue;
            }
            Segment::Line(to) | Segment::Quad(_, to) | Segment::Cubic(_, _, to) => to,
            _ => {
                debug_assert!(false, "unhandled path segment {segment:?}");
                continue;
            }
        };
        // A segment with nothing before it starts where the last one ended.
        if !open {
            builder.begin(point(last));
            (open, start) = (true, last);
        }
        match *segment {
            Segment::Line(to) => builder.line_to(point(to)),
            Segment::Quad(control, to) => builder.quadratic_bezier_to(point(control), point(to)),
            Segment::Cubic(first, second, to) => {
                builder.cubic_bezier_to(point(first), point(second), point(to))
            }
            _ => unreachable!(),
        };
        last = to;
    }
    if open {
        builder.end(false);
    }
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut drawn = false;
    for event in builder.build().iter().flattened(TOLERANCE * pixel) {
        let push = |points: &mut Vec<Point>, at: math::Point| {
            let at = Point { x: at.x, y: at.y };
            if points.last() != Some(&at) {
                points.push(at);
            }
        };
        match event {
            PathEvent::Begin { at } => {
                current.clear();
                drawn = false;
                push(&mut current, at);
            }
            PathEvent::Line { to, .. } => {
                drawn = true;
                push(&mut current, to);
            }
            PathEvent::End { close, .. } => {
                if current.len() > 1 && current.first() == current.last() {
                    current.pop();
                }
                // A subpath that went nowhere is a point twice over, which
                // strokes draw as a dot where the caps make one.
                if current.len() == 1 && (drawn || close) {
                    current.push(current[0]);
                }
                lines.push((std::mem::take(&mut current), close));
            }
            PathEvent::Quadratic { .. } | PathEvent::Cubic { .. } => {}
        }
    }
    lines
}

/// Cuts each polyline into the dashes of `pattern`, pairs of drawn and
/// skipped lengths, starting `offset` into it for every polyline. A closed
/// polyline whose first and last dash meet at its start gets them joined. A
/// dash of no length is a dot for caps that draw one, and nothing for a butt
/// cap. A stroke that would be cut into more than `Dash::MAX_COUNT` dashes, or
/// whose length is not a number, is left whole.
fn dashed(
    lines: Vec<(Vec<Point>, bool)>,
    pattern: &[f32],
    offset: f32,
    cap: Cap,
    pixel: f32,
) -> Vec<(Vec<Point>, bool)> {
    let length = |a: Point, b: Point| sub(b, a).x.hypot(sub(b, a).y);
    let total: f32 = lines
        .iter()
        .map(|(points, closed)| {
            let ring = if *closed {
                length(points[points.len() - 1], points[0])
            } else {
                0.0
            };
            points
                .windows(2)
                .map(|pair| length(pair[0], pair[1]))
                .sum::<f32>()
                + ring
        })
        .sum();
    let cycle: f32 = pattern.iter().sum();
    let count = total / cycle * pattern.len() as f32 / 2.0;
    if !(total.is_finite() && cycle.is_finite() && offset.is_finite())
        || count > Dash::MAX_COUNT as f32
    {
        return lines;
    }
    let mut out = Vec::new();
    for (points, closed) in lines {
        if points.iter().all(|point| *point == points[0]) {
            out.push((points, closed));
            continue;
        }
        let mut stops = points.clone();
        if closed {
            stops.push(points[0]);
        }
        let mut index = 0;
        let mut into = offset.rem_euclid(cycle);
        while into > 0.0 && into >= pattern[index] {
            into -= pattern[index];
            index = (index + 1) % pattern.len();
        }
        let mut remaining = pattern[index] - into;
        let started_on = index % 2 == 0;
        let first = out.len();
        let mut current = started_on.then(|| vec![stops[0]]);
        let mut toggled = false;
        for (number, pair) in stops.windows(2).enumerate() {
            let last = number + 2 == stops.len();
            let (a, b) = (pair[0], pair[1]);
            let span = length(a, b);
            if span == 0.0 {
                continue;
            }
            let direction = Point {
                x: (b.x - a.x) / span,
                y: (b.y - a.y) / span,
            };
            let mut at = 0.0;
            loop {
                let step = remaining.min(span - at);
                at += step;
                remaining -= step;
                let here = if at >= span {
                    b
                } else {
                    along(a, direction, at)
                };
                if remaining > 0.0 {
                    // The segment ends inside this stretch of the pattern.
                    if let Some(dash) = current.as_mut()
                        && dash.last() != Some(&here)
                    {
                        dash.push(here);
                    }
                    break;
                }
                toggled = true;
                if index % 2 == 0 {
                    if let Some(mut dash) = current.take() {
                        let nothing = dash.len() == 1 && dash.last() == Some(&here);
                        if dash.last() != Some(&here) {
                            dash.push(here);
                        } else if nothing && cap != Cap::Butt {
                            // A dash of no length is a dot where the caps say.
                            dash.push(along(here, direction, pixel * 0.01));
                        }
                        if !nothing || cap != Cap::Butt {
                            out.push((dash, false));
                        }
                    }
                } else if at < span || !last {
                    current = Some(vec![here]);
                }
                index = (index + 1) % pattern.len();
                remaining = pattern[index];
            }
        }
        match current {
            Some(_) if closed && !toggled => out.push((points, true)),
            Some(mut dash) if closed && started_on && out.len() > first => {
                dash.extend_from_slice(&out[first].0[1..]);
                out[first].0 = dash;
            }
            Some(dash) if dash.len() >= 2 => out.push((dash, false)),
            _ => {}
        }
    }
    out
}

fn sub(a: Point, b: Point) -> Point {
    Point {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

fn along(a: Point, direction: Point, by: f32) -> Point {
    Point {
        x: a.x + direction.x * by,
        y: a.y + direction.y * by,
    }
}

/// The unit normal on the left of travelling from `a` to `b`, the side a
/// ring of positive area holds its inside on.
fn left(a: Point, b: Point) -> Point {
    let d = sub(b, a);
    let length = d.x.hypot(d.y);
    if length == 0.0 {
        return Point::default();
    }
    Point {
        x: -d.y / length,
        y: d.x / length,
    }
}

/// How the corners of a stroke are turned.
struct Joining {
    join: Join,
    limit: f32,
    pixel: f32,
}

/// The vector `v` turned by `angle` radians.
fn turned(v: Point, angle: f32) -> Point {
    let (sin, cos) = angle.sin_cos();
    Point {
        x: v.x * cos - v.y * sin,
        y: v.x * sin + v.y * cos,
    }
}

/// How many straight pieces keep an arc of `angle` radians, `radius` from its
/// centre, within the flattening tolerance.
fn arc_pieces(radius: f32, angle: f32, pixel: f32) -> usize {
    let tolerance = TOLERANCE * pixel;
    let step = if radius > tolerance {
        2.0 * (1.0 - tolerance / radius).acos()
    } else {
        std::f32::consts::PI
    };
    ((angle.abs() / step).ceil() as usize).clamp(1, MAX_ARC_PIECES)
}

/// The end of a polyline at `at`, reached from `prev`, whose ring has just run
/// along its left side and is about to run back along its right.
fn cap(ring: &mut Vec<Point>, at: Point, prev: Point, half: f32, cap: Cap, pixel: f32) {
    let normal = left(prev, at);
    let ahead = Point {
        x: normal.y,
        y: -normal.x,
    };
    match cap {
        Cap::Square => {
            for side in [half, -half] {
                ring.push(along(along(at, normal, side), ahead, half));
            }
        }
        Cap::Round => {
            let pieces = arc_pieces(half, std::f32::consts::PI, pixel);
            for piece in 1..pieces {
                let angle = std::f32::consts::PI * piece as f32 / pieces as f32;
                ring.push(along(at, turned(normal, -angle), half));
            }
        }
        _ => {}
    }
}

/// The line `half` to the left of a polyline, joined as `joining` says, and
/// through the corner itself on the inside of a turn.
fn offset(points: &[Point], closed: bool, half: f32, joining: &Joining) -> Vec<Point> {
    let count = points.len();
    let mut out = Vec::with_capacity(count * 2);
    for index in 0..count {
        let at = points[index];
        let before = (index > 0 || closed).then(|| points[(index + count - 1) % count]);
        let after = (index + 1 < count || closed).then(|| points[(index + 1) % count]);
        let (incoming, outgoing) = match (before, after) {
            (Some(before), Some(after)) => (left(before, at), left(at, after)),
            (None, Some(after)) => {
                let normal = left(at, after);
                (normal, normal)
            }
            (Some(before), None) => {
                let normal = left(before, at);
                (normal, normal)
            }
            (None, None) => continue,
        };
        let turn = sub(at, before.unwrap_or(at)).x * sub(after.unwrap_or(at), at).y
            - sub(at, before.unwrap_or(at)).y * sub(after.unwrap_or(at), at).x;
        let cosine = incoming.x * outgoing.x + incoming.y * outgoing.y;
        if incoming == outgoing || cosine > 0.9999 {
            out.push(along(at, incoming, half));
        } else if turn > 0.0 {
            // Turning left: this side is the inside of the corner. Where the
            // two offset edges meet is the corner, unless that is further
            // back than the segments are long; then it goes through the
            // point itself, and the other side covers what that leaves.
            let back = half * ((1.0 - cosine) / (1.0 + cosine)).sqrt();
            let length = |a: Point, b: Point| sub(b, a).x.hypot(sub(b, a).y);
            let shortest = length(before.unwrap_or(at), at).min(length(at, after.unwrap_or(at)));
            if back <= shortest {
                let sum = Point {
                    x: incoming.x + outgoing.x,
                    y: incoming.y + outgoing.y,
                };
                out.push(along(at, sum, half / (1.0 + cosine)));
            } else {
                out.push(along(at, incoming, half));
                out.push(at);
                out.push(along(at, outgoing, half));
            }
        } else {
            // The miter is 1 / cos(θ / 2) half widths long.
            let sum = Point {
                x: incoming.x + outgoing.x,
                y: incoming.y + outgoing.y,
            };
            let squared = (1.0 + cosine) / 2.0;
            if joining.join == Join::Miter && squared > 0.0 && 1.0 / squared.sqrt() <= joining.limit
            {
                out.push(along(at, sum, half / (1.0 + cosine)));
            } else {
                out.push(along(at, incoming, half));
                if joining.join == Join::Round {
                    let angle = -cosine.clamp(-1.0, 1.0).acos();
                    let pieces = arc_pieces(half, angle, joining.pixel);
                    for piece in 1..pieces {
                        let by = angle * piece as f32 / pieces as f32;
                        out.push(along(at, turned(incoming, by), half));
                    }
                }
                out.push(along(at, outgoing, half));
            }
        }
    }
    out
}

/// The edges of rings by the rows of the shape they cross, so that how many
/// times the rings wind around a point is counted from one row's edges.
struct Edges {
    top: f32,
    row: f32,
    rows: Vec<Vec<(Point, Point)>>,
}

impl Edges {
    fn new(rings: &[Vec<Point>]) -> Self {
        let count: usize = rings.iter().map(Vec::len).sum();
        let (mut top, mut bottom) = (f32::INFINITY, f32::NEG_INFINITY);
        for point in rings.iter().flatten() {
            top = top.min(point.y);
            bottom = bottom.max(point.y);
        }
        let rows = ((count as f32).sqrt().ceil() as usize).max(1);
        let row = ((bottom - top) / rows as f32).max(f32::MIN_POSITIVE);
        let mut edges = Self {
            top,
            row,
            rows: vec![Vec::new(); rows],
        };
        for ring in rings {
            for (index, a) in ring.iter().enumerate() {
                let b = ring[(index + 1) % ring.len()];
                let (first, last) = (edges.index(a.y.min(b.y)), edges.index(a.y.max(b.y)));
                for row in &mut edges.rows[first..=last] {
                    row.push((*a, b));
                }
            }
        }
        edges
    }

    fn index(&self, y: f32) -> usize {
        (((y - self.top) / self.row).max(0.0) as usize).min(self.rows.len() - 1)
    }

    /// Whether `rule` takes the point as inside.
    fn covers(&self, point: Point, rule: FillRule) -> bool {
        let winding = self.winding(point);
        match rule {
            FillRule::EvenOdd => winding % 2 != 0,
            _ => winding != 0,
        }
    }

    /// Counter-clockwise positive, in the sense `left` points inward.
    fn winding(&self, point: Point) -> i32 {
        let mut winding = 0;
        for (a, b) in &self.rows[self.index(point.y)] {
            let side = (b.x - a.x) * (point.y - a.y) - (point.x - a.x) * (b.y - a.y);
            if a.y <= point.y {
                if b.y > point.y && side > 0.0 {
                    winding += 1;
                }
            } else if b.y <= point.y && side < 0.0 {
                winding -= 1;
            }
        }
        winding
    }
}

/// Fills closed rings by `rule`. Each edge with the shape on one
/// side only gets the fringe on the other.
fn fill_rings(rings: &[Vec<Point>], rule: FillRule, pixel: f32, out: &mut Vec<Corner>) {
    let half = pixel / 2.0;
    let edges = Edges::new(rings);
    let mut inner_path = LyonPath::builder();
    let mut sides = Vec::new();
    let mut inner = Vec::new();
    let mut outer = Vec::new();
    let mut fringe = Vec::new();
    for ring in rings.iter().filter(|ring| ring.len() >= 3) {
        let count = ring.len();
        // Along the left normal: 1 when the outside is that way, -1 when
        // it is the other, 0 when the shape is on both sides.
        sides.clear();
        sides.extend((0..count).map(|index| {
            let (a, b) = (ring[index], ring[(index + 1) % count]);
            let middle = Point {
                x: (a.x + b.x) / 2.0,
                y: (a.y + b.y) / 2.0,
            };
            let normal = left(a, b);
            let filled = |by: f32| edges.covers(along(middle, normal, by), rule);
            match (filled(half / 2.0), filled(-half / 2.0)) {
                (false, true) => 1.0,
                (true, false) => -1.0,
                _ => 0.0,
            }
        }));
        inner.clear();
        outer.clear();
        for (index, point) in ring.iter().enumerate() {
            let before = (index + count - 1) % count;
            let (incoming, outgoing) = (
                left(ring[before], *point),
                left(*point, ring[(index + 1) % count]),
            );
            let sum = Point {
                x: incoming.x + outgoing.x,
                y: incoming.y + outgoing.y,
            };
            let cosine = incoming.x * outgoing.x + incoming.y * outgoing.y;
            // Half a pixel from both edges, limited at sharp corners.
            let reach = half / (1.0 + cosine).max(0.25);
            let side = if sides[before] == sides[index] {
                sides[index]
            } else {
                sides[before] + sides[index]
            };
            inner.push(along(*point, sum, -reach * side));
            outer.push(along(*point, sum, reach * side));
        }
        inner_path.begin(math::point(inner[0].x, inner[0].y));
        for point in &inner[1..] {
            inner_path.line_to(math::point(point.x, point.y));
        }
        inner_path.end(true);
        for at in 0..count {
            if sides[at] == 0.0 {
                continue;
            }
            let next = (at + 1) % count;
            fringe.extend([
                (inner[at], 1.0),
                (outer[at], 0.0),
                (outer[next], 0.0),
                (inner[at], 1.0),
                (outer[next], 0.0),
                (inner[next], 1.0),
            ]);
        }
    }
    let mut buffers: VertexBuffers<Point, u32> = VertexBuffers::new();
    let filled = FillTessellator::new().tessellate_path(
        &inner_path.build(),
        &FillOptions::default().with_fill_rule(match rule {
            FillRule::EvenOdd => lyon_tessellation::FillRule::EvenOdd,
            _ => lyon_tessellation::FillRule::NonZero,
        }),
        &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex| {
            let at = vertex.position();
            Point { x: at.x, y: at.y }
        }),
    );
    if filled.is_ok() {
        out.extend(
            buffers
                .indices
                .iter()
                .map(|index| (buffers.vertices[*index as usize], 1.0)),
        );
    }
    out.extend(fringe);
}

#[cfg(test)]
mod tests {
    use super::*;
    use glassmere_scene::Dash;

    fn square(size: f32) -> Path {
        let mut path = Path::new();
        path.move_to(Point { x: 0.0, y: 0.0 })
            .line_to(Point { x: size, y: 0.0 })
            .line_to(Point { x: size, y: size })
            .line_to(Point { x: 0.0, y: size })
            .close();
        path
    }

    fn everywhere() -> Rect {
        Rect {
            x: -1.0e6,
            y: -1.0e6,
            width: 2.0e6,
            height: 2.0e6,
        }
    }

    fn solid(width: f32) -> Stroke {
        Stroke {
            width,
            ..Stroke::default()
        }
    }

    fn stroked(path: &Path, stroke: &Stroke) -> f32 {
        let mut corners = Vec::new();
        super::stroke(path, stroke, everywhere(), 1.0, &mut corners);
        covered(&corners)
    }

    fn line(length: f32) -> Path {
        let mut line = Path::new();
        line.move_to(Point { x: 0.0, y: 0.0 })
            .line_to(Point { x: length, y: 0.0 });
        line
    }

    /// The area the triangles cover, each weighed by its mean coverage.
    fn covered(corners: &[Corner]) -> f32 {
        corners
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[(a, ca), (b, cb), (c, cc)]| {
                let area = ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() / 2.0;
                area * (ca + cb + cc) / 3.0
            })
            .sum()
    }

    #[test]
    fn a_filled_square_covers_its_area_either_way_round() {
        for path in [square(10.0), {
            let mut back = Path::new();
            back.move_to(Point { x: 0.0, y: 0.0 })
                .line_to(Point { x: 0.0, y: 10.0 })
                .line_to(Point { x: 10.0, y: 10.0 })
                .line_to(Point { x: 10.0, y: 0.0 })
                .close();
            back
        }] {
            let mut corners = Vec::new();
            fill(&path, FillRule::NonZero, 1.0, &mut corners);
            // The fringe's corners overlap a little, outside the square.
            let area = covered(&corners);
            assert!((area - 100.0).abs() < 0.5, "{area}");
        }
    }

    #[test]
    fn a_ring_wound_the_other_way_inside_cuts_a_hole() {
        let mut path = square(10.0);
        path.move_to(Point { x: 3.0, y: 3.0 })
            .line_to(Point { x: 3.0, y: 7.0 })
            .line_to(Point { x: 7.0, y: 7.0 })
            .line_to(Point { x: 7.0, y: 3.0 })
            .close();
        let mut corners = Vec::new();
        fill(&path, FillRule::NonZero, 1.0, &mut corners);
        let area = covered(&corners);
        assert!((area - 84.0).abs() < 0.5, "{area}");
    }

    #[test]
    fn a_stroke_covers_its_width_along_its_length() {
        let mut line = Path::new();
        line.move_to(Point { x: 0.0, y: 0.0 })
            .line_to(Point { x: 20.0, y: 0.0 });
        let mut corners = Vec::new();
        assert_eq!(
            stroke(&line, &solid(4.0), everywhere(), 1.0, &mut corners),
            1.0
        );
        let area = covered(&corners);
        assert!((area - 80.0).abs() < 0.5, "{area}");

        // A closed square's outline is a band, with mitred corners.
        let mut corners = Vec::new();
        stroke(&square(10.0), &solid(2.0), everywhere(), 1.0, &mut corners);
        let area = covered(&corners);
        assert!((area - (144.0 - 64.0)).abs() < 1.0, "{area}");

        // Thinner than a pixel: a pixel wide and fainter.
        let mut corners = Vec::new();
        assert_eq!(
            stroke(&line, &solid(0.5), everywhere(), 1.0, &mut corners),
            0.5
        );
    }

    #[test]
    fn caps_extend_an_open_stroke_by_their_shape() {
        let area = |cap| stroked(&line(20.0), &Stroke { cap, ..solid(4.0) });
        assert!((area(Cap::Butt) - 80.0).abs() < 0.5);
        assert!((area(Cap::Square) - 96.0).abs() < 0.5);
        // Two half circles of radius 2.
        let round = area(Cap::Round);
        assert!(
            (round - (80.0 + 4.0 * std::f32::consts::PI)).abs() < 1.2,
            "{round}"
        );
    }

    #[test]
    fn joins_shape_the_outer_corner() {
        let area = |join| stroked(&square(10.0), &Stroke { join, ..solid(2.0) });
        // Each outer corner is a unit square mitred, half of one bevelled
        // and a quarter circle rounded.
        assert!((area(Join::Miter) - 80.0).abs() < 1.0);
        assert!((area(Join::Bevel) - 78.0).abs() < 1.0);
        let round = area(Join::Round);
        assert!(
            (round - (80.0 - 4.0 * (1.0 - std::f32::consts::FRAC_PI_4))).abs() < 1.0,
            "{round}"
        );
    }

    #[test]
    fn a_miter_past_its_limit_is_bevelled() {
        let mut spike = Path::new();
        spike
            .move_to(Point { x: 0.0, y: 0.0 })
            .line_to(Point { x: 20.0, y: 0.0 })
            .line_to(Point { x: 0.0, y: 2.0 });
        let area = |miter_limit| {
            stroked(
                &spike,
                &Stroke {
                    miter_limit,
                    ..solid(2.0)
                },
            )
        };
        assert!(area(100.0) > area(4.0) + 5.0);
    }

    #[test]
    fn a_dash_draws_the_pattern_along_the_stroke() {
        let dashed = |lengths: Vec<f32>, offset| {
            stroked(
                &line(20.0),
                &Stroke {
                    dash: Some(Dash { lengths, offset }),
                    ..solid(2.0)
                },
            )
        };
        // Three dashes of four.
        // The fringes of the ends overlap a little.
        assert!((dashed(vec![4.0, 4.0], 0.0) - 24.0).abs() < 1.5);
        // Starting two in: 0 to 2, 6 to 10 and 14 to 18.
        assert!((dashed(vec![4.0, 4.0], 2.0) - 20.0).abs() < 1.5);
        // An odd pattern repeats, and a bad one is solid.
        assert!((dashed(vec![4.0], 0.0) - 24.0).abs() < 1.5);
        assert!((dashed(vec![-1.0, 4.0], 0.0) - 40.0).abs() < 0.5);
    }

    #[test]
    fn a_closed_dash_joins_where_the_ring_starts() {
        let lines = dashed(
            polylines(&square(10.0), 1.0),
            &[15.0, 5.0],
            0.0,
            Cap::Butt,
            1.0,
        );
        // 40 long: dashes 0 to 15 and 20 to 35, and the 5 left over runs
        // on into the first.
        let lengths: Vec<usize> = lines.iter().map(|(points, _)| points.len()).collect();
        assert_eq!(lines.len(), 2, "{lengths:?}");
        assert!(lines.iter().all(|(_, closed)| !closed));
    }

    #[test]
    fn a_dash_of_no_length_is_a_dot_with_round_caps() {
        let dots = Stroke {
            cap: Cap::Round,
            dash: Some(Dash::new([0.0, 5.0])),
            ..solid(2.0)
        };
        // Four dots on a line 20 long, each a circle of radius 1.
        let area = stroked(&line(20.0), &dots);
        assert!((area - 4.0 * std::f32::consts::PI).abs() < 1.0, "{area}");
    }

    #[test]
    fn even_odd_cuts_a_hole_whichever_way_the_inner_ring_winds() {
        let mut path = square(10.0);
        path.move_to(Point { x: 2.0, y: 2.0 })
            .line_to(Point { x: 8.0, y: 2.0 })
            .line_to(Point { x: 8.0, y: 8.0 })
            .line_to(Point { x: 2.0, y: 8.0 })
            .close();
        let area = |rule| {
            let mut corners = Vec::new();
            fill(&path, rule, 1.0, &mut corners);
            covered(&corners)
        };
        assert!((area(FillRule::NonZero) - 100.0).abs() < 0.5);
        assert!((area(FillRule::EvenOdd) - 64.0).abs() < 0.5);
    }

    fn dashes(lengths: Vec<f32>, offset: f32, cap: Cap) -> Stroke {
        Stroke {
            cap,
            dash: Some(Dash { lengths, offset }),
            ..solid(2.0)
        }
    }

    #[test]
    fn a_point_that_is_not_a_number_ends_the_dashing_instead_of_hanging() {
        let mut path = Path::new();
        path.move_to(Point { x: 0.0, y: 0.0 })
            .line_to(Point {
                x: f32::NAN,
                y: 5.0,
            })
            .line_to(Point { x: 20.0, y: 0.0 });
        let mut corners = Vec::new();
        let dashed = dashes(vec![2.0, 2.0], 0.0, Cap::Butt);
        super::stroke(&path, &dashed, everywhere(), 1.0, &mut corners);
        // Finite points, a length that is not.
        let overflowing = vec![(
            vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 3.0e38, y: 0.0 },
                Point { x: -3.0e38, y: 0.0 },
            ],
            false,
        )];
        let kept = super::dashed(overflowing, &[2.0, 2.0], 0.0, Cap::Butt, 1.0);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn more_dashes_than_the_most_draw_solid_and_quickly() {
        let started = std::time::Instant::now();
        let mut corners = Vec::new();
        let dashed = dashes(vec![1.0, 1.0], 0.0, Cap::Butt);
        super::stroke(&line(20_000.0), &dashed, everywhere(), 1.0, &mut corners);
        assert!(corners.len() < 100, "{}", corners.len());
        let area = covered(&corners);
        assert!((area - 40_000.0).abs() < 10.0, "{area}");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn the_most_dashes_still_tessellate_in_reasonable_time() {
        let started = std::time::Instant::now();
        let mut corners = Vec::new();
        let dashed = dashes(vec![1.0, 1.0], 0.0, Cap::Butt);
        let long = line((Dash::MAX_COUNT * 2) as f32);
        super::stroke(&long, &dashed, everywhere(), 1.0, &mut corners);
        assert!(corners.len() > 1000);
        let elapsed = started.elapsed();
        assert!(elapsed < std::time::Duration::from_secs(10), "{elapsed:?}");
    }

    #[test]
    fn dashes_out_of_view_are_not_tessellated() {
        let view = Rect {
            x: 0.0,
            y: -5.0,
            width: 100.0,
            height: 10.0,
        };
        let dashed = dashes(vec![1.0, 1.0], 0.0, Cap::Butt);
        let mut seen = Vec::new();
        super::stroke(&line(4000.0), &dashed, view, 1.0, &mut seen);
        let mut all = Vec::new();
        super::stroke(&line(4000.0), &dashed, everywhere(), 1.0, &mut all);
        assert!(seen.len() * 10 < all.len());
        assert!(!seen.is_empty());
    }

    #[test]
    fn a_dash_of_no_length_draws_only_where_the_caps_do() {
        let area = |cap| stroked(&line(20.0), &dashes(vec![0.0, 5.0], 0.0, cap));
        assert_eq!(area(Cap::Butt), 0.0);
        assert!(area(Cap::Round) > 10.0);
        assert!(area(Cap::Square) > 10.0);
    }

    #[test]
    fn dashes_shorter_than_a_pixel_keep_their_share_of_ink() {
        // Four of every ten lengths drawn, two wide.
        let area = stroked(&line(10.0), &dashes(vec![0.4, 0.6], 0.0, Cap::Butt));
        // The fringes of the stretched dashes overlap a little; the renderers
        // are compared by pixels in compare_gpu.
        assert!((area - 8.0).abs() < 2.0, "{area}");
    }

    #[test]
    fn a_stroke_that_goes_nowhere_is_a_dot_where_the_caps_draw_one() {
        let mut point = Path::new();
        point
            .move_to(Point { x: 5.0, y: 5.0 })
            .line_to(Point { x: 5.0, y: 5.0 });
        let capped = |cap| Stroke { cap, ..solid(4.0) };
        assert_eq!(stroked(&point, &capped(Cap::Butt)), 0.0);
        let round = stroked(&point, &capped(Cap::Round));
        assert!((round - 4.0 * std::f32::consts::PI).abs() < 1.5, "{round}");
        let square = stroked(&point, &capped(Cap::Square));
        assert!((square - 16.0).abs() < 1.0, "{square}");
        let mut lone = Path::new();
        lone.move_to(Point { x: 5.0, y: 5.0 });
        assert_eq!(stroked(&lone, &capped(Cap::Round)), 0.0);
        let mut closed = Path::new();
        closed.move_to(Point { x: 5.0, y: 5.0 }).close();
        assert!(stroked(&closed, &capped(Cap::Round)) > 10.0);
    }

    #[test]
    fn a_closed_line_is_joined_at_both_ends_and_not_capped() {
        let mut path = line(10.0);
        path.close();
        let area = |join, cap| {
            stroked(
                &path,
                &Stroke {
                    join,
                    cap,
                    ..solid(2.0)
                },
            )
        };
        // Round joins make a capsule whatever the caps say.
        let capsule = 20.0 + std::f32::consts::PI;
        assert!((area(Join::Round, Cap::Butt) - capsule).abs() < 1.0);
        assert!((area(Join::Round, Cap::Square) - capsule).abs() < 1.0);
        // Bevelled, there is nothing past the ends.
        let bevel = area(Join::Bevel, Cap::Round);
        assert!((bevel - 20.0).abs() < 1.5, "{bevel}");
    }

    #[test]
    fn an_arc_around_a_huge_radius_is_a_bounded_number_of_pieces() {
        let pi = std::f32::consts::PI;
        assert_eq!(arc_pieces(1.0e30, pi, 1.0), MAX_ARC_PIECES);
        assert_eq!(arc_pieces(f32::NAN, pi, 1.0), 1);
    }

    #[test]
    fn the_reach_of_a_stroke_is_always_a_number() {
        let reach = |width, miter_limit| {
            super::reach(&Stroke {
                width,
                miter_limit,
                ..Stroke::default()
            })
        };
        assert_eq!(reach(0.0, f32::INFINITY), 0.0);
        assert_eq!(reach(2.0, f32::NAN), 2.0);
        assert_eq!(reach(2.0, 4.0), 8.0);
        assert_eq!(reach(2.0, 0.5), 2.0);
    }

    #[test]
    fn a_dash_offset_wraps_the_pattern_either_way() {
        let area = |offset| stroked(&line(20.0), &dashes(vec![4.0, 4.0], offset, Cap::Butt));
        let base = area(2.0);
        for offset in [-6.0, 10.0, 802.0, -806.0] {
            let wrapped = area(offset);
            assert!((wrapped - base).abs() < 1.0, "{offset}: {wrapped} / {base}");
        }
    }

    #[test]
    fn a_stroke_past_the_most_dashes_is_drawn_solid() {
        let whole = stroked(&line(100.0), &solid(2.0));
        let fine = stroked(&line(100.0), &dashes(vec![0.01, 0.01], 0.0, Cap::Butt));
        assert!((whole - fine).abs() < 0.5);
    }
}
