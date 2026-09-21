use crate::{Length, Style};
use std::time::Duration;

/// Hover time before a tooltip appears.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TabsResponse {
    /// The selection changed since the last declaration.
    pub changed: bool,
    /// A closable tab's close button was activated (or Delete pressed on it).
    pub closed: Option<usize>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopoverResponse {
    /// The popover was closed by Escape or an outside click since the last declaration.
    pub dismissed: bool,
}
