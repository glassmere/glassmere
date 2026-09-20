use crate::Theme;
use rust_gui_scene::{Color, FontFamily, TextStyle, TextWrap};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    /// A fraction of the parent's size, e.g. 1.0 for 100%.
    Percent(f32),
}

/// Paint properties a pseudo-state may override. Layout never changes with state,
/// so state transitions stay repaint-only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paint {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub radius: Option<f32>,
}

impl Paint {
    pub fn background(color: Color) -> Self {
        Self {
            background: Some(color),
            ..Self::default()
        }
    }
    pub fn foreground(color: Color) -> Self {
        Self {
            foreground: Some(color),
            ..Self::default()
        }
    }
}

/// Overrides applied on top of the theme defaults while a state is active.
/// Precedence when several apply: disabled, then pressed, hover, focused.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateStyles {
    pub hover: Paint,
    pub pressed: Paint,
    pub focused: Paint,
    pub disabled: Paint,
}

/// Layout and paint properties of one widget. Text properties are `None` by
/// default and inherit from the nearest ancestor, then the theme; `background`
/// `None` paints the widget kind's theme default.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub width: Length,
    pub height: Length,
    pub padding: f32,
    pub gap: f32,
    pub grow: f32,
    pub radius: f32,
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub font_size: Option<f32>,
    pub font_family: Option<FontFamily>,
    pub font_weight: Option<u16>,
    pub italic: Option<bool>,
    pub wrap: TextWrap,
    pub states: Option<Box<StateStyles>>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            width: Length::Auto,
            height: Length::Auto,
            padding: 0.0,
            gap: 12.0,
            grow: 0.0,
            radius: 0.0,
            background: None,
            foreground: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            italic: None,
            wrap: TextWrap::None,
            states: None,
        }
    }
}

macro_rules! setter {
    ($name:ident, $field:ident, $type:ty) => {
        pub fn $name(mut self, value: $type) -> Self {
            self.$field = value;
            self
        }
    };
    ($name:ident, $field:ident, $type:ty, some) => {
        pub fn $name(mut self, value: $type) -> Self {
            self.$field = Some(value);
            self
        }
    };
}

impl Style {
    /// Themed control surface with the default control padding and radius.
    pub fn button() -> Self {
        Self {
            padding: 12.0,
            radius: 8.0,
            ..Self::default()
        }
    }

    setter!(width, width, Length);
    setter!(height, height, Length);
    setter!(padding, padding, f32);
    setter!(gap, gap, f32);
    setter!(grow, grow, f32);
    setter!(radius, radius, f32);
    setter!(wrap, wrap, TextWrap);
    setter!(background, background, Color, some);
    setter!(foreground, foreground, Color, some);
    setter!(font_size, font_size, f32, some);
    setter!(font_family, font_family, FontFamily, some);
    setter!(font_weight, font_weight, u16, some);
    setter!(italic, italic, bool, some);

    fn state(mut self, apply: impl FnOnce(&mut StateStyles)) -> Self {
        apply(self.states.get_or_insert_with(Default::default));
        self
    }
    pub fn on_hover(self, paint: Paint) -> Self {
        self.state(|states| states.hover = paint)
    }
    pub fn on_pressed(self, paint: Paint) -> Self {
        self.state(|states| states.pressed = paint)
    }
    pub fn on_focused(self, paint: Paint) -> Self {
        self.state(|states| states.focused = paint)
    }
    pub fn on_disabled(self, paint: Paint) -> Self {
        self.state(|states| states.disabled = paint)
    }

    /// Fills unset text properties from the parent's resolved values.
    pub fn resolve_text(&self, inherited: &ResolvedText) -> ResolvedText {
        ResolvedText {
            foreground: self.foreground.unwrap_or(inherited.foreground),
            font_size: self.font_size.unwrap_or(inherited.font_size),
            font_family: self
                .font_family
                .clone()
                .unwrap_or_else(|| inherited.font_family.clone()),
            font_weight: self.font_weight.unwrap_or(inherited.font_weight),
            italic: self.italic.unwrap_or(inherited.italic),
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

/// Concrete text properties after inheritance; what measurement and paint use.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedText {
    pub foreground: Color,
    pub font_size: f32,
    pub font_family: FontFamily,
    pub font_weight: u16,
    pub italic: bool,
}

impl ResolvedText {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            foreground: theme.text,
            font_size: theme.font_size,
            font_family: theme.font_family.clone(),
            font_weight: 400,
            italic: false,
        }
    }

    pub fn text_style(&self, wrap: TextWrap) -> TextStyle {
        TextStyle {
            font_size: self.font_size,
            family: self.font_family.clone(),
            weight: self.font_weight,
            italic: self.italic,
            wrap,
        }
    }
}
