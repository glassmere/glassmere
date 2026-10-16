//! The app-owned docking model: a tree of splits and tab groups, in-window
//! floating groups and detached windows, with a small text format.
//!
//! ```text
//! dock v1
//! main {
//!     split horizontal 0.25 {
//!         tabs 0 "files" "search"
//!         split vertical 0.7 {
//!             tabs 1 "editor" "preview"
//!             tabs 0 "output"
//!         }
//!     }
//! }
//! float 120 80 320 240 { tabs 0 "inspector" }
//! window 1 640 480 { tabs 0 "console" }
//! ```
use crate::Axis;
use crate::controls::DockAction;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DockWindowId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockZone {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// One dock surface: the main window's dock or a detached window's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockArea {
    Main,
    Window(DockWindowId),
}

/// Where a panel currently lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockPlace {
    Main,
    Float(usize),
    Window(DockWindowId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum DockTarget {
    /// Beside (or, for `Center`, into) the group that contains `anchor`.
    Group { anchor: String, zone: DockZone },
    /// Into the group that contains `anchor`, at tab position `index`.
    Tab { anchor: String, index: usize },
    /// Along a whole edge of an area; `Center` joins its first group.
    Edge { area: DockArea, zone: DockZone },
    /// A floating group inside the main window, relative to the dock.
    Float {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// A new detached window of this logical size.
    Window { width: f32, height: f32 },
}

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DockTabs {
    pub panels: Vec<String>,
    pub active: usize,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DockNode {
    Tabs(DockTabs),
    /// `Axis::Horizontal` places `first` left of `second`; `fraction` is the
    /// first side's share.
    Split {
        axis: Axis,
        fraction: f32,
        first: Box<DockNode>,
        second: Box<DockNode>,
    },
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DockFloat {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub tabs: DockTabs,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DockWindow {
    pub id: DockWindowId,
    pub width: f32,
    pub height: f32,
    pub root: DockNode,
}

/// Panels are identified by app-chosen strings. Ids the app no longer declares
/// are skipped when rendering, so old saved layouts keep loading.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DockLayout {
    pub root: Option<DockNode>,
    /// Back to front.
    pub floating: Vec<DockFloat>,
    pub windows: Vec<DockWindow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockLayoutError {
    pub line: usize,
    pub message: String,
}
impl std::fmt::Display for DockLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dock layout line {}: {}", self.line, self.message)
    }
}
impl std::error::Error for DockLayoutError {}

impl DockTabs {
    fn of(panel: &str) -> Self {
        Self {
            panels: vec![panel.to_owned()],
            active: 0,
        }
    }
}

impl DockNode {
    pub fn tabs<S: Into<String>>(panels: impl IntoIterator<Item = S>) -> Self {
        Self::Tabs(DockTabs {
            panels: panels.into_iter().map(Into::into).collect(),
            active: 0,
        })
    }
    /// `first` left of `second`.
    pub fn row(fraction: f32, first: Self, second: Self) -> Self {
        Self::Split {
            axis: Axis::Horizontal,
            fraction,
            first: Box::new(first),
            second: Box::new(second),
        }
    }
    /// `first` above `second`.
    pub fn column(fraction: f32, first: Self, second: Self) -> Self {
        Self::Split {
            axis: Axis::Vertical,
            fraction,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn groups<'a>(&'a self, out: &mut Vec<&'a DockTabs>) {
        match self {
            Self::Tabs(tabs) => out.push(tabs),
            Self::Split { first, second, .. } => {
                first.groups(out);
                second.groups(out);
            }
        }
    }

    fn groups_mut<'a>(&'a mut self, out: &mut Vec<&'a mut DockTabs>) {
        match self {
            Self::Tabs(tabs) => out.push(tabs),
            Self::Split { first, second, .. } => {
                first.groups_mut(out);
                second.groups_mut(out);
            }
        }
    }

    /// The first tab group in reading order; every node has one.
    fn first_group_mut(&mut self) -> &mut DockTabs {
        match self {
            Self::Tabs(tabs) => tabs,
            Self::Split { first, .. } => first.first_group_mut(),
        }
    }

    /// The tab-group node that contains `panel`.
    fn group_node_mut(&mut self, panel: &str) -> Option<&mut DockNode> {
        match self {
            Self::Tabs(tabs) => tabs.panels.iter().any(|id| id == panel).then_some(self),
            Self::Split { first, second, .. } => first
                .group_node_mut(panel)
                .or_else(|| second.group_node_mut(panel)),
        }
    }

    /// Drops empty groups, collapses one-sided splits and repairs numbers.
    fn normalized(self) -> Option<Self> {
        match self {
            Self::Tabs(mut tabs) => {
                if tabs.panels.is_empty() {
                    return None;
                }
                tabs.active = tabs.active.min(tabs.panels.len() - 1);
                Some(Self::Tabs(tabs))
            }
            Self::Split {
                axis,
                fraction,
                first,
                second,
            } => match (first.normalized(), second.normalized()) {
                (Some(first), Some(second)) => Some(Self::Split {
                    axis,
                    fraction: if fraction.is_finite() {
                        fraction.clamp(0.05, 0.95)
                    } else {
                        0.5
                    },
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (one, other) => one.or(other),
            },
        }
    }

    fn wrap(&mut self, zone: DockZone, panel: &str) {
        let fresh = Self::Tabs(DockTabs::of(panel));
        let old = std::mem::replace(self, Self::Tabs(DockTabs::default()));
        *self = match zone {
            DockZone::Left => Self::row(0.5, fresh, old),
            DockZone::Right => Self::row(0.5, old, fresh),
            DockZone::Top => Self::column(0.5, fresh, old),
            DockZone::Bottom | DockZone::Center => Self::column(0.5, old, fresh),
        };
    }

    fn write(&self, out: &mut String, depth: usize) {
        let indent = "    ".repeat(depth);
        match self {
            Self::Tabs(tabs) => {
                let _ = write!(out, "{indent}tabs {}", tabs.active);
                for panel in &tabs.panels {
                    let _ = write!(out, " {}", quote(panel));
                }
                out.push('\n');
            }
            Self::Split {
                axis,
                fraction,
                first,
                second,
            } => {
                let axis = match axis {
                    Axis::Horizontal => "horizontal",
                    Axis::Vertical => "vertical",
                };
                let _ = writeln!(out, "{indent}split {axis} {fraction} {{");
                first.write(out, depth + 1);
                second.write(out, depth + 1);
                let _ = writeln!(out, "{indent}}}");
            }
        }
    }
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

impl DockLayout {
    pub fn new(root: DockNode) -> Self {
        Self {
            root: Some(root),
            ..Self::default()
        }
    }

    /// Does what a dock change asks of the layout: the change a message from
    /// `Ui::dock` carries. The view only declares; `update` calls this.
    /// Returns whether the layout changed.
    pub fn apply(&mut self, action: DockAction) -> bool {
        let changed = match action {
            DockAction::Move { panel, target } => self.move_panel(&panel, target),
            DockAction::FloatRect {
                float,
                x,
                y,
                width,
                height,
            } => {
                // A float being moved or resized is also the frontmost one.
                match self.floating_index(&float) {
                    Some(index) => {
                        let mut target = self.floating.remove(index);
                        (target.x, target.y, target.width, target.height) = (x, y, width, height);
                        self.floating.push(target);
                        true
                    }
                    None => false,
                }
            }
            DockAction::Raise(float) => match self.floating_index(&float) {
                Some(index) => {
                    let raised = self.floating.remove(index);
                    self.floating.push(raised);
                    true
                }
                None => false,
            },
            DockAction::Select(panel) => {
                let mut changed = false;
                for tabs in self.all_groups_mut() {
                    if let Some(index) = tabs.panels.iter().position(|id| *id == panel) {
                        changed = tabs.active != index;
                        tabs.active = index;
                        break;
                    }
                }
                changed
            }
            DockAction::Close(panel) => self.close(&panel),
            DockAction::Ratio {
                area,
                path,
                fraction,
            } => match self.split_fraction_mut(area, &path) {
                Some(share) if fraction.is_finite() => {
                    let fraction = fraction.clamp(0.0, 1.0);
                    let changed = *share != fraction;
                    *share = fraction;
                    changed
                }
                _ => false,
            },
        };
        self.normalize();
        changed
    }

    fn floating_index(&self, float: &str) -> Option<usize> {
        self.floating
            .iter()
            .position(|candidate| candidate.tabs.panels.first().map(String::as_str) == Some(float))
    }

    /// The share of the split at `path` from the area's root, if there is one.
    fn split_fraction_mut(&mut self, area: DockArea, path: &[usize]) -> Option<&mut f32> {
        let mut node = match area {
            DockArea::Main => self.root.as_mut()?,
            DockArea::Window(id) => {
                &mut self.windows.iter_mut().find(|window| window.id == id)?.root
            }
        };
        for &step in path {
            let DockNode::Split { first, second, .. } = node else {
                return None;
            };
            node = if step == 0 {
                &mut **first
            } else {
                &mut **second
            };
        }
        match node {
            DockNode::Split { fraction, .. } => Some(fraction),
            DockNode::Tabs(_) => None,
        }
    }

    fn all_groups(&self) -> Vec<&DockTabs> {
        let mut groups = Vec::new();
        if let Some(root) = &self.root {
            root.groups(&mut groups);
        }
        groups.extend(self.floating.iter().map(|float| &float.tabs));
        for window in &self.windows {
            window.root.groups(&mut groups);
        }
        groups
    }

    fn all_groups_mut(&mut self) -> Vec<&mut DockTabs> {
        let mut groups = Vec::new();
        if let Some(root) = &mut self.root {
            root.groups_mut(&mut groups);
        }
        groups.extend(self.floating.iter_mut().map(|float| &mut float.tabs));
        for window in &mut self.windows {
            window.root.groups_mut(&mut groups);
        }
        groups
    }

    /// Every panel id, main area first.
    pub fn panels(&self) -> Vec<&str> {
        self.all_groups()
            .into_iter()
            .flat_map(|tabs| tabs.panels.iter().map(String::as_str))
            .collect()
    }

    pub fn contains(&self, panel: &str) -> bool {
        self.panels().contains(&panel)
    }

    /// The ids sharing a tab group with `panel`, itself included.
    pub fn group_of(&self, panel: &str) -> Option<&[String]> {
        self.all_groups()
            .into_iter()
            .find(|tabs| tabs.panels.iter().any(|id| id == panel))
            .map(|tabs| tabs.panels.as_slice())
    }

    pub fn place_of(&self, panel: &str) -> Option<DockPlace> {
        let has = |tabs: &DockTabs| tabs.panels.iter().any(|id| id == panel);
        let in_node = |node: &DockNode| {
            let mut groups = Vec::new();
            node.groups(&mut groups);
            groups.into_iter().any(has)
        };
        if self.root.as_ref().is_some_and(in_node) {
            return Some(DockPlace::Main);
        }
        if let Some(index) = self.floating.iter().position(|float| has(&float.tabs)) {
            return Some(DockPlace::Float(index));
        }
        self.windows
            .iter()
            .find(|window| in_node(&window.root))
            .map(|window| DockPlace::Window(window.id))
    }

    /// The tab groups of one area in tab order; the main area's floating
    /// groups follow its tree.
    pub fn groups_in(&self, area: DockArea) -> Vec<&[String]> {
        let mut groups = Vec::new();
        match area {
            DockArea::Main => {
                if let Some(root) = &self.root {
                    root.groups(&mut groups);
                }
                groups.extend(self.floating.iter().map(|float| &float.tabs));
            }
            DockArea::Window(id) => {
                if let Some(window) = self.window(id) {
                    window.root.groups(&mut groups);
                }
            }
        }
        groups
            .into_iter()
            .map(|tabs| tabs.panels.as_slice())
            .collect()
    }

    pub fn window(&self, id: DockWindowId) -> Option<&DockWindow> {
        self.windows.iter().find(|window| window.id == id)
    }

    /// Selects the panel's tab. False when the panel is not in the layout.
    pub fn activate(&mut self, panel: &str) -> bool {
        for tabs in self.all_groups_mut() {
            if let Some(index) = tabs.panels.iter().position(|id| id == panel) {
                tabs.active = index;
                return true;
            }
        }
        false
    }

    /// Brings a panel into view: selects it, or adds it to the main area's
    /// first group when it is not in the layout.
    pub fn show(&mut self, panel: &str) {
        if !self.activate(panel) {
            self.insert(
                panel,
                &DockTarget::Edge {
                    area: DockArea::Main,
                    zone: DockZone::Center,
                },
            );
        }
    }

    pub fn close(&mut self, panel: &str) -> bool {
        let removed = self.take(panel);
        self.normalize();
        removed
    }

    /// Removes every panel the predicate rejects, for example ids a newer
    /// version of the app no longer has.
    pub fn retain_panels(&mut self, mut keep: impl FnMut(&str) -> bool) {
        for tabs in self.all_groups_mut() {
            tabs.panels.retain(|id| keep(id));
        }
        self.normalize();
    }

    /// Moves a detached window's panels into the main area and forgets the window.
    pub fn redock(&mut self, id: DockWindowId) -> bool {
        let Some(index) = self.windows.iter().position(|window| window.id == id) else {
            return false;
        };
        let window = self.windows.remove(index);
        let mut groups = Vec::new();
        window.root.groups(&mut groups);
        for panel in groups.into_iter().flat_map(|tabs| &tabs.panels) {
            self.show(panel);
        }
        true
    }

    /// Forgets a detached window and returns the panels it held.
    pub fn remove_window(&mut self, id: DockWindowId) -> Vec<String> {
        let Some(index) = self.windows.iter().position(|window| window.id == id) else {
            return Vec::new();
        };
        let mut groups = Vec::new();
        let window = self.windows.remove(index);
        window.root.groups(&mut groups);
        groups
            .into_iter()
            .flat_map(|tabs| tabs.panels.clone())
            .collect()
    }

    /// Moves a panel. False (and no change) when the panel or the target's
    /// anchor is unknown, or the move would change nothing.
    pub fn move_panel(&mut self, panel: &str, target: DockTarget) -> bool {
        if !self.contains(panel) {
            return false;
        }
        // An anchor must survive the panel's removal: use a group mate instead.
        let mate = |layout: &Self| {
            layout
                .group_of(panel)
                .and_then(|group| group.iter().find(|id| *id != panel).cloned())
        };
        let target = match target {
            DockTarget::Group { anchor, zone } if anchor == panel => match mate(self) {
                Some(anchor) if zone != DockZone::Center => DockTarget::Group { anchor, zone },
                _ => return false,
            },
            DockTarget::Tab { anchor, index } if anchor == panel => match mate(self) {
                Some(anchor) => DockTarget::Tab { anchor, index },
                None => return false,
            },
            other => other,
        };
        match &target {
            DockTarget::Group { anchor, zone } => {
                let same_group = self
                    .group_of(anchor)
                    .is_some_and(|group| group.iter().any(|id| id == panel));
                if !self.contains(anchor) || (*zone == DockZone::Center && same_group) {
                    return false;
                }
            }
            DockTarget::Tab { anchor, .. } if !self.contains(anchor) => return false,
            DockTarget::Edge {
                area: DockArea::Window(id),
                ..
            } if self.window(*id).is_none() => return false,
            _ => {}
        }
        let target = match target {
            // Tab positions are given in the strip the user sees, which still
            // holds the dragged tab when it is reordered within its own group.
            DockTarget::Tab { anchor, index } => {
                let before = self
                    .group_of(&anchor)
                    .and_then(|group| group.iter().position(|id| id == panel))
                    .is_some_and(|position| position < index);
                DockTarget::Tab {
                    anchor,
                    index: index - usize::from(before),
                }
            }
            other => other,
        };
        let before = self.clone();
        self.take(panel);
        if !self.insert(panel, &target) {
            *self = before;
            return false;
        }
        self.normalize();
        *self != before
    }

    /// Removes the id without normalizing, so containers survive until insertion.
    fn take(&mut self, panel: &str) -> bool {
        let mut removed = false;
        for tabs in self.all_groups_mut() {
            if let Some(index) = tabs.panels.iter().position(|id| id == panel) {
                tabs.panels.remove(index);
                if index < tabs.active || tabs.active >= tabs.panels.len() {
                    tabs.active = tabs.active.saturating_sub(1);
                }
                removed = true;
            }
        }
        removed
    }

    fn insert(&mut self, panel: &str, target: &DockTarget) -> bool {
        match target {
            DockTarget::Tab { anchor, index } => {
                let Some(tabs) = self
                    .all_groups_mut()
                    .into_iter()
                    .find(|tabs| tabs.panels.iter().any(|id| id == anchor))
                else {
                    return false;
                };
                let index = (*index).min(tabs.panels.len());
                tabs.panels.insert(index, panel.to_owned());
                tabs.active = index;
                true
            }
            DockTarget::Group { anchor, zone } => {
                let floating = self
                    .floating
                    .iter()
                    .any(|float| float.tabs.panels.iter().any(|id| id == anchor));
                // Floating groups do not split.
                if *zone == DockZone::Center || floating {
                    return self.insert(
                        panel,
                        &DockTarget::Tab {
                            anchor: anchor.clone(),
                            index: usize::MAX,
                        },
                    );
                }
                let node = self
                    .root
                    .iter_mut()
                    .chain(self.windows.iter_mut().map(|window| &mut window.root))
                    .find_map(|root| root.group_node_mut(anchor));
                node.map(|node| node.wrap(*zone, panel)).is_some()
            }
            DockTarget::Edge { area, zone } => {
                let root = match area {
                    DockArea::Main => match &mut self.root {
                        Some(root) => root,
                        None => {
                            self.root = Some(DockNode::Tabs(DockTabs::of(panel)));
                            return true;
                        }
                    },
                    DockArea::Window(id) => {
                        match self.windows.iter_mut().find(|window| window.id == *id) {
                            Some(window) => &mut window.root,
                            None => return false,
                        }
                    }
                };
                if *zone == DockZone::Center {
                    let tabs = root.first_group_mut();
                    tabs.panels.push(panel.to_owned());
                    tabs.active = tabs.panels.len() - 1;
                    return true;
                }
                root.wrap(*zone, panel);
                // A whole-edge dock starts as a quarter, not a half.
                if let DockNode::Split { fraction, .. } = root {
                    *fraction = match zone {
                        DockZone::Left | DockZone::Top => 0.25,
                        _ => 0.75,
                    };
                }
                true
            }
            DockTarget::Float {
                x,
                y,
                width,
                height,
            } => {
                self.floating.push(DockFloat {
                    x: *x,
                    y: *y,
                    width: *width,
                    height: *height,
                    tabs: DockTabs::of(panel),
                });
                true
            }
            DockTarget::Window { width, height } => {
                let id = DockWindowId(
                    self.windows
                        .iter()
                        .map(|window| window.id.0)
                        .max()
                        .unwrap_or(0)
                        + 1,
                );
                self.windows.push(DockWindow {
                    id,
                    width: *width,
                    height: *height,
                    root: DockNode::Tabs(DockTabs::of(panel)),
                });
                true
            }
        }
    }

    /// Drops empty groups, floats and windows, collapses one-sided splits,
    /// removes duplicate ids and repairs out-of-range numbers. `Ui::dock`
    /// calls this, so hand-edited or deserialized layouts are safe to use.
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for tabs in self.all_groups_mut() {
            tabs.panels.retain(|id| seen.insert(id.clone()));
        }
        self.root = self.root.take().and_then(DockNode::normalized);
        self.floating.retain_mut(|float| {
            float.tabs.active = float
                .tabs
                .active
                .min(float.tabs.panels.len().saturating_sub(1));
            for value in [&mut float.x, &mut float.y] {
                if !value.is_finite() {
                    *value = 0.0;
                }
            }
            float.width = finite_or(float.width, 320.0).max(160.0);
            float.height = finite_or(float.height, 240.0).max(100.0);
            !float.tabs.panels.is_empty()
        });
        let mut ids = std::collections::HashSet::new();
        self.windows = std::mem::take(&mut self.windows)
            .into_iter()
            .filter_map(|window| {
                Some(DockWindow {
                    width: finite_or(window.width, 640.0).max(200.0),
                    height: finite_or(window.height, 480.0).max(150.0),
                    root: window.root.normalized()?,
                    id: window.id,
                })
            })
            .filter(|window| ids.insert(window.id))
            .collect();
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("dock v1\n");
        out.push_str("main {\n");
        if let Some(root) = &self.root {
            root.write(&mut out, 1);
        }
        out.push_str("}\n");
        for float in &self.floating {
            let _ = writeln!(
                out,
                "float {} {} {} {} {{",
                float.x, float.y, float.width, float.height
            );
            DockNode::Tabs(float.tabs.clone()).write(&mut out, 1);
            out.push_str("}\n");
        }
        for window in &self.windows {
            let _ = writeln!(
                out,
                "window {} {} {} {{",
                window.id.0, window.width, window.height
            );
            window.root.write(&mut out, 1);
            out.push_str("}\n");
        }
        out
    }

    pub fn parse(text: &str) -> Result<Self, DockLayoutError> {
        let mut parser = Parser {
            tokens: tokenize(text)?,
            position: 0,
            last_line: text.lines().count().max(1),
        };
        parser.word("dock")?;
        parser.word("v1")?;
        let mut layout = Self::default();
        let mut main_seen = false;
        while let Some((line, token)) = parser.next() {
            match token {
                Token::Word(word) if word == "main" => {
                    if std::mem::replace(&mut main_seen, true) {
                        return fail(line, "`main` appears twice");
                    }
                    parser.open()?;
                    if !parser.closes() {
                        layout.root = Some(parser.node()?);
                    }
                    parser.close()?;
                }
                Token::Word(word) if word == "float" => {
                    let [x, y, width, height] = [
                        parser.number()?,
                        parser.number()?,
                        parser.number()?,
                        parser.number()?,
                    ];
                    parser.open()?;
                    let DockNode::Tabs(tabs) = parser.node()? else {
                        return fail(line, "a float holds one `tabs` group");
                    };
                    parser.close()?;
                    layout.floating.push(DockFloat {
                        x,
                        y,
                        width,
                        height,
                        tabs,
                    });
                }
                Token::Word(word) if word == "window" => {
                    let id = parser.integer()?;
                    let (width, height) = (parser.number()?, parser.number()?);
                    parser.open()?;
                    let root = parser.node()?;
                    parser.close()?;
                    layout.windows.push(DockWindow {
                        id: DockWindowId(id as u64),
                        width,
                        height,
                        root,
                    });
                }
                other => return fail(line, format!("unexpected {other}")),
            }
        }
        layout.normalize();
        Ok(layout)
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn fail<T>(line: usize, message: impl Into<String>) -> Result<T, DockLayoutError> {
    Err(DockLayoutError {
        line,
        message: message.into(),
    })
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Open,
    Close,
    Word(String),
    Text(String),
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => f.write_str("`{`"),
            Self::Close => f.write_str("`}`"),
            Self::Word(word) => write!(f, "`{word}`"),
            Self::Text(text) => write!(f, "{}", quote(text)),
        }
    }
}

fn tokenize(text: &str) -> Result<Vec<(usize, Token)>, DockLayoutError> {
    let mut tokens = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let mut chars = line.chars().peekable();
        while let Some(&next) = chars.peek() {
            match next {
                _ if next.is_whitespace() => {
                    chars.next();
                }
                '/' => break, // comment to the end of the line
                '{' | '}' => {
                    chars.next();
                    tokens.push((
                        number,
                        if next == '{' {
                            Token::Open
                        } else {
                            Token::Close
                        },
                    ));
                }
                '"' => {
                    chars.next();
                    let mut value = String::new();
                    loop {
                        match chars.next() {
                            Some('"') => break,
                            Some('\\') => match chars.next() {
                                Some(escaped @ ('"' | '\\')) => value.push(escaped),
                                _ => return fail(number, "only \\\" and \\\\ can be escaped"),
                            },
                            Some(other) => value.push(other),
                            None => return fail(number, "unclosed string"),
                        }
                    }
                    tokens.push((number, Token::Text(value)));
                }
                _ => {
                    let mut word = String::new();
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                            break;
                        }
                        word.push(c);
                        chars.next();
                    }
                    tokens.push((number, Token::Word(word)));
                }
            }
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    position: usize,
    last_line: usize,
}

impl Parser {
    fn next(&mut self) -> Option<(usize, Token)> {
        let token = self.tokens.get(self.position).cloned();
        self.position += 1;
        token
    }
    fn expect(&mut self, what: &str) -> Result<(usize, Token), DockLayoutError> {
        let line = self.last_line;
        self.next()
            .ok_or(())
            .or_else(|()| fail(line, format!("expected {what}, found the end")))
    }
    fn word(&mut self, word: &str) -> Result<(), DockLayoutError> {
        match self.expect(&format!("`{word}`"))? {
            (_, Token::Word(found)) if found == word => Ok(()),
            (line, other) => fail(line, format!("expected `{word}`, found {other}")),
        }
    }
    fn open(&mut self) -> Result<(), DockLayoutError> {
        match self.expect("`{`")? {
            (_, Token::Open) => Ok(()),
            (line, other) => fail(line, format!("expected `{{`, found {other}")),
        }
    }
    fn close(&mut self) -> Result<(), DockLayoutError> {
        match self.expect("`}`")? {
            (_, Token::Close) => Ok(()),
            (line, other) => fail(line, format!("expected `}}`, found {other}")),
        }
    }
    fn closes(&self) -> bool {
        matches!(self.tokens.get(self.position), Some((_, Token::Close)))
    }
    fn number(&mut self) -> Result<f32, DockLayoutError> {
        match self.expect("a number")? {
            (line, Token::Word(word)) => word
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or(())
                .or_else(|()| fail(line, format!("invalid number `{word}`"))),
            (line, other) => fail(line, format!("expected a number, found {other}")),
        }
    }
    fn integer(&mut self) -> Result<usize, DockLayoutError> {
        match self.expect("an integer")? {
            (line, Token::Word(word)) => word
                .parse::<usize>()
                .or_else(|_| fail(line, format!("invalid integer `{word}`"))),
            (line, other) => fail(line, format!("expected an integer, found {other}")),
        }
    }
    fn node(&mut self) -> Result<DockNode, DockLayoutError> {
        match self.expect("`tabs` or `split`")? {
            (_, Token::Word(word)) if word == "tabs" => {
                let active = self.integer()?;
                let mut panels = Vec::new();
                while let Some((_, Token::Text(panel))) = self.tokens.get(self.position) {
                    panels.push(panel.clone());
                    self.position += 1;
                }
                Ok(DockNode::Tabs(DockTabs { panels, active }))
            }
            (line, Token::Word(word)) if word == "split" => {
                let axis = match self.expect("an axis")? {
                    (_, Token::Word(axis)) if axis == "horizontal" => Axis::Horizontal,
                    (_, Token::Word(axis)) if axis == "vertical" => Axis::Vertical,
                    (line, other) => {
                        return fail(
                            line,
                            format!("expected `horizontal` or `vertical`, found {other}"),
                        );
                    }
                };
                let fraction = self.number()?;
                self.open()?;
                if self.closes() {
                    return fail(line, "a split needs two children");
                }
                let first = self.node()?;
                if self.closes() {
                    return fail(line, "a split needs two children");
                }
                let second = self.node()?;
                self.close()?;
                Ok(DockNode::Split {
                    axis,
                    fraction,
                    first: Box::new(first),
                    second: Box::new(second),
                })
            }
            (line, other) => fail(line, format!("expected `tabs` or `split`, found {other}")),
        }
    }
}
