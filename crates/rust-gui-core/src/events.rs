//! Watching the events of a subtree, and stopping them.
//!
//! Widgets read their own input through their `Response`. An `Ui::event_area`
//! is for what is above them: a panel that wants to know what happened inside
//! it, one that takes an event before the widgets do, or a widget that tells
//! its container something without the application threading state through.
use crate::Key;
use rust_gui_scene::Point;

/// What reached an area, in the order it arrived.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    PointerDown(Point),
    PointerUp(Point),
    PointerMoved(Point),
    Scroll {
        delta_x: f32,
        delta_y: f32,
    },
    Key {
        key: Key,
        shift: bool,
    },
    Text(String),
    /// The widget was activated: a click that reached it, Enter or Space on it,
    /// or the Activate that assistive technology sends. A click sends its
    /// pointer events first.
    Activate,
}

impl UiEvent {
    pub(crate) fn pointer(&self) -> bool {
        matches!(
            self,
            Self::PointerDown(_) | Self::PointerUp(_) | Self::PointerMoved(_)
        )
    }

    fn keyboard(&self) -> bool {
        matches!(self, Self::Key { .. } | Self::Text(_))
    }
}

/// What an area watches, and whether it lets the event through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventArea {
    pub pointer: bool,
    pub keyboard: bool,
    pub scroll: bool,
    /// See the event before the widgets inside rather than after them.
    pub capture: bool,
    /// Nothing else sees the event: no widget, and no area further along.
    pub consume: bool,
}

impl EventArea {
    pub fn pointer() -> Self {
        Self {
            pointer: true,
            ..Self::default()
        }
    }

    pub fn keyboard() -> Self {
        Self {
            keyboard: true,
            ..Self::default()
        }
    }

    pub fn scroll() -> Self {
        Self {
            scroll: true,
            ..Self::default()
        }
    }

    pub fn all() -> Self {
        Self {
            pointer: true,
            keyboard: true,
            scroll: true,
            ..Self::default()
        }
    }

    /// Before the widgets inside, so that consuming keeps it from them.
    pub fn capturing(mut self) -> Self {
        self.capture = true;
        self
    }

    pub fn consuming(mut self) -> Self {
        self.consume = true;
        self
    }

    pub(crate) fn watches(&self, event: &UiEvent) -> bool {
        match event {
            _ if event.pointer() => self.pointer,
            _ if event.keyboard() => self.keyboard,
            UiEvent::Scroll { .. } => self.scroll,
            _ => false,
        }
    }
}
