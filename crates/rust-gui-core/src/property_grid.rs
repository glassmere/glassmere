use crate::Style;
use std::collections::{HashMap, HashSet};

/// What a property grid sends when the user does something to it. A value
/// names its row by path, such as `Layout/Width`; the grid's own state (the
/// filter, a collapsed group) is applied with [`PropertyGridState::apply`].
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyMsg {
    /// The filter box changed to this text.
    Filter(String),
    /// A group was opened or collapsed.
    Group {
        path: String,
        collapsed: bool,
    },
    /// The divider between the name and value columns moved: the name
    /// column's new share of the width, between 0.0 and 1.0.
    NameShare(f32),
    Number {
        path: String,
        value: f64,
    },
    Bool {
        path: String,
        value: bool,
    },
    Text {
        path: String,
        value: String,
    },
    Choice {
        path: String,
        value: usize,
    },
    Color {
        path: String,
        value: rust_gui_scene::Color,
    },
}

/// What a property grid remembers between frames. The app owns it, so it can
/// be saved with the rest of a workspace.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridState {
    /// Text of the filter box; rows whose name or group contains it show.
    pub filter: String,
    /// Paths of collapsed groups, such as `Layout` or `Layout/Margins`.
    pub collapsed: HashSet<String>,
    /// The name column's share of the width, moved by the header's divider.
    pub name_share: f32,
    /// Half-typed colour values by property path.
    pub(crate) drafts: HashMap<String, String>,
}

impl Default for PropertyGridState {
    fn default() -> Self {
        Self {
            filter: String::new(),
            collapsed: HashSet::new(),
            name_share: 0.4,
            drafts: HashMap::new(),
        }
    }
}

impl PropertyGridState {
    /// Applies the grid's own changes. Value changes are the application's to
    /// store, so they change nothing here.
    pub fn apply(&mut self, message: &PropertyMsg) {
        match message {
            PropertyMsg::Filter(text) => self.filter.clone_from(text),
            PropertyMsg::Group { path, collapsed } => {
                if *collapsed {
                    self.collapsed.insert(path.clone());
                } else {
                    self.collapsed.remove(path);
                }
            }
            PropertyMsg::NameShare(share) if share.is_finite() => {
                self.name_share = share.clamp(0.0, 1.0);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGridOptions {
    /// Accessible name of the filter box and the grid.
    pub label: String,
    /// Whether the filter box shows.
    pub filter: bool,
    /// Rows scroll inside this height; `None` lets the grid grow.
    pub height: Option<f32>,
    /// Style of the whole grid; full width unless it says otherwise.
    pub style: Style,
}

impl Default for PropertyGridOptions {
    fn default() -> Self {
        Self {
            label: "Properties".into(),
            filter: true,
            height: None,
            style: Style::default(),
        }
    }
}

impl PropertyGridOptions {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn filter(mut self, filter: bool) -> Self {
        self.filter = filter;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

/// Paths of the properties that changed this frame, such as `Layout/Width`;
/// a reset counts as a change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PropertyGridResponse {
    pub changed: Vec<String>,
}

impl PropertyGridResponse {
    pub fn any(&self) -> bool {
        !self.changed.is_empty()
    }
}
