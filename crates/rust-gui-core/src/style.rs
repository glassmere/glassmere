use rust_gui_scene::{Color, FontFamily, TextStyle, TextWrap};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    /// A fraction of the parent's size, e.g. 1.0 for 100%.
    Percent(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub width: Length,
    pub height: Length,
    pub padding: f32,
    pub gap: f32,
    pub grow: f32,
    pub background: Color,
    pub foreground: Color,
    pub font_size: f32,
    pub radius: f32,
    pub font_family: FontFamily,
    pub font_weight: u16,
    pub italic: bool,
    pub wrap: TextWrap,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            width: Length::Auto,
            height: Length::Auto,
            padding: 0.0,
            gap: 12.0,
            grow: 0.0,
            background: Color::TRANSPARENT,
            foreground: Color::rgb(226, 232, 240),
            font_size: 16.0,
            radius: 0.0,
            font_family: FontFamily::SansSerif,
            font_weight: 400,
            italic: false,
            wrap: TextWrap::None,
        }
    }
}

impl Style {
    pub fn text_style(&self) -> TextStyle {
        TextStyle {
            font_size: self.font_size,
            family: self.font_family.clone(),
            weight: self.font_weight,
            italic: self.italic,
            wrap: self.wrap,
        }
    }
    pub fn button() -> Self {
        Self {
            padding: 12.0,
            background: Color::rgb(45, 66, 84),
            radius: 8.0,
            ..Self::default()
        }
    }

    pub(crate) fn layout(&self, row: bool) -> taffy::Style {
        use taffy::prelude::*;
        let dimension = |value| match value {
            Length::Auto => Dimension::auto(),
            Length::Px(value) => Dimension::length(value.max(0.0)),
            Length::Percent(value) => Dimension::percent(value.max(0.0)),
        };
        taffy::Style {
            size: Size {
                width: dimension(self.width),
                height: dimension(self.height),
            },
            min_size: Size {
                width: Dimension::length(0.0),
                height: Dimension::length(0.0),
            },
            padding: Rect::length(self.padding.max(0.0)),
            gap: Size::length(self.gap.max(0.0)),
            flex_direction: if row {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            },
            align_items: Some(AlignItems::Start),
            flex_grow: self.grow.max(0.0),
            flex_shrink: 0.0,
            ..taffy::Style::default()
        }
    }
}
