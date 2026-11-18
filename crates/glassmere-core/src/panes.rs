use crate::{Length, Style};
use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use core::time::Duration;

/// Hover time before a tooltip appears.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);

/// Which side of its widget a tooltip prefers. It flips to the opposite side
/// when it would leave the window, and is always kept inside the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipPlacement {
    /// Below the widget, level with its start edge; above it when there is no
    /// room below. A tooltip for the text under the pointer in an editor
    /// follows the pointer instead.
    #[default]
    Auto,
    /// Above the widget, level with its start edge.
    Above,
    /// Below the widget, level with its start edge.
    Below,
    /// Beside the widget, on the side it starts on: the left in a
    /// left-to-right window and the right in a right-to-left one.
    Before,
    /// Beside the widget, on the side it ends on: the right in a
    /// left-to-right window and the left in a right-to-left one.
    After,
}

/// Hover and focus help for a widget; see [`Ui::tooltip_with`](crate::Ui::tooltip_with).
/// Text alone converts into one, so `ui.tooltip_with("Save")` works.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tooltip {
    pub(crate) text: String,
    pub(crate) placement: TooltipPlacement,
    pub(crate) delay: Option<Duration>,
    // Boxed because most tooltips have none.
    pub(crate) style: Option<Box<Style>>,
}

impl Tooltip {
    /// A tooltip with `text`, placed as `TooltipPlacement::Auto` after
    /// [`TOOLTIP_DELAY`].
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Which side of the widget it prefers.
    pub fn placement(mut self, placement: TooltipPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// How long the pointer rests on the widget, or the keyboard focus stays
    /// on it, before the tooltip shows, instead of [`TOOLTIP_DELAY`]. A
    /// tooltip that follows another one closely shows after a shorter time.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// How the bubble looks. These take effect: `background`, `foreground`,
    /// `padding`, `radius`, `max_width` in pixels (the width of the bubble,
    /// padding included), `font_size`, `font_family`, `font_weight` and
    /// `italic`. A background, padding or radius left unset or zero takes the
    /// theme's.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(Box::new(style));
        self
    }
}

impl From<&str> for Tooltip {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Tooltip {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&String> for Tooltip {
    fn from(text: &String) -> Self {
        Self::new(text)
    }
}

impl From<Cow<'_, str>> for Tooltip {
    fn from(text: Cow<'_, str>) -> Self {
        Self::new(text)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub label: String,
    pub closable: bool,
}

impl Tab {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            closable: false,
        }
    }
    pub fn closable(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            closable: true,
        }
    }
}

/// Direction along which split panes are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Axis {
    /// Panes side by side; the divider moves left and right.
    Horizontal,
    /// Panes stacked; the divider moves up and down.
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitOptions {
    /// Minimum pixel size of the first pane.
    pub min_first: f32,
    /// Minimum pixel size of the second pane.
    pub min_second: f32,
    /// Divider thickness in logical pixels.
    pub divider: f32,
    /// Style of the split container; give it a size.
    pub style: Style,
}

impl Default for SplitOptions {
    fn default() -> Self {
        Self {
            min_first: 80.0,
            min_second: 80.0,
            divider: 6.0,
            style: Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopoverOptions {
    pub width: Length,
    /// Panel style; unset background, radius and padding take popover defaults.
    pub style: Style,
}

impl Default for PopoverOptions {
    fn default() -> Self {
        Self {
            width: Length::Px(280.0),
            style: Style::default(),
        }
    }
}
