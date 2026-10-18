#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
    /// Written along the end edge of the item, such as `Ctrl+S`.
    pub hint: String,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            hint: String::new(),
        }
    }
    pub fn disabled(label: impl Into<String>) -> Self {
        Self {
            enabled: false,
            ..Self::new(label)
        }
    }
    /// A menu item for a command: its label, whether it is enabled, and its
    /// shortcut as the hint. Pass `Ui::command(&command)` to honour a binding
    /// the user changed.
    pub fn command(command: &crate::Command) -> Self {
        Self {
            label: command.label.clone(),
            enabled: command.enabled,
            hint: command
                .shortcut
                .as_ref()
                .map_or_else(String::new, crate::Shortcut::describe),
        }
    }
}

/// Range, step and formatting of `Ui::number_input`.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberOptions {
    /// Unbounded by default.
    pub min: f64,
    pub max: f64,
    /// What one arrow key, wheel notch or button press adds, and the grid
    /// committed values snap to (counted from `min`, or from zero without
    /// one). Zero or less disables snapping.
    pub step: f64,
    /// Digits after the decimal point; `None` shows as many as `step` has.
    pub decimals: Option<usize>,
    /// Shown after the number, such as `" px"`; typing it is optional.
    pub suffix: String,
    /// Style of the whole field; 160 wide unless it says otherwise.
    pub style: crate::Style,
}

impl Default for NumberOptions {
    fn default() -> Self {
        Self {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            step: 1.0,
            decimals: None,
            suffix: String::new(),
            style: crate::Style::default(),
        }
    }
}

impl NumberOptions {
    pub fn range(mut self, min: f64, max: f64) -> Self {
        (self.min, self.max) = (min, max.max(min));
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = Some(decimals);
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    pub fn style(mut self, style: crate::Style) -> Self {
        self.style = style;
        self
    }

    fn places(&self) -> usize {
        self.decimals.unwrap_or_else(|| {
            // The fewest digits that write the step exactly, up to nine.
            (0..9)
                .find(|places| {
                    let scaled = self.step.abs() * 10f64.powi(*places);
                    (scaled - scaled.round()).abs() < 1e-9
                })
                .unwrap_or(9) as usize
        })
    }

    /// Clamps to the range, snaps to the step and drops floating-point dust.
    pub fn constrain(&self, value: f64) -> f64 {
        let origin = if self.min.is_finite() { self.min } else { 0.0 };
        if !value.is_finite() {
            return origin.min(self.max);
        }
        let snapped = if self.step > 0.0 {
            origin + ((value - origin) / self.step).round() * self.step
        } else {
            value
        };
        let scale = 1e9;
        ((snapped * scale).round() / scale).clamp(self.min, self.max)
    }

    /// Plain notation: no grouping and a decimal point. Widgets show numbers
    /// in the locale installed on their context instead; see `format_in`.
    pub fn format(&self, value: f64) -> String {
        format!(
            "{}{}",
            crate::locale_format::plain_number(value, self.places()),
            self.suffix
        )
    }

    /// The number as the context's locale writes it, with the suffix.
    pub fn format_in(&self, context: &crate::Context, value: f64) -> String {
        format!("{}{}", self.bare_in(context, value), self.suffix)
    }

    /// Without the suffix, for an editor whose unit is shown beside it.
    pub(crate) fn bare_in(&self, context: &crate::Context, value: f64) -> String {
        context.format_number(value, self.places())
    }

    /// The constrained number in text typed in the context's locale.
    pub fn parse_in(&self, context: &crate::Context, text: &str) -> Option<f64> {
        let text = text.trim();
        let text = text.strip_suffix(self.suffix.trim()).unwrap_or(text);
        context
            .parse_number(text)
            .map(|number| self.constrain(number))
    }

    /// The constrained number in `text`, which may carry the suffix and use a
    /// decimal comma; `None` when it is not a number.
    pub fn parse(&self, text: &str) -> Option<f64> {
        let text = text.trim();
        let text = text
            .strip_suffix(self.suffix.trim())
            .unwrap_or(text)
            .trim()
            .replace('\u{2212}', "-");
        let text = if text.contains('.') {
            text
        } else {
            text.replace(',', ".")
        };
        text.parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .map(|number| self.constrain(number))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Control {
    #[default]
    None,
    /// A memoized container, holding the hash of the state it was declared
    /// from. An unchanged hash keeps the subtree.
    Memo(u64),
    /// Text written along the end edge of a control, such as a shortcut.
    Hint(String),
    EventArea(crate::EventArea),
    /// A strip that moves the window, and maximizes it when pressed twice.
    TitleBar,
    /// The maximize button of a title bar the application draws. Windows
    /// asks where it is, to offer its snap layouts over it.
    MaximizeButton,
    /// An edge or a corner that resizes the window.
    ResizeGrip(crate::ResizeEdge),
    /// The track of a grid's sideways scrollbar. The grid scrolls its own
    /// columns, so a drag is passed on as how far the content moves for
    /// each pixel the thumb does.
    GridScrollbar {
        ratio: f32,
    },
    TextInput {
        label: String,
        value: String,
        placeholder: String,
        /// Number fields: the committed value with its range, which arrow
        /// keys, the wheel and assistive technology step.
        number: Option<crate::NumericValue>,
        /// Autocomplete fields: what the popup offers; `None` for plain inputs.
        suggestions: Option<Vec<MenuItem>>,
        /// Number fields: the text is what the user is typing, not a value
        /// the field formatted. Only such text is ever parsed back.
        draft: bool,
        password: bool,
    },
    /// A button the keyboard skips because the field beside it has the keys.
    Stepper,
    Toast {
        label: String,
        /// Errors interrupt a screen reader; the rest wait their turn.
        assertive: bool,
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
    /// A WYSIWYG editor over an app-owned `RichDocument`.
    RichEditor {
        label: String,
        blocks: Vec<crate::rich_editor::RichBlockView>,
        anchor: crate::RichPosition,
        cursor: crate::RichPosition,
        font_size: f32,
        read_only: bool,
    },
    Image {
        /// `None` while a file loads or after it failed to.
        image: Option<glassmere_scene::Image>,
        failed: bool,
        fit: crate::ImageFit,
        tint: crate::ImageTint,
    },
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
        marks: Vec<crate::text_area::AreaMark>,
        gutter_marks: Vec<(usize, crate::Severity)>,
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
    /// A handle at the end edge of something sized in pixels, such as a grid
    /// column: dragging it or pressing the arrows asks for a new size.
    Sizer {
        label: String,
        size: f32,
        min: f32,
        max: f32,
    },
    ContextMenu {
        items: Vec<MenuItem>,
    },
    Popover,
    Choice {
        /// Written along the end edge, such as a shortcut.
        hint: String,
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
        id: String,
        shortcut: Option<crate::Shortcut>,
        while_editing: bool,
        scoped: bool,
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
    DragEnded(crate::DragOutcome),
    Gesture(crate::GestureResponse),
    Dismiss,
    Dock(DockAction),
    TextOps(Vec<crate::text_area::TextOp>),
    RichOps(Vec<crate::rich_editor::RichOp>),
}

/// What a drag, click or menu pick in a dock asks of its layout. Applied by
/// [`crate::DockLayout::apply`].
#[derive(Clone, Debug)]
pub enum DockAction {
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
    /// A tab was selected in its group.
    Select(String),
    /// A panel was closed by its tab button, the Delete key or the panel menu.
    Close(String),
    /// A divider moved. `path` leads from the area's root to the split (0 is
    /// its first side, 1 its second); `fraction` is the first side's share.
    Ratio {
        area: crate::DockArea,
        path: Vec<usize>,
        fraction: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PopupState {
    pub owner: crate::NodeId,
    pub highlighted: usize,
    pub reveal_highlight: bool,
    /// Absolute logical anchor for context menus; `None` opens below the owner.
    pub anchor: Option<glassmere_scene::Point>,
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
