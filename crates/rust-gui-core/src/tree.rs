use crate::{
    CustomWidget, Length, Shortcut, Style, Ui,
    focus::ScopeState,
    style::{Flow, Paint, ResolvedText},
    stylesheet::StyleSheet,
    theme::Theme,
    ui::{Element, Kind, WidgetKey},
};
use crate::{
    TextEdit, TextSelection,
    controls::{Control, ControlUpdate, PopupState},
    editor::Editor,
};
use rust_gui_scene::{
    Color, Point, Primitive, Rect, Scene, Size, TextMeasurer, TextStyle, TextWrap,
};
mod animation_state;
mod collection_input;
mod control_input;
mod control_paint;
pub mod diagnostics;
mod dock_input;
mod drag_input;
mod focus_input;
mod image;
mod overlay_input;
mod rich_editor;
mod rich_text;
mod text_area_input;
mod toasts;
mod touch_input;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
pub use touch_input::{GestureResponse, Pen, Swipe, SwipeDirection, TouchPhase};

/// Monotonic within a Context; a removed node's identity is never reused there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Dirty: u8 {
        const TREE = 1;
        const STYLE = 2;
        const LAYOUT = 4;
        const PAINT = 8;
        const ACCESSIBILITY = 16;
        const TEXT = 32;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Group,
    Label,
    Button,
    ScrollView,
    TextInput,
    Checkbox,
    Radio,
    Slider,
    ComboBox,
    MenuButton,
    Menu,
    ListBox,
    Option,
    MenuItem,
    Tree,
    TreeItem,
    Grid,
    Row,
    RowGroup,
    Cell,
    ColumnHeader,
    Dialog,
    TabList,
    Tab,
    TabPanel,
    Splitter,
    Image,
    SpinButton,
    /// A toast; `Alert` for errors.
    Status,
    Alert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Semantics {
    pub role: Role,
    pub label: String,
    pub disabled: bool,
    pub focusable: bool,
}

#[derive(Clone, Debug)]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub bounds: Rect,
    pub semantics: Semantics,
    pub focused: bool,
    pub clip: Rect,
    pub scroll: Option<ScrollState>,
    pub value: Option<String>,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub numeric: Option<NumericValue>,
    /// Set for toasts.
    pub live: Option<Live>,
    pub expanded: Option<bool>,
    pub active_descendant: Option<NodeId>,
    pub text_selection: Option<TextSelection>,
    pub is_popup: bool,
    /// A modal dialog layer; assistive technology treats it as modal.
    pub modal: bool,
    /// Tooltip text, exposed as the accessible description.
    pub description: Option<String>,
    /// Splitter dividers report the axis their panes are laid out along.
    pub orientation: Option<crate::Axis>,
    pub collection: Option<CollectionInfo>,
    pub drag: Option<crate::DragInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionInfo {
    pub item: Option<crate::ItemId>,
    pub index: usize,
    pub total: usize,
    pub level: usize,
    pub column: Option<usize>,
    pub columns: Option<usize>,
    pub sort: Option<crate::SortDirection>,
}

/// How assistive technology announces a node whose content appears on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Live {
    Polite,
    Assertive,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericValue {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    /// Vertical offset and how far it can go.
    pub offset: f32,
    pub maximum: f32,
    /// Horizontal offset from the start side (the left, or the right in a
    /// right-to-left container) and how far it can go.
    pub offset_x: f32,
    pub maximum_x: f32,
}

/// Actions from accessibility or another host use the same retained identities
/// and activation queue as pointer and keyboard input. Removed IDs are ignored.
#[derive(Clone, Debug)]
pub enum Action {
    Focus,
    Activate,
    ScrollBy(f32),
    ScrollTo(f32),
    /// Horizontal, in physical direction: positive moves the view right.
    ScrollByX(f32),
    /// Horizontal offset from the start side.
    ScrollToX(f32),
    ScrollIntoView,
    SetText(String),
    SetNumber(f64),
    Increment,
    Decrement,
    Expand,
    Collapse,
    StartDrag,
    Drop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Tab,
    Enter,
    Space,
    Escape,
    ArrowUp,
    ArrowDown,
    PageUp,
    PageDown,
    Home,
    End,
    ArrowLeft,
    ArrowRight,
    Backspace,
    Delete,
    /// The Menu key or Shift+F10.
    ContextMenu,
    /// Starts editing the current cell of a grid.
    F2,
}

#[derive(Clone, Debug)]
pub enum Event {
    PointerMoved(Point),
    PointerLeft,
    PointerDown,
    PointerDownWithShift,
    PointerUp,
    KeyDown {
        key: Key,
        shift: bool,
        repeat: bool,
    },
    KeyUp(Key),
    FocusLost,
    /// Logical pixels. Positive `delta_y` moves toward the end of content;
    /// positive `delta_x` moves the view to the right.
    Scroll {
        delta_x: f32,
        delta_y: f32,
    },
    Text(TextEdit),
    Shortcut(crate::Shortcut),
    /// The host supplies a logical drop point for external data.
    DropAt {
        position: Point,
        payload: crate::DragPayload,
    },
    /// A context-menu request (right click) at a logical point.
    ContextMenu(Point),
    /// A command for the focused code editor, such as find or add cursor.
    Editor(crate::EditorCommand),
    /// A finger or a pen. Each contact is a pointer of its own, told apart by
    /// `id`, which the host may reuse once the contact has ended.
    Touch {
        id: u64,
        phase: TouchPhase,
        position: Point,
        /// Pressure from 0 to 1, where the device reports it.
        force: Option<f32>,
        /// Tilt and buttons, for a pen that reports them.
        pen: Option<Pen>,
    },
    /// A pinch on a trackpad, or Ctrl with the wheel, for the gesture area
    /// under the pointer: 1.1 is a tenth larger. Not handled when there is no
    /// such area, so that a host can fall back to scrolling.
    Zoom(f32),
    /// A turn on a trackpad for the gesture area under the pointer, in
    /// clockwise radians.
    Rotate(f32),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    /// Queued activations consumed by this declaration. Hosts should declare
    /// once more after these callbacks so earlier labels see the updated state.
    pub activations: usize,
    pub created: usize,
    pub reused: usize,
    pub removed: usize,
    /// Subtrees `Ui::memo` kept without declaring them again.
    pub skipped: usize,
    pub text_measures: usize,
    /// Number of root layout requests (Taffy caches unchanged subtrees internally).
    pub layout_passes: usize,
    pub scene_rebuilt: bool,
    pub dirty: Dirty,
    pub paint_nodes: usize,
    pub paint_cache_hits: usize,
    /// Monotonic declaration counter for this context.
    pub frame: u64,
    /// CPU time spent running the view closure and reconciling the tree.
    pub declare: Duration,
    /// CPU time spent in layout and bounds propagation; zero when skipped.
    pub layout: Duration,
    /// CPU time spent generating and flattening paint chunks; zero when skipped.
    pub paint: Duration,
}

/// Inspector-facing facts about one node that ordinary semantics do not carry.
#[derive(Clone, Debug)]
pub struct NodeDetails {
    /// Explicit key, or `#index` for positional children.
    pub key: String,
    pub kind: String,
    pub text: String,
    pub style: Style,
    /// Text properties after inheritance from ancestors and the theme.
    pub resolved: ResolvedText,
    /// What the most recent declaration changed on this node; empty if untouched.
    pub changed: Dirty,
    /// Whether this node's paint chunk was regenerated by the most recent declaration.
    pub painted: bool,
}

struct PaintChunk {
    before: Vec<Primitive>,
    children: Vec<Arc<PaintChunk>>,
    after: Vec<Primitive>,
}

impl PaintChunk {
    fn append_to(&self, output: &mut Vec<Primitive>) {
        output.extend_from_slice(&self.before);
        for child in &self.children {
            child.append_to(output);
        }
        output.extend_from_slice(&self.after);
    }
}

struct Node {
    key: WidgetKey,
    kind: Kind,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    children_by_key: HashMap<WidgetKey, NodeId>,
    text: String,
    style: Style,
    enabled: bool,
    layout_id: taffy::NodeId,
    bounds: Rect,
    clip: Rect,
    scroll: ScrollState,
    custom: Option<CustomWidget>,
    paint_cache: Option<Arc<PaintChunk>>,
    control: Control,
    // Labels and other controls pay one pointer, not an inline editing history.
    editor: Option<Box<Editor>>,
    changed: Dirty,
    changed_frame: u64,
    painted_frame: u64,
    resolved: ResolvedText,
    scope: Option<Box<ScopeState>>,
    tooltip: Option<String>,
    anim: Option<Box<animation_state::NodeAnim>>,
    area: Option<Box<text_area_input::AreaState>>,
    rich: Option<Box<rich_text::RichState>>,
    rich_editor: Option<Box<rich_editor::RichEditorState>>,
    /// Width of the label of a control that is not a text leaf, for
    /// right-aligning it in right-to-left UI.
    label_width: f32,
    /// Width of a menu item's hint, which is written along the end edge.
    hint_width: f32,
    /// Consecutive frames in which all of this node's new children replaced
    /// as many removed ones.
    churn: u8,
    /// Parent-relative layout rectangle; layout animations start from the old one.
    layout_rect: Rect,
}

impl Node {
    fn text_style(&self) -> TextStyle {
        self.resolved.text_style(self.style.wrap)
    }

    fn focusable(&self) -> bool {
        self.enabled
            && self.control != Control::Stepper
            // Pointer-only helpers: a column sizer, whose keyboard path is its
            // header's menu, and drag sources or drop targets without a name.
            && !matches!(
                &self.control,
                Control::Sizer { .. }
            )
            && !matches!(
                &self.control,
                Control::DragSource { label, .. } | Control::DropTarget { label, .. }
                    if label.is_empty()
            )
            && (matches!(
                self.kind,
                Kind::Button
                    | Kind::Scroll
                    | Kind::TextInput
                    | Kind::Checkbox
                    | Kind::Radio
                    | Kind::Slider
                    | Kind::ComboBox
                    | Kind::MenuButton
                    | Kind::Choice
                    | Kind::Collection
                    | Kind::DragSource
                    | Kind::DropTarget
                    | Kind::Divider
                    | Kind::DockHandle
                    | Kind::TextArea
                    | Kind::RichEditor
            ) || matches!(self.control, Control::Tab { selected: true, .. })
                || matches!(self.control, Control::RichText { links: 1.., .. })
                || self.custom.as_ref().is_some_and(|widget| {
                    widget.semantics.role == Role::Button && widget.semantics.focusable
                }))
    }

    /// Whether a press can land on this node: anything focusable or
    /// activatable, and the pointer-only column sizer.
    fn takes_pointer(&self) -> bool {
        self.focusable()
            || self.activatable()
            || (self.enabled && matches!(self.control, Control::Sizer { .. }))
    }

    fn activatable(&self) -> bool {
        self.enabled
            && (matches!(
                self.kind,
                Kind::Button
                    | Kind::Checkbox
                    | Kind::Radio
                    | Kind::ComboBox
                    | Kind::MenuButton
                    | Kind::Choice
                    | Kind::CollectionRow
                    | Kind::Tab
            ) || self
                .custom
                .as_ref()
                .is_some_and(|widget| widget.semantics.role == Role::Button))
    }

    fn semantics(&self) -> Semantics {
        if let Some(widget) = &self.custom {
            let mut semantics = widget.semantics.clone();
            semantics.focusable = self.focusable();
            return semantics;
        }
        Semantics {
            role: match &self.control {
                Control::Collection { kind, .. } => match kind {
                    crate::collection::CollectionKind::List => Role::ListBox,
                    crate::collection::CollectionKind::Tree => Role::Tree,
                    crate::collection::CollectionKind::Grid => Role::RowGroup,
                },
                Control::CollectionRow { kind, .. } => match kind {
                    crate::collection::CollectionKind::List => Role::Option,
                    crate::collection::CollectionKind::Tree => Role::TreeItem,
                    crate::collection::CollectionKind::Grid => Role::Row,
                },
                Control::Grid { .. } => Role::Grid,
                Control::GridCell { header: true, .. } => Role::ColumnHeader,
                Control::GridCell { .. } => Role::Cell,
                Control::TabList => Role::TabList,
                Control::Tab { .. } => Role::Tab,
                Control::TabPanel => Role::TabPanel,
                Control::Divider { .. } | Control::Sizer { .. } => Role::Splitter,
                Control::TextInput {
                    number: Some(_), ..
                } => Role::SpinButton,
                Control::Toast {
                    assertive: true, ..
                } => Role::Alert,
                Control::Toast { .. } => Role::Status,
                _ => match self.kind {
                    Kind::Label | Kind::RichText => Role::Label,
                    // An image without a name is decorative.
                    Kind::Image if !self.text.is_empty() => Role::Image,
                    Kind::Button => Role::Button,
                    Kind::Scroll => Role::ScrollView,
                    Kind::TextInput | Kind::TextArea | Kind::RichEditor => Role::TextInput,
                    Kind::Checkbox => Role::Checkbox,
                    Kind::Radio => Role::Radio,
                    Kind::Slider => Role::Slider,
                    Kind::ComboBox => Role::ComboBox,
                    Kind::MenuButton => Role::MenuButton,
                    Kind::Dialog => Role::Dialog,
                    Kind::Popup => {
                        if matches!(self.control, Control::Popup { menu: true }) {
                            Role::Menu
                        } else {
                            Role::ListBox
                        }
                    }
                    Kind::Choice => {
                        if matches!(self.control, Control::Choice { menu: true, .. }) {
                            Role::MenuItem
                        } else {
                            Role::Option
                        }
                    }
                    _ => Role::Group,
                },
            },
            label: match &self.control {
                Control::TextInput { label, .. }
                | Control::Checkbox { label, .. }
                | Control::Radio { label, .. }
                | Control::Slider { label, .. }
                | Control::Combo { label, .. }
                | Control::Collection { label, .. }
                | Control::CollectionRow { label, .. }
                | Control::TextArea { label, .. }
                | Control::RichEditor { label, .. }
                | Control::Grid { label, .. } => label.clone(),
                Control::DragSource { label, .. } | Control::DropTarget { label, .. } => {
                    label.clone()
                }
                Control::Dialog { title, .. } => title.clone(),
                Control::Toast { label, .. } => label.clone(),
                Control::Divider { .. } => "Splitter".into(),
                Control::Sizer { label, .. } => label.clone(),
                Control::DockFloat => "Floating panel".into(),
                Control::DockHandle { resize: false, .. } => "Move floating panel".into(),
                Control::DockHandle { resize: true, .. } => "Resize floating panel".into(),
                // A shortcut is written beside the item, and read with it.
                Control::Choice { hint, .. } | Control::Hint(hint) if !hint.is_empty() => {
                    format!("{}, {hint}", self.text)
                }
                _ => self.text.clone(),
            },
            disabled: !self.enabled,
            focusable: self.focusable(),
        }
    }
}

/// A file the host keeps current for `Ui::watched_bytes`.
struct Watched {
    bytes: Arc<[u8]>,
    text: Option<Arc<str>>,
    error: Option<String>,
}

pub struct Context {
    nodes: HashMap<NodeId, Node>,
    layout: taffy::TaffyTree<Measurement>,
    root: NodeId,
    frame: u64,
    next_id: u64,
    hovered: Option<NodeId>,
    focused: Option<NodeId>,
    captured: Option<NodeId>,
    keyboard_pressed: Option<(NodeId, Key)>,
    pointer: Option<Point>,
    clicks: HashSet<NodeId>,
    pending: Dirty,
    scene: Scene,
    stats: FrameStats,
    geometry_dirty: bool,
    pending_damage: Option<Rect>,
    damage: Vec<Rect>,
    updates: HashMap<NodeId, ControlUpdate>,
    bindings: HashMap<String, Option<Shortcut>>,
    /// Event areas of the last declaration, and what has reached each of them
    /// since. Both are empty in a view that declares none, and the dispatch
    /// then costs a lookup.
    areas: HashMap<NodeId, crate::EventArea>,
    /// A cell because `Ui` holds the context by a shared reference.
    pending_area_events: std::cell::RefCell<HashMap<NodeId, Vec<crate::UiEvent>>>,
    /// Nodes that contain something with input waiting, so that a memoized
    /// container above it declares again. Rebuilt for each declaration.
    awaiting: HashSet<NodeId>,
    /// Subtrees the declaration just skipped. A cell because `Ui` only holds
    /// a shared reference to the context.
    skipped: std::cell::Cell<usize>,
    submitted: HashSet<NodeId>,
    pub(crate) popup: Option<PopupState>,
    drag: Option<Box<crate::drag::DragState>>,
    /// Source of a drag the host handed to the operating system.
    exported_drag: Option<NodeId>,
    dock_drag: Option<Box<dock_input::DockDrag>>,
    dock_grab: Option<dock_input::DockGrab>,
    clipboard_out: Option<String>,
    redraw_requested: bool,
    caret_movement: crate::CaretMovement,
    direction: crate::Direction,
    watched: HashMap<std::path::PathBuf, Watched>,
    watch_requests: Vec<std::path::PathBuf>,
    images: HashMap<std::path::PathBuf, image::LoadedImage>,
    image_requests: Vec<std::path::PathBuf>,
    hot_save: bool,
    hot_state: Option<String>,
    diagnostic: diagnostics::DiagnosticState,
    toast: toasts::ToastState,
    touches: HashMap<u64, touch_input::Touch>,
    /// When and where the last tap ended, for telling a double tap.
    last_tap: Option<(Duration, Point)>,
    fling: Option<touch_input::Fling>,
    pub(crate) messages: Option<Box<dyn crate::Messages>>,
    pub(crate) format: Option<Box<dyn crate::LocaleFormat>>,
    pub(crate) missing_messages: std::sync::Mutex<Vec<String>>,
    theme: Theme,
    stylesheet: StyleSheet,
    style_warnings: Vec<String>,
    noted_focus: Option<NodeId>,
    pending_restore: Option<NodeId>,
    now: Duration,
    hover_since: Option<(NodeId, Duration)>,
    tooltip: Option<NodeId>,
    new_popovers: Vec<NodeId>,
    reduced_motion: bool,
    resizing: bool,
    animating_nodes: Vec<NodeId>,
    ghosts: Vec<animation_state::Ghost>,
}

#[derive(Clone, Debug, PartialEq)]
struct Measurement {
    size: Size,
    wrapped: Option<(String, TextStyle)>,
    /// Rich blocks measure their styled runs instead of the plain text.
    spans: Option<Vec<rust_gui_scene::TextSpan>>,
}

fn layout_style(style: &Style, kind: Kind, direction: crate::Direction) -> taffy::Style {
    let mut result = style.layout(match kind {
        Kind::Row | Kind::SplitRow | Kind::TabList | Kind::Toast => Flow::Row,
        Kind::Grid => Flow::Grid,
        _ => Flow::Column,
    });
    let rtl = direction == crate::Direction::Rtl;
    if rtl {
        result.direction = taffy::Direction::Rtl;
    }
    if matches!(kind, Kind::Popup | Kind::Popover) {
        result.position = taffy::Position::Absolute;
        result.inset.top = taffy::LengthPercentageAuto::percent(1.0);
        // Overlays hang from the start side of their owner.
        if rtl {
            result.inset.right = taffy::LengthPercentageAuto::length(0.0);
        } else {
            result.inset.left = taffy::LengthPercentageAuto::length(0.0);
        }
    }
    if kind == Kind::Toasts {
        // The bottom corner at the end of the reading direction.
        result.position = taffy::Position::Absolute;
        result.inset.bottom = taffy::LengthPercentageAuto::length(16.0);
        if rtl {
            result.inset.left = taffy::LengthPercentageAuto::length(16.0);
        } else {
            result.inset.right = taffy::LengthPercentageAuto::length(16.0);
        }
    }
    if kind == Kind::Dialog {
        result.position = taffy::Position::Absolute;
        result.inset.top = taffy::LengthPercentageAuto::length(0.0);
        result.inset.left = taffy::LengthPercentageAuto::length(0.0);
    }
    result
}

/// A text leaf that anchors an overlay (popup or popover) gains children, and
/// Taffy then ignores its measure context; keep its intrinsic size as a minimum.
fn anchor_minimum(style: &mut taffy::Style, element: &Element, measured: Size) {
    if element.kind.text_leaf() && !element.children.is_empty() {
        let padding = element.style.padding.max(0.0) * 2.0;
        style.min_size = taffy::Size {
            width: taffy::Dimension::length(measured.width + padding),
            height: taffy::Dimension::length(measured.height + padding),
        };
    }
}

impl Measurement {
    fn fixed(size: Size) -> Self {
        Self {
            size,
            wrapped: None,
            spans: None,
        }
    }
    fn text(text: &str, text_style: TextStyle, measurer: &mut impl TextMeasurer) -> Self {
        Self {
            size: measurer.measure_styled(text, &text_style, None),
            wrapped: (text_style.wrap == TextWrap::Word).then(|| (text.to_owned(), text_style)),
            spans: None,
        }
    }
    /// An image's natural size; nothing until a file has loaded.
    fn image(element: &Element) -> Option<Self> {
        let Control::Image { image, .. } = &element.control else {
            return None;
        };
        Some(Self::fixed(
            image
                .as_ref()
                .map_or(Size::default(), rust_gui_scene::Image::size),
        ))
    }
    /// A text leaf: plain, or the styled runs of a rich block.
    fn leaf(element: &Element, text_style: TextStyle, measurer: &mut impl TextMeasurer) -> Self {
        let Control::RichText { runs, .. } = &element.control else {
            return Self::text(&element.text, text_style, measurer);
        };
        let spans = rich_text::spans(runs, Color::TRANSPARENT);
        Self {
            size: measurer.geometry_rich(&spans, &text_style, None).size,
            wrapped: (text_style.wrap == TextWrap::Word).then(|| (String::new(), text_style)),
            spans: Some(spans),
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        let mut layout = taffy::TaffyTree::new();
        layout.disable_rounding();
        let theme = Theme::dark();
        let style = Style::default();
        let layout_id = layout
            .new_leaf(style.layout(Flow::Column))
            .expect("new root layout");
        let root = NodeId(1);
        let node = Node {
            key: WidgetKey::Named("$root".into()),
            kind: Kind::Column,
            parent: None,
            children: Vec::new(),
            children_by_key: HashMap::new(),
            text: String::new(),
            style,
            enabled: true,
            layout_id,
            bounds: Rect::default(),
            clip: Rect::default(),
            scroll: ScrollState::default(),
            custom: None,
            paint_cache: None,
            anim: None,
            area: None,
            rich: None,
            rich_editor: None,
            label_width: 0.0,
            hint_width: 0.0,
            churn: 0,
            layout_rect: Rect::default(),
            control: Control::None,
            editor: None,
            changed: Dirty::all(),
            changed_frame: 0,
            painted_frame: 0,
            resolved: ResolvedText::from_theme(&theme),
            scope: None,
            tooltip: None,
        };
        Self {
            nodes: HashMap::from([(root, node)]),
            layout,
            root,
            frame: 0,
            next_id: 2,
            hovered: None,
            focused: None,
            captured: None,
            keyboard_pressed: None,
            pointer: None,
            clicks: HashSet::new(),
            pending: Dirty::all(),
            scene: Scene::default(),
            stats: FrameStats::default(),
            geometry_dirty: false,
            pending_damage: None,
            damage: Vec::new(),
            updates: HashMap::new(),
            bindings: HashMap::new(),
            areas: HashMap::new(),
            pending_area_events: std::cell::RefCell::new(HashMap::new()),
            awaiting: HashSet::new(),
            skipped: std::cell::Cell::new(0),
            submitted: HashSet::new(),
            popup: None,
            drag: None,
            exported_drag: None,
            dock_drag: None,
            dock_grab: None,
            clipboard_out: None,
            redraw_requested: false,
            caret_movement: crate::CaretMovement::Visual,
            direction: crate::Direction::Ltr,
            watched: HashMap::new(),
            images: HashMap::new(),
            image_requests: Vec::new(),
            watch_requests: Vec::new(),
            hot_save: false,
            hot_state: None,
            diagnostic: diagnostics::DiagnosticState::new(),
            toast: toasts::ToastState::default(),
            touches: HashMap::new(),
            last_tap: None,
            fling: None,
            messages: None,
            format: None,
            missing_messages: std::sync::Mutex::new(Vec::new()),
            theme,
            stylesheet: StyleSheet::default(),
            style_warnings: Vec::new(),
            noted_focus: None,
            pending_restore: None,
            now: Duration::ZERO,
            hover_since: None,
            tooltip: None,
            new_popovers: Vec::new(),
            reduced_motion: false,
            resizing: false,
            animating_nodes: Vec::new(),
            ghosts: Vec::new(),
        }
    }
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn theme(&self) -> &Theme {
        &self.theme
    }
    /// Switches design tokens at runtime. Paint is regenerated; layout only
    /// when the resolved fonts change.
    pub fn set_theme(&mut self, theme: Theme) {
        let theme = self.stylesheet.apply_to(&theme);
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        self.repaint_all();
    }

    /// Everything repaints and the accessibility tree is rebuilt: what a new
    /// theme or a new language needs.
    pub(crate) fn repaint_all(&mut self) {
        for node in self.nodes.values_mut() {
            node.paint_cache = None;
        }
        self.pending |= Dirty::STYLE | Dirty::PAINT | Dirty::ACCESSIBILITY;
    }
    pub fn stylesheet(&self) -> &StyleSheet {
        &self.stylesheet
    }
    /// Whether Left and Right move on screen (the default) or in reading
    /// order inside bidirectional text. Applies to every text editor.
    pub fn set_caret_movement(&mut self, movement: crate::CaretMovement) {
        self.caret_movement = movement;
    }

    pub fn caret_movement(&self) -> crate::CaretMovement {
        self.caret_movement
    }

    /// Files the last declaration asked for through `Ui::watched_bytes`. Hosts
    /// load them, watch them for changes and report through `set_watched_file`.
    pub fn watched_paths(&self) -> &[std::path::PathBuf] {
        &self.watch_requests
    }

    /// Whether the host has delivered this file (or an error for it) yet.
    pub fn watched_known(&self, path: &std::path::Path) -> bool {
        self.watched.contains_key(path)
    }

    /// Delivers a file's content or a read error. An error keeps the last good
    /// content. Returns whether a declaration should follow.
    pub fn set_watched_file(
        &mut self,
        path: &std::path::Path,
        content: std::result::Result<Vec<u8>, String>,
    ) -> bool {
        let old = self.watched.get(path);
        let changed = match content {
            Ok(bytes) => {
                if old.is_some_and(|old| *old.bytes == *bytes && old.error.is_none()) {
                    return false;
                }
                let text = std::str::from_utf8(&bytes).ok().map(Arc::from);
                self.watched.insert(
                    path.to_owned(),
                    Watched {
                        bytes: bytes.into(),
                        text,
                        error: None,
                    },
                );
                true
            }
            Err(error) => {
                let entry = self.watched.entry(path.to_owned()).or_insert(Watched {
                    bytes: Arc::from([]),
                    text: None,
                    error: None,
                });
                let first = entry.error.as_deref() != Some(&error);
                entry.error = Some(error);
                first
            }
        };
        if changed {
            self.pending |= Dirty::PAINT;
        }
        changed
    }

    /// Why the last load of a watched file failed, if it did.
    pub fn watch_error(&self, path: &std::path::Path) -> Option<&str> {
        self.watched.get(path)?.error.as_deref()
    }

    pub(crate) fn watched_bytes(&self, path: &std::path::Path) -> Option<Arc<[u8]>> {
        let watched = self.watched.get(path)?;
        // A file that never loaded has no content to show.
        (watched.error.is_none() || !watched.bytes.is_empty()).then(|| watched.bytes.clone())
    }

    pub(crate) fn watched_text(&self, path: &std::path::Path) -> Option<Arc<str>> {
        self.watched.get(path)?.text.clone()
    }

    /// Asks the next declaration to save the app's state through
    /// `Ui::hot_state`; hosts call this before a development restart.
    pub fn request_hot_save(&mut self) {
        self.hot_save = true;
    }

    pub(crate) fn hot_save_requested(&self) -> bool {
        self.hot_save
    }

    /// The state text the last declaration saved, once.
    pub fn take_hot_state(&mut self) -> Option<String> {
        self.hot_state.take()
    }

    /// The direction of the whole window; subtrees override it with
    /// `Style::direction`. Layout, text alignment and widget internals follow.
    pub fn set_direction(&mut self, direction: crate::Direction) {
        if self.direction == direction {
            return;
        }
        self.direction = direction;
        let root = self.nodes[&self.root].layout_id;
        let mut style = self.layout.style(root).unwrap().clone();
        style.direction = match direction {
            crate::Direction::Ltr => taffy::Direction::Ltr,
            crate::Direction::Rtl => taffy::Direction::Rtl,
        };
        self.layout.set_style(root, style).unwrap();
        for node in self.nodes.values_mut() {
            node.paint_cache = None;
        }
        self.pending |= Dirty::all();
    }

    pub fn direction(&self) -> crate::Direction {
        self.direction
    }

    fn root_text(&self) -> ResolvedText {
        ResolvedText {
            direction: self.direction,
            ..ResolvedText::from_theme(&self.theme)
        }
    }

    /// Installs named classes and, if the sheet declares one, its theme.
    pub fn set_stylesheet(&mut self, sheet: StyleSheet) {
        self.stylesheet = sheet;
        let theme = self.stylesheet.apply_to(&self.theme);
        self.set_theme(theme);
        self.style_warnings.clear();
        self.pending |= Dirty::PAINT;
    }
    /// Unknown class names requested since the stylesheet was installed.
    pub fn style_warnings(&self) -> &[String] {
        &self.style_warnings
    }
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }
    pub fn hovered(&self) -> Option<NodeId> {
        self.hovered
    }
    pub fn captured(&self) -> Option<NodeId> {
        self.captured
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn stats(&self) -> FrameStats {
        self.stats
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Conservative logical damage from the most recent declaration. The GPU
    /// host still presents a full surface; partial presentation is a later step.
    pub fn damage_regions(&self) -> &[Rect] {
        &self.damage
    }

    /// Parent-first snapshots for platform accessibility adapters and inspectors.
    pub fn snapshots(&self) -> Vec<NodeSnapshot> {
        let mut order = Vec::with_capacity(self.nodes.len());
        self.order(self.root, &mut order);
        order.into_iter().filter_map(|id| self.node(id)).collect()
    }

    pub fn node(&self, id: NodeId) -> Option<NodeSnapshot> {
        self.nodes.get(&id).map(|node| NodeSnapshot {
            id,
            parent: node.parent,
            children: node.children.clone(),
            bounds: node.bounds,
            semantics: {
                let mut semantics = node.semantics();
                let key = match node.control {
                    Control::Divider { .. } => Some("rust-gui-splitter"),
                    Control::DockFloat => Some("rust-gui-dock-floating"),
                    Control::DockHandle { resize: false, .. } => Some("rust-gui-dock-move"),
                    Control::DockHandle { resize: true, .. } => Some("rust-gui-dock-resize"),
                    _ => None,
                };
                if let Some(key) = key {
                    semantics.label = self.tr(key, &[]);
                }
                semantics
            },
            focused: self.focused == Some(id),
            clip: node.clip,
            scroll: matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection).then_some(node.scroll),
            value: match &node.control {
                Control::TextInput { value, .. } => Some(value.clone()),
                // Assistive technology reads the caret's line.
                Control::TextArea {
                    lines,
                    first_line,
                    primary,
                    ..
                } => primary
                    .0
                    .checked_sub(*first_line)
                    .and_then(|index| lines.get(index))
                    .map(|line| line.text.clone()),
                Control::RichEditor { blocks, cursor, .. } => {
                    blocks.get(cursor.block).map(|block| block.text.clone())
                }
                Control::Combo {
                    items, selected, ..
                } => items.get(*selected).map(|item| item.label.clone()),
                _ => None,
            },
            checked: match node.control {
                Control::Checkbox { checked, .. } => Some(checked),
                _ => None,
            },
            selected: match node.control {
                Control::Radio { selected, .. }
                | Control::Tab { selected, .. }
                | Control::CollectionRow { selected, .. }
                | Control::Choice {
                    selected,
                    menu: false,
                    ..
                } => Some(selected),
                _ => None,
            },
            numeric: match node.control {
                Control::Slider {
                    value,
                    min,
                    max,
                    step,
                    ..
                } => Some(NumericValue {
                    value,
                    min,
                    max,
                    step,
                }),
                Control::TextInput { number, .. } => number,
                Control::Sizer { size, min, max, .. } => Some(NumericValue {
                    value: f64::from(size),
                    min: f64::from(min),
                    max: f64::from(max),
                    step: 10.0,
                }),
                Control::Divider { fraction, .. } => Some(NumericValue {
                    value: f64::from(fraction) * 100.0,
                    min: 0.0,
                    max: 100.0,
                    step: 1.0,
                }),
                _ => None,
            },
            live: match node.control {
                Control::Toast {
                    assertive: true, ..
                } => Some(Live::Assertive),
                Control::Toast { .. } => Some(Live::Polite),
                _ => None,
            },
            expanded: if let Control::CollectionRow { expanded, .. } = node.control { expanded } else {
                let suggests = matches!(node.control, Control::TextInput { suggestions: Some(_), .. });
                // An autocomplete list is open only while it has rows to show.
                (suggests || matches!(node.kind, Kind::ComboBox | Kind::MenuButton)).then_some(
                    self.popup.is_some_and(|popup| popup.owner == id)
                        && (!suggests || !node.children.is_empty()),
                )
            },
            active_descendant: self
                .popup
                .filter(|popup| popup.owner == id)
                .and_then(|popup| {
                    node.children
                        .first()
                        .and_then(|panel| self.nodes[panel].children.get(popup.highlighted))
                        .copied()
                }).or_else(|| if let Control::Collection { selected: Some(selected), .. } = node.control {
                    node.children.iter().find(|id| matches!(self.nodes[id].control, Control::CollectionRow { item, .. } if item == selected)).copied()
                } else { None }),
            text_selection: node.editor.as_ref().map(|editor| editor.selection),
            is_popup: matches!(node.kind, Kind::Popup | Kind::Popover),
            modal: matches!(node.control, Control::Dialog { modal: true, .. }),
            description: node.tooltip.clone(),
            orientation: match node.control {
                Control::Divider { axis, .. } => Some(axis),
                Control::Sizer { .. } => Some(crate::Axis::Horizontal),
                _ => None,
            },
            drag: match node.control {
                Control::DragSource { .. } => Some(crate::DragInfo { source: node.enabled, can_drop: false, dragging: self.drag.as_ref().is_some_and(|drag| drag.source == id && drag.active) }),
                Control::DropTarget { accepts, .. } => Some(crate::DragInfo { source: false, can_drop: node.enabled && self.drag.as_ref().is_some_and(|drag| drag.active && drag.payload.kind() == accepts), dragging: false }),
                _ => None,
            },
            collection: match node.control {
                Control::Collection { count, .. } => Some(CollectionInfo { item: None, index: 0, total: count, level: 0, column: None, columns: None, sort: None }),
                Control::CollectionRow { item, index, total, depth, .. } => Some(CollectionInfo { item: Some(item), index, total, level: depth + 1, column: None, columns: None, sort: None }),
                Control::Grid { rows, columns, .. } => Some(CollectionInfo { item: None, index: 0, total: rows, level: 0, column: None, columns: Some(columns), sort: None }),
                Control::GridCell { row, column, sort, .. } => Some(CollectionInfo { item: None, index: row, total: 0, level: 0, column: Some(column), columns: None, sort }),
                _ => None,
            },
        })
    }

    /// Key, kind, style and change facts for inspectors and test harnesses.
    pub fn details(&self, id: NodeId) -> Option<NodeDetails> {
        self.nodes.get(&id).map(|node| NodeDetails {
            key: match &node.key {
                WidgetKey::Named(name) => name.clone(),
                WidgetKey::Position(index) => format!("#{index}"),
            },
            kind: format!("{:?}", node.kind),
            text: node.text.clone(),
            style: node.style.clone(),
            resolved: node.resolved.clone(),
            changed: if node.changed_frame == self.frame {
                node.changed
            } else {
                Dirty::empty()
            },
            painted: node.painted_frame == self.frame,
        })
    }

    /// Find explicit keys relative to the root, e.g. &["main", "save"].
    pub fn find(&self, path: &[&str]) -> Option<NodeId> {
        let mut id = self.root;
        for key in path {
            id = *self.nodes[&id]
                .children_by_key
                .get(&WidgetKey::Named((*key).into()))?;
        }
        Some(id)
    }

    /// What has reached an event area since its last declaration.
    pub(crate) fn take_area_events(&self, id: NodeId) -> Vec<crate::UiEvent> {
        self.pending_area_events
            .borrow_mut()
            .remove(&id)
            .unwrap_or_default()
    }

    /// The path from the root down to `id`.
    fn path_to(&self, id: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut next = Some(id);
        while let Some(current) = next {
            path.push(current);
            next = self.nodes.get(&current).and_then(|node| node.parent);
        }
        path.reverse();
        path
    }

    /// Offers an event to the areas along a path, first going down to the
    /// node it concerns and then coming back up. True when one consumed it,
    /// and the widgets under it must not see it.
    fn travel(&mut self, event: crate::UiEvent, path: &[NodeId]) -> bool {
        let mut arrived: Vec<(NodeId, bool)> = Vec::new();
        let mut consumed = false;
        // Down first: an area that captures may take the event away from the
        // widgets inside it.
        for id in path {
            let Some(area) = self.areas.get(id) else {
                continue;
            };
            if area.capture && area.watches(&event) {
                arrived.push((*id, area.consume));
                if area.consume {
                    consumed = true;
                    break;
                }
            }
        }
        if !consumed {
            // Then up, innermost first, over what the widgets left.
            for id in path.iter().rev() {
                let Some(area) = self.areas.get(id) else {
                    continue;
                };
                if !area.capture && area.watches(&event) {
                    arrived.push((*id, area.consume));
                    if area.consume {
                        break;
                    }
                }
            }
        }
        if arrived.is_empty() {
            return false;
        }
        let mut pending = self.pending_area_events.borrow_mut();
        for (id, _) in &arrived {
            pending.entry(*id).or_default().push(event.clone());
        }
        drop(pending);
        // Something is waiting for a declaration that will report it.
        self.stats.activations += 1;
        consumed
    }

    /// The innermost node a point is inside, whatever it is. Areas contain
    /// things rather than taking the pointer themselves, so this is not the
    /// hit test.
    fn deepest_at(&self, point: Point) -> Option<NodeId> {
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        order
            .into_iter()
            .rev()
            .find(|id| self.nodes[id].clip.contains(point))
    }

    /// Where an event goes: what it is over, or what has the focus.
    fn travelled(&mut self, event: &Event) -> bool {
        if self.areas.is_empty() {
            return false;
        }
        let pointer = self.pointer;
        let (carried, target) = match event {
            Event::PointerDown | Event::PointerDownWithShift => (
                pointer.map(crate::UiEvent::PointerDown),
                pointer.and_then(|at| self.deepest_at(at)),
            ),
            Event::PointerUp => (
                pointer.map(crate::UiEvent::PointerUp),
                pointer.and_then(|at| self.deepest_at(at)),
            ),
            Event::PointerMoved(at) => (
                Some(crate::UiEvent::PointerMoved(*at)),
                self.deepest_at(*at),
            ),
            Event::Scroll { delta_x, delta_y } => (
                Some(crate::UiEvent::Scroll {
                    delta_x: *delta_x,
                    delta_y: *delta_y,
                }),
                pointer.and_then(|at| self.deepest_at(at)),
            ),
            Event::KeyDown { key, shift, .. } => (
                Some(crate::UiEvent::Key {
                    key: *key,
                    shift: *shift,
                }),
                self.focused,
            ),
            Event::Text(TextEdit::Insert(text)) => {
                (Some(crate::UiEvent::Text(text.clone())), self.focused)
            }
            _ => (None, None),
        };
        let Some(carried) = carried else {
            return false;
        };
        // Without a node under it, an event still belongs to the window.
        let path = target.map_or_else(|| vec![self.root], |id| self.path_to(id));
        let consumed = self.travel(carried, &path);
        // Where the pointer is remains the window's business: an area may
        // keep a move from the widgets, but not from the context.
        if consumed && let Event::PointerMoved(at) = event {
            self.pointer = Some(*at);
        }
        consumed
    }

    /// Sends what `Ui::emit` left, from the node that emitted it.
    fn deliver_emitted(&mut self, emitted: Vec<(NodeId, crate::UiEvent)>) {
        for (from, event) in emitted {
            if self.nodes.contains_key(&from) {
                let path = self.path_to(from);
                self.travel(event, &path);
            }
        }
    }

    /// How many parents a node has, for choosing the innermost of two.
    pub(crate) fn depth(&self, id: NodeId) -> usize {
        let mut depth = 0;
        let mut next = self.nodes.get(&id).and_then(|node| node.parent);
        while let Some(current) = next {
            depth += 1;
            next = self.nodes.get(&current).and_then(|node| node.parent);
        }
        depth
    }

    /// Bindings the user changed, by command id. `None` unbinds a command.
    pub fn set_binding(&mut self, id: impl Into<String>, shortcut: Option<Shortcut>) {
        self.bindings.insert(id.into(), shortcut);
        self.pending |= Dirty::PAINT;
        self.stats.activations += 1;
    }

    pub fn clear_bindings(&mut self) {
        self.bindings.clear();
        self.stats.activations += 1;
    }

    /// What the user bound a command to, when anything.
    pub fn binding(&self, id: &str) -> Option<&Option<Shortcut>> {
        self.bindings.get(id)
    }

    /// The state hash a memoized container was declared from.
    pub(crate) fn memo_hash(&self, id: NodeId) -> Option<u64> {
        match self.nodes.get(&id)?.control {
            Control::Memo(hash) => Some(hash),
            _ => None,
        }
    }

    /// Whether anything in the subtree has input waiting to be applied. Kept
    /// as a set of ancestors so that asking is a lookup, not a walk.
    pub(crate) fn awaits_declaration(&self, id: NodeId) -> bool {
        self.awaiting.contains(&id)
    }

    pub(crate) fn count_skipped(&self) {
        self.skipped.set(self.skipped.get() + 1);
    }

    pub(crate) fn child(
        &self,
        parent: Option<NodeId>,
        key: &WidgetKey,
        kind: Kind,
    ) -> Option<NodeId> {
        let id = *self.nodes.get(&parent?)?.children_by_key.get(key)?;
        (self.nodes[&id].kind == kind).then_some(id)
    }

    /// Declare once after an event or application update. An unchanged declaration
    /// reuses nodes and skips both layout and scene generation.
    pub fn run(
        &mut self,
        size: Size,
        measurer: &mut impl TextMeasurer,
        view: impl FnOnce(&mut Ui<'_>),
    ) -> FrameStats {
        self.stats = FrameStats::default();
        self.frame += 1;
        self.stats.frame = self.frame;
        self.damage.clear();
        self.resizing = size != self.scene.size;
        self.begin_animation_frame();
        self.update_toasts();
        self.update_touches();
        let declare_started = Instant::now();
        let declare_span = tracing::trace_span!("rust_gui.declare").entered();
        let mut clicks = std::mem::take(&mut self.clicks);
        let mut updates = std::mem::take(&mut self.updates);
        let mut submitted = std::mem::take(&mut self.submitted);
        let mut warnings = std::mem::take(&mut self.style_warnings);
        let mut theme_request = None;
        let mut clipboard = None;
        let mut redraw = false;
        let mut focus = None;
        let mut host = crate::ui::HostOutbox::default();
        let mut layers = Vec::new();
        self.stats.activations = clicks.len() + updates.len() + submitted.len();
        // Ancestors of everything with input waiting, so that a memoized
        // container knows in one lookup whether it must declare again.
        self.awaiting.clear();
        let waiting: Vec<NodeId> = clicks
            .iter()
            .chain(updates.keys())
            .chain(submitted.iter())
            .copied()
            .collect();
        for id in waiting {
            let mut ancestor = Some(id);
            while let Some(node) = ancestor.filter(|id| self.awaiting.insert(*id)) {
                ancestor = self.nodes.get(&node).and_then(|node| node.parent);
            }
        }
        self.skipped.set(0);
        self.nodes.get_mut(&self.root).unwrap().resolved = self.root_text();
        let mut ui = Ui {
            context: self,
            parent: Some(self.root),
            clicks: &mut clicks,
            elements: Vec::new(),
            keys: HashSet::new(),
            updates: &mut updates,
            submitted: &mut submitted,
            warnings: &mut warnings,
            theme_request: &mut theme_request,
            clipboard: &mut clipboard,
            redraw: &mut redraw,
            focus: &mut focus,
            host: &mut host,
            layers: &mut layers,
            enabled: true,
        };
        view(&mut ui);
        ui.declare_toasts();
        let mut elements = ui.elements;
        let mut dialog_keys = HashSet::new();
        for layer in &layers {
            assert!(
                dialog_keys.insert(layer.key.clone()),
                "duplicate dialog key: {:?}",
                layer.key
            );
        }
        elements.extend(layers);
        self.style_warnings = warnings;
        if clipboard.is_some() {
            self.clipboard_out = clipboard;
        }
        self.redraw_requested = redraw;
        self.deliver_emitted(std::mem::take(&mut host.emitted));
        self.watch_requests = host.watched;
        self.image_requests = host.images;
        self.trim_images();
        self.apply_toasts(host.toasts);
        if host.hot_state.is_some() {
            self.hot_state = host.hot_state;
            self.hot_save = false;
        }
        if let Some(format) = host.format {
            self.set_format(format);
            self.stats.activations += 1;
        }
        if let Some(messages) = host.messages {
            // Like a theme: texts already looked up in this declaration are
            // in the old language, so the host declares once more.
            self.set_messages(messages);
            self.stats.activations += 1;
        }
        if let Some(theme) = theme_request {
            // Declared token reads used the old theme; count this like an
            // activation so hosts declare once more before presenting.
            self.set_theme(theme);
            self.nodes.get_mut(&self.root).unwrap().resolved = self.root_text();
            self.stats.activations += 1;
        }
        let size = Size::new(size.width.max(0.0), size.height.max(0.0));
        if self.scene.size != size {
            let root = self.nodes.get_mut(&self.root).unwrap();
            root.style.width = Length::Px(size.width);
            root.style.height = Length::Px(size.height);
            let direction = self.direction;
            self.layout
                .set_style(
                    root.layout_id,
                    layout_style(&root.style, Kind::Column, direction),
                )
                .unwrap();
            self.pending |= Dirty::LAYOUT | Dirty::PAINT;
        }
        self.reconcile(self.root, elements, measurer);
        self.stats.skipped = self.skipped.get();
        let commands: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.enabled)
            .filter_map(|(id, node)| match &node.control {
                Control::Command {
                    shortcut: Some(shortcut),
                    ..
                } => Some((*id, shortcut.clone())),
                _ => None,
            })
            .collect();
        let mut shortcuts = HashSet::new();
        for (id, shortcut) in commands {
            // Two scopes may bind the same keys; one scope may not.
            let scope = self.scope_of(id);
            assert!(
                shortcuts.insert((scope, shortcut.clone())),
                "duplicate enabled command shortcut in one scope: {shortcut:?}"
            );
        }
        self.areas = self
            .nodes
            .iter()
            .filter_map(|(id, node)| match node.control {
                Control::EventArea(area) => Some((*id, area)),
                _ => None,
            })
            .collect();
        self.pending_area_events
            .borrow_mut()
            .retain(|id, _| self.areas.contains_key(id));
        self.repair_interaction();
        self.enforce_modal();
        self.focus_new_popovers();
        if let Some((parent, key)) = focus
            && let Some(target) = self
                .nodes
                .get(&parent.unwrap_or(self.root))
                .and_then(|node| node.children_by_key.get(&key))
                .copied()
            && self.nodes[&target].focusable()
        {
            self.focus_control(target);
        }
        self.update_tooltip();
        self.note_focus();
        drop(declare_span);
        self.stats.declare = declare_started.elapsed();
        let layout_started = Instant::now();
        let layout_span = tracing::trace_span!("rust_gui.layout").entered();
        let layout_ran = self.pending.contains(Dirty::LAYOUT);
        self.diagnose_accessibility();
        if self.pending.contains(Dirty::LAYOUT) {
            let root = self.nodes[&self.root].layout_id;
            self.layout
                .compute_layout_with_measure(
                    root,
                    taffy::Size {
                        width: taffy::AvailableSpace::Definite(size.width),
                        height: taffy::AvailableSpace::Definite(size.height),
                    },
                    |known, available, _, context, _| {
                        let measured = if let Some(Measurement {
                            wrapped: Some((text, style)),
                            spans,
                            ..
                        }) = context
                        {
                            let width = known.width.or(match available.width {
                                taffy::AvailableSpace::Definite(width) => Some(width),
                                taffy::AvailableSpace::MinContent => Some(1.0),
                                taffy::AvailableSpace::MaxContent => None,
                            });
                            self.stats.text_measures += 1;
                            match spans {
                                Some(spans) => measurer.geometry_rich(spans, style, width).size,
                                None => measurer.measure_styled(text, style, width),
                            }
                        } else {
                            context.map(|context| context.size).unwrap_or_default()
                        };
                        taffy::Size {
                            width: known.width.unwrap_or(measured.width),
                            height: known.height.unwrap_or(measured.height),
                        }
                    },
                )
                .unwrap();
            self.stats.layout_passes = 1;
            self.geometry_dirty = true;
            self.pending |= Dirty::PAINT;
        }
        if self.geometry_dirty {
            self.refresh_bounds(size);
        }
        drop(layout_span);
        self.stats.layout = layout_started.elapsed();
        self.diagnose_layout(layout_ran);
        self.prepare_controls(measurer);
        let hover = self.pointer.and_then(|p| self.hit_test(p));
        if hover != self.hovered {
            for id in [self.hovered, hover].into_iter().flatten() {
                self.invalidate_paint(id);
            }
            self.hovered = hover;
            self.pending |= Dirty::PAINT;
        }
        if self.pending.contains(Dirty::PAINT) {
            let paint_started = Instant::now();
            let _paint_span = tracing::trace_span!("rust_gui.paint").entered();
            let mut scene = Scene {
                size,
                primitives: Vec::new(),
            };
            self.paint(self.root).append_to(&mut scene.primitives);
            for popover in self.popovers() {
                self.paint(popover).append_to(&mut scene.primitives);
            }
            if let Some(popup) = self.popup
                && let Some(panel) = self
                    .nodes
                    .get(&popup.owner)
                    .and_then(|node| node.children.first())
                    .copied()
            {
                self.paint(panel).append_to(&mut scene.primitives);
            }
            self.paint_ghosts(&mut scene);
            self.paint_dock_drag(&mut scene, measurer);
            self.paint_tooltip(&mut scene, measurer);
            self.scene = scene;
            self.stats.scene_rebuilt = true;
            if let Some(damage) = self.pending_damage.take() {
                self.damage.push(damage);
            }
            self.stats.paint = paint_started.elapsed();
        }
        self.diagnose_paint();
        self.stats.dirty = self.pending;
        self.pending = Dirty::empty();
        self.stats
    }

    fn reconcile(
        &mut self,
        parent: NodeId,
        elements: Vec<Element>,
        measurer: &mut impl TextMeasurer,
    ) {
        let old_children = self.nodes[&parent].children.clone();
        let inherited = self.nodes[&parent].resolved.clone();
        let mut available: HashMap<_, _> = old_children
            .iter()
            .map(|id| (self.nodes[id].key.clone(), *id))
            .collect();
        let mut children = Vec::with_capacity(elements.len());
        let (mut created_here, mut removed_here) = (0usize, 0usize);
        for element in elements {
            let existing = available.remove(&element.key).filter(|id| {
                if self.nodes[id].kind == element.kind {
                    true
                } else {
                    self.remove(*id);
                    removed_here += 1;
                    false
                }
            });
            let resolved = element.style.resolve_text(&inherited);
            // A memoized container that was not declared again keeps the
            // children it has, so its subtree is never visited.
            let kept = existing.is_some_and(|id| {
                element.children.is_empty()
                    && matches!(element.control, Control::Memo(hash)
                        if self.memo_hash(id) == Some(hash))
            });
            let id = if let Some(id) = existing {
                self.stats.reused += 1;
                let frame = self.frame;
                let node = self.nodes.get_mut(&id).unwrap();
                let paint_changed = node.text != element.text
                    || node.style != element.style
                    || node.resolved != resolved
                    || node.enabled != element.enabled
                    || node.custom != element.custom
                    || node.control != element.control;
                // Colors repaint; only font and wrapping changes re-measure.
                let text_changed = node.text != element.text
                    || node.text_style() != resolved.text_style(element.style.wrap)
                    // Bold, italic and code runs change advances without changing the text.
                    || (element.kind == Kind::RichText && node.control != element.control);
                let mut changed = Dirty::empty();
                if paint_changed {
                    changed |= Dirty::PAINT;
                }
                if text_changed {
                    changed |= Dirty::TEXT;
                }
                if node.style != element.style {
                    changed |= Dirty::STYLE;
                }
                if node.style.layout(Flow::Column) != element.style.layout(Flow::Column) {
                    changed |= Dirty::LAYOUT;
                }
                if node.enabled != element.enabled
                    || node.control != element.control
                    || node.tooltip != element.tooltip
                {
                    changed |= Dirty::ACCESSIBILITY;
                }
                if node.tooltip != element.tooltip {
                    self.pending |= Dirty::ACCESSIBILITY;
                }
                if !changed.is_empty() {
                    node.changed = changed;
                    node.changed_frame = frame;
                }
                if text_changed && element.kind.text_leaf() {
                    let measured = Measurement::leaf(
                        &element,
                        resolved.text_style(element.style.wrap),
                        measurer,
                    );
                    self.stats.text_measures += 1;
                    if self.layout.get_node_context(node.layout_id) != Some(&measured) {
                        self.layout
                            .set_node_context(node.layout_id, Some(measured))
                            .unwrap();
                        self.pending |= Dirty::LAYOUT;
                        self.diagnostic.layout_cause.get_or_insert(id);
                    }
                    self.pending |= Dirty::TEXT | Dirty::PAINT | Dirty::ACCESSIBILITY;
                }
                if node.custom != element.custom {
                    if let Some(widget) = &element.custom
                        && self.layout.get_node_context(node.layout_id)
                            != Some(&Measurement::fixed(widget.size))
                    {
                        self.layout
                            .set_node_context(node.layout_id, Some(Measurement::fixed(widget.size)))
                            .unwrap();
                        self.pending |= Dirty::LAYOUT;
                    }
                    self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
                }
                if let Some(measured) = Measurement::image(&element)
                    && self.layout.get_node_context(node.layout_id) != Some(&measured)
                {
                    self.layout
                        .set_node_context(node.layout_id, Some(measured))
                        .unwrap();
                    self.pending |= Dirty::LAYOUT;
                }
                let mut layout_style =
                    layout_style(&element.style, element.kind, resolved.direction);
                if let Some(measurement) = self.layout.get_node_context(node.layout_id) {
                    anchor_minimum(&mut layout_style, &element, measurement.size);
                }
                if *self.layout.style(node.layout_id).unwrap() != layout_style {
                    self.layout.set_style(node.layout_id, layout_style).unwrap();
                    self.pending |= Dirty::LAYOUT;
                    self.diagnostic.layout_cause.get_or_insert(id);
                }
                if node.style != element.style {
                    self.pending |= Dirty::STYLE | Dirty::PAINT;
                }
                if node.enabled != element.enabled {
                    self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
                }
                node.text = element.text;
                node.style = element.style;
                node.resolved = resolved;
                node.tooltip = element.tooltip;
                node.enabled = element.enabled;
                node.custom = element.custom;
                if let Control::Collection { offset, .. }
                | Control::TextArea {
                    scroll_y: offset, ..
                } = element.control
                    && node.scroll.offset != offset
                {
                    node.scroll.offset = offset;
                    self.geometry_dirty = true;
                }
                if let Control::TextInput { value, .. } = &element.control {
                    node.editor
                        .get_or_insert_with(|| Box::new(Editor::new(value)))
                        .synchronize(value);
                }
                node.control = element.control;
                if paint_changed {
                    self.invalidate_paint(id);
                }
                id
            } else {
                let measured = if let Some(widget) = &element.custom {
                    Measurement::fixed(widget.size)
                } else if element.kind.text_leaf() {
                    self.stats.text_measures += 1;
                    Measurement::leaf(&element, resolved.text_style(element.style.wrap), measurer)
                } else {
                    Measurement::image(&element).unwrap_or(Measurement::fixed(Size::default()))
                };
                let mut leaf_style = layout_style(&element.style, element.kind, resolved.direction);
                anchor_minimum(&mut leaf_style, &element, measured.size);
                let layout_id = self
                    .layout
                    .new_leaf_with_context(leaf_style, measured)
                    .unwrap();
                let id = NodeId(self.next_id);
                self.next_id = self
                    .next_id
                    .checked_add(1)
                    .expect("node identity exhausted");
                self.nodes.insert(
                    id,
                    Node {
                        anim: animation_state::NodeAnim::entering(element.style.enter),
                        area: None,
                        rich: None,
                        rich_editor: None,
                        label_width: 0.0,
                        hint_width: 0.0,
                        churn: 0,
                        layout_rect: Rect::default(),
                        key: element.key,
                        kind: element.kind,
                        parent: Some(parent),
                        children: Vec::new(),
                        children_by_key: HashMap::new(),
                        text: element.text,
                        style: element.style,
                        enabled: element.enabled,
                        layout_id,
                        bounds: Rect::default(),
                        clip: Rect::default(),
                        scroll: ScrollState {
                            offset: if let Control::Collection { offset, .. } = element.control {
                                offset
                            } else {
                                0.0
                            },
                            maximum: 0.0,
                            ..ScrollState::default()
                        },
                        custom: element.custom,
                        paint_cache: None,
                        editor: match &element.control {
                            Control::TextInput { value, .. } => Some(Box::new(Editor::new(value))),
                            _ => None,
                        },
                        control: element.control,
                        changed: Dirty::all(),
                        changed_frame: self.frame,
                        painted_frame: 0,
                        resolved,
                        scope: matches!(
                            element.kind,
                            Kind::FocusScope | Kind::Dialog | Kind::Popover
                        )
                        .then(|| Box::new(ScopeState::default())),
                        tooltip: element.tooltip,
                    },
                );
                // Popovers and autofocus scopes take focus when they appear.
                if element.kind == Kind::Popover
                    || matches!(
                        self.nodes[&id].control,
                        Control::FocusScope {
                            scope: crate::FocusScope {
                                autofocus: true,
                                ..
                            }
                        }
                    )
                {
                    self.new_popovers.push(id);
                }
                self.stats.created += 1;
                created_here += 1;
                self.pending |= Dirty::all();
                id
            };
            if !kept {
                self.reconcile(id, element.children, measurer);
            }
            children.push(id);
        }
        removed_here += available.len();
        for id in available.into_values() {
            self.remove_with_exit(id);
        }
        if old_children != children {
            let layout_children: Vec<_> =
                children.iter().map(|id| self.nodes[id].layout_id).collect();
            self.layout
                .set_children(self.nodes[&parent].layout_id, &layout_children)
                .unwrap();
            let children_by_key = children
                .iter()
                .map(|id| (self.nodes[id].key.clone(), *id))
                .collect();
            let node = self.nodes.get_mut(&parent).unwrap();
            node.children = children;
            node.children_by_key = children_by_key;
            self.invalidate_paint(parent);
            self.pending |= Dirty::TREE | Dirty::LAYOUT | Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        self.diagnose_children(parent, created_here, removed_here);
    }

    fn remove(&mut self, id: NodeId) {
        let node = self.nodes.remove(&id).unwrap();
        // Popovers fall back to their anchor when nothing was focused before them.
        if let Some(state) = &node.scope
            && let Some(target) = state.restore_to.or((node.kind == Kind::Popover)
                .then_some(node.parent)
                .flatten())
            && matches!(
                node.control,
                Control::Dialog { .. }
                    | Control::Popover
                    | Control::FocusScope {
                        scope: crate::FocusScope { restore: true, .. }
                    }
            )
            && self.pending_restore.is_none()
        {
            self.pending_restore = Some(target);
        }
        self.add_damage(node.clip);
        for child in node.children {
            self.remove(child);
        }
        self.layout.remove(node.layout_id).unwrap();
        self.clicks.remove(&id);
        self.updates.remove(&id);
        self.submitted.remove(&id);
        self.stats.removed += 1;
        self.pending |= Dirty::TREE | Dirty::LAYOUT | Dirty::PAINT | Dirty::ACCESSIBILITY;
    }

    fn repair_interaction(&mut self) {
        if self.popup.is_some_and(|popup| {
            !self
                .nodes
                .get(&popup.owner)
                .is_some_and(|node| node.enabled)
        }) {
            self.popup = None;
        }
        let valid = |id: NodeId| self.nodes.get(&id).is_some_and(Node::focusable);
        if self.focused.is_some_and(|id| !valid(id)) {
            self.focused = None;
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        let restored = self
            .pending_restore
            .take()
            .filter(|target| self.focused.is_none() && valid(*target));
        if restored.is_some() {
            self.focused = restored;
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        if self
            .captured
            .is_some_and(|id| !self.nodes.get(&id).is_some_and(Node::takes_pointer))
        {
            self.captured = None;
            self.pending |= Dirty::PAINT;
        }
        if self.keyboard_pressed.is_some_and(|(id, _)| !valid(id)) {
            self.keyboard_pressed = None;
            self.pending |= Dirty::PAINT;
        }
        if let Some(target) = restored {
            self.invalidate_paint(target);
        }
        self.cancel_compositions_except(self.focused);
        if self.drag.as_ref().is_some_and(|drag| {
            !self
                .nodes
                .get(&drag.source)
                .is_some_and(|node| node.enabled)
        }) {
            self.cancel_drag();
        }
    }

    fn refresh_bounds(&mut self, size: Size) {
        self.update_bounds(
            self.root,
            Point::default(),
            Rect {
                x: 0.0,
                y: 0.0,
                width: size.width,
                height: size.height,
            },
        );
        self.geometry_dirty = false;
        self.pending |= Dirty::ACCESSIBILITY;
    }

    fn update_bounds(&mut self, id: NodeId, origin: Point, clip: Rect) {
        let viewport = self.nodes[&self.root].bounds;
        let node = &self.nodes[&id];
        let old_bounds = node.bounds;
        let old_clip = node.clip;
        let old_scroll = node.scroll;
        // How far the children reach past the node's own box, per side.
        let (content_left, content_right, content_bottom) = node
            .children
            .iter()
            .filter(|child| !matches!(self.nodes[child].kind, Kind::Popup | Kind::Popover))
            .map(|child| *self.layout.layout(self.nodes[child].layout_id).unwrap())
            .fold(
                (0.0_f32, 0.0_f32, 0.0_f32),
                |(left, right, bottom), layout| {
                    (
                        left.min(layout.location.x),
                        right.max(layout.location.x + layout.size.width),
                        bottom.max(layout.location.y + layout.size.height),
                    )
                },
            );
        let node = self.nodes.get_mut(&id).unwrap();
        let layout = self.layout.layout(node.layout_id).unwrap();
        node.bounds = Rect {
            x: origin.x + layout.location.x,
            y: origin.y + layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        };
        let layout_rect = Rect {
            x: layout.location.x,
            y: layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        };
        let old_layout_rect = std::mem::replace(&mut node.layout_rect, layout_rect);
        let clip = if matches!(node.kind, Kind::Popup | Kind::Popover) {
            // Context menus anchor at the request point instead of below the owner.
            let anchored = (node.kind == Kind::Popup)
                .then(|| {
                    self.popup
                        .filter(|popup| Some(popup.owner) == node.parent)
                        .and_then(|popup| popup.anchor)
                })
                .flatten();
            if let Some(anchor) = anchored {
                node.bounds.x = if node.resolved.rtl() {
                    anchor.x - node.bounds.width
                } else {
                    anchor.x
                };
                node.bounds.y = anchor.y;
            }
            node.bounds.width = node.bounds.width.min(viewport.width);
            node.bounds.height = node.bounds.height.min(viewport.height);
            node.bounds.x = node
                .bounds
                .x
                .clamp(0.0, (viewport.width - node.bounds.width).max(0.0));
            if node.bounds.y + node.bounds.height > viewport.height {
                let above = anchored.map_or(origin.y, |anchor| anchor.y);
                node.bounds.y = (above - node.bounds.height).max(0.0);
            }
            viewport
        } else {
            clip
        };
        node.clip = clip.intersect(node.bounds);
        if matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection) {
            node.scroll.maximum =
                (content_bottom + node.style.padding.max(0.0) - node.bounds.height).max(0.0);
            node.scroll.offset = node.scroll.offset.clamp(0.0, node.scroll.maximum);
        }
        let rtl = node.resolved.rtl();
        if matches!(node.kind, Kind::Scroll | Kind::Collection) {
            // Content overflows at the end side: past the right edge, or past
            // the left edge in a right-to-left container.
            let padding = node.style.padding.max(0.0);
            let overflow = if rtl {
                padding - content_left
            } else {
                content_right + padding - node.bounds.width
            };
            // A fraction of a pixel from rounding is not worth a scrollbar.
            node.scroll.maximum_x = if overflow >= 0.5 { overflow } else { 0.0 };
            node.scroll.offset_x = node.scroll.offset_x.clamp(0.0, node.scroll.maximum_x);
        }
        let origin = Point {
            x: node.bounds.x
                + if rtl {
                    node.scroll.offset_x
                } else {
                    -node.scroll.offset_x
                },
            y: node.bounds.y - node.scroll.offset,
        };
        let clip = node.clip;
        let children = node.children.clone();
        if old_bounds != node.bounds {
            self.diagnostic.bounds_changed = true;
        }
        if old_bounds != node.bounds || old_clip != node.clip || old_scroll != node.scroll {
            self.add_damage(old_clip);
            self.invalidate_paint(id);
        }
        self.note_layout_change(id, old_layout_rect, layout_rect);
        for child in children {
            self.update_bounds(child, origin, clip);
        }
    }

    fn add_damage(&mut self, rect: Rect) {
        if rect.width > 0.0 && rect.height > 0.0 {
            self.pending_damage = Some(self.pending_damage.map_or(rect, |old| old.union(rect)));
        }
    }

    fn invalidate_paint(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        self.add_damage(node.clip);
        let mut next = Some(id);
        while let Some(id) = next {
            let node = self.nodes.get_mut(&id).unwrap();
            next = node.parent;
            if node.paint_cache.take().is_none() {
                break;
            }
        }
        self.pending |= Dirty::PAINT;
    }

    /// Background, text color and radius for the node's current interaction
    /// state: theme defaults for the widget kind, then the style's overrides.
    fn appearance(&self, id: NodeId, node: &Node) -> crate::animation::Shown {
        let theme = &self.theme;
        let pressed = (self.captured == Some(id) && self.hovered == Some(id))
            || self.touch_pressed(id)
            || self
                .keyboard_pressed
                .is_some_and(|(pressed, _)| pressed == id);
        let hovered = self.hovered == Some(id);
        let focused = self.focused == Some(id);
        let mut background = node.style.background.or(match node.kind {
            Kind::Button
            | Kind::TextInput
            | Kind::TextArea
            | Kind::RichEditor
            | Kind::ComboBox
            | Kind::MenuButton => Some(theme.control),
            Kind::Popup | Kind::Popover => Some(theme.popup),
            Kind::Divider | Kind::DockHandle => Some(if hovered || pressed {
                theme.accent
            } else {
                theme.track
            }),
            Kind::Dialog => {
                matches!(node.control, Control::Dialog { modal: true, .. }).then_some(theme.scrim)
            }
            _ => None,
        });
        let mut foreground = node.resolved.foreground;
        let mut radius = node.style.radius;
        let mut opacity = node.style.opacity;
        let mut transform = node.style.transform;
        if node.kind == Kind::Button {
            if !node.enabled {
                background = Some(theme.control_disabled);
            } else if pressed {
                background = Some(theme.control_pressed);
            } else if hovered {
                background = Some(theme.control_hover);
            }
        }
        if !node.enabled {
            foreground = theme.text_disabled;
        }
        if let Some(states) = &node.style.states {
            let mut apply = |paint: &Paint| {
                if let Some(color) = paint.background {
                    background = Some(color);
                }
                if let Some(color) = paint.foreground {
                    foreground = color;
                }
                if let Some(value) = paint.radius {
                    radius = value;
                }
                if let Some(value) = paint.opacity {
                    opacity = value;
                }
                if let Some(value) = paint.transform {
                    transform = value;
                }
            };
            if focused {
                apply(&states.focused);
            }
            if hovered && node.enabled {
                apply(&states.hover);
            }
            if pressed && node.enabled {
                apply(&states.pressed);
            }
            if !node.enabled {
                apply(&states.disabled);
            }
        }
        crate::animation::Shown {
            background,
            foreground,
            radius,
            opacity,
            transform,
        }
    }

    fn paint(&mut self, id: NodeId) -> Arc<PaintChunk> {
        if let Some(chunk) = &self.nodes[&id].paint_cache {
            self.stats.paint_cache_hits += 1;
            return chunk.clone();
        }
        self.stats.paint_nodes += 1;
        self.nodes.get_mut(&id).unwrap().painted_frame = self.frame;
        let target = self.appearance(id, &self.nodes[&id]);
        let animation_state::Displayed { shown, layer } = self.displayed(id, target);
        let crate::animation::Shown {
            background,
            foreground,
            radius,
            ..
        } = shown;
        if layer.is_some() {
            let bounds = self.nodes[&id].bounds;
            self.diagnose_layer(id, bounds);
        }
        let node = &self.nodes[&id];
        let mut scene = Scene::default();
        if let Some((opacity, transform)) = layer {
            scene.primitives.push(Primitive::PushLayer {
                bounds: node.bounds,
                opacity,
                transform,
            });
        }
        // A shadow is behind the node, so it is drawn before the clip that
        // bounds the node's own painting.
        if let Some(shadow) = &node.style.shadow
            && !shadow.inset
            && shadow.color.3 > 0
        {
            scene.primitives.push(Primitive::Shadow {
                rect: node.bounds,
                radius,
                color: shadow.color,
                blur: shadow.blur,
                spread: shadow.spread,
                offset: Point {
                    x: shadow.offset.0,
                    y: shadow.offset.1,
                },
                inset: false,
            });
        }
        scene.primitives.push(Primitive::PushClip(node.bounds));
        let focused = self.focused == Some(id);
        // There is no stroke primitive. Over an opaque background the ring is
        // the focus colour with the background drawn back inside it, which
        // keeps rounded corners. That cannot work for a node that is
        // transparent or translucent (a scroll page, a bare container): the
        // focus colour would flood it. Those get four edges instead.
        let opaque = background.filter(|background| background.3 == 255);
        if let (true, Some(background)) = (focused, opaque) {
            scene.primitives.push(Primitive::Quad {
                rect: node.bounds,
                color: self.theme.focus,
                radius,
            });
            let rect = Rect {
                x: node.bounds.x + 2.0,
                y: node.bounds.y + 2.0,
                width: (node.bounds.width - 4.0).max(0.0),
                height: (node.bounds.height - 4.0).max(0.0),
            };
            scene.primitives.push(Primitive::Quad {
                rect,
                color: background,
                radius: (radius - 2.0).max(0.0),
            });
        } else {
            if let Some(background) = background
                && background.3 > 0
            {
                scene.primitives.push(Primitive::Quad {
                    rect: node.bounds,
                    color: background,
                    radius,
                });
            }
            // A ramp fills the node over whatever flat colour it has.
            if let Some(gradient) = &node.style.gradient
                && gradient.stops.len() >= 2
            {
                scene.primitives.push(Primitive::Gradient {
                    rect: node.bounds,
                    radius,
                    from: Point {
                        x: gradient.from.0,
                        y: gradient.from.1,
                    },
                    to: Point {
                        x: gradient.to.0,
                        y: gradient.to.1,
                    },
                    stops: gradient.stops.clone(),
                    radial: gradient.radial,
                });
            }
            if focused {
                let Rect {
                    x,
                    y,
                    width,
                    height,
                } = node.bounds;
                let ring = 2.0_f32.min(width / 2.0).min(height / 2.0);
                for rect in [
                    Rect {
                        x,
                        y,
                        width,
                        height: ring,
                    },
                    Rect {
                        x,
                        y: y + height - ring,
                        width,
                        height: ring,
                    },
                    Rect {
                        x,
                        y,
                        width: ring,
                        height,
                    },
                    Rect {
                        x: x + width - ring,
                        y,
                        width: ring,
                        height,
                    },
                ] {
                    scene.primitives.push(Primitive::Quad {
                        rect,
                        color: self.theme.focus,
                        radius: 0.0,
                    });
                }
            }
        }
        self.paint_background_image(node, &mut scene, foreground, radius);
        if node.kind == Kind::RichText {
            self.paint_rich_text(id, &mut scene, foreground);
        } else if node.kind == Kind::Image {
            self.paint_image(id, &mut scene, foreground, radius);
        } else if !node.text.is_empty() {
            let padding = node.style.padding.max(0.0);
            let mut rect = Rect {
                x: node.bounds.x + padding,
                y: node.bounds.y + padding,
                width: (node.bounds.width - 2.0 * padding).max(0.0),
                height: (node.bounds.height - 2.0 * padding).max(0.0),
            };
            // Wrapped blocks are aligned by the shaper; a single line in a wider
            // box is moved to the right here.
            if node.resolved.rtl() && node.style.wrap == TextWrap::None {
                // Text leaves were measured for layout; other labelled controls
                // (sliders) are measured while controls are prepared.
                let width = if node.kind.text_leaf() {
                    self.layout
                        .get_node_context(node.layout_id)
                        .map_or(0.0, |measured| measured.size.width)
                } else {
                    node.label_width
                };
                if width > 0.0 {
                    rect.x += (rect.width - width).max(0.0);
                }
            }
            let color = foreground;
            if let Some((start, end)) = self.suggestion_match(node) {
                // The part of a suggestion that was typed stands out.
                let span = |text: &str, weight| rust_gui_scene::TextSpan {
                    text: text.to_owned(),
                    color: None,
                    weight,
                    italic: None,
                    family: None,
                };
                scene.primitives.push(Primitive::RichText {
                    rect,
                    spans: vec![
                        span(&node.text[..start], None),
                        span(&node.text[start..end], Some(700)),
                        span(&node.text[end..], None),
                    ],
                    style: node.text_style(),
                    color,
                });
            } else if node.text_style()
                == (TextStyle {
                    font_size: node.resolved.font_size,
                    ..TextStyle::default()
                })
            {
                scene.primitives.push(Primitive::Text {
                    rect,
                    text: node.text.clone(),
                    font_size: node.resolved.font_size,
                    color,
                });
            } else {
                scene.primitives.push(Primitive::StyledText {
                    rect,
                    text: node.text.clone(),
                    style: node.text_style(),
                    color,
                });
            }
        }
        // A menu item's shortcut, along the edge the text ends at.
        if let Control::Choice { hint, .. } | Control::Hint(hint) = &node.control
            && !hint.is_empty()
            && node.hint_width > 0.0
        {
            let padding = node.style.padding.max(0.0);
            let inner = (node.bounds.width - 2.0 * padding).max(0.0);
            let offset = if node.resolved.rtl() {
                0.0
            } else {
                (inner - node.hint_width).max(0.0)
            };
            scene.primitives.push(Primitive::StyledText {
                rect: Rect {
                    x: node.bounds.x + padding + offset,
                    y: node.bounds.y + padding,
                    width: node.hint_width,
                    height: (node.bounds.height - 2.0 * padding).max(0.0),
                },
                text: hint.clone(),
                style: node.text_style(),
                color: if node.enabled {
                    self.theme.text_muted
                } else {
                    self.theme.text_disabled
                },
            });
        }
        if let Some(shadow) = &node.style.shadow
            && shadow.inset
            && shadow.color.3 > 0
        {
            scene.primitives.push(Primitive::Shadow {
                rect: node.bounds,
                radius,
                color: shadow.color,
                blur: shadow.blur,
                spread: shadow.spread,
                offset: Point {
                    x: shadow.offset.0,
                    y: shadow.offset.1,
                },
                inset: true,
            });
        }
        if let Some(widget) = &node.custom {
            let offset = Point {
                x: node.bounds.x,
                y: node.bounds.y,
            };
            scene.primitives.extend(
                widget
                    .primitives
                    .iter()
                    .map(|primitive| primitive.translated(offset)),
            );
        }
        self.paint_control(id, &mut scene);
        let node = &self.nodes[&id];
        let children = node
            .children
            .iter()
            .copied()
            .filter(|child| !matches!(self.nodes[child].kind, Kind::Popup | Kind::Popover))
            .collect::<Vec<_>>();
        let before = std::mem::take(&mut scene.primitives);
        if node.scroll.maximum_x > 0.0 {
            let track = node.bounds.width;
            let width = (track * track / (track + node.scroll.maximum_x))
                .max(12.0)
                .min(track);
            let travelled = (track - width) * node.scroll.offset_x / node.scroll.maximum_x;
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    // The thumb starts at the start side, like the content.
                    x: if node.resolved.rtl() {
                        node.bounds.x + track - width - travelled
                    } else {
                        node.bounds.x + travelled
                    },
                    y: node.bounds.y + node.bounds.height - 4.0,
                    width,
                    height: 4.0,
                },
                color: self.theme.scrollbar,
                radius: 2.0,
            });
        }
        if node.scroll.maximum > 0.0 {
            let track = node.bounds.height;
            let height = (track * track / (track + node.scroll.maximum))
                .max(12.0)
                .min(track);
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: if node.resolved.rtl() {
                        node.bounds.x
                    } else {
                        node.bounds.x + node.bounds.width - 4.0
                    },
                    y: node.bounds.y + (track - height) * node.scroll.offset / node.scroll.maximum,
                    width: 4.0,
                    height,
                },
                color: self.theme.scrollbar,
                radius: 2.0,
            });
        }
        scene.primitives.push(Primitive::PopClip);
        if layer.is_some() {
            scene.primitives.push(Primitive::PopLayer);
        }
        let after = scene.primitives;
        let children = children
            .into_iter()
            .map(|child| self.paint(child))
            .collect();
        let chunk = Arc::new(PaintChunk {
            before,
            children,
            after,
        });
        self.nodes.get_mut(&id).unwrap().paint_cache = Some(chunk.clone());
        chunk
    }

    fn order(&self, id: NodeId, result: &mut Vec<NodeId>) {
        result.push(id);
        for child in &self.nodes[&id].children {
            self.order(*child, result);
        }
    }

    pub fn hit_test(&self, position: Point) -> Option<NodeId> {
        // A capturing area that consumes pointer events stands between the
        // pointer and everything under it, like a scrim: nothing there is
        // hovered, pressed or dragged while it is up.
        if !self.areas.is_empty()
            && let Some(deepest) = self.deepest_at(position)
            && self.path_to(deepest).iter().any(|id| {
                matches!(self.areas.get(id), Some(area) if area.capture && area.consume && area.pointer)
            })
        {
            return None;
        }
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        for popover in self.popovers() {
            self.order(popover, &mut order);
        }
        if let Some(popup) = self.popup
            && let Some(panel) = self
                .nodes
                .get(&popup.owner)
                .and_then(|node| node.children.first())
        {
            self.order(*panel, &mut order);
        }
        order.into_iter().rev().find(|id| {
            let node = &self.nodes[id];
            let dialog_surface = match node.kind {
                Kind::Dialog => matches!(node.control, Control::Dialog { modal: true, .. }),
                Kind::Container => node
                    .parent
                    .is_some_and(|parent| self.nodes[&parent].kind == Kind::Dialog),
                _ => false,
            };
            (node.takes_pointer()
                // Floating groups block clicks to whatever lies under them.
                || matches!(
                    node.kind,
                    Kind::Popup | Kind::Popover | Kind::DockFloat | Kind::Toast
                )
                || dialog_surface)
                && node.clip.contains(position)
        })
    }

    /// Returns true if a declaration/redraw is needed. Pointer capture survives
    /// leaving the window, but activation requires release over the same button.
    pub fn event(&mut self, event: Event) -> bool {
        // Areas see an event before and after the widgets, and may keep it
        // from them.
        if self.travelled(&event) {
            self.note_focus();
            return true;
        }
        match event {
            Event::Touch {
                id,
                phase,
                position,
                force,
                pen,
            } => {
                let handled = self.touch_event(id, phase, position, force, pen);
                self.note_focus();
                return handled;
            }
            Event::Zoom(factor) => return self.pointer_gesture(factor, 0.0),
            Event::Rotate(radians) => return self.pointer_gesture(1.0, radians),
            Event::FocusLost => self.stop_touches(),
            // Any other way of scrolling or pressing ends a fling.
            Event::PointerDown | Event::PointerDownWithShift | Event::Scroll { .. } => {
                self.fling = None;
            }
            _ => {}
        }
        let handled = self.handle_event(event.clone());
        self.track_hover(&event);
        self.note_focus();
        handled
    }

    fn handle_event(&mut self, event: Event) -> bool {
        if let Some(handled) = self.drag_event(&event) {
            return handled;
        }
        if let Some(handled) = self.dock_event(&event) {
            return handled;
        }
        if let Some(handled) = self.rich_editor_event(&event) {
            return handled;
        }
        if let Some(handled) = self.text_area_event(&event) {
            return handled;
        }
        if let Some(handled) = self.rich_text_event(&event) {
            return handled;
        }
        if let Event::Shortcut(shortcut) = &event {
            // The innermost scope that holds the focus wins, so a panel may
            // take a shortcut the window also has.
            let mut best: Option<(usize, NodeId)> = None;
            for (id, node) in &self.nodes {
                let Control::Command {
                    shortcut: Some(binding),
                    while_editing,
                    scoped,
                    ..
                } = &node.control
                else {
                    continue;
                };
                if !node.enabled || binding != shortcut || !(*while_editing || !self.accepts_text())
                {
                    continue;
                }
                let scope = self.scope_of(*id);
                let inside = match (scope, self.focused) {
                    (Some(scope), Some(focused)) => self.within(focused, scope),
                    // A command outside every scope belongs to the window.
                    (None, _) => true,
                    (Some(_), None) => false,
                };
                if *scoped && !inside {
                    continue;
                }
                let depth = if inside { self.depth(*id) } else { 0 };
                if best.is_none_or(|(deepest, _)| depth > deepest) {
                    best = Some((depth, *id));
                }
            }
            return best.is_some_and(|(_, id)| self.activate(id));
        }
        if let Some(handled) = self.collection_event(&event) {
            return handled;
        }
        if let Some(handled) = self.control_event(&event) {
            return handled;
        }
        if let Some(handled) = self.dialog_event(&event) {
            return handled;
        }
        if let Event::Scroll { delta_x, delta_y } = event {
            // The wheel steps a number field only while it has the keyboard
            // and the pointer, so scrolling a page never edits values in passing.
            if let Some(id) = self.focused.filter(|id| self.hovered == Some(*id))
                && delta_y != 0.0
                && self.step_number(id, if delta_y < 0.0 { 1.0 } else { -1.0 })
            {
                return true;
            }
            let vertical = delta_y != 0.0 && self.scroll_at_pointer(delta_y);
            let horizontal = delta_x != 0.0 && self.scroll_x_at_pointer(delta_x);
            return vertical || horizontal;
        }
        let before = (
            self.hovered,
            self.focused,
            self.captured,
            self.keyboard_pressed,
        );
        let mut activated = false;
        match event {
            Event::PointerMoved(position) => {
                self.pointer = Some(position);
                self.hovered = self.hit_test(position);
            }
            Event::PointerLeft => {
                self.pointer = None;
                self.hovered = None;
            }
            Event::PointerDown | Event::PointerDownWithShift => {
                self.captured = self.hovered;
                self.focused = self.hovered.filter(|id| self.nodes[id].focusable());
                self.keyboard_pressed = None;
            }
            Event::PointerUp => {
                if let Some(id) = self
                    .captured
                    .take()
                    .filter(|id| self.hovered == Some(*id) && self.nodes[id].activatable())
                {
                    activated = self.activate(id);
                }
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } => {
                self.captured = None;
                self.keyboard_pressed = None;
            }
            Event::KeyDown {
                key: key @ (Key::Enter | Key::Space),
                repeat: false,
                ..
            } => {
                if self.keyboard_pressed.is_none() {
                    self.keyboard_pressed = self
                        .focused
                        .filter(|id| self.nodes[id].activatable())
                        .map(|id| (id, key));
                }
            }
            Event::KeyDown { key, .. }
                if matches!(
                    key,
                    Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Home
                        | Key::End
                ) =>
            {
                let mut target = self.focused;
                while let Some(id) = target {
                    let node = &self.nodes[&id];
                    if node.kind == Kind::Scroll && matches!(key, Key::ArrowLeft | Key::ArrowRight)
                    {
                        let step = if key == Key::ArrowRight { 40.0 } else { -40.0 };
                        if self.scroll_x_by(id, step) {
                            return true;
                        }
                    } else if node.kind == Kind::Scroll {
                        let offset = match key {
                            Key::ArrowUp => node.scroll.offset - 40.0,
                            Key::ArrowDown => node.scroll.offset + 40.0,
                            Key::PageUp => node.scroll.offset - node.bounds.height * 0.9,
                            Key::PageDown => node.scroll.offset + node.bounds.height * 0.9,
                            Key::Home => 0.0,
                            Key::End => node.scroll.maximum,
                            _ => unreachable!(),
                        };
                        if self.set_scroll(id, offset) {
                            return true;
                        }
                    }
                    target = self.nodes[&id].parent;
                }
            }
            Event::KeyDown { .. } => {}
            Event::KeyUp(key) => {
                if let Some((id, pressed_key)) = self.keyboard_pressed
                    && pressed_key == key
                {
                    self.keyboard_pressed = None;
                    if self.focused == Some(id) {
                        activated = self.activate(id);
                    }
                }
            }
            Event::FocusLost => {
                self.captured = None;
                self.keyboard_pressed = None;
                self.hovered = None;
                self.pointer = None;
                self.focused = None;
                self.clicks.clear();
            }
            Event::Scroll { .. } => unreachable!(),
            Event::Text(_)
            | Event::Shortcut(_)
            | Event::DropAt { .. }
            | Event::ContextMenu(_)
            | Event::Editor(_)
            // Handled in `event`: contacts as pointer events, the others by
            // the gesture area under the pointer.
            | Event::Touch { .. }
            | Event::Zoom(_)
            | Event::Rotate(_) => {}
        }
        let after = (
            self.hovered,
            self.focused,
            self.captured,
            self.keyboard_pressed,
        );
        if before.1 != after.1 {
            self.cancel_compositions_except(self.focused);
            self.pending |= Dirty::ACCESSIBILITY;
            if let Some(id) = self.focused {
                self.scroll_into_view(id);
            }
        }
        if before != after || activated {
            for id in [
                before.0,
                before.1,
                before.2,
                before.3.map(|(id, _)| id),
                after.0,
                after.1,
                after.2,
                after.3.map(|(id, _)| id),
            ]
            .into_iter()
            .flatten()
            {
                self.invalidate_paint(id);
            }
            self.pending |= Dirty::PAINT;
            true
        } else {
            false
        }
    }

    fn set_scroll(&mut self, id: NodeId, offset: f32) -> bool {
        if !offset.is_finite() {
            return false;
        }
        let Some(node) = self.nodes.get_mut(&id) else {
            return false;
        };
        if !matches!(
            node.kind,
            Kind::Scroll | Kind::Popup | Kind::Collection | Kind::TextArea | Kind::RichEditor
        ) {
            return false;
        }
        let offset = offset.clamp(0.0, node.scroll.maximum);
        if node.scroll.offset == offset {
            return false;
        }
        node.scroll.offset = offset;
        self.scrolled(id);
        true
    }

    /// Horizontal offset from the start side.
    fn set_scroll_x(&mut self, id: NodeId, offset: f32) -> bool {
        if !offset.is_finite() {
            return false;
        }
        let Some(node) = self.nodes.get_mut(&id) else {
            return false;
        };
        if !matches!(node.kind, Kind::Scroll | Kind::Collection) {
            return false;
        }
        let offset = offset.clamp(0.0, node.scroll.maximum_x);
        if node.scroll.offset_x == offset {
            return false;
        }
        node.scroll.offset_x = offset;
        self.scrolled(id);
        true
    }

    /// Moves the view by physical pixels: positive is to the right, which in a
    /// right-to-left container is back toward the start.
    fn scroll_x_by(&mut self, id: NodeId, delta: f32) -> bool {
        let Some(node) = self.nodes.get(&id) else {
            return false;
        };
        if delta == 0.0 {
            return false;
        }
        // A grid scrolls its own columns, so that frozen ones stay put and
        // only the ones in view exist; it is told instead of being moved.
        if matches!(
            node.control,
            Control::Collection {
                kind: crate::collection::CollectionKind::Grid,
                ..
            }
        ) {
            return self.grid_scroll_x(id, delta);
        }
        let delta = if node.resolved.rtl() { -delta } else { delta };
        self.set_scroll_x(id, node.scroll.offset_x + delta)
    }

    /// After an offset changed: children move, and the pointer may be over
    /// something else now.
    fn scrolled(&mut self, id: NodeId) {
        self.invalidate_paint(id);
        self.geometry_dirty = true;
        self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        self.refresh_bounds(self.scene.size);
        let hovered = self.pointer.and_then(|position| self.hit_test(position));
        for id in [self.hovered, hovered].into_iter().flatten() {
            self.invalidate_paint(id);
        }
        self.hovered = hovered;
    }

    /// The innermost container under the pointer that can still move sideways.
    fn scroll_x_at_pointer(&mut self, delta: f32) -> bool {
        let Some(pointer) = self.pointer else {
            return false;
        };
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        order.into_iter().rev().any(|id| {
            let node = &self.nodes[&id];
            matches!(node.kind, Kind::Scroll | Kind::Collection)
                && node.clip.contains(pointer)
                && self.scroll_x_by(id, delta)
        })
    }

    fn scroll_at_pointer(&mut self, delta: f32) -> bool {
        let Some(pointer) = self.pointer else {
            return false;
        };
        if let Some(popup) = self.popup
            && let Some(panel) = self.nodes[&popup.owner].children.first().copied()
            && self.nodes[&panel].clip.contains(pointer)
        {
            return self.set_scroll(panel, self.nodes[&panel].scroll.offset + delta);
        }
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        for id in order.into_iter().rev() {
            let node = &self.nodes[&id];
            if matches!(
                node.kind,
                Kind::Scroll | Kind::Popup | Kind::Collection | Kind::TextArea | Kind::RichEditor
            ) && node.clip.contains(pointer)
                && self.set_scroll(id, node.scroll.offset + delta)
            {
                return true;
            }
        }
        false
    }

    fn scroll_into_view(&mut self, target: NodeId) -> bool {
        let mut parent = self.nodes.get(&target).and_then(|node| node.parent);
        let mut changed = false;
        while let Some(id) = parent {
            let node = &self.nodes[&id];
            if matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection) {
                let bounds = self.nodes[&target].bounds;
                let top = node.bounds.y + node.style.padding.max(0.0);
                let bottom = node.bounds.y + node.bounds.height - node.style.padding.max(0.0);
                let delta = if bounds.y < top {
                    bounds.y - top
                } else if bounds.y + bounds.height > bottom {
                    bounds.y + bounds.height - bottom
                } else {
                    0.0
                };
                changed |= self.set_scroll(id, node.scroll.offset + delta);
                // The same for the other axis; the start edge wins when the
                // target is wider than the view.
                let node = &self.nodes[&id];
                let bounds = self.nodes[&target].bounds;
                let padding = node.style.padding.max(0.0);
                let (left, right) = (
                    node.bounds.x + padding,
                    node.bounds.x + node.bounds.width - padding,
                );
                let (before, after) = (bounds.x - left, bounds.x + bounds.width - right);
                let sideways = if node.resolved.rtl() {
                    if after > 0.0 { after } else { before.min(0.0) }
                } else if before < 0.0 {
                    before
                } else {
                    after.max(0.0)
                };
                changed |= self.scroll_x_by(id, sideways);
                if self.nodes[&id].kind == Kind::Popup {
                    break;
                }
            }
            parent = self.nodes[&id].parent;
        }
        changed
    }

    pub fn action(&mut self, target: NodeId, action: Action) -> bool {
        let handled = self.handle_action(target, action);
        self.note_focus();
        handled
    }

    fn handle_action(&mut self, target: NodeId, action: Action) -> bool {
        if let Some(handled) = self.drag_action(target, &action) {
            return handled;
        }
        if let Some(handled) = self.collection_action(target, &action) {
            return handled;
        }
        if let Some(handled) = self.control_action(target, &action) {
            return handled;
        }
        let Some(node) = self.nodes.get(&target) else {
            return false;
        };
        match action {
            Action::Focus if node.focusable() => {
                if let Some(old) = self.focused {
                    self.invalidate_paint(old);
                }
                self.focused = Some(target);
                self.invalidate_paint(target);
                self.keyboard_pressed = None;
                self.scroll_into_view(target);
                self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
                true
            }
            Action::Activate if node.activatable() => self.activate(target),
            Action::ScrollBy(delta) => self.set_scroll(target, node.scroll.offset + delta),
            Action::ScrollTo(offset) => self.set_scroll(target, offset),
            Action::ScrollByX(delta) => self.scroll_x_by(target, delta),
            Action::ScrollToX(offset) => self.set_scroll_x(target, offset),
            Action::ScrollIntoView => self.scroll_into_view(target),
            _ => false,
        }
    }
}
