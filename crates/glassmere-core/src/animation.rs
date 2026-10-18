//! Animation vocabulary: easing, transitions, visual transforms, enter/exit
//! effects and keyframes. The retained engine lives in `tree::animation_state`.
use glassmere_scene::{Affine, Color, Point, Rect};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// CSS `cubic-bezier(x1, y1, x2, y2)`; x values are clamped to 0..=1.
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// Maps linear progress in 0..=1 to eased progress.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let (x1, y1, x2, y2) = match self {
            Self::Linear => return t,
            Self::EaseIn => (0.42, 0.0, 1.0, 1.0),
            Self::EaseOut => (0.0, 0.0, 0.58, 1.0),
            Self::EaseInOut => (0.42, 0.0, 0.58, 1.0),
            Self::CubicBezier(x1, y1, x2, y2) => (x1.clamp(0.0, 1.0), y1, x2.clamp(0.0, 1.0), y2),
        };
        let bezier = |a: f32, b: f32, s: f32| {
            let inverse = 1.0 - s;
            3.0 * inverse * inverse * s * a + 3.0 * inverse * s * s * b + s * s * s
        };
        // x(s) is monotonic for clamped control points, so bisection converges.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if bezier(x1, x2, middle) < t {
                low = middle;
            } else {
                high = middle;
            }
        }
        bezier(y1, y2, (low + high) / 2.0)
    }
}

/// How a property change is animated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}

impl Transition {
    /// An ease-out transition lasting `milliseconds`.
    pub fn new(milliseconds: u64) -> Self {
        Self {
            duration: Duration::from_millis(milliseconds),
            easing: Easing::EaseOut,
            delay: Duration::ZERO,
        }
    }
    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }
    pub fn delay(mut self, milliseconds: u64) -> Self {
        self.delay = Duration::from_millis(milliseconds);
        self
    }

    /// Eased progress at `elapsed` since the start, and whether it finished.
    pub(crate) fn progress(&self, elapsed: Duration) -> (f32, bool) {
        let active = elapsed.saturating_sub(self.delay);
        if self.duration.is_zero() || active >= self.duration {
            return (1.0, elapsed >= self.delay);
        }
        let linear = active.as_secs_f32() / self.duration.as_secs_f32();
        (self.easing.apply(linear), false)
    }
}

/// A visual-only transform about the node's center. Hit testing, focus and
/// accessibility keep using layout bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translate: (f32, f32),
    pub scale: (f32, f32),
    /// Clockwise degrees.
    pub rotate: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translate: (0.0, 0.0),
        scale: (1.0, 1.0),
        rotate: 0.0,
    };

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            translate: (x, y),
            ..Self::IDENTITY
        }
    }
    pub fn scale(factor: f32) -> Self {
        Self {
            scale: (factor, factor),
            ..Self::IDENTITY
        }
    }
    pub fn rotate(degrees: f32) -> Self {
        Self {
            rotate: degrees,
            ..Self::IDENTITY
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    pub(crate) fn lerp(self, to: Self, t: f32) -> Self {
        Self {
            translate: (
                lerp(self.translate.0, to.translate.0, t),
                lerp(self.translate.1, to.translate.1, t),
            ),
            scale: (
                lerp(self.scale.0, to.scale.0, t),
                lerp(self.scale.1, to.scale.1, t),
            ),
            rotate: lerp(self.rotate, to.rotate, t),
        }
    }

    /// `self` followed by `other`, componentwise (adequate for effects).
    pub(crate) fn combined(self, other: Self) -> Self {
        Self {
            translate: (
                self.translate.0 + other.translate.0,
                self.translate.1 + other.translate.1,
            ),
            scale: (self.scale.0 * other.scale.0, self.scale.1 * other.scale.1),
            rotate: self.rotate + other.rotate,
        }
    }

    /// The scene matrix for a node occupying `bounds`.
    pub(crate) fn to_affine(self, bounds: Rect) -> Affine {
        let center = Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        };
        Affine::scale(self.scale.0, self.scale.1)
            .then(Affine::rotate(self.rotate))
            .about(center)
            .then(Affine::translate(self.translate.0, self.translate.1))
    }
}

/// The state a node animates from when it appears (`Style::enter`) or to when
/// it disappears (`Style::exit`), relative to its resting look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effect {
    pub opacity: f32,
    pub transform: Transform,
    pub transition: Transition,
}

impl Effect {
    pub fn fade(milliseconds: u64) -> Self {
        Self {
            opacity: 0.0,
            transform: Transform::IDENTITY,
            transition: Transition::new(milliseconds),
        }
    }
    /// Fades while moving from (or to) an offset.
    pub fn slide(x: f32, y: f32, milliseconds: u64) -> Self {
        Self {
            transform: Transform::translate(x, y),
            ..Self::fade(milliseconds)
        }
    }
    /// Fades while scaling from (or to) `factor`.
    pub fn zoom(factor: f32, milliseconds: u64) -> Self {
        Self {
            transform: Transform::scale(factor),
            ..Self::fade(milliseconds)
        }
    }
}

/// One stop of a keyframe animation. Unset properties are not overridden.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keyframe {
    /// Position in 0..=1.
    pub at: f32,
    pub opacity: Option<f32>,
    pub transform: Option<Transform>,
    pub background: Option<Color>,
    pub foreground: Option<Color>,
}

impl Keyframe {
    pub fn at(at: f32) -> Self {
        Self {
            at,
            ..Self::default()
        }
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = Some(transform);
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    pub fn foreground(mut self, color: Color) -> Self {
        self.foreground = Some(color);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Once,
    Count(u32),
    Forever,
}

/// A keyframe animation that starts when the node appears (or when the
/// animation value changes) and holds its last keyframe when it ends.
/// `Repeat::Forever` keeps the window redrawing while the node is painted.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub keyframes: Vec<Keyframe>,
    pub duration: Duration,
    pub easing: Easing,
    pub repeat: Repeat,
    /// Play every second iteration backwards.
    pub alternate: bool,
}

impl Animation {
    pub fn new(keyframes: Vec<Keyframe>, milliseconds: u64) -> Self {
        let mut keyframes = keyframes;
        keyframes.sort_by(|a, b| a.at.total_cmp(&b.at));
        Self {
            keyframes,
            duration: Duration::from_millis(milliseconds),
            easing: Easing::Linear,
            repeat: Repeat::Once,
            alternate: false,
        }
    }
    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }
    pub fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }
    pub fn alternate(mut self, alternate: bool) -> Self {
        self.alternate = alternate;
        self
    }
    /// Opacity breathing between 1 and `low`, forever.
    pub fn pulse(low: f32, milliseconds: u64) -> Self {
        Self::new(
            vec![
                Keyframe::at(0.0).opacity(1.0),
                Keyframe::at(1.0).opacity(low),
            ],
            milliseconds,
        )
        .easing(Easing::EaseInOut)
        .repeat(Repeat::Forever)
        .alternate(true)
    }
    /// One clockwise turn per `milliseconds`, forever.
    pub fn spin(milliseconds: u64) -> Self {
        Self::new(
            vec![
                Keyframe::at(0.0).transform(Transform::rotate(0.0)),
                Keyframe::at(1.0).transform(Transform::rotate(360.0)),
            ],
            milliseconds,
        )
        .repeat(Repeat::Forever)
    }

    /// Overrides at `elapsed`, and whether the animation is still running.
    pub(crate) fn sample(&self, elapsed: Duration) -> (Keyframe, bool) {
        let Some(last) = self.keyframes.last() else {
            return (Keyframe::default(), false);
        };
        if self.duration.is_zero() {
            return (*last, false);
        }
        let cycles = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let limit = match self.repeat {
            Repeat::Once => Some(1.0),
            Repeat::Count(count) => Some(count.max(1) as f32),
            Repeat::Forever => None,
        };
        let running = limit.is_none_or(|limit| cycles < limit);
        let cycles = limit.map_or(cycles, |limit| cycles.min(limit));
        let mut iteration = cycles.floor();
        let mut local = cycles - iteration;
        if !running {
            // Hold the end of the final iteration.
            iteration = (cycles - 1.0).max(0.0).floor();
            local = 1.0;
        }
        if self.alternate && (iteration as u64) % 2 == 1 {
            local = 1.0 - local;
        }
        (self.at(self.easing.apply(local)), running)
    }

    fn at(&self, position: f32) -> Keyframe {
        let next = self
            .keyframes
            .iter()
            .position(|frame| frame.at >= position)
            .unwrap_or(self.keyframes.len() - 1);
        let to = self.keyframes[next];
        let from = self.keyframes[next.saturating_sub(1)];
        let span = to.at - from.at;
        let t = if span > 0.0 {
            ((position - from.at) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        Keyframe {
            at: position,
            opacity: mix(from.opacity, to.opacity, |a, b| lerp(a, b, t)),
            transform: mix(from.transform, to.transform, |a, b| a.lerp(b, t)),
            background: mix(from.background, to.background, |a, b| lerp_color(a, b, t)),
            foreground: mix(from.foreground, to.foreground, |a, b| lerp_color(a, b, t)),
        }
    }
}

fn mix<T: Copy>(from: Option<T>, to: Option<T>, blend: impl Fn(T, T) -> T) -> Option<T> {
    match (from, to) {
        (Some(from), Some(to)) => Some(blend(from, to)),
        (value, None) | (None, value) => value,
    }
}

pub(crate) fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

pub(crate) fn lerp_color(from: Color, to: Color, t: f32) -> Color {
    let channel = |a: u8, b: u8| {
        lerp(f32::from(a), f32::from(b), t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color(
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
        channel(from.3, to.3),
    )
}

/// Everything the engine interpolates for one node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shown {
    pub background: Option<Color>,
    pub foreground: Color,
    pub radius: f32,
    pub opacity: f32,
    pub transform: Transform,
}

impl Shown {
    pub(crate) fn lerp(self, to: Self, t: f32) -> Self {
        // A missing background fades from or to the other side's transparent twin.
        let clear = |color: Color| Color(color.0, color.1, color.2, 0);
        let background = match (self.background, to.background) {
            (None, None) => None,
            (Some(from), Some(to)) => Some(lerp_color(from, to, t)),
            (None, Some(to)) => Some(lerp_color(clear(to), to, t)),
            (Some(from), None) => Some(lerp_color(from, clear(from), t)),
        };
        Self {
            background,
            foreground: lerp_color(self.foreground, to.foreground, t),
            radius: lerp(self.radius, to.radius, t),
            opacity: lerp(self.opacity, to.opacity, t),
            transform: self.transform.lerp(to.transform, t),
        }
    }
}

pub(crate) fn lerp_rect(from: Rect, to: Rect, t: f32) -> Rect {
    Rect {
        x: lerp(from.x, to.x, t),
        y: lerp(from.y, to.y, t),
        width: lerp(from.width, to.width, t),
        height: lerp(from.height, to.height, t),
    }
}
