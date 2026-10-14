//! What a widget method takes.
//!
//! Every widget is one method. Its first argument is the plain thing it needs
//! most of the time, and a builder of the same name when it needs more:
//!
//! ```ignore
//! ui.button("Save");
//! ui.button(Button::new("Save").style(danger).enabled(!saving));
//! ```
//!
//! Identity is not here. A widget is identified by where it is declared, and
//! [`Ui::keyed`](crate::Ui::keyed) names one when that is not enough.

use crate::Style;

/// Text. `ui.label("Ready")` is the short form of `Label::new("Ready")`.
#[derive(Clone, Debug, Default)]
pub struct Label {
    pub(crate) text: String,
    pub(crate) style: Option<Style>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
        }
    }

    /// Replaces the default style, which is `Style::default()`.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// A button. `ui.button("Save")` is the short form of `Button::new("Save")`.
#[derive(Clone, Debug, Default)]
pub struct Button {
    pub(crate) text: String,
    pub(crate) style: Option<Style>,
    pub(crate) enabled: bool,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
            enabled: true,
        }
    }

    /// Replaces the default style, which is `Style::button()`.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Greys the button out and stops it answering. `Ui::enabled` does the
    /// same to everything declared in it.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// A single-line text field. `ui.text_input("Name", &mut name)` is the short
/// form of `TextInput::new("Name")`; the string it edits stays a separate
/// argument, so the builder can be reused and the borrow ends with the call.
#[derive(Clone, Debug, Default)]
pub struct TextInput {
    pub(crate) label: String,
    pub(crate) placeholder: String,
    pub(crate) style: Option<Style>,
}

impl TextInput {
    /// The label is what a screen reader announces, not what is drawn.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            placeholder: String::new(),
            style: None,
        }
    }

    /// Greyed text shown while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Replaces the default style, which is a field-sized `Style::button()`.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// A drop-down. `ui.combo_box("Theme", &mut choice, &items)` is the short form
/// of `ComboBox::new("Theme")`.
#[derive(Clone, Debug, Default)]
pub struct ComboBox {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
}

impl ComboBox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: None,
        }
    }

    /// Replaces the default style, which is a field-sized `Style::button()`.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// `impl From<&str>` and `impl From<String>` for a builder whose first field
/// is its text, so `ui.button("Save")` and `ui.button(Button::new("Save"))`
/// are the same call.
macro_rules! from_text {
    ($type:ty) => {
        impl From<&str> for $type {
            fn from(text: &str) -> Self {
                Self::new(text)
            }
        }

        impl From<String> for $type {
            fn from(text: String) -> Self {
                Self::new(text)
            }
        }

        impl From<&String> for $type {
            fn from(text: &String) -> Self {
                Self::new(text.as_str())
            }
        }

        impl From<std::borrow::Cow<'_, str>> for $type {
            fn from(text: std::borrow::Cow<'_, str>) -> Self {
                Self::new(text.into_owned())
            }
        }
    };
}

from_text!(Label);
from_text!(Button);
from_text!(TextInput);
from_text!(ComboBox);
