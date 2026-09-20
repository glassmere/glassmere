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
