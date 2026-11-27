//! Arcs, and paths written as SVG path data.

use super::{Path, Segment};
use crate::Point;
use alloc::string::String;
use core::fmt;

/// Why a string of SVG path data was not read: where the trouble starts, as a
/// byte offset into the string, and what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvgPathError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for SvgPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "path data, byte {}: {}", self.offset, self.message)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SvgPathError {}

impl Path {
    /// The point the next segment starts from: where the last one ended, or
    /// the start of the subpath after a `close`. `None` for an empty path.
    pub fn current_point(&self) -> Option<Point> {
        let mut start = None;
        let mut current = None;
        for segment in &self.segments {
            match *segment {
                Segment::Move(to) => {
                    start = Some(to);
                    current = Some(to);
                }
                Segment::Line(to) | Segment::Quad(_, to) | Segment::Cubic(_, _, to) => {
                    current = Some(to);
                }
                Segment::Close => current = start,
            }
        }
        current
    }

    /// An elliptical arc from the current point to `to`, as SVG's `A` command
    /// draws it: `radii` are the ellipse's, `rotation` turns its axes by that
    /// many degrees, `large` picks the longer of the two arcs that fit, and
    /// `sweep` the one that runs clockwise on screen (y down). Radii too small
    /// to reach `to` are scaled up until they do, and a zero radius is a
    /// straight line. The arc is stored as cubic curves, at most a quarter
    /// turn each. On a path with no current point it starts one at `to`.
    pub fn arc_to(
        &mut self,
        radii: (f32, f32),
        rotation: f32,
        large: bool,
        sweep: bool,
        to: Point,
    ) -> &mut Self {
        let Some(from) = self.current_point() else {
            return self.move_to(to);
        };
        let (mut rx, mut ry) = (f64::from(radii.0.abs()), f64::from(radii.1.abs()));
        if from == to {
            return self;
        }
        if rx == 0.0 || ry == 0.0 || !rx.is_finite() || !ry.is_finite() {
            return self.line_to(to);
        }
        let phi = f64::from(rotation).to_radians();
        let (sin, cos) = (libm::sin(phi), libm::cos(phi));
        let (dx, dy) = (
            (f64::from(from.x) - f64::from(to.x)) / 2.0,
            (f64::from(from.y) - f64::from(to.y)) / 2.0,
        );
        // The start point in the ellipse's own, unrotated frame.
        let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
        if lambda > 1.0 {
            let scale = libm::sqrt(lambda);
            rx *= scale;
            ry *= scale;
        }
        let (rx2, ry2) = (rx * rx, ry * ry);
        let numerator = rx2 * ry2 - rx2 * y1 * y1 - ry2 * x1 * x1;
        let denominator = rx2 * y1 * y1 + ry2 * x1 * x1;
        let sign = if large == sweep { -1.0 } else { 1.0 };
        let coefficient = if denominator == 0.0 {
            0.0
        } else {
            sign * libm::sqrt((numerator / denominator).max(0.0))
        };
        let (cx_, cy_) = (coefficient * rx * y1 / ry, -coefficient * ry * x1 / rx);
        let centre = (
            cos * cx_ - sin * cy_ + (f64::from(from.x) + f64::from(to.x)) / 2.0,
            sin * cx_ + cos * cy_ + (f64::from(from.y) + f64::from(to.y)) / 2.0,
        );
        let angle =
            |ux: f64, uy: f64, vx: f64, vy: f64| libm::atan2(ux * vy - uy * vx, ux * vx + uy * vy);
        let start = angle(1.0, 0.0, (x1 - cx_) / rx, (y1 - cy_) / ry);
        let mut span = angle(
            (x1 - cx_) / rx,
            (y1 - cy_) / ry,
            (-x1 - cx_) / rx,
            (-y1 - cy_) / ry,
        );
        let tau = 2.0 * core::f64::consts::PI;
        if !sweep && span > 0.0 {
            span -= tau;
        } else if sweep && span < 0.0 {
            span += tau;
        }
        let pieces = libm::ceil(libm::fabs(span) / (core::f64::consts::PI / 2.0) - 1e-9).max(1.0);
        let delta = span / pieces;
        let alpha = 4.0 / 3.0 * libm::tan(delta / 4.0);
        let map = |x: f64, y: f64| Point {
            x: (centre.0 + rx * x * cos - ry * y * sin) as f32,
            y: (centre.1 + rx * x * sin + ry * y * cos) as f32,
        };
        let mut theta = start;
        for index in 0..pieces as usize {
            let next = theta + delta;
            let (s0, c0) = (libm::sin(theta), libm::cos(theta));
            let (s1, c1) = (libm::sin(next), libm::cos(next));
            let end = if index + 1 == pieces as usize {
                to
            } else {
                map(c1, s1)
            };
            self.cubic_to(
                map(c0 - alpha * s0, s0 + alpha * c0),
                map(c1 + alpha * s1, s1 - alpha * c1),
                end,
            );
            theta = next;
        }
        self
    }

    /// A path from SVG path data, the string of an icon's `d` attribute:
    /// `M`, `L`, `H`, `V`, `C`, `S`, `Q`, `T`, `A` and `Z`, each in capitals
    /// for absolute and in lower case for relative coordinates, with
    /// repeated coordinates read as repeated commands and the arc flags
    /// allowed to touch the numbers after them (`a1 1 0 011 1`). Nothing is
    /// drawn or fetched, and no dependency is needed.
    ///
    /// ```
    /// use glassmere_scene::Path;
    ///
    /// let check = Path::from_svg_data("M4 12l5 5L20 6").unwrap();
    /// assert_eq!(check.bounds().width, 16.0);
    /// ```
    pub fn from_svg_data(data: &str) -> Result<Self, SvgPathError> {
        Parser::new(data).run()
    }
}

struct Parser<'a> {
    data: &'a str,
    at: usize,
    path: Path,
    current: Point,
    start: Point,
    /// The last cubic's second control point and the last quadratic's
    /// control point, which `S` and `T` reflect; `None` after anything else.
    cubic: Option<Point>,
    quad: Option<Point>,
}

impl<'a> Parser<'a> {
    fn new(data: &'a str) -> Self {
        let origin = Point { x: 0.0, y: 0.0 };
        Self {
            data,
            at: 0,
            path: Path::new(),
            current: origin,
            start: origin,
            cubic: None,
            quad: None,
        }
    }

    fn error<T>(&self, message: &str) -> Result<T, SvgPathError> {
        Err(SvgPathError {
            offset: self.at,
            message: message.into(),
        })
    }

    fn bytes(&self) -> &'a [u8] {
        self.data.as_bytes()
    }

    fn skip_separators(&mut self) {
        while let Some(&byte) = self.bytes().get(self.at) {
            if byte.is_ascii_whitespace() || byte == b',' {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn at_number(&mut self) -> bool {
        self.skip_separators();
        matches!(
            self.bytes().get(self.at),
            Some(b'0'..=b'9' | b'.' | b'-' | b'+')
        )
    }

    fn number(&mut self) -> Result<f32, SvgPathError> {
        self.skip_separators();
        let bytes = self.bytes();
        let start = self.at;
        let mut at = start;
        if matches!(bytes.get(at), Some(b'-' | b'+')) {
            at += 1;
        }
        let digits = |at: &mut usize| {
            let from = *at;
            while matches!(bytes.get(*at), Some(b'0'..=b'9')) {
                *at += 1;
            }
            *at - from
        };
        let mut count = digits(&mut at);
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            count += digits(&mut at);
        }
        if count == 0 {
            return self.error("a number is expected");
        }
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            let mut exponent = at + 1;
            if matches!(bytes.get(exponent), Some(b'-' | b'+')) {
                exponent += 1;
            }
            if digits(&mut exponent) > 0 {
                at = exponent;
            }
        }
        let value: f32 = match self.data[start..at].parse() {
            Ok(value) => value,
            Err(_) => return self.error("a number is expected"),
        };
        if !value.is_finite() {
            return self.error("a number is too large");
        }
        self.at = at;
        Ok(value)
    }

    /// An arc flag: one digit, 0 or 1, with nothing needed after it.
    fn flag(&mut self) -> Result<bool, SvgPathError> {
        self.skip_separators();
        match self.bytes().get(self.at) {
            Some(b'0') => {
                self.at += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.at += 1;
                Ok(true)
            }
            _ => self.error("an arc flag, 0 or 1, is expected"),
        }
    }

    fn point(&mut self, relative: bool) -> Result<Point, SvgPathError> {
        let (x, y) = (self.number()?, self.number()?);
        Ok(if relative {
            Point {
                x: self.current.x + x,
                y: self.current.y + y,
            }
        } else {
            Point { x, y }
        })
    }

    fn run(mut self) -> Result<Path, SvgPathError> {
        let mut command: Option<u8> = None;
        loop {
            self.skip_separators();
            let Some(&byte) = self.bytes().get(self.at) else {
                break;
            };
            if byte.is_ascii_alphabetic() {
                self.at += 1;
                command = Some(byte);
                if matches!(byte, b'Z' | b'z') {
                    self.path.close();
                    self.current = self.start;
                    self.cubic = None;
                    self.quad = None;
                    // What follows `Z` without a command is an error, not a
                    // repeat of it.
                    command = None;
                    continue;
                }
                if !"MmLlHhVvCcSsQqTtAa".contains(byte as char) {
                    self.at -= 1;
                    return self.error("an unknown command");
                }
            }
            let Some(letter) = command else {
                return self.error("a command is expected");
            };
            if self.path.segments.is_empty() && !matches!(letter, b'M' | b'm') {
                // At the command, not after it.
                self.at = self
                    .at
                    .saturating_sub(usize::from(byte.is_ascii_alphabetic()));
                return self.error("path data starts with a move");
            }
            self.command(letter)?;
            // A move that is followed by more coordinates draws lines.
            command = Some(match letter {
                b'M' => b'L',
                b'm' => b'l',
                other => other,
            });
            if !self.at_number()
                && !matches!(self.bytes().get(self.at), Some(b'A'..=b'Z' | b'a'..=b'z'))
            {
                self.skip_separators();
                if self.at < self.bytes().len() {
                    return self.error("unexpected character");
                }
            }
        }
        Ok(self.path)
    }

    fn command(&mut self, letter: u8) -> Result<(), SvgPathError> {
        let relative = letter.is_ascii_lowercase();
        let (mut cubic, mut quad) = (None, None);
        match letter.to_ascii_uppercase() {
            b'M' => {
                let to = self.point(relative)?;
                self.path.move_to(to);
                self.start = to;
                self.current = to;
            }
            b'L' => {
                let to = self.point(relative)?;
                self.path.line_to(to);
                self.current = to;
            }
            b'H' => {
                let x = self.number()?;
                let to = Point {
                    x: if relative { self.current.x + x } else { x },
                    y: self.current.y,
                };
                self.path.line_to(to);
                self.current = to;
            }
            b'V' => {
                let y = self.number()?;
                let to = Point {
                    x: self.current.x,
                    y: if relative { self.current.y + y } else { y },
                };
                self.path.line_to(to);
                self.current = to;
            }
            b'C' => {
                let (a, b, to) = (
                    self.point(relative)?,
                    self.point(relative)?,
                    self.point(relative)?,
                );
                self.path.cubic_to(a, b, to);
                self.current = to;
                cubic = Some(b);
            }
            b'S' => {
                let reflected = self.cubic.map_or(self.current, |previous| Point {
                    x: 2.0 * self.current.x - previous.x,
                    y: 2.0 * self.current.y - previous.y,
                });
                let (b, to) = (self.point(relative)?, self.point(relative)?);
                self.path.cubic_to(reflected, b, to);
                self.current = to;
                cubic = Some(b);
            }
            b'Q' => {
                let (control, to) = (self.point(relative)?, self.point(relative)?);
                self.path.quad_to(control, to);
                self.current = to;
                quad = Some(control);
            }
            b'T' => {
                let control = self.quad.map_or(self.current, |previous| Point {
                    x: 2.0 * self.current.x - previous.x,
                    y: 2.0 * self.current.y - previous.y,
                });
                let to = self.point(relative)?;
                self.path.quad_to(control, to);
                self.current = to;
                quad = Some(control);
            }
            b'A' => {
                let (rx, ry, rotation) = (self.number()?, self.number()?, self.number()?);
                let (large, sweep) = (self.flag()?, self.flag()?);
                let to = self.point(relative)?;
                self.path.arc_to((rx, ry), rotation, large, sweep, to);
                self.current = to;
            }
            _ => return self.error("an unknown command"),
        }
        self.cubic = cubic;
        self.quad = quad;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn p(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    fn near(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 0.02 && (a.y - b.y).abs() < 0.02
    }

    fn cubic_at(from: Point, c: &Segment, t: f32) -> Point {
        let Segment::Cubic(a, b, to) = *c else {
            panic!("a cubic is expected");
        };
        let u = 1.0 - t;
        let at = |f: f32, a: f32, b: f32, to: f32| {
            u * u * u * f + 3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t * to
        };
        p(at(from.x, a.x, b.x, to.x), at(from.y, a.y, b.y, to.y))
    }

    #[test]
    fn a_quarter_arc_is_one_cubic_on_the_circle() {
        let mut path = Path::new();
        path.move_to(p(10.0, 0.0))
            .arc_to((10.0, 10.0), 0.0, false, true, p(0.0, 10.0));
        assert_eq!(path.segments.len(), 2);
        // Clockwise on screen from (10, 0): through the middle of the corner.
        let middle = cubic_at(p(10.0, 0.0), &path.segments[1], 0.5);
        assert!(near(middle, p(7.0711, 7.0711)), "{middle:?}");
        assert_eq!(path.current_point(), Some(p(0.0, 10.0)));
    }

    #[test]
    fn the_sweep_and_the_large_flag_pick_which_of_the_four_arcs() {
        let side = |large, sweep| {
            let mut path = Path::new();
            path.move_to(p(0.0, 0.0))
                .arc_to((10.0, 10.0), 0.0, large, sweep, p(10.0, 10.0));
            path.bounds()
        };
        // The short arcs bulge to opposite sides of the chord.
        let (short_cw, short_ccw) = (side(false, true), side(false, false));
        assert!(short_cw.x + short_cw.width <= 10.0 + 1e-3 && short_cw.y >= -1e-3);
        assert!(short_ccw.x >= -1e-3 && short_ccw.y + short_ccw.height <= 10.0 + 1e-3);
        // The long arcs go around the far side of the circle.
        let long = side(true, true);
        assert!(long.y < -1.0 || long.x + long.width > 11.0, "{long:?}");
    }

    #[test]
    fn radii_too_small_are_scaled_and_a_zero_radius_is_a_line() {
        let mut path = Path::new();
        path.move_to(p(0.0, 0.0))
            .arc_to((1.0, 1.0), 0.0, false, true, p(20.0, 0.0));
        // The half circle the chord forces: 10 high.
        assert!(
            (path.bounds().height - 10.0).abs() < 0.05,
            "{:?}",
            path.bounds()
        );
        let mut line = Path::new();
        line.move_to(p(0.0, 0.0))
            .arc_to((0.0, 5.0), 0.0, false, true, p(4.0, 4.0));
        assert_eq!(line.segments[1], Segment::Line(p(4.0, 4.0)));
    }

    #[test]
    fn a_circle_of_two_arcs_comes_back_to_where_it_began() {
        let mut path = Path::new();
        path.move_to(p(0.0, 5.0))
            .arc_to((5.0, 5.0), 0.0, true, true, p(10.0, 5.0))
            .arc_to((5.0, 5.0), 0.0, true, true, p(0.0, 5.0))
            .close();
        let bounds = path.bounds();
        assert!((bounds.width - 10.0).abs() < 0.05 && (bounds.height - 10.0).abs() < 0.05);
    }

    #[test]
    fn absolute_relative_and_repeated_commands() {
        let path = Path::from_svg_data("M10 10 20 10 l0 5,-5 0 z").unwrap();
        assert_eq!(
            path.segments,
            [
                Segment::Move(p(10.0, 10.0)),
                Segment::Line(p(20.0, 10.0)),
                Segment::Line(p(20.0, 15.0)),
                Segment::Line(p(15.0, 15.0)),
                Segment::Close,
            ]
        );
        let path = Path::from_svg_data("m1 1h2v3H0V0").unwrap();
        assert_eq!(path.current_point(), Some(p(0.0, 0.0)));
        assert_eq!(path.segments[1], Segment::Line(p(3.0, 1.0)));
        assert_eq!(path.segments[2], Segment::Line(p(3.0, 4.0)));
    }

    #[test]
    fn smooth_curves_reflect_the_previous_control_point() {
        let path = Path::from_svg_data("M0 0C0 4 4 4 4 0S8 -4 8 0Q10 4 12 0T16 0").unwrap();
        assert_eq!(
            path.segments[2],
            Segment::Cubic(p(4.0, -4.0), p(8.0, -4.0), p(8.0, 0.0))
        );
        assert_eq!(path.segments[4], Segment::Quad(p(14.0, -4.0), p(16.0, 0.0)));
    }

    #[test]
    fn numbers_may_touch_and_flags_need_no_separator() {
        let path = Path::from_svg_data("M.5.5-1-1 1e1-2E0").unwrap();
        assert_eq!(path.segments[0], Segment::Move(p(0.5, 0.5)));
        assert_eq!(path.segments[1], Segment::Line(p(-1.0, -1.0)));
        assert_eq!(path.segments[2], Segment::Line(p(10.0, -2.0)));
        // `a2 2 0 011 2` is the flags 0 and 1, then 1 and 2.
        let arc = Path::from_svg_data("M0 0a2 2 0 011 2").unwrap();
        assert_eq!(arc.current_point(), Some(p(1.0, 2.0)));
    }

    #[test]
    fn a_real_icon_reads_and_has_the_size_it_draws() {
        // A circle with a tick, the way an icon set writes one.
        let icon = Path::from_svg_data(
            "M12 2a10 10 0 100 20 10 10 0 000-20zm-2 14.5-4.5-4.5 1.4-1.4 3.1 3.1 6.6-6.6 1.4 1.4z",
        )
        .unwrap();
        let bounds = icon.bounds();
        assert!((bounds.width - 20.0).abs() < 0.1 && (bounds.height - 20.0).abs() < 0.1);
    }

    #[test]
    fn what_is_not_path_data_says_where() {
        let bad = |data: &str| Path::from_svg_data(data).unwrap_err();
        assert_eq!(bad("L1 1").offset, 0);
        assert_eq!(bad("M1 1 X").offset, 5);
        assert_eq!(bad("M1").offset, 2);
        assert_eq!(bad("M0 0 a1 1 0 2 0 1 1").offset, 12);
        assert_eq!(bad("M0 0 z 1 1").offset, 7);
        assert!(bad("M1 1 $").to_string().contains("byte"));
        assert_eq!(Path::from_svg_data("").unwrap(), Path::new());
    }
}
