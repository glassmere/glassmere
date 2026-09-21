#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
        }
    }
    pub fn disabled(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Control {
    #[default]
    None,
    TextInput {
        label: String,
        value: String,
        placeholder: String,
    },
    Checkbox {
        label: String,
        checked: bool,
    },
    Radio {
        label: String,
        selected: bool,
    },
    Slider {
        label: String,
        value: f64,
        min: f64,
        max: f64,
        step: f64,
    },
    Combo {
        label: String,
        selected: usize,
        items: Vec<MenuItem>,
    },
    Menu {
        items: Vec<MenuItem>,
    },
    Popup {
        menu: bool,
    },
    FocusScope {
        scope: crate::FocusScope,
    },
    Dialog {
        title: String,
        modal: bool,
        dismissible: bool,
    },
    Tabs {
        count: usize,
        selected: usize,
    },
    TabList,
    Tab {
        index: usize,
        selected: bool,
        closable: bool,
    },
    TabPanel,
    Split {
        axis: crate::Axis,
    },
    Dock {
        area: crate::DockArea,
        native: bool,
    },
    DockGroup {
        /// Visible panel ids in tab order.
        panels: Vec<String>,
        float: bool,
    },
    DockFloat,
    RichText {
        runs: Vec<crate::RichRun>,
        /// Number of link runs; a block with links is focusable.
        links: usize,
    },
    /// A multi-line editor over an app-owned document: the declared window of
    /// lines plus what is needed to paint and navigate it.
    TextArea {
        label: String,
        lines: Vec<crate::text_area::AreaLine>,
        first_line: usize,
        total_lines: usize,
        selections: Vec<crate::text_area::AreaSelection>,
        selection_count: usize,
        primary: (usize, usize),
        font_size: f32,
        line_numbers: bool,
        read_only: bool,
        scroll_y: f32,
    },
    /// A floating group's grip (`resize: false`) or resize corner, with the
    /// float's declared rectangle. Floats are named by their first panel.
    DockHandle {
        float: String,
        resize: bool,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Divider {
        axis: crate::Axis,
        fraction: f32,
        min_first: f32,
        min_second: f32,
    },
    ContextMenu {
        items: Vec<MenuItem>,
    },
    Popover,
    Choice {
        index: usize,
        selected: bool,
        menu: bool,
    },
    Collection {
        label: String,
        kind: crate::collection::CollectionKind,
        count: usize,
        row_height: f32,
        offset: f32,
        selected: Option<crate::ItemId>,
    },
    CollectionRow {
        label: String,
        item: crate::ItemId,
        index: usize,
        total: usize,
        selected: bool,
        kind: crate::collection::CollectionKind,
        depth: usize,
        expanded: Option<bool>,
    },
    Grid {
        label: String,
        rows: usize,
        columns: usize,
    },
    GridCell {
        row: usize,
        column: usize,
        header: bool,
        sort: Option<crate::SortDirection>,
    },
    Command {
        shortcut: Option<crate::Shortcut>,
        while_editing: bool,
    },
    DragSource {
        label: String,
        payload: crate::DragPayload,
    },
    DropTarget {
        label: String,
        accepts: crate::DropKind,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum ControlUpdate {
    Text(String),
    Number(f64),
    Choice(usize),
    Collection(Vec<crate::collection::CollectionInput>),
    Drop(crate::DragPayload),
    Dismiss,
    Dock(DockAction),
    TextOps(Vec<crate::text_area::AreaOp>),
}

#[derive(Clone, Debug)]
pub(crate) enum DockAction {
    Move {
        panel: String,
        target: crate::DockTarget,
    },
    FloatRect {
        float: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Raise(String),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PopupState {
    pub owner: crate::NodeId,
    pub highlighted: usize,
    pub reveal_highlight: bool,
    /// Absolute logical anchor for context menus; `None` opens below the owner.
    pub anchor: Option<rust_gui_scene::Point>,
    /// Focus to restore when the owner itself is not focusable.
    pub return_focus: Option<crate::NodeId>,
}

pub(crate) fn slider_value(value: f64, min: f64, max: f64, step: f64) -> f64 {
    let value = if value.is_finite() { value } else { min };
    if value <= min {
        return min;
    }
    if value >= max {
        return max;
    }
    (min + ((value - min) / step).round() * step).clamp(min, max)
}
