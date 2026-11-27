//! A path cut into dashes.

use super::{Path, Segment};
use crate::Point;
use alloc::vec::Vec;

/// A subpath as the straight lines its curves are flattened to.
struct Line {
    points: Vec<Point>,
}

fn distance(a: Point, b: Point) -> f32 {
    libm::sqrtf((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y))
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

/// How many straight pieces a curve whose control polygon is `length` long
/// is cut into: enough that a piece is a couple of pixels.
fn pieces(length: f32) -> usize {
    (libm::ceilf(length / 2.0) as usize).clamp(4, 64)
}

fn flatten(path: &Path) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut start = Point { x: 0.0, y: 0.0 };
    for segment in &path.segments {
        match *segment {
            Segment::Move(to) => {
                start = to;
                lines.push(Line {
                    points: alloc::vec![to],
                });
            }
            Segment::Line(to) => {
                if let Some(line) = lines.last_mut() {
                    line.points.push(to);
                }
            }
            Segment::Quad(control, to) => {
                if let Some(line) = lines.last_mut() {
                    let from = *line.points.last().unwrap_or(&start);
                    let count = pieces(distance(from, control) + distance(control, to));
                    for step in 1..=count {
                        let t = step as f32 / count as f32;
                        let (a, b) = (lerp(from, control, t), lerp(control, to, t));
                        line.points.push(lerp(a, b, t));
                    }
                }
            }
            Segment::Cubic(first, second, to) => {
                if let Some(line) = lines.last_mut() {
                    let from = *line.points.last().unwrap_or(&start);
                    let count = pieces(
                        distance(from, first) + distance(first, second) + distance(second, to),
                    );
                    for step in 1..=count {
                        let t = step as f32 / count as f32;
                        let (a, b, c) = (
                            lerp(from, first, t),
                            lerp(first, second, t),
                            lerp(second, to, t),
                        );
                        line.points.push(lerp(lerp(a, b, t), lerp(b, c, t), t));
                    }
                }
            }
            Segment::Close => {
                if let Some(line) = lines.last_mut() {
                    line.points.push(start);
                }
                // What follows a close and has no move of its own starts
                // from where the subpath did.
                lines.push(Line {
                    points: alloc::vec![start],
                });
            }
        }
    }
    lines
}

impl Path {
    /// The path cut into dashes, as the outline of a stroke with a dash
    /// pattern would be: `dashes` alternates the length of a dash and of the
    /// gap after it (an odd number of entries is read twice over, as in SVG's
    /// `stroke-dasharray`), and `phase` is how far into the pattern the path
    /// starts. Each subpath starts the pattern again. Curves are cut as the
    /// straight pieces they are drawn from, so a dash that turns a corner
    /// keeps the corner.
    ///
    /// The result is a path of open subpaths to stroke, with the stroke's
    /// own ends: use it with a `Stroke` and no fill. A pattern that is empty,
    /// has a negative or non-finite length, or adds up to nothing leaves the
    /// path as it is.
    ///
    /// ```
    /// use glassmere_scene::{Path, Point};
    ///
    /// let mut line = Path::new();
    /// line.move_to(Point { x: 0.0, y: 0.0 }).line_to(Point { x: 10.0, y: 0.0 });
    /// // A dash of 2 and a gap of 3: from 0 to 2, and from 5 to 7.
    /// assert_eq!(line.dashed(&[2.0, 3.0], 0.0).segments().len(), 4);
    /// ```
    pub fn dashed(&self, dashes: &[f32], phase: f32) -> Path {
        let valid = !dashes.is_empty()
            && dashes.iter().all(|d| d.is_finite() && *d >= 0.0)
            && dashes.iter().sum::<f32>() > 0.0
            && phase.is_finite();
        if !valid {
            return self.clone();
        }
        let pattern: Vec<f32> = if dashes.len() % 2 == 1 {
            dashes.iter().chain(dashes).copied().collect()
        } else {
            dashes.to_vec()
        };
        let period: f32 = pattern.iter().sum();
        let mut out = Path::new();
        for line in flatten(self) {
            if line.points.len() < 2 {
                continue;
            }
            // Where in the pattern this subpath begins: `index` is the entry
            // and `left` what remains of it. The pattern has a positive
            // length, so this ends.
            let mut at = libm::fmodf(phase, period);
            if at < 0.0 {
                at += period;
            }
            let mut index = 0;
            while at >= pattern[index] {
                at -= pattern[index];
                index = (index + 1) % pattern.len();
            }
            let mut left = pattern[index] - at;
            let mut drawing = index % 2 == 0;
            if drawing {
                out.move_to(line.points[0]);
            }
            for window in line.points.windows(2) {
                let (from, to) = (window[0], window[1]);
                let length = distance(from, to);
                let mut done = 0.0;
                while length - done > left {
                    done += left;
                    let here = lerp(from, to, done / length);
                    if drawing {
                        out.line_to(here);
                    } else {
                        out.move_to(here);
                    }
                    drawing = !drawing;
                    index = (index + 1) % pattern.len();
                    left = pattern[index];
                }
                left -= length - done;
                if drawing {
                    out.line_to(to);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    fn line(length: f32) -> Path {
        let mut path = Path::new();
        path.move_to(p(0.0, 0.0)).line_to(p(length, 0.0));
        path
    }

    /// The `(from, to)` of each dash of a path made of single straight dashes.
    fn dashes(path: &Path) -> Vec<(f32, f32)> {
        let mut found = Vec::new();
        let mut from = None;
        for segment in path.segments() {
            match *segment {
                Segment::Move(at) => from = Some(at.x),
                Segment::Line(at) => {
                    if let Some(start) = from.take() {
                        found.push((start, at.x));
                    }
                }
                _ => {}
            }
        }
        found
    }

    fn near(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(a, b)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3)
    }

    #[test]
    fn a_line_is_cut_where_the_pattern_says() {
        let cut = line(10.0).dashed(&[2.0, 3.0], 0.0);
        assert!(
            near(&dashes(&cut), &[(0.0, 2.0), (5.0, 7.0)]),
            "{:?}",
            dashes(&cut)
        );
    }

    #[test]
    fn the_phase_starts_the_pattern_part_way_in() {
        let cut = line(10.0).dashed(&[2.0, 3.0], 1.0);
        assert!(
            near(&dashes(&cut), &[(0.0, 1.0), (4.0, 6.0), (9.0, 10.0)]),
            "{:?}",
            dashes(&cut)
        );
        // Into the gap: the next dash starts where the gap ends.
        let gap = line(10.0).dashed(&[2.0, 3.0], 3.0);
        assert!(
            near(&dashes(&gap), &[(2.0, 4.0), (7.0, 9.0)]),
            "{:?}",
            dashes(&gap)
        );
        // A negative phase is the same place, a period earlier.
        let back = line(10.0).dashed(&[2.0, 3.0], -2.0);
        assert!(near(&dashes(&back), &dashes(&gap)), "{:?}", dashes(&back));
    }

    #[test]
    fn an_odd_pattern_is_read_twice_over() {
        assert_eq!(
            line(12.0).dashed(&[2.0], 0.0),
            line(12.0).dashed(&[2.0, 2.0], 0.0)
        );
    }

    #[test]
    fn a_pattern_that_cannot_dash_leaves_the_path_alone() {
        let path = line(10.0);
        for bad in [&[][..], &[0.0, 0.0], &[-1.0, 2.0], &[f32::NAN]] {
            assert_eq!(path.dashed(bad, 0.0), path);
        }
        assert_eq!(path.dashed(&[2.0], f32::INFINITY), path);
    }

    #[test]
    fn a_dash_that_turns_a_corner_keeps_the_corner() {
        let mut corner = Path::new();
        corner
            .move_to(p(0.0, 0.0))
            .line_to(p(10.0, 0.0))
            .line_to(p(10.0, 10.0));
        let cut = corner.dashed(&[14.0, 100.0], 0.0);
        assert_eq!(
            cut.segments(),
            &[
                Segment::Move(p(0.0, 0.0)),
                Segment::Line(p(10.0, 0.0)),
                Segment::Line(p(10.0, 4.0)),
            ]
        );
    }

    #[test]
    fn a_curve_dashes_to_about_the_length_it_should() {
        // A circle of radius 20, drawn as two half circles of cubics.
        let k = 0.552_284_8 * 20.0;
        let mut circle = Path::new();
        circle
            .move_to(p(20.0, 0.0))
            .cubic_to(p(20.0 + k, 0.0), p(40.0, 20.0 - k), p(40.0, 20.0))
            .cubic_to(p(40.0, 20.0 + k), p(20.0 + k, 40.0), p(20.0, 40.0))
            .cubic_to(p(20.0 - k, 40.0), p(0.0, 20.0 + k), p(0.0, 20.0))
            .cubic_to(p(0.0, 20.0 - k), p(20.0 - k, 0.0), p(20.0, 0.0))
            .close();
        let cut = circle.dashed(&[4.0, 4.0], 0.0);
        let mut on = 0.0;
        let mut at = None;
        for segment in cut.segments() {
            match *segment {
                Segment::Move(to) => at = Some(to),
                Segment::Line(to) => {
                    if let Some(from) = at {
                        on += distance(from, to);
                    }
                    at = Some(to);
                }
                _ => {}
            }
        }
        let half = core::f32::consts::PI * 20.0;
        assert!((on - half).abs() < half * 0.03, "{on} against {half}");
    }

    #[test]
    fn each_subpath_starts_the_pattern_again() {
        let mut two = Path::new();
        two.move_to(p(0.0, 0.0)).line_to(p(5.0, 0.0));
        two.move_to(p(0.0, 10.0)).line_to(p(5.0, 10.0));
        let cut = two.dashed(&[2.0, 3.0], 0.0);
        let starts: Vec<f32> = cut
            .segments()
            .iter()
            .filter_map(|s| match s {
                Segment::Move(at) => Some(at.y),
                _ => None,
            })
            .collect();
        assert_eq!(starts, [0.0, 10.0]);
    }
}
