//! Renderer-independent geometry, text measurement, and ordered paint commands.
mod image;
mod text;
pub use image::{Image, VectorImage};
pub use text::{
    Affinity, CaretStop, FontFamily, MonospaceMeasurer, TextCluster, TextGeometry, TextLine,
    TextMeasurer, TextSpan, TextStyle, TextWrap,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn translated(self, offset: Point) -> Self {
        Self {
            x: self.x + offset.x,
            y: self.y + offset.y,
            ..self
        }
    }

    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: (self.x + self.width).max(other.x + other.width) - x,
            height: (self.y + self.height).max(other.y + other.height) - y,
        }
    }

    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.width && p.y < self.y + self.height
    }

    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            width: (self.x + self.width).min(other.x + other.width).max(x) - x,
            height: (self.y + self.height).min(other.y + other.height).max(y) - y,
        }
    }
}

/// sRGB color, with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const TRANSPARENT: Self = Self(0, 0, 0, 0);
    pub const WHITE: Self = Self(255, 255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 255)
    }

    pub fn linear(self) -> [f32; 4] {
        let convert = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        [
            convert(self.0),
            convert(self.1),
            convert(self.2),
            f32::from(self.3) / 255.0,
        ]
    }
}

/// A 2×3 affine transform in absolute logical pixels:
/// `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    pub fn scale(x: f32, y: f32) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }

    /// Clockwise rotation in degrees (y grows downward).
    pub fn rotate(degrees: f32) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `self` applied first, then `next`.
    pub fn then(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// The same transform applied around `pivot` instead of the origin.
    pub fn about(self, pivot: Point) -> Self {
        Self::translate(-pivot.x, -pivot.y)
            .then(self)
            .then(Self::translate(pivot.x, pivot.y))
    }

    pub fn apply(self, point: Point) -> Point {
        Point {
            x: self.a * point.x + self.c * point.y + self.e,
            y: self.b * point.x + self.d * point.y + self.f,
        }
    }

    pub fn is_identity(self) -> bool {
        const EPSILON: f32 = 1e-5;
        (self.a - 1.0).abs() < EPSILON
            && self.b.abs() < EPSILON
            && self.c.abs() < EPSILON
            && (self.d - 1.0).abs() < EPSILON
            && self.e.abs() < EPSILON
            && self.f.abs() < EPSILON
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    Quad {
        rect: Rect,
        color: Color,
        radius: f32,
    },
    Text {
        rect: Rect,
        text: String,
        font_size: f32,
        color: Color,
    },
    StyledText {
        rect: Rect,
        text: String,
        style: TextStyle,
        color: Color,
    },
    /// One shaped block of differently styled runs. `style` gives the font
    /// size, wrapping and the defaults spans fall back to, `color` the default
    /// colour. Lines break at `\n` and, when wrapping, at `rect.width`.
    RichText {
        rect: Rect,
        spans: Vec<TextSpan>,
        style: TextStyle,
        color: Color,
    },
    /// Draws the `source` part of an image into `rect`, scaled with linear
    /// filtering. `source` is in the image's own units (pixels of a raster
    /// image, natural-size units of a vector image); fit modes and nine-slice
    /// are expressed by the caller through `rect` and `source`. `tint` replaces
    /// the image's colour and keeps its alpha, for single-colour icons.
    /// `radius` rounds the corners of `rect`. Vector images are rasterized for
    /// the device pixels `rect` covers.
    Image {
        rect: Rect,
        image: Image,
        source: Rect,
        tint: Option<Color>,
        radius: f32,
    },
    PushClip(Rect),
    PopClip,
    /// Starts a group. Everything up to the matching `PopLayer` is rendered as
    /// one image covering `bounds` (content outside is clipped), then composited
    /// into the parent with `opacity` applied to the whole group and `transform`
    /// applied to its pixels in absolute logical coordinates. The clip active at
    /// the push bounds the composited result; clips inside apply to the group's
    /// untransformed content. Layers nest. Renderers may draw a layer with full
    /// opacity and an identity transform inline.
    PushLayer {
        bounds: Rect,
        opacity: f32,
        transform: Affine,
    },
    PopLayer,
}

impl Primitive {
    pub fn translated(&self, offset: Point) -> Self {
        match self {
            Self::Quad {
                rect,
                color,
                radius,
            } => Self::Quad {
                rect: rect.translated(offset),
                color: *color,
                radius: *radius,
            },
            Self::Text {
                rect,
                text,
                font_size,
                color,
            } => Self::Text {
                rect: rect.translated(offset),
                text: text.clone(),
                font_size: *font_size,
                color: *color,
            },
            Self::PushClip(rect) => Self::PushClip(rect.translated(offset)),
            Self::StyledText {
                rect,
                text,
                style,
                color,
            } => Self::StyledText {
                rect: rect.translated(offset),
                text: text.clone(),
                style: style.clone(),
                color: *color,
            },
            Self::RichText {
                rect,
                spans,
                style,
                color,
            } => Self::RichText {
                rect: rect.translated(offset),
                spans: spans.clone(),
                style: style.clone(),
                color: *color,
            },
            Self::Image {
                rect,
                image,
                source,
                tint,
                radius,
            } => Self::Image {
                rect: rect.translated(offset),
                image: image.clone(),
                source: *source,
                tint: *tint,
                radius: *radius,
            },
            Self::PopClip => Self::PopClip,
            // Coordinates are absolute, so moving a layer conjugates its transform.
            Self::PushLayer {
                bounds,
                opacity,
                transform,
            } => Self::PushLayer {
                bounds: bounds.translated(offset),
                opacity: *opacity,
                transform: Affine::translate(-offset.x, -offset.y)
                    .then(*transform)
                    .then(Affine::translate(offset.x, offset.y)),
            },
            Self::PopLayer => Self::PopLayer,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub size: Size,
    pub primitives: Vec<Primitive>,
}
