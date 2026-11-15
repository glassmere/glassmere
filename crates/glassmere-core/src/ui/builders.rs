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

use crate::{Date, DateStyle, Style, Weekday};

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

/// A button. `ui.button("Save")` is the short form of `Button::new("Save")`,
/// and `Button::new("Save").on_click(Msg::Save)` sends a message when it is
/// clicked.
#[derive(Clone, Debug)]
pub struct Button<M = ()> {
    pub(crate) text: String,
    pub(crate) style: Option<Style>,
    pub(crate) enabled: bool,
    pub(crate) on_click: Option<M>,
}

impl<M> Default for Button<M> {
    fn default() -> Self {
        Self {
            text: String::new(),
            style: None,
            enabled: false,
            on_click: None,
        }
    }
}

impl<M> Button<M> {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
            enabled: true,
            on_click: None,
        }
    }

    /// The message a click sends: by pointer, keyboard, touch or assistive
    /// technology alike.
    pub fn on_click(mut self, message: M) -> Self {
        self.on_click = Some(message);
        self
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
#[derive(Clone, Debug)]
pub struct TextInput<M = ()> {
    pub(crate) label: String,
    pub(crate) placeholder: String,
    pub(crate) style: Option<Style>,
    pub(crate) on_submit: Option<M>,
    pub(crate) password: bool,
    pub(crate) reveal: bool,
}

impl<M> Default for TextInput<M> {
    fn default() -> Self {
        Self::new("")
    }
}

impl<M> TextInput<M> {
    /// The label is what a screen reader announces, not what is drawn.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            placeholder: String::new(),
            style: None,
            on_submit: None,
            password: false,
            reveal: false,
        }
    }

    /// A password field: it draws a bullet for each character, will not copy
    /// or cut, and tells assistive technology it holds a password, which
    /// then reads neither the text nor where its words are.
    pub fn password(mut self) -> Self {
        self.password = true;
        self
    }

    /// A password field with a button beside it that shows the text and
    /// hides it again. While the text is shown the field behaves as a plain
    /// one: it can be copied, and assistive technology reads it. The field
    /// and its button are then a row, which takes the field's key and width.
    pub fn password_with_reveal(mut self) -> Self {
        self.password = true;
        self.reveal = true;
        self
    }

    /// The message Enter sends.
    pub fn on_submit(mut self, message: M) -> Self {
        self.on_submit = Some(message);
        self
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

/// A month to pick a day from. `ui.calendar("Departure", selected, Msg::Pick)`
/// is the short form of `Calendar::new("Departure")`.
#[derive(Clone, Debug, Default)]
pub struct Calendar {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
    pub(crate) min: Option<Date>,
    pub(crate) max: Option<Date>,
    pub(crate) first_weekday: Option<Weekday>,
}

impl Calendar {
    /// The label is what a screen reader announces, not what is drawn.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    /// Replaces the default style, which pads the calendar by 8 pixels.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// The earliest day that can be picked; earlier days are disabled.
    pub fn min(mut self, min: Date) -> Self {
        self.min = Some(min);
        self
    }

    /// The latest day that can be picked; later days are disabled.
    pub fn max(mut self, max: Date) -> Self {
        self.max = Some(max);
        self
    }

    /// The weekday each row starts with; Monday by default.
    pub fn first_weekday(mut self, weekday: Weekday) -> Self {
        self.first_weekday = Some(weekday);
        self
    }
}

/// A field that shows a date and opens a calendar to pick another.
/// `ui.date_picker("Departure", value, Msg::Departure)` is the short form of
/// `DatePicker::new("Departure")`.
#[derive(Clone, Debug, Default)]
pub struct DatePicker {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
    pub(crate) min: Option<Date>,
    pub(crate) max: Option<Date>,
    pub(crate) first_weekday: Option<Weekday>,
    pub(crate) date_style: DateStyle,
    pub(crate) placeholder: String,
}

impl DatePicker {
    /// The label is what a screen reader announces, not what is drawn.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    /// Replaces the default style, which is a field-sized `Style::button()`.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// The earliest day that can be picked; earlier days are disabled.
    pub fn min(mut self, min: Date) -> Self {
        self.min = Some(min);
        self
    }

    /// The latest day that can be picked; later days are disabled.
    pub fn max(mut self, max: Date) -> Self {
        self.max = Some(max);
        self
    }

    /// The weekday each row of the calendar starts with; Monday by default.
    pub fn first_weekday(mut self, weekday: Weekday) -> Self {
        self.first_weekday = Some(weekday);
        self
    }

    /// How the field writes its date; `Medium` by default.
    pub fn date_style(mut self, style: DateStyle) -> Self {
        self.date_style = style;
        self
    }

    /// Greyed text shown while there is no date.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
}

/// `impl From<&str>` and `impl From<String>` for a builder whose first field
/// is its text, so `ui.button("Save")` and `ui.button(Button::new("Save"))`
/// are the same call.
macro_rules! from_text {
    ($type:ident $(<$message:ident>)?) => {
        impl$(<$message>)? From<&str> for $type$(<$message>)? {
            fn from(text: &str) -> Self {
                Self::new(text)
            }
        }

        impl$(<$message>)? From<String> for $type$(<$message>)? {
            fn from(text: String) -> Self {
                Self::new(text)
            }
        }

        impl$(<$message>)? From<&String> for $type$(<$message>)? {
            fn from(text: &String) -> Self {
                Self::new(text.as_str())
            }
        }

        impl$(<$message>)? From<std::borrow::Cow<'_, str>> for $type$(<$message>)? {
            fn from(text: std::borrow::Cow<'_, str>) -> Self {
                Self::new(text.into_owned())
            }
        }
    };
}

from_text!(Label);
from_text!(Button<M>);
from_text!(TextInput<M>);
from_text!(ComboBox);
from_text!(Calendar);
from_text!(DatePicker);
