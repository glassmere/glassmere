//! Renderer-independent geometry, text measurement, and ordered paint commands.
mod text;
pub use text::{
    CaretStop, FontFamily, MonospaceMeasurer, TextCluster, TextGeometry, TextMeasurer, TextStyle,
    TextWrap,
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
    PushClip(Rect),
    PopClip,
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
            Self::PopClip => Self::PopClip,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub size: Size,
    pub primitives: Vec<Primitive>,
}
