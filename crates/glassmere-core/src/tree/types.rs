//! The public records the tree hands out: identities, roles, events, actions
//! and statistics.

use super::*;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Role {
    #[default]
    Group,
    Label,
    Button,
    ScrollView,
    TextInput,
    PasswordInput,
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

/// What a node is, for the keyboard, the accessibility tree and the tests.
/// A widget of the application's own fills this in itself; `value` and
/// `numeric` are what assistive technology reads out, so a dial that reports
/// neither is a dial that says nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Semantics {
    pub role: Role,
    pub label: String,
    pub disabled: bool,
    /// Joins the tab order and can be given the keyboard, whatever the role.
    pub focusable: bool,
    /// What it holds, read out after the label.
    pub value: Option<String>,
    /// What it holds on a scale, for a slider or a dial.
    pub numeric: Option<NumericValue>,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
    /// A command chosen somewhere outside the window: a native menu, a tray
    /// icon, or an application's own path to it. Activates the command
    /// declared with this id, as its button would.
    Command(String),
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
#[non_exhaustive]
pub struct FrameStats {
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
    /// CPU time spent running the view and reconciling the tree.
    pub declare: Duration,
    /// CPU time spent in layout and bounds propagation; zero when skipped.
    pub layout: Duration,
    /// CPU time spent generating and flattening paint chunks; zero when skipped.
    pub paint: Duration,
    /// CPU time the text measurer spent shaping and measuring, which is part
    /// of declare, layout and paint rather than extra to them.
    pub shaping: Duration,
    /// Elements the view built this frame. Each one is a declaration the
    /// framework allocated and threw away, so this is what a frame costs in
    /// allocation, and what `Ui::memo` saves.
    pub elements: usize,
    /// Bytes of keys and text those elements carried.
    pub element_bytes: usize,
    /// Whether the view ran this frame. An `App` declares only when
    /// something it reads may have changed; `Context::run` always does.
    pub declared: bool,
    /// Messages the batch delivered to `App::update`.
    pub messages: usize,
}

/// What the framework is holding on the CPU. Counted by walking the tree, so
/// it is exact for what it names and silent about what it cannot see: the
/// layout engine's own arena, the allocator's overhead, and the glyph atlas,
/// which lives on the GPU behind glyphon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemoryReport {
    pub nodes: usize,
    /// The nodes themselves, without what they point at.
    pub node_bytes: usize,
    /// Text, keys and resolved style strings the nodes hold.
    pub text_bytes: usize,
    /// Primitives in the last scene, and what they occupy.
    pub scene_primitives: usize,
    pub scene_bytes: usize,
    /// Nodes holding a paint cache, and the primitives in those caches.
    pub cached_nodes: usize,
    pub cached_primitives: usize,
    /// Decoded images kept, and their pixels.
    pub images: usize,
    pub image_bytes: usize,
}

/// Inspector-facing facts about one node that ordinary semantics do not carry.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct NodeDetails {
    /// Explicit key, or the declaring call site as `file.rs:line:column`,
    /// with `#n` for the n-th repeat of one site.
    pub key: String,
    /// Whether the key was given with `Ui::keyed` rather than taken from
    /// the call site.
    pub explicit: bool,
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
