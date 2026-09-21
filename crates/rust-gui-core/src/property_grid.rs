use crate::Style;
use std::collections::{HashMap, HashSet};

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
