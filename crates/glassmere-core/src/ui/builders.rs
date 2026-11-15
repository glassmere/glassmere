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
    pub(crate) selectable: bool,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
            selectable: false,
        }
    }

    /// Replaces the default style, which is `Style::default()`.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Lets the text be selected and copied, but never edited: by dragging
    /// across it, or with the keyboard once Tab has brought focus to it
    /// (Shift and the arrow keys, Home and End, Ctrl/Cmd + A, then Ctrl/Cmd +
    /// C). It draws no border and no background.
    pub fn selectable(mut self) -> Self {
        self.selectable = true;
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

/// A link: accent-coloured, underlined text that sends a message like a
/// button. `ui.link("Documentation")` is the short form of
/// `Link::new("Documentation")`, and `Link::new("Documentation")
/// .on_click(Msg::Open)` sends a message when it is clicked. Opening an
/// address is the application's to do in that message.
#[derive(Clone, Debug)]
pub struct Link<M = ()> {
    pub(crate) text: String,
    pub(crate) style: Option<Style>,
    pub(crate) enabled: bool,
    pub(crate) on_click: Option<M>,
}

impl<M> Default for Link<M> {
    fn default() -> Self {
        Self {
            text: String::new(),
            style: None,
            enabled: false,
            on_click: None,
        }
    }
}

impl<M> Link<M> {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: None,
            enabled: true,
            on_click: None,
        }
    }

    /// The message a click sends: by pointer, keyboard, touch or assistive
    /// technology alike. Enter activates a focused link; Space does not.
    pub fn on_click(mut self, message: M) -> Self {
        self.on_click = Some(message);
        self
    }

    /// Replaces the default style, which has a little padding and no
    /// background. A foreground in the style replaces the accent, and the
    /// text stays on one line whatever the style's wrapping says.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Greys the link out and stops it answering. `Ui::enabled` does the
    /// same to everything declared in it.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// A slider with two thumbs. `ui.range_slider("Price", low..=high, range,
/// step, Msg::Price)` is the short form of `RangeSlider::new("Price")`; the
/// label is written above the track, and names each thumb as "Price minimum"
/// and "Price maximum".
#[derive(Clone, Debug, Default)]
pub struct RangeSlider {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
}

impl RangeSlider {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: None,
        }
    }

    /// Replaces the default style, which is 280 by 64 pixels with the
    /// caption in its padding. The thumbs fill the box, so give it a size.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
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

/// A progress bar. `ui.progress("Upload", Some(0.4))` is the short form of
/// `Progress::new("Upload")`; the label is what a screen reader announces, and
/// is not drawn.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
}

impl Progress {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: None,
        }
    }

    /// Replaces the default style, which is a bar 280 wide and 8 high. The
    /// style's background is the track, its foreground is the filled part,
    /// and its radius rounds both ends.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// A busy indicator for work that cannot say how far it is, drawn as dots
/// that turn. `ui.spinner("Loading")` is the short form of
/// `Spinner::new("Loading")`; the label is what a screen reader announces,
/// and is not drawn.
#[derive(Clone, Debug, Default)]
pub struct Spinner {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
}

impl Spinner {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: None,
        }
    }

    /// Replaces the default style, which is 24 square. The smaller side is
    /// the diameter, and the foreground colors the dots.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// An on/off switch. `ui.switch("Wi-Fi", on, Msg::Wifi)` is the short form of
/// `Switch::new("Wi-Fi")`; the label is written beside the switch and is its
/// accessible name.
#[derive(Clone, Debug, Default)]
pub struct Switch {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
}

impl Switch {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: None,
        }
    }

    /// Replaces the default style, which is a button's padding with a short
    /// transition that moves the thumb. The label stays on one line whatever
    /// the style's wrapping says.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// A section that its header opens and closes. `ui.collapsible("Advanced",
/// open, Msg::Advanced, |ui| ..)` is the short form of
/// `Collapsible::new("Advanced")`; the label is written in the header and is
/// its accessible name.
#[derive(Clone, Debug, Default)]
pub struct Collapsible {
    pub(crate) label: String,
    pub(crate) style: Option<Style>,
}

impl Collapsible {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: None,
        }
    }

    /// Replaces the default style of the header, which is a padded row with
    /// no background of its own. The header's label stays on one line
    /// whatever the style's wrapping says; the content is styled by what the
    /// application declares in it.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
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
from_text!(Progress);
from_text!(Spinner);
from_text!(Switch);
from_text!(Collapsible);
from_text!(RangeSlider);
from_text!(Link<M>);
