use crate::{Length, NodeId, Style};

/// Focus behavior of a `Ui::focus_scope` group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FocusScope {
    /// Tab and Shift+Tab cycle inside the scope while it holds focus.
    pub trap: bool,
    /// When the scope disappears, focus returns to the node focused before it.
    pub restore: bool,
    /// Arrow keys move focus geometrically between focusables in the scope,
    /// except from controls that use arrows themselves.
    pub arrows: bool,
    /// When the scope appears, focus moves to its first focusable control.
    pub autofocus: bool,
}

/// Presentation of a `Ui::dialog`. Modal dialogs paint a scrim, block pointer
/// and wheel input to content behind them and trap Tab; non-modal dialogs
/// float over content without blocking it.
#[derive(Clone, Debug, PartialEq)]
pub struct DialogOptions {
    pub title: String,
    pub modal: bool,
    /// Escape and, for modal dialogs, a click on the scrim close the dialog.
    pub dismissible: bool,
    pub width: Length,
    /// Panel style; unset background, radius and padding take dialog defaults.
    pub style: Style,
}

impl DialogOptions {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            modal: true,
            dismissible: true,
            width: Length::Px(420.0),
            style: Style::default(),
        }
    }
    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

/// Retained per-scope focus memory.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScopeState {
    pub last_focused: Option<NodeId>,
    pub restore_to: Option<NodeId>,
}
