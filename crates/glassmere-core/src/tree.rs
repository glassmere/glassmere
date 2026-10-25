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
use glassmere_scene::{
    Color, Point, Primitive, Rect, Scene, Size, TextMeasurer, TextStyle, TextWrap,
};
mod actions;
mod animation_state;
mod collection_input;
mod control_input;
mod control_paint;
pub mod diagnostics;
mod dock_input;
mod drag_input;
mod edge_scroll;
mod editor_text;
mod events;
mod focus_input;
mod handlers;
mod image;
mod measure;
mod overlay_input;
mod paint;
mod rich_editor;
mod rich_text;
mod run;
mod scrollbar_input;
mod scrolling;
mod snapshot;
mod text_area_input;
mod toasts;
mod touch_input;
mod types;
use measure::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
pub use touch_input::{GestureResponse, Pen, Swipe, SwipeDirection, TouchPhase};
pub use types::*;

/// A second press on a title bar this soon and this near the first means
/// "maximize" rather than another move.
const TITLE_DOUBLE_PRESS: Duration = Duration::from_millis(400);
const TITLE_DOUBLE_PRESS_NEAR: f32 = 24.0;

struct PaintChunk {
    before: Vec<Primitive>,
    children: Vec<Arc<PaintChunk>>,
    after: Vec<Primitive>,
}

impl PaintChunk {
    /// Primitives in this chunk and everything under it.
    fn count(&self) -> usize {
        self.before.len()
            + self.after.len()
            + self
                .children
                .iter()
                .map(|child| child.count())
                .sum::<usize>()
    }

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
    /// The slot of what this node sends, in the handler slab of the `App`
    /// driving the context.
    handler: Option<u32>,
    /// Whether the view read this node's hover or focus when it last
    /// declared it, and whether the declaration running now has. A change in
    /// an observed state declares again; in any other it only repaints.
    observed: std::cell::Cell<bool>,
    observing: std::cell::Cell<bool>,
}

impl Node {
    /// Whether the horizontal offset counts from the right. Editors lay their
    /// text out left to right whatever the direction around them.
    fn scrolls_x_from_end(&self) -> bool {
        self.resolved.rtl() && self.kind != Kind::TextArea
    }

    /// The box inside the padding.
    fn content_box(&self) -> Rect {
        let padding = self.style.padding.max(0.0);
        Rect {
            x: self.bounds.x + padding,
            y: self.bounds.y + padding,
            width: (self.bounds.width - padding * 2.0).max(0.0),
            height: (self.bounds.height - padding * 2.0).max(0.0),
        }
    }

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
                || self
                    .custom
                    .as_ref()
                    .is_some_and(|widget| widget.semantics.focusable))
    }

    /// Whether a press can land on this node: anything focusable or
    /// activatable, the pointer-only column sizer, and the window frame a
    /// view draws for itself.
    fn takes_pointer(&self) -> bool {
        self.focusable()
            || self.activatable()
            || (self.enabled
                && matches!(
                    self.control,
                    Control::Sizer { .. }
                        | Control::TitleBar
                        | Control::ResizeGrip(_)
                        | Control::GridScrollbar { .. }
                ))
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
        let mut semantics = self.named();
        // A control whose text is a symbol, or nothing at all, is announced
        // by its tooltip: that is what makes an icon button nameable, and
        // what the `Inaccessible` diagnostic already counts as a name.
        if !semantics.label.chars().any(char::is_alphanumeric)
            && let Some(tooltip) = self.tooltip.as_ref().filter(|text| !text.trim().is_empty())
        {
            semantics.label = tooltip.clone();
        }
        semantics
    }

    /// The name a control carries itself, before a tooltip is considered.
    fn named(&self) -> Semantics {
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
                Control::TextInput { password: true, .. } => Role::PasswordInput,
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
                    Kind::TextInput => Role::TextInput,
                    Kind::TextArea | Kind::RichEditor => Role::MultilineTextInput,
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
            value: None,
            numeric: None,
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
    /// The clicks a pointer made, as opposed to keys or assistive
    /// technology: a widget that hears pointer events gets no `Activate` for them.
    pointer_clicks: HashSet<NodeId>,
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
    /// Elements the view built, and the bytes of key and text in them.
    declared: std::cell::Cell<usize>,
    declared_bytes: std::cell::Cell<usize>,
    /// Font families the declaration named, for the host to check.
    families: std::cell::RefCell<std::collections::BTreeSet<String>>,
    /// Element vectors from earlier declarations, waiting to be filled
    /// again. A declaration is thrown away every frame; the memory it used
    /// does not have to be.
    spare: std::cell::RefCell<Vec<Vec<Element>>>,
    submitted: HashSet<NodeId>,
    pub(crate) popup: Option<PopupState>,
    drag: Option<Box<crate::drag::DragState>>,
    /// Source of a drag the host handed to the operating system.
    exported_drag: Option<NodeId>,
    dock_drag: Option<Box<dock_input::DockDrag>>,
    dock_grab: Option<dock_input::DockGrab>,
    scrollbar_grab: Option<scrollbar_input::ScrollbarGrab>,
    /// A press held on a scrollbar's track, paging again while it lasts.
    track_hold: Option<scrollbar_input::TrackHold>,
    /// The node whose scrollbar the pointer is on, drawn wider.
    bar_hovered: Option<NodeId>,
    /// What `Effects::copy` left for the host, what `Effects::paste` asked
    /// for, and what the host last read back.
    clip_out: Option<crate::Clip>,
    paste_requested: bool,
    clip_in: Option<crate::Clip>,
    /// What the host has been asked for, and what it has answered.
    file_request: Option<crate::FileRequest>,
    chosen_files: Option<Vec<std::path::PathBuf>>,
    opened_files: Vec<std::path::PathBuf>,
    notifications: Vec<crate::Notification>,
    menu_out: Option<crate::Menu>,
    tray_out: Option<Option<crate::Tray>>,
    /// What the host has been given, so that declaring the same menu or
    /// tray again asks for nothing.
    menu_shown: Option<crate::Menu>,
    /// The host shows `menu_shown` in a menu bar of the window's own.
    native_menu_bar: bool,
    tray_shown: Option<crate::Tray>,
    /// What a title bar of the application's own asked the window to do,
    /// what the host says about the window, and the size it was last given.
    window_commands: Vec<crate::WindowCommand>,
    window_info: crate::WindowInfo,
    size: Size,
    /// When and where the title bar was last pressed, for telling a second
    /// press that means "maximize" from two separate ones.
    last_title_press: Option<(Duration, Point)>,
    /// Where the maximize button of a title bar the view draws itself is,
    /// for a host that has to tell the platform.
    maximize_button: Option<Rect>,
    /// What the overlays painted straight into the last scene. They are not
    /// nodes, so nothing invalidates them: the frame one appears, moves or
    /// goes has to name its rectangle itself.
    painted_overlays: Vec<Rect>,
    redraw_requested: bool,
    caret_movement: crate::CaretMovement,
    direction: crate::Direction,
    watched: HashMap<std::path::PathBuf, Watched>,
    watch_requests: Vec<std::path::PathBuf>,
    images: HashMap<std::path::PathBuf, image::LoadedImage>,
    image_requests: Vec<std::path::PathBuf>,
    hot_save: bool,
    /// How many theme, messages and format requests of the app this context
    /// has applied.
    pub(crate) settings: (u64, u64, u64),
    hot_state: Option<String>,
    diagnostic: diagnostics::DiagnosticState,
    toast: toasts::ToastState,
    touches: HashMap<u64, touch_input::Touch>,
    /// When and where the last tap ended, for telling a double tap.
    last_tap: Option<(Duration, Point)>,
    fling: Option<touch_input::Fling>,
    /// When a selection drag held outside an editor last scrolled it.
    edge_scroll_at: Option<Duration>,
    pub(crate) messages: Option<Box<dyn crate::Messages>>,
    pub(crate) format: Option<Box<dyn crate::LocaleFormat>>,
    pub(crate) missing_messages: std::sync::Mutex<Vec<String>>,
    theme: Theme,
    stylesheet: StyleSheet,
    style_warnings: Vec<String>,
    state_graph: Vec<crate::GraphNode>,
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
    /// The handler slab of the `App` driving this context, typed by its
    /// message; `None` before an `App` has declared.
    handlers: Option<Box<dyn std::any::Any>>,
    /// What reconcile did to handlers, for the typed code to apply: the
    /// declared handler each node now has, and the slots of removed nodes.
    handler_moves: Vec<(NodeId, Option<u32>)>,
    freed_handlers: Vec<u32>,
    /// Pending filters each grid declared, read by the next batch.
    grid_filters: std::cell::RefCell<Vec<(NodeId, crate::collection::FilterNote)>>,
    /// The filter delays of grids, by the node of their rows.
    grid_timers: HashMap<NodeId, crate::collection::FilterTimer>,
    /// What grids declared with `data_grid` keep: cursor, sideways scroll
    /// and the cell being edited, by the node of their rows.
    grid_nodes: std::cell::RefCell<HashMap<NodeId, crate::collection::GridNode>>,
    /// Collection input a declaration queued for the next batch.
    grid_pending: std::cell::RefCell<Vec<(NodeId, crate::collection::CollectionInput)>>,
    /// Nodes whose text the user edited in the batch being resolved.
    text_typed: HashSet<NodeId>,
    /// The selection a message-form collection last scrolled into view.
    revealed: std::cell::RefCell<HashMap<NodeId, (Option<crate::ItemId>, u32)>>,
    /// The top row of each message-form list, kept in place across a reorder.
    list_anchors: std::cell::RefCell<HashMap<NodeId, crate::collection::ListAnchor>>,
    /// The selection each message-form collection last reported as lost, so
    /// an application that keeps it is told once, not every batch.
    lost_selections: std::cell::RefCell<HashMap<NodeId, crate::ItemId>>,
    /// Activations, edits and submissions in the order they arrived, beside
    /// the per-kind maps.
    queued: Vec<(NodeId, Queued)>,
    /// Elements whose hover or focus the view read before their node existed.
    observed_new: std::cell::RefCell<Vec<u32>>,
    /// Something the view reads may have changed since it last declared.
    view_stale: bool,
}

/// What reached a node, in the order of `Context::queued`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Queued {
    Click,
    Update,
    Submit,
    Area,
}

/// Who is hovered, focused and pressed, compared from frame to frame for the
/// nodes the view observed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Interaction {
    hovered: Option<NodeId>,
    focused: Option<NodeId>,
    pressed: Option<NodeId>,
    keyboard: Option<NodeId>,
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
            handler: None,
            observed: std::cell::Cell::new(false),
            observing: std::cell::Cell::new(false),
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
            pointer_clicks: HashSet::new(),
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
            declared: std::cell::Cell::new(0),
            declared_bytes: std::cell::Cell::new(0),
            families: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            spare: std::cell::RefCell::new(Vec::new()),
            submitted: HashSet::new(),
            popup: None,
            drag: None,
            exported_drag: None,
            dock_drag: None,
            dock_grab: None,
            scrollbar_grab: None,
            track_hold: None,
            bar_hovered: None,
            clip_out: None,
            paste_requested: false,
            clip_in: None,
            file_request: None,
            chosen_files: None,
            opened_files: Vec::new(),
            notifications: Vec::new(),
            menu_out: None,
            tray_out: None,
            menu_shown: None,
            native_menu_bar: false,
            tray_shown: None,
            window_commands: Vec::new(),
            window_info: crate::WindowInfo::default(),
            size: Size::default(),
            last_title_press: None,
            maximize_button: None,
            painted_overlays: Vec::new(),
            redraw_requested: false,
            caret_movement: crate::CaretMovement::Visual,
            direction: crate::Direction::Ltr,
            watched: HashMap::new(),
            images: HashMap::new(),
            image_requests: Vec::new(),
            watch_requests: Vec::new(),
            hot_save: false,
            settings: (0, 0, 0),
            hot_state: None,
            diagnostic: diagnostics::DiagnosticState::new(),
            toast: toasts::ToastState::default(),
            touches: HashMap::new(),
            last_tap: None,
            fling: None,
            edge_scroll_at: None,
            messages: None,
            format: None,
            missing_messages: std::sync::Mutex::new(Vec::new()),
            theme,
            stylesheet: StyleSheet::default(),
            style_warnings: Vec::new(),
            state_graph: Vec::new(),
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
            handlers: None,
            handler_moves: Vec::new(),
            freed_handlers: Vec::new(),
            grid_filters: Default::default(),
            grid_timers: HashMap::new(),
            grid_nodes: Default::default(),
            grid_pending: Default::default(),
            text_typed: HashSet::new(),
            revealed: Default::default(),
            list_anchors: Default::default(),
            lost_selections: Default::default(),
            queued: Vec::new(),
            observed_new: std::cell::RefCell::new(Vec::new()),
            view_stale: true,
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
        self.view_stale = true;
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
                entry.error = Some(error.clone());
                if first {
                    self.diagnose_resource(path, &error);
                }
                first
            }
        };
        if changed {
            self.pending |= Dirty::PAINT;
            self.view_stale = true;
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

    /// Asks for the app's state through `App::save_state` at the next
    /// batch; hosts call this before a development restart.
    pub fn request_hot_save(&mut self) {
        self.hot_save = true;
    }

    /// Answers a hot-save request with the state an `App` saves, and spends it.
    pub(crate) fn answer_hot_save(&mut self, state: impl FnOnce() -> Option<String>) {
        if self.hot_save {
            self.hot_save = false;
            self.hot_state = state();
        }
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
        self.view_stale = true;
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
        self.view_stale = true;
    }
    /// What this window's tree, scene and caches occupy. Walks every node,
    /// so ask for it when an inspector is open rather than every frame.
    pub fn memory(&self) -> MemoryReport {
        let mut report = MemoryReport {
            nodes: self.nodes.len(),
            node_bytes: self.nodes.len() * std::mem::size_of::<Node>(),
            scene_primitives: self.scene.primitives.len(),
            scene_bytes: self.scene.primitives.len() * std::mem::size_of::<Primitive>(),
            ..MemoryReport::default()
        };
        for node in self.nodes.values() {
            report.text_bytes += node.text.len()
                + match &node.key {
                    crate::ui::WidgetKey::Named(key) => key.len(),
                    crate::ui::WidgetKey::Site(..) => 0,
                };
            if let Some(chunk) = &node.paint_cache {
                report.cached_nodes += 1;
                report.cached_primitives += chunk.count();
            }
        }
        for loaded in self.images.values() {
            report.images += 1;
            report.image_bytes += loaded
                .pixels()
                .map_or(0, |(_, _, pixels): (u32, u32, &[u8])| pixels.len());
        }
        report
    }

    /// Unknown class names requested since the stylesheet was installed.
    pub fn style_warnings(&self) -> &[String] {
        &self.style_warnings
    }

    /// What `Effects::copy` left, for the host to put on the clipboard.
    pub fn take_clip(&mut self) -> Option<crate::Clip> {
        self.clip_out.take()
    }

    /// Whether the application asked for what is on the clipboard. Answer with
    /// `set_clip`, which asks for a declaration of its own.
    pub fn take_paste_request(&mut self) -> bool {
        std::mem::take(&mut self.paste_requested)
    }

    /// What the host read from the clipboard.
    pub fn set_clip(&mut self, clip: crate::Clip) {
        self.clip_in = Some(clip);
        self.redraw_requested = true;
    }

    /// What the application asked the window to do: move it, resize it, minimize,
    /// maximize or close it. Hosts take these after a declaration.
    pub fn take_window_commands(&mut self) -> Vec<crate::WindowCommand> {
        std::mem::take(&mut self.window_commands)
    }

    /// What the host says about the window. Hosts set it before declaring;
    /// a view reads it through `Ui::window`.
    pub fn set_window_info(&mut self, info: crate::WindowInfo) {
        if self.window_info != info {
            self.window_info = info;
            self.pending |= Dirty::PAINT;
            self.view_stale = true;
        }
    }

    pub(crate) fn window_info(&self) -> crate::WindowInfo {
        self.window_info
    }

    /// Where the maximize button of a title bar the view draws itself sits,
    /// in logical coordinates, for a host that tells the platform so that it
    /// can offer its own window layouts over it.
    pub fn maximize_button(&self) -> Option<Rect> {
        self.maximize_button
    }

    /// The logical size the window was last declared at.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The edge a node resizes the window by, for a host choosing a cursor.
    pub fn resize_grip(&self, id: NodeId) -> Option<crate::ResizeEdge> {
        match self.nodes.get(&id)?.control {
            Control::ResizeGrip(edge) => Some(edge),
            _ => None,
        }
    }

    /// A file dialog a view asked for. Hosts put it up after the
    /// declaration and answer with `set_chosen_files`.
    pub fn take_file_request(&mut self) -> Option<crate::FileRequest> {
        self.file_request.take()
    }

    /// What the dialog answered; an empty list means it was cancelled.
    pub fn set_chosen_files(&mut self, paths: Vec<std::path::PathBuf>) {
        self.chosen_files = Some(paths);
        self.redraw_requested = true;
    }

    /// Files the operating system asked the application to open. Hosts set
    /// this before the first declaration and whenever more arrive.
    pub fn set_opened_files(&mut self, paths: Vec<std::path::PathBuf>) {
        self.opened_files = paths;
        self.redraw_requested = true;
    }

    pub(crate) fn opened_files(&self) -> &[std::path::PathBuf] {
        &self.opened_files
    }

    /// Notifications a view asked for, in the order it asked.
    pub fn take_notifications(&mut self) -> Vec<crate::Notification> {
        std::mem::take(&mut self.notifications)
    }

    /// The native menu a view last declared, taken once so that a host
    /// rebuilds it only when it changes.
    pub fn take_menu(&mut self) -> Option<crate::Menu> {
        self.menu_out.take()
    }

    /// Whether the host shows the menu a view declared in a menu bar of the
    /// window's own, so that `Ui::menu_bar` has nothing left to draw.
    pub fn set_native_menu_bar(&mut self, shown: bool) {
        if self.native_menu_bar != shown {
            self.native_menu_bar = shown;
            self.view_stale = true;
        }
    }

    /// Whether `menu` is the one the host shows in the window's menu bar.
    pub(crate) fn menu_bar_is_native(&self, menu: &crate::Menu) -> bool {
        self.native_menu_bar && self.menu_shown.as_ref() == Some(menu)
    }

    /// The tray icon a view last declared, or `Some(None)` to take it away.
    pub fn take_tray(&mut self) -> Option<Option<crate::Tray>> {
        self.tray_out.take()
    }

    /// What `Ui::observe` last saw of a reactive store, for the inspector.
    pub fn state_graph(&self) -> &[crate::GraphNode] {
        &self.state_graph
    }
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }
    /// A press on a title bar of the application's own moves the window, and
    /// a second press near the first maximizes it instead; a press on an
    /// edge resizes. The platform takes the pointer over from there, so
    /// nothing more arrives until the drag is done.
    fn press_window_frame(&mut self) {
        let Some(control) = self
            .captured
            .and_then(|id| self.nodes.get(&id))
            .map(|node| node.control.clone())
        else {
            return;
        };
        let at = self.pointer.unwrap_or_default();
        match control {
            Control::TitleBar => {
                let again = self.last_title_press.is_some_and(|(when, from)| {
                    self.now.saturating_sub(when) <= TITLE_DOUBLE_PRESS
                        && (from.x - at.x).hypot(from.y - at.y) <= TITLE_DOUBLE_PRESS_NEAR
                });
                self.last_title_press = (!again).then_some((self.now, at));
                self.window_commands.push(if again {
                    crate::WindowCommand::ToggleMaximize
                } else {
                    crate::WindowCommand::Drag
                });
                // The platform swallows the release, so nothing is held.
                self.captured = None;
            }
            Control::ResizeGrip(edge) => {
                self.window_commands
                    .push(crate::WindowCommand::Resize(edge));
                self.captured = None;
            }
            _ => {}
        }
    }

    /// Whether the node is being held down, by pointer, touch or keyboard.
    pub fn pressed(&self, id: NodeId) -> bool {
        (self.captured == Some(id) && self.hovered == Some(id))
            || self.touch_pressed(id)
            || self
                .keyboard_pressed
                .is_some_and(|(pressed, _)| pressed == id)
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

    /// Conservative logical damage from the most recent declaration, which the
    /// renderers use to limit what they redraw.
    pub fn damage_regions(&self) -> &[Rect] {
        &self.damage
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
        self.view_stale = true;
    }

    pub fn clear_bindings(&mut self) {
        self.bindings.clear();
        self.view_stale = true;
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

    /// An empty element vector, reusing one from an earlier frame when
    /// there is one.
    pub(crate) fn take_spare(&self) -> Vec<Element> {
        self.spare.borrow_mut().pop().unwrap_or_default()
    }

    /// Keeps an emptied vector for the next declaration. The cap is well
    /// above any real declaration's depth and breadth, so this holds on to
    /// nothing that matters.
    pub(crate) fn give_spare(&self, mut vector: Vec<Element>) {
        const KEEP: usize = 512;
        let mut spare = self.spare.borrow_mut();
        if spare.len() < KEEP && vector.capacity() > 0 {
            vector.clear();
            spare.push(vector);
        }
    }

    /// A font family a style named, kept for the host to check once the
    /// declaration is over.
    pub(crate) fn note_family(&self, name: &str) {
        let mut families = self.families.borrow_mut();
        if !families.contains(name) {
            families.insert(name.to_owned());
        }
    }

    /// One more element, with the bytes its key and text carry.
    /// Returns which element of the declaration it is, from 1.
    pub(crate) fn count_element(&self, bytes: usize) -> u32 {
        let count = self.declared.get() + 1;
        self.declared.set(count);
        self.declared_bytes.set(self.declared_bytes.get() + bytes);
        count as u32
    }

    /// The view read this node's hover or focus.
    pub(crate) fn observe_node(&self, id: NodeId) {
        if let Some(node) = self.nodes.get(&id) {
            node.observing.set(true);
        }
    }

    /// The view read the hover or focus of an element whose node does not
    /// exist yet; it is observed from the moment it is created.
    pub(crate) fn observe_element(&self, seq: u32) {
        self.observed_new.borrow_mut().push(seq);
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
}
