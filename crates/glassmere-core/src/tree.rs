#[cfg(not(feature = "std"))]
use crate::float::FloatExt as _;
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
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use glassmere_scene::{
    Color, Point, Primitive, Rect, Scene, Size, TextMeasurer, TextStyle, TextWrap,
};
mod actions;
mod animation_state;
mod bound;
mod calendar_input;
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
mod press_count;
mod rich_editor;
mod rich_text;
mod run;
mod scrollbar_input;
mod scrolling;
mod snapshot;
mod submenu_hover;
mod text_area_input;
mod toasts;
mod touch_input;
mod types;
use crate::clock::Instant;
use crate::collections::{HashMap, HashSet};
use alloc::rc::Rc;
#[cfg(feature = "std")]
use alloc::sync::Arc;
use core::time::Duration;
use measure::*;
pub use touch_input::{GestureResponse, Pen, Swipe, SwipeDirection, TouchPhase};
pub use types::*;

/// A second press on a title bar this soon and this near the first means
/// "maximize" rather than another move.
/// What one walk over the nodes finds for `prepare_controls`, by what each
/// needs done.
#[derive(Default)]
struct Preparing {
    baseline_rows: Vec<NodeId>,
    text_areas: Vec<NodeId>,
    rich_texts: Vec<NodeId>,
    rich_editors: Vec<NodeId>,
    /// Sliders whose label is measured.
    labelled: Vec<NodeId>,
    /// Rows whose hint is measured.
    hinted: Vec<NodeId>,
    editors: Vec<NodeId>,
    /// Whether the lists hold what a walk over the nodes finds: set once
    /// `prepare_controls` has stored them, dropped by every declaration. It
    /// holds because nodes are created, removed, and given a kind, style,
    /// children or editor only while a declaration reconciles; whatever
    /// else starts to do that must drop it too.
    valid: bool,
}

impl Preparing {
    fn clear(&mut self) {
        self.baseline_rows.clear();
        self.text_areas.clear();
        self.rich_texts.clear();
        self.rich_editors.clear();
        self.labelled.clear();
        self.hinted.clear();
        self.editors.clear();
    }
}

/// The key of the panel menu a dock puts at the end of a tab strip.
const STRIP_MENU: &str = "$menu";
/// How wide the buttons that scroll an overflowing tab strip are.
pub(crate) const STRIP_BUTTON: f32 = 22.0;
/// How far the shade beside a scroll button reaches over the tabs.
const STRIP_SHADE: f32 = 14.0;
const TITLE_DOUBLE_PRESS: Duration = Duration::from_millis(400);
const TITLE_DOUBLE_PRESS_NEAR: f32 = 24.0;

struct PaintChunk {
    /// What the node draws itself: the first `split` of them go before its
    /// children and the rest after.
    primitives: Vec<Primitive>,
    split: usize,
    children: Vec<Rc<PaintChunk>>,
    /// Primitives in this chunk and everything under it.
    total: usize,
    /// The assembly that last put this chunk into the scene and the index it
    /// put it at, or `(0, 0)` before any has. A chunk is only ever replaced,
    /// whole, by a value that starts at `(0, 0)`, so until something else is
    /// written over that range, the range holds it. Anything that rewrote the
    /// `primitives`, `split` or `children` of a chunk would have to set this
    /// back to `(0, 0)`.
    placed: core::cell::Cell<(u64, usize)>,
}

impl PaintChunk {
    fn new(primitives: Vec<Primitive>, split: usize, children: Vec<Rc<PaintChunk>>) -> Self {
        debug_assert!(split <= primitives.len());
        let total = primitives.len() + children.iter().map(|child| child.total).sum::<usize>();
        Self {
            primitives,
            split,
            children,
            total,
            placed: core::cell::Cell::new((0, 0)),
        }
    }

    /// Takes what a chunk that is to be written over has room for: its
    /// primitives, emptied, and the buffer of the first text among them. It
    /// lets go of its children first, and of the list they are in, so that the
    /// chunk of a child, which is painted after this one, is held by no other
    /// chunk and can be written over too. What is left of the chunk is not fit
    /// to be read: `put_in` writes it over.
    fn take_room(&mut self) -> (Vec<Primitive>, Option<String>) {
        self.children = Vec::new();
        let text = self
            .primitives
            .iter_mut()
            .find_map(|primitive| match primitive {
                Primitive::Text { text, .. } | Primitive::StyledText { text, .. } => {
                    Some(core::mem::take(text))
                }
                _ => None,
            });
        self.primitives.clear();
        (core::mem::take(&mut self.primitives), text)
    }

    /// This chunk in the allocation `spare` is, written over, when nothing
    /// else holds that, and in a new one when something does. What is written
    /// is the whole chunk, so that nothing of the one it replaces is left in
    /// it: not its total, and not where it was placed.
    fn put_in(self, spare: Option<Rc<Self>>) -> Rc<Self> {
        match spare {
            Some(mut spare) => match Rc::get_mut(&mut spare) {
                Some(slot) => {
                    *slot = self;
                    spare
                }
                None => Rc::new(self),
            },
            None => Rc::new(self),
        }
    }

    /// Primitives in this chunk and everything under it.
    fn count(&self) -> usize {
        self.total
    }

    fn append_to(&self, output: &mut Vec<Primitive>) {
        let (before, after) = self.primitives.split_at(self.split);
        output.extend_from_slice(before);
        for child in &self.children {
            child.append_to(output);
        }
        output.extend_from_slice(after);
    }

    /// Writes what `append_to` would append into `output` from the index
    /// `*at` on, over the primitives there, and moves `*at` past it.
    /// `assembly` is the number of this assembly of the scene. It is at least
    /// 2, so that the one before it is never the 0 of a chunk not yet placed.
    ///
    /// An assembly writes forward only and cuts the scene where it ends. A
    /// chunk that the assembly before put at this very index is therefore
    /// still whole there and is left alone, if the scene reaches that far: an
    /// assembly that did not finish leaves it empty, and `*at` must not pass
    /// its end.
    fn place(&self, output: &mut Vec<Primitive>, at: &mut usize, assembly: u64) {
        let here = *at;
        let end = here + self.total;
        let there = self.placed.get() == (assembly - 1, here) && end <= output.len();
        self.placed.set((assembly, here));
        if there {
            *at = end;
            return;
        }
        let (before, after) = self.primitives.split_at(self.split);
        place_slice(before, output, at);
        for child in &self.children {
            child.place(output, at, assembly);
        }
        place_slice(after, output, at);
        debug_assert_eq!(*at, end);
    }

    /// Checks that `scene` holds what `append_to` appends from the index
    /// `from` on, and returns the index after it.
    #[cfg(debug_assertions)]
    fn assert_placed(&self, scene: &[Primitive], from: usize) -> usize {
        let (before, after) = self.primitives.split_at(self.split);
        let mut at = assert_primitives(scene, before, from);
        for child in &self.children {
            at = child.assert_placed(scene, at);
        }
        assert_primitives(scene, after, at)
    }
}

/// Writes `source` into `output` from the index `*at` on, over the primitives
/// there for as far as they reach and after them from there, and moves `*at`
/// past it. Every slot written becomes a copy of its source.
fn place_slice(source: &[Primitive], output: &mut Vec<Primitive>, at: &mut usize) {
    let fits = source.len().min(output.len() - *at);
    let (over, rest) = source.split_at(fits);
    for (slot, primitive) in output[*at..*at + fits].iter_mut().zip(over) {
        overwrite(slot, primitive);
    }
    if !rest.is_empty() {
        output.extend_from_slice(rest);
    }
    *at += source.len();
}

/// Makes `slot` a copy of `source`, writing text over the text the slot held
/// instead of allocating it again, and the fields of a primitive that owns
/// nothing one by one. No field is left alone for being equal already: 0.0
/// and -0.0 are equal, and are not the same.
fn overwrite(slot: &mut Primitive, source: &Primitive) {
    match (slot, source) {
        (
            Primitive::Text {
                rect,
                text,
                font_size,
                color,
            },
            Primitive::Text {
                rect: source_rect,
                text: source_text,
                font_size: source_size,
                color: source_color,
            },
        ) => {
            *rect = *source_rect;
            *font_size = *source_size;
            *color = *source_color;
            refill(text, source_text);
        }
        (
            Primitive::StyledText {
                rect,
                text,
                style,
                color,
            },
            Primitive::StyledText {
                rect: source_rect,
                text: source_text,
                style: source_style,
                color: source_color,
            },
        ) => {
            *rect = *source_rect;
            *color = *source_color;
            style.clone_from(source_style);
            refill(text, source_text);
        }
        (
            Primitive::Quad {
                rect,
                color,
                radius,
            },
            Primitive::Quad {
                rect: source_rect,
                color: source_color,
                radius: source_radius,
            },
        ) => {
            *rect = *source_rect;
            *color = *source_color;
            *radius = *source_radius;
        }
        (Primitive::PushClip(rect), Primitive::PushClip(source_rect)) => *rect = *source_rect,
        (Primitive::PopClip, Primitive::PopClip) | (Primitive::PopLayer, Primitive::PopLayer) => {}
        (
            Primitive::PushLayer {
                bounds,
                opacity,
                transform,
            },
            Primitive::PushLayer {
                bounds: source_bounds,
                opacity: source_opacity,
                transform: source_transform,
            },
        ) => {
            *bounds = *source_bounds;
            *opacity = *source_opacity;
            *transform = *source_transform;
        }
        (
            Primitive::Shadow {
                rect,
                radius,
                color,
                blur,
                spread,
                offset,
                inset,
            },
            Primitive::Shadow {
                rect: source_rect,
                radius: source_radius,
                color: source_color,
                blur: source_blur,
                spread: source_spread,
                offset: source_offset,
                inset: source_inset,
            },
        ) => {
            *rect = *source_rect;
            *radius = *source_radius;
            *color = *source_color;
            *blur = *source_blur;
            *spread = *source_spread;
            *offset = *source_offset;
            *inset = *source_inset;
        }
        (slot, source) => *slot = source.clone(),
    }
}

/// Makes `into` a copy of `from`. The room `into` has is kept when `from` fits
/// in it and it is not more than twice the length of `from` plus 32 bytes: one
/// long text must not leave its room to every text that comes after it.
fn refill(into: &mut String, from: &String) {
    let room = into.capacity();
    if room >= from.len() && room <= from.len().saturating_mul(2).saturating_add(32) {
        into.clone_from(from);
    } else {
        *into = from.clone();
    }
}

/// Checks that `scene` holds `primitives` from the index `at` on, and returns
/// the index after them. A primitive with a NaN in it is not equal to itself,
/// so its text stands in for it.
#[cfg(debug_assertions)]
fn assert_primitives(scene: &[Primitive], primitives: &[Primitive], at: usize) -> usize {
    for (index, primitive) in primitives.iter().enumerate() {
        let placed = scene.get(at + index);
        let same = |placed: &Primitive| {
            placed == primitive || format!("{placed:?}") == format!("{primitive:?}")
        };
        assert!(
            placed.is_some_and(same),
            "{placed:?} is at {} of the scene, where {primitive:?} was placed",
            at + index
        );
    }
    at + primitives.len()
}

struct Node {
    key: WidgetKey,
    kind: Kind,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    children_by_key: crate::collections::KeyMap<NodeId>,
    text: String,
    style: Style,
    enabled: bool,
    layout_id: taffy::NodeId,
    bounds: Rect,
    clip: Rect,
    scroll: ScrollState,
    custom: Option<CustomWidget>,
    paint_cache: Option<Rc<PaintChunk>>,
    control: Control,
    // Labels and other controls pay one pointer, not an inline editing history.
    editor: Option<Box<Editor>>,
    changed: Dirty,
    changed_frame: u64,
    painted_frame: u64,
    resolved: ResolvedText,
    scope: Option<Box<ScopeState>>,
    tooltip: Option<Box<crate::Tooltip>>,
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
    observed: core::cell::Cell<bool>,
    observing: core::cell::Cell<bool>,
    /// The text this label follows, and the version of it last shown. Set
    /// only by a declaration; `Context::refresh_bound` is what reads it.
    bound: Option<Box<Bound>>,
}

/// An element declared with a text source: its sequence number, the source
/// and the version of it the declared text was read at.
type DeclaredBound = (u32, Rc<dyn crate::TextSource>, u64);

/// A label's text source: shown again when its version moves, whether or
/// not the view ran.
struct Bound {
    source: Rc<dyn crate::TextSource>,
    seen: u64,
}

impl Node {
    /// Whether the horizontal offset counts from the right. Editors lay their
    /// text out left to right whatever the direction around them.
    fn scrolls_x_from_end(&self) -> bool {
        self.resolved.rtl() && self.kind != Kind::TextArea
    }

    /// The box inside the padding.
    fn content_box(&self) -> Rect {
        let padding = self.style.inset();
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

    /// Whether the node draws something that moves while it is on screen.
    fn busy(&self) -> bool {
        matches!(
            self.control,
            Control::Progress { value: None, .. } | Control::Spinner { .. }
        )
    }

    fn focusable(&self) -> bool {
        self.enabled
            && !matches!(
                self.control,
                Control::Stepper
                    | Control::StripScroll { .. }
                    | Control::CalendarStep { .. }
                    | Control::CalendarDay { .. }
                    | Control::CalendarGrid { popup: true, .. }
            )
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
                    | Kind::Switch
                    | Kind::Disclosure
                    | Kind::Link
                    | Kind::Radio
                    | Kind::Slider
                    | Kind::ComboBox
                    | Kind::MenuButton
                    | Kind::Choice
                    | Kind::Calendar
                    | Kind::Collection
                    | Kind::DragSource
                    | Kind::DropTarget
                    | Kind::Divider
                    | Kind::DockHandle
                    | Kind::TextArea
                    | Kind::RichEditor
            ) || matches!(self.control, Control::Tab { selected: true, .. })
                || matches!(self.control, Control::RichText { links: 1.., .. })
                || matches!(self.control, Control::SelectableText)
                || self
                    .custom
                    .as_ref()
                    .is_some_and(|widget| widget.semantics.focusable))
    }

    /// Text that is selected and copied but never edited.
    fn read_only(&self) -> bool {
        matches!(self.control, Control::SelectableText)
    }

    /// A tab strip's scroll button while no tabs are hidden on its side: it
    /// is laid out but neither drawn nor reachable, and not reported.
    fn parked(&self) -> bool {
        matches!(self.control, Control::StripScroll { .. })
            && (self.clip.width <= 0.0 || self.clip.height <= 0.0)
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
                    | Kind::Switch
                    | Kind::Disclosure
                    | Kind::Link
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
            && let Some(tooltip) = self
                .tooltip
                .as_ref()
                .filter(|tooltip| !tooltip.text.trim().is_empty())
        {
            semantics.label = tooltip.text.clone();
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
                Control::Grid { .. } | Control::CalendarGrid { .. } => Role::Grid,
                Control::CalendarRow { .. } => Role::Row,
                Control::CalendarWeekday { .. } => Role::ColumnHeader,
                Control::CalendarDay { .. } => Role::Cell,
                Control::CalendarPopup { .. } => Role::Dialog,
                Control::GridCell { header: true, .. } => Role::ColumnHeader,
                Control::GridCell { .. } => Role::Cell,
                Control::TabList => Role::TabList,
                Control::Tab { .. } => Role::Tab,
                Control::TabPanel => Role::TabPanel,
                Control::Divider { .. } | Control::Sizer { .. } => Role::Splitter,
                Control::TextInput { password: true, .. } => Role::PasswordInput,
                Control::Submenu { .. } => Role::MenuItem,
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
                    Kind::Button | Kind::Disclosure | Kind::Link => Role::Button,
                    Kind::Scroll => Role::ScrollView,
                    Kind::TextInput => Role::TextInput,
                    Kind::TextArea | Kind::RichEditor => Role::MultilineTextInput,
                    Kind::Checkbox | Kind::Switch => Role::Checkbox,
                    Kind::Radio => Role::Radio,
                    Kind::Slider => Role::Slider,
                    Kind::Progress | Kind::Spinner => Role::Status,
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
                | Control::Switch { label, .. }
                | Control::Disclosure { label, .. }
                | Control::Radio { label, .. }
                | Control::Slider { label, .. }
                | Control::RangeSlider { label }
                | Control::Progress { label, .. }
                | Control::Spinner { label }
                | Control::Combo { label, .. }
                | Control::Collection { label, .. }
                | Control::CollectionRow { label, .. }
                | Control::TextArea { label, .. }
                | Control::RichEditor { label, .. }
                | Control::Grid { label, .. }
                | Control::DatePicker { label, .. }
                | Control::CalendarPopup { label }
                | Control::CalendarGrid { label, .. } => label.clone(),
                Control::CalendarWeekday { name, .. } | Control::CalendarDay { name, .. } => {
                    name.clone()
                }
                Control::DragSource { label, .. } | Control::DropTarget { label, .. } => {
                    label.clone()
                }
                Control::Dialog { title, .. } => title.clone(),
                Control::Toast { label, .. } => label.clone(),
                Control::Divider { .. } => "Splitter".into(),
                Control::Sizer { label, .. } => label.clone(),
                Control::Reveal { name, .. } => name.clone(),
                Control::DockFloat => "Floating panel".into(),
                Control::DockHandle { resize: false, .. } => "Move floating panel".into(),
                Control::DockHandle { resize: true, .. } => "Resize floating panel".into(),
                // A shortcut is written beside the item, and read with it.
                Control::Choice { hint, .. } | Control::Hint(hint) if !hint.is_empty() => {
                    format!("{}, {hint}", self.text)
                }
                // The arrow after a menu button's text is drawn, not read:
                // the role already says a menu opens. A button that is only
                // the arrow keeps it, and is named by its tooltip.
                Control::Menu { .. } => {
                    let name = self.text.trim_end_matches('▾').trim_end();
                    if name.is_empty() { &self.text } else { name }.to_owned()
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
#[cfg(feature = "std")]
struct Watched {
    bytes: Arc<[u8]>,
    text: Option<Arc<str>>,
    error: Option<String>,
}

pub struct Context {
    nodes: crate::collections::FastMap<NodeId, Node>,
    layout: taffy::TaffyTree<Measurement>,
    root: NodeId,
    frame: u64,
    /// The number of the last layout computation begun; a measurer's answer
    /// that a leaf remembers is good only within the one that got it.
    layout_run: u64,
    next_id: u64,
    hovered: Option<NodeId>,
    focused: Option<NodeId>,
    captured: Option<NodeId>,
    /// The word or paragraph a double or triple press took in a selectable
    /// label, as a byte range of its text, while that press is held.
    taken_range: Option<(NodeId, core::ops::Range<usize>)>,
    keyboard_pressed: Option<(NodeId, Key)>,
    /// A node in a title bar that took focus from a press, which shows no
    /// focus ring: a desktop's title bar never does.
    quiet_focus: Option<NodeId>,
    /// Whether the last input that could move focus was the keyboard, which
    /// is what shows the focus ring on a widget that takes no typed text.
    /// True before any input, so a window that opens with something focused
    /// shows it.
    keyboard_modality: bool,
    pointer: Option<Point>,
    clicks: HashSet<NodeId>,
    /// The clicks a pointer made, as opposed to keys or assistive
    /// technology: a widget that hears pointer events gets no `Activate` for them.
    pointer_clicks: HashSet<NodeId>,
    pending: Dirty,
    scene: Scene,
    /// The number of the last scene assembly begun, which paint chunks
    /// remember. It starts at 1 and is raised before each assembly, so the
    /// first is 2 and none is 0, the number of a chunk not yet in a scene.
    assembly: u64,
    /// The chunks that nodes had when their paint was invalidated, by node,
    /// until they are painted again: the new chunk is written over the old one
    /// when nothing else holds it. A node is cached by its `paint_cache` alone;
    /// what is here is not, and is never looked at to tell.
    spare_chunks: crate::collections::FastMap<NodeId, Rc<PaintChunk>>,
    stats: FrameStats,
    geometry_dirty: bool,
    /// Whether the bounds walk is the plain one the tests compare it with.
    #[cfg(test)]
    reference_walk: bool,
    /// Whether every clip was cut from its parent's by the last
    /// `refresh_bounds` and no node has been given another parent since.
    clips_valid: bool,
    /// How many popovers the tree holds.
    popover_count: usize,
    /// How many popups the tree holds.
    popup_count: usize,
    /// How many text areas the tree holds.
    text_area_count: usize,
    /// How many rich editors the tree holds.
    rich_editor_count: usize,
    /// The nodes `prepare_controls` found last frame, kept for their room.
    preparing: Preparing,
    /// Tabs that became selected in this declaration; layout scrolls their
    /// strips to show them.
    reveal_tabs: Vec<NodeId>,
    pending_damage: Vec<Rect>,
    damage: Vec<Rect>,
    updates: HashMap<NodeId, ControlUpdate>,
    bindings: HashMap<String, Option<Shortcut>>,
    /// Event areas of the last declaration, and what has reached each of them
    /// since. Both are empty in a view that declares none, and the dispatch
    /// then costs a lookup.
    areas: HashMap<NodeId, crate::EventArea>,
    /// A cell because `Ui` holds the context by a shared reference.
    pending_area_events: core::cell::RefCell<HashMap<NodeId, Vec<crate::UiEvent>>>,
    /// Nodes that contain something with input waiting, so that a memoized
    /// container above it declares again. Rebuilt for each declaration.
    awaiting: HashSet<NodeId>,
    /// Subtrees the declaration just skipped. A cell because `Ui` only holds
    /// a shared reference to the context.
    skipped: core::cell::Cell<usize>,
    /// Elements the view built, and the bytes of key and text in them.
    declared: core::cell::Cell<usize>,
    declared_bytes: core::cell::Cell<usize>,
    /// Font families the declaration named, for the host to check.
    families: core::cell::RefCell<alloc::collections::BTreeSet<String>>,
    /// Element vectors from earlier declarations, waiting to be filled
    /// again. A declaration is thrown away every frame; the memory it used
    /// does not have to be.
    spare: core::cell::RefCell<Vec<Vec<Element>>>,
    /// Emptied lists of node ids, which placing the children of a node copies
    /// them into.
    id_spare: Vec<Vec<NodeId>>,
    submitted: HashSet<NodeId>,
    pub(crate) popup: Option<PopupState>,
    /// The menus the open popup is a submenu of, outermost first. `popup`
    /// is the innermost, which has the keyboard. Empty whenever `popup` is
    /// none.
    pub(crate) popup_parents: Vec<PopupState>,
    /// The row of an open menu the pointer is resting on, which opens a
    /// submenu or closes the open one when the wait is over.
    menu_dwell: Option<submenu_hover::MenuDwell>,
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
    #[cfg(feature = "std")]
    file_request: Option<crate::FileRequest>,
    #[cfg(feature = "std")]
    chosen_files: Option<Vec<std::path::PathBuf>>,
    #[cfg(feature = "std")]
    opened_files: Vec<std::path::PathBuf>,
    notifications: Vec<crate::Notification>,
    menu_out: Option<crate::Menu>,
    tray_out: Option<Option<crate::Tray>>,
    /// What the host has been given, so that declaring the same menu or
    /// tray again asks for nothing.
    menu_shown: Option<crate::Menu>,
    tray_shown: Option<crate::Tray>,
    /// What a title bar of the application's own asked the window to do,
    /// what the host says about the window, and the size it was last given.
    window_commands: Vec<crate::WindowCommand>,
    window_info: crate::WindowInfo,
    size: Size,
    /// When and where the title bar was last pressed, for telling a second
    /// press that means "maximize" from two separate ones.
    last_title_press: Option<(Duration, Point)>,
    /// Minimize, maximize and close of a title bar the view draws, for a
    /// host that has to tell the platform where they are. The nodes rather
    /// than their rectangles, because they are found when the tree is
    /// reconciled and only laid out after.
    caption_buttons: [Option<NodeId>; 3],
    /// What the overlays painted straight into the last scene. They are not
    /// nodes, so nothing invalidates them: the frame one appears, moves or
    /// goes has to name its rectangle itself.
    painted_overlays: Vec<Rect>,
    redraw_requested: bool,
    caret_movement: crate::CaretMovement,
    direction: crate::Direction,
    #[cfg(feature = "std")]
    watched: HashMap<std::path::PathBuf, Watched>,
    #[cfg(feature = "std")]
    watch_requests: Vec<std::path::PathBuf>,
    #[cfg(feature = "std")]
    images: HashMap<std::path::PathBuf, image::LoadedImage>,
    #[cfg(feature = "std")]
    image_requests: Vec<std::path::PathBuf>,
    /// Animated images being shown, by the node that shows them.
    image_plays: HashMap<NodeId, image::Play>,
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
    /// The last press of the pointer, for counting a double and a triple
    /// click.
    last_press: Option<press_count::Press>,
    /// Which press of a quick row the press being handled is (1, 2 or 3).
    /// Set only while `Context::event` hands a press on, and 1 at every other
    /// time, so that a press reaching `handle_event` another way (touch) is
    /// always a single one.
    press_count: u8,
    fling: Option<touch_input::Fling>,
    /// When a selection drag held outside an editor last scrolled it.
    edge_scroll_at: Option<Duration>,
    pub(crate) messages: Option<Box<dyn crate::Messages>>,
    pub(crate) format: Option<Box<dyn crate::LocaleFormat>>,
    pub(crate) missing_messages: core::cell::RefCell<Vec<String>>,
    theme: Theme,
    stylesheet: StyleSheet,
    style_warnings: Vec<String>,
    state_graph: Vec<crate::GraphNode>,
    noted_focus: Option<NodeId>,
    pending_restore: Option<NodeId>,
    now: Duration,
    hover_since: Option<(NodeId, Duration)>,
    /// Where the keyboard moved focus last, and when: the dwell of the tooltip it brings up.
    focus_since: Option<(NodeId, Duration)>,
    /// The tooltip on show, and what brought it.
    tooltip: Option<(NodeId, overlay_input::Dwell)>,
    /// When a tooltip last went because its widget was left.
    tooltip_left: Option<Duration>,
    new_popovers: Vec<NodeId>,
    reduced_motion: bool,
    resizing: bool,
    animating_nodes: Vec<NodeId>,
    ghosts: Vec<animation_state::Ghost>,
    progress_memo: Option<animation_state::ProgressMemo>,
    /// The handler slab of the `App` driving this context, typed by its
    /// message; `None` before an `App` has declared.
    handlers: Option<Box<dyn core::any::Any>>,
    /// What reconcile did to handlers, for the typed code to apply: the
    /// declared handler each node now has, and the slots of removed nodes.
    handler_moves: Vec<(NodeId, Option<u32>)>,
    freed_handlers: Vec<u32>,
    /// Pending filters each grid declared, read by the next batch.
    grid_filters: core::cell::RefCell<Vec<(NodeId, crate::collection::FilterNote)>>,
    /// The filter delays of grids, by the node of their rows.
    grid_timers: HashMap<NodeId, crate::collection::FilterTimer>,
    /// What grids declared with `data_grid` keep: cursor, sideways scroll
    /// and the cell being edited, by the node of their rows.
    grid_nodes: core::cell::RefCell<HashMap<NodeId, crate::collection::GridNode>>,
    /// Collection input a declaration queued for the next batch.
    grid_pending: core::cell::RefCell<Vec<(NodeId, crate::collection::CollectionInput)>>,
    /// Nodes whose text the user edited in the batch being resolved.
    text_typed: HashSet<NodeId>,
    /// The selection a message-form collection last scrolled into view.
    revealed: core::cell::RefCell<HashMap<NodeId, (Option<crate::ItemId>, u32)>>,
    /// The top row of each message-form list, kept in place across a reorder.
    list_anchors: core::cell::RefCell<HashMap<NodeId, crate::collection::ListAnchor>>,
    /// The selection each message-form collection last reported as lost, so
    /// an application that keeps it is told once, not every batch.
    lost_selections: core::cell::RefCell<HashMap<NodeId, crate::ItemId>>,
    /// Activations, edits and submissions in the order they arrived, beside
    /// the per-kind maps.
    queued: Vec<(NodeId, Queued)>,
    /// Elements whose hover or focus the view read before their node existed.
    observed_new: core::cell::RefCell<Vec<u32>>,
    /// Elements declared with a text source this frame, with the version of
    /// it they were declared at, in the order they were declared.
    bound_new: core::cell::RefCell<Vec<DeclaredBound>>,
    /// The nodes that follow a text source.
    bound_nodes: Vec<NodeId>,
    /// Something the view reads may have changed since it last declared.
    view_stale: bool,
    /// Input arrived that handlers answered in full. It declares the view
    /// through the messages they send, and only when the application does
    /// not say that the view reads nothing those change; see
    /// `App::view_state`.
    view_stale_handled: bool,
    /// The dock being declared or reconciled, whose panels are found by id
    /// wherever they sit. A cell because `Ui` only holds a shared reference
    /// to the context.
    dock: core::cell::Cell<Option<NodeId>>,
    /// The node of each panel's content, by dock and panel id. The content
    /// of a panel whose tab is not selected stays here, parked: its nodes
    /// have no parent and the tree does not reach them.
    panels: HashMap<NodeId, HashMap<String, NodeId>>,
    /// Where the cursor of each calendar is, which the view does not hold.
    calendar_views: HashMap<NodeId, crate::calendar::View>,
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
            children_by_key: Default::default(),
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
            observed: core::cell::Cell::new(false),
            observing: core::cell::Cell::new(false),
            bound: None,
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
            nodes: crate::collections::FastMap::from_iter([(root, node)]),
            layout,
            root,
            frame: 0,
            layout_run: 0,
            next_id: 2,
            hovered: None,
            focused: None,
            captured: None,
            taken_range: None,
            keyboard_pressed: None,
            quiet_focus: None,
            keyboard_modality: true,
            pointer: None,
            clicks: HashSet::new(),
            pointer_clicks: HashSet::new(),
            pending: Dirty::all(),
            scene: Scene::default(),
            assembly: 1,
            spare_chunks: Default::default(),
            stats: FrameStats::default(),
            geometry_dirty: false,
            #[cfg(test)]
            reference_walk: false,
            clips_valid: false,
            popover_count: 0,
            popup_count: 0,
            text_area_count: 0,
            rich_editor_count: 0,
            preparing: Preparing::default(),
            reveal_tabs: Vec::new(),
            pending_damage: Vec::new(),
            damage: Vec::new(),
            updates: HashMap::new(),
            bindings: HashMap::new(),
            areas: HashMap::new(),
            pending_area_events: core::cell::RefCell::new(HashMap::new()),
            awaiting: HashSet::new(),
            skipped: core::cell::Cell::new(0),
            declared: core::cell::Cell::new(0),
            declared_bytes: core::cell::Cell::new(0),
            families: core::cell::RefCell::new(alloc::collections::BTreeSet::new()),
            spare: core::cell::RefCell::new(Vec::new()),
            id_spare: Vec::new(),
            submitted: HashSet::new(),
            popup: None,
            popup_parents: Vec::new(),
            menu_dwell: None,
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
            #[cfg(feature = "std")]
            file_request: None,
            #[cfg(feature = "std")]
            chosen_files: None,
            #[cfg(feature = "std")]
            opened_files: Vec::new(),
            notifications: Vec::new(),
            menu_out: None,
            tray_out: None,
            menu_shown: None,
            tray_shown: None,
            window_commands: Vec::new(),
            window_info: crate::WindowInfo::default(),
            size: Size::default(),
            last_title_press: None,
            caption_buttons: [None; 3],
            painted_overlays: Vec::new(),
            redraw_requested: false,
            caret_movement: crate::CaretMovement::Visual,
            direction: crate::Direction::Ltr,
            #[cfg(feature = "std")]
            watched: HashMap::new(),
            #[cfg(feature = "std")]
            images: HashMap::new(),
            #[cfg(feature = "std")]
            image_requests: Vec::new(),
            image_plays: HashMap::new(),
            #[cfg(feature = "std")]
            watch_requests: Vec::new(),
            hot_save: false,
            settings: (0, 0, 0),
            hot_state: None,
            diagnostic: diagnostics::DiagnosticState::new(),
            toast: toasts::ToastState::default(),
            touches: HashMap::new(),
            last_tap: None,
            last_press: None,
            press_count: 1,
            fling: None,
            edge_scroll_at: None,
            messages: None,
            format: None,
            missing_messages: core::cell::RefCell::new(Vec::new()),
            theme,
            stylesheet: StyleSheet::default(),
            style_warnings: Vec::new(),
            state_graph: Vec::new(),
            noted_focus: None,
            pending_restore: None,
            now: Duration::ZERO,
            hover_since: None,
            focus_since: None,
            tooltip: None,
            tooltip_left: None,
            new_popovers: Vec::new(),
            reduced_motion: false,
            resizing: false,
            animating_nodes: Vec::new(),
            ghosts: Vec::new(),
            progress_memo: None,
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
            observed_new: core::cell::RefCell::new(Vec::new()),
            bound_new: core::cell::RefCell::new(Vec::new()),
            bound_nodes: Vec::new(),
            view_stale: true,
            view_stale_handled: false,
            dock: core::cell::Cell::new(None),
            panels: HashMap::new(),
            calendar_views: HashMap::new(),
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
    #[cfg(feature = "std")]
    pub fn watched_paths(&self) -> &[std::path::PathBuf] {
        &self.watch_requests
    }

    /// Whether the host has delivered this file (or an error for it) yet.
    #[cfg(feature = "std")]
    pub fn watched_known(&self, path: &std::path::Path) -> bool {
        self.watched.contains_key(path)
    }

    /// Delivers a file's content or a read error. An error keeps the last good
    /// content. Returns whether a declaration should follow.
    #[cfg(feature = "std")]
    pub fn set_watched_file(
        &mut self,
        path: &std::path::Path,
        content: core::result::Result<Vec<u8>, String>,
    ) -> bool {
        let old = self.watched.get(path);
        let changed = match content {
            Ok(bytes) => {
                if old.is_some_and(|old| *old.bytes == *bytes && old.error.is_none()) {
                    return false;
                }
                let text = core::str::from_utf8(&bytes).ok().map(Arc::from);
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
    #[cfg(feature = "std")]
    pub fn watch_error(&self, path: &std::path::Path) -> Option<&str> {
        self.watched.get(path)?.error.as_deref()
    }

    #[cfg(feature = "std")]
    pub(crate) fn watched_bytes(&self, path: &std::path::Path) -> Option<Arc<[u8]>> {
        let watched = self.watched.get(path)?;
        // A file that never loaded has no content to show.
        (watched.error.is_none() || !watched.bytes.is_empty()).then(|| watched.bytes.clone())
    }

    #[cfg(feature = "std")]
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
            node_bytes: self.nodes.len() * core::mem::size_of::<Node>(),
            scene_primitives: self.scene.primitives.len(),
            scene_bytes: self.scene.primitives.len() * core::mem::size_of::<Primitive>(),
            ..MemoryReport::default()
        };
        for node in self.nodes.values() {
            report.text_bytes += node.text.len()
                + match &node.key {
                    crate::ui::WidgetKey::Named(key) | crate::ui::WidgetKey::Panel(key) => {
                        key.len()
                    }
                    crate::ui::WidgetKey::Site(..) => 0,
                };
            if let Some(chunk) = &node.paint_cache {
                report.cached_nodes += 1;
                report.cached_primitives += chunk.count();
            }
        }
        #[cfg(feature = "std")]
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
        core::mem::take(&mut self.paste_requested)
    }

    /// What the host read from the clipboard.
    pub fn set_clip(&mut self, clip: crate::Clip) {
        self.clip_in = Some(clip);
        self.redraw_requested = true;
    }

    /// What the application asked the window to do: move it, resize it, minimize,
    /// maximize or close it. Hosts take these after a declaration.
    pub fn take_window_commands(&mut self) -> Vec<crate::WindowCommand> {
        core::mem::take(&mut self.window_commands)
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

    /// Whether a node is a title bar or inside one.
    fn in_title_bar(&self, id: NodeId) -> bool {
        let mut at = Some(id);
        while let Some(id) = at {
            let Some(node) = self.nodes.get(&id) else {
                return false;
            };
            if node.control == Control::TitleBar {
                return true;
            }
            at = node.parent;
        }
        false
    }

    /// Where the maximize button of a title bar the view draws itself sits,
    /// in logical coordinates, for a host that tells the platform so that it
    /// can offer its own window layouts over it.
    pub fn maximize_button(&self) -> Option<Rect> {
        self.caption_button(1)
    }

    /// Where its minimize button sits, as `maximize_button`.
    pub fn minimize_button(&self) -> Option<Rect> {
        self.caption_button(0)
    }

    /// Where its close button sits, as `maximize_button`.
    pub fn close_button(&self) -> Option<Rect> {
        self.caption_button(2)
    }

    /// Where a caption button was last laid out.
    fn caption_button(&self, index: usize) -> Option<Rect> {
        self.nodes
            .get(&self.caption_buttons[index]?)
            .map(|node| node.bounds)
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
    #[cfg(feature = "std")]
    pub fn take_file_request(&mut self) -> Option<crate::FileRequest> {
        self.file_request.take()
    }

    /// What the dialog answered; an empty list means it was cancelled.
    #[cfg(feature = "std")]
    pub fn set_chosen_files(&mut self, paths: Vec<std::path::PathBuf>) {
        self.chosen_files = Some(paths);
        self.redraw_requested = true;
    }

    /// Files the operating system asked the application to open. Hosts set
    /// this before the first declaration and whenever more arrive.
    #[cfg(feature = "std")]
    pub fn set_opened_files(&mut self, paths: Vec<std::path::PathBuf>) {
        self.opened_files = paths;
        self.redraw_requested = true;
    }

    #[cfg(feature = "std")]
    pub(crate) fn opened_files(&self) -> &[std::path::PathBuf] {
        &self.opened_files
    }

    /// Notifications a view asked for, in the order it asked.
    pub fn take_notifications(&mut self) -> Vec<crate::Notification> {
        core::mem::take(&mut self.notifications)
    }

    /// The native menu a view last declared, taken once so that a host
    /// rebuilds it only when it changes.
    pub fn take_menu(&mut self) -> Option<crate::Menu> {
        self.menu_out.take()
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
    /// renderers use to limit what they redraw. Changes that overlap or nearly
    /// touch share a rectangle, and there are never more than eight.
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

    /// Keeps an emptied list for the next copy. The caps are well above a
    /// real tree's depth and breadth, so a one-off container with a great
    /// many children does not leave its room behind.
    fn give_ids(&mut self, mut ids: Vec<NodeId>) {
        const KEEP: usize = 256;
        const LARGEST: usize = 4096;
        if ids.capacity() > 0 && ids.capacity() <= LARGEST && self.id_spare.len() < KEEP {
            ids.clear();
            self.id_spare.push(ids);
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

    /// The element just declared follows `source`, which was at `version`
    /// when its text was read.
    pub(crate) fn bind_element(&self, seq: u32, source: Rc<dyn crate::TextSource>, version: u64) {
        self.bound_new.borrow_mut().push((seq, source, version));
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
        let id = match key {
            // A panel's content is wherever the dock last put it.
            WidgetKey::Panel(panel) => *self.panels.get(&self.dock.get()?)?.get(panel)?,
            _ => *self.nodes.get(&parent?)?.children_by_key.get(key)?,
        };
        (self.nodes[&id].kind == kind).then_some(id)
    }

    /// Declares the panels of `dock` to be the ones `child` finds, until
    /// [`Context::leave_dock`] is given what this returns.
    pub(crate) fn enter_dock(&self, dock: Option<NodeId>) -> Option<NodeId> {
        self.dock.replace(dock)
    }

    pub(crate) fn leave_dock(&self, outer: Option<NodeId>) {
        self.dock.set(outer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use glassmere_scene::{Affine, FontFamily, GradientStop, Image, Path, Stroke, TextSpan};

    fn rect(x: f32) -> Rect {
        Rect {
            x,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        }
    }

    /// A colour that tells the primitive at `x` from others.
    fn color(x: f32) -> Color {
        Color(x as u8, 2, 3, 255)
    }

    fn quad(x: f32) -> Primitive {
        Primitive::Quad {
            rect: rect(x),
            color: color(x),
            radius: 0.5,
        }
    }

    fn text(x: f32, text: &str) -> Primitive {
        Primitive::Text {
            rect: rect(x),
            text: text.into(),
            font_size: 12.0 + x,
            color: color(x),
        }
    }

    fn styled(x: f32, text: &str) -> Primitive {
        Primitive::StyledText {
            rect: rect(x),
            text: text.into(),
            style: TextStyle {
                font_size: 10.0 + x,
                family: FontFamily::Named(format!("family {x}")),
                weight: 700,
                ..TextStyle::default()
            },
            color: color(x),
        }
    }

    fn chunk(
        before: Vec<Primitive>,
        children: Vec<Rc<PaintChunk>>,
        after: Vec<Primitive>,
    ) -> Rc<PaintChunk> {
        let split = before.len();
        let mut primitives = before;
        primitives.extend(after);
        Rc::new(PaintChunk::new(primitives, split, children))
    }

    /// The scene `chunks` flatten to.
    fn flattened(chunks: &[Rc<PaintChunk>]) -> Vec<Primitive> {
        let mut flat = Vec::new();
        for chunk in chunks {
            chunk.append_to(&mut flat);
        }
        flat
    }

    /// Puts `chunks` one after another over `buffer` in assembly `assembly`
    /// and cuts the buffer where they end, as the scene is assembled. Returns
    /// where that is.
    fn assemble(buffer: &mut Vec<Primitive>, chunks: &[Rc<PaintChunk>], assembly: u64) -> usize {
        let mut at = 0;
        for chunk in chunks {
            chunk.place(buffer, &mut at, assembly);
        }
        buffer.truncate(at);
        at
    }

    /// One primitive of every kind, told apart from those of another call by
    /// `x`.
    fn every_kind(x: f32) -> Vec<Primitive> {
        let mut path = Path::new();
        path.move_to(Point { x, y: 1.0 })
            .line_to(Point { x: 3.0, y: 4.0 })
            .close();
        vec![
            quad(x),
            text(x, "text"),
            styled(x, "styled"),
            Primitive::RichText {
                rect: rect(x),
                spans: vec![TextSpan::new("one"), TextSpan::new("two").weight(700)],
                style: TextStyle::default(),
                color: Color(1, 1, 1, 255),
            },
            Primitive::Image {
                rect: rect(x),
                image: Image::from_rgba8(1, 1, vec![9, 8, 7, 255]).unwrap(),
                source: rect(0.0),
                tint: Some(Color(1, 2, 3, 4)),
                radius: 1.0,
            },
            Primitive::PushClip(rect(x)),
            Primitive::PopClip,
            Primitive::PushLayer {
                bounds: rect(x),
                opacity: 0.5,
                transform: Affine::translate(x, 2.0),
            },
            Primitive::Gradient {
                rect: rect(x),
                radius: 2.0,
                from: Point { x: 0.0, y: 0.0 },
                to: Point { x: 1.0, y: 1.0 },
                stops: vec![
                    GradientStop {
                        at: 0.0,
                        color: Color(0, 0, 0, 255),
                    },
                    GradientStop {
                        at: 1.0,
                        color: Color(255, 255, 255, 255),
                    },
                ],
                radial: false,
            },
            Primitive::Shadow {
                rect: rect(x),
                radius: 1.0,
                color: Color(0, 0, 0, 128),
                blur: 4.0,
                spread: 1.0,
                offset: Point { x: 1.0, y: 2.0 },
                inset: false,
            },
            Primitive::Custom {
                rect: rect(x),
                id: 7,
            },
            Primitive::PopLayer,
            Primitive::Path {
                path,
                fill: Some(Color(1, 2, 3, 255)),
                stroke: Some(Stroke {
                    width: 1.0,
                    color: Color(4, 5, 6, 255),
                }),
            },
        ]
    }

    /// Where a kind of primitive is in `every_kind`. A kind added to the
    /// scene fails to compile here until the list has one of it.
    fn kind(primitive: &Primitive) -> usize {
        match primitive {
            Primitive::Quad { .. } => 0,
            Primitive::Text { .. } => 1,
            Primitive::StyledText { .. } => 2,
            Primitive::RichText { .. } => 3,
            Primitive::Image { .. } => 4,
            Primitive::PushClip(_) => 5,
            Primitive::PopClip => 6,
            Primitive::PushLayer { .. } => 7,
            Primitive::Gradient { .. } => 8,
            Primitive::Shadow { .. } => 9,
            Primitive::Custom { .. } => 10,
            Primitive::PopLayer => 11,
            Primitive::Path { .. } => 12,
        }
    }

    #[test]
    fn a_slot_of_any_kind_takes_a_primitive_of_any_kind() {
        assert!(every_kind(0.0).iter().map(kind).eq(0..13));
        for slot in every_kind(1.0) {
            for source in every_kind(2.0) {
                let mut written = slot.clone();
                overwrite(&mut written, &source);
                assert_eq!(written, source, "{slot:?} written over by {source:?}");
                assert_eq!(format!("{written:?}"), format!("{source:?}"));
            }
        }
    }

    /// The text of a primitive that has some.
    fn held(primitive: &Primitive) -> &String {
        match primitive {
            Primitive::Text { text, .. } | Primitive::StyledText { text, .. } => text,
            other => panic!("{other:?} has no text"),
        }
    }

    #[test]
    fn a_text_slot_takes_text_of_any_length_whatever_room_it_has() {
        let texts = ["", "a", "a longer text than most of the others", "ünï ✓"];
        for old in texts {
            for room in [0, 8, 4096] {
                for new in texts {
                    let mut plain = text(1.0, "");
                    let mut styled_slot = styled(1.0, "");
                    for slot in [&mut plain, &mut styled_slot] {
                        let (Primitive::Text { text, .. } | Primitive::StyledText { text, .. }) =
                            slot
                        else {
                            unreachable!()
                        };
                        *text = String::with_capacity(room);
                        text.push_str(old);
                    }
                    for (mut slot, source) in
                        [(plain, text(2.0, new)), (styled_slot, styled(2.0, new))]
                    {
                        let (capacity, allocation) = (held(&slot).capacity(), held(&slot).as_ptr());
                        overwrite(&mut slot, &source);
                        assert_eq!(slot, source, "{old:?} over {new:?} with room {room}");
                        // Room that fits is used, not given back for a clone.
                        if capacity >= new.len() && capacity <= new.len() * 2 + 32 {
                            assert_eq!(held(&slot).as_ptr(), allocation, "room {room} for {new:?}");
                        }
                    }
                }
            }
        }
    }

    /// A text, a styled text and a quad with `x` for every float they have.
    fn floats(x: f32) -> [Primitive; 3] {
        let (rect, color) = (Rect { x, ..rect(0.0) }, Color(1, 1, 1, 1));
        [
            Primitive::Text {
                rect,
                text: "text".into(),
                font_size: x,
                color,
            },
            Primitive::StyledText {
                rect,
                text: "text".into(),
                style: TextStyle {
                    font_size: x,
                    ..TextStyle::default()
                },
                color,
            },
            Primitive::Quad {
                rect,
                color,
                radius: x,
            },
        ]
    }

    #[test]
    fn a_slot_takes_the_bits_of_its_source_and_not_only_a_value_equal_to_them() {
        // 0.0 and -0.0 are equal and print differently, and NaN is not equal
        // to itself. The primitives are compared as the text they print.
        for (old, new) in [(0.0, -0.0), (-0.0, 0.0), (f32::NAN, 1.0), (1.0, f32::NAN)] {
            let sources = floats(new);
            for (mut slot, source) in floats(old).into_iter().zip(&sources) {
                overwrite(&mut slot, source);
                assert_eq!(
                    format!("{slot:?}"),
                    format!("{source:?}"),
                    "{old} then {new}"
                );
            }
        }
    }

    /// The kinds of primitive that own nothing, with a value of their own in
    /// every field: `seed` and what is added to it for each field, through
    /// `float`. Two seeds a few apart give primitives that differ in every
    /// field, and no two fields of one primitive hold the same.
    fn plain_with(seed: f32, float: fn(f32) -> f32) -> [Primitive; 6] {
        let rect = |at: f32| Rect {
            x: float(at),
            y: float(at + 1.0),
            width: float(at + 2.0),
            height: float(at + 3.0),
        };
        let byte = |at: f32| (at as u32 % 250) as u8;
        let color = |at: f32| Color(byte(at), byte(at + 1.0), byte(at + 2.0), byte(at + 3.0));
        [
            Primitive::Quad {
                rect: rect(seed),
                color: color(seed + 4.0),
                radius: float(seed + 8.0),
            },
            Primitive::PushClip(rect(seed + 9.0)),
            Primitive::PopClip,
            Primitive::PushLayer {
                bounds: rect(seed + 13.0),
                opacity: float(seed + 17.0),
                transform: Affine {
                    a: float(seed + 18.0),
                    b: float(seed + 19.0),
                    c: float(seed + 20.0),
                    d: float(seed + 21.0),
                    e: float(seed + 22.0),
                    f: float(seed + 23.0),
                },
            },
            Primitive::PopLayer,
            Primitive::Shadow {
                rect: rect(seed + 24.0),
                radius: float(seed + 28.0),
                color: color(seed + 29.0),
                blur: float(seed + 33.0),
                spread: float(seed + 34.0),
                offset: Point {
                    x: float(seed + 35.0),
                    y: float(seed + 36.0),
                },
                inset: (seed as u32 & 1) == 0,
            },
        ]
    }

    fn plain(seed: f32) -> [Primitive; 6] {
        plain_with(seed, |x| x)
    }

    /// One primitive of every kind, those that own nothing with a value of
    /// their own in every field.
    fn distinct(seed: f32, float: fn(f32) -> f32) -> Vec<Primitive> {
        let mut kinds = every_kind(seed);
        for primitive in plain_with(seed, float) {
            let at = kind(&primitive);
            kinds[at] = primitive;
        }
        kinds
    }

    /// The fields of a primitive that owns nothing, as bits: `==` and `Debug`
    /// do not tell 0.0 from -0.0 or one NaN from another. A primitive that
    /// owns something has none.
    fn words(primitive: &Primitive) -> Vec<u32> {
        let rect = |rect: &Rect| [rect.x, rect.y, rect.width, rect.height].map(f32::to_bits);
        let color = |color: &Color| [color.0, color.1, color.2, color.3].map(u32::from);
        match primitive {
            Primitive::Quad {
                rect: at,
                color: paint,
                radius,
            } => [&rect(at)[..], &color(paint), &[radius.to_bits()]].concat(),
            Primitive::PushClip(at) => rect(at).to_vec(),
            Primitive::PushLayer {
                bounds,
                opacity,
                transform,
            } => {
                let Affine { a, b, c, d, e, f } = *transform;
                let transform = [a, b, c, d, e, f].map(f32::to_bits);
                [&rect(bounds)[..], &[opacity.to_bits()], &transform].concat()
            }
            Primitive::Shadow {
                rect: at,
                radius,
                color: paint,
                blur,
                spread,
                offset,
                inset,
            } => {
                let floats = [radius, blur, spread, &offset.x, &offset.y].map(|x| x.to_bits());
                [&rect(at)[..], &color(paint), &floats, &[u32::from(*inset)]].concat()
            }
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_slot_takes_every_field_of_its_source_whatever_kind_each_is_and_whatever_the_floats() {
        fn same(x: f32) -> f32 {
            x
        }
        fn zero(_: f32) -> f32 {
            0.0
        }
        fn negative(_: f32) -> f32 {
            -0.0
        }
        fn nan(_: f32) -> f32 {
            f32::from_bits(0x7fc0_1234)
        }
        fn other_nan(_: f32) -> f32 {
            f32::from_bits(0x7fc0_5678)
        }
        // How the floats of the slot and of its source are made.
        type Floats = fn(f32) -> f32;
        let cases: [(Floats, Floats); 7] = [
            (same, same),
            (zero, negative),
            (negative, zero),
            (zero, nan),
            (nan, zero),
            (other_nan, nan),
            (nan, other_nan),
        ];
        for (slot_float, source_float) in cases {
            let (slots, sources) = (distinct(100.0, slot_float), distinct(201.0, source_float));
            assert!(slots.iter().map(kind).eq(0..13));
            for slot in &slots {
                for source in &sources {
                    let mut written = slot.clone();
                    overwrite(&mut written, source);
                    let note = format!("{slot:?} written over by {source:?}");
                    assert_eq!(format!("{written:?}"), format!("{source:?}"), "{note}");
                    assert_eq!(words(&written), words(source), "{note}");
                }
            }
        }
    }

    #[test]
    fn a_slice_that_fits_the_output_exactly_or_is_empty_leaves_its_length_and_room_alone() {
        let source = [quad(1.0), text(2.0, "two"), styled(3.0, "three")];
        for (length, at, source) in [
            (4, 1, &source[..]),
            (3, 0, &source[..]),
            (4, 3, &source[..0]),
        ] {
            let mut output: Vec<Primitive> = (0..length).map(|x| quad(10.0 + x as f32)).collect();
            output.reserve_exact(12);
            let room = output.capacity();
            let before = output.clone();
            let mut next = at;
            place_slice(source, &mut output, &mut next);
            assert_eq!(next, at + source.len());
            assert_eq!((output.len(), output.capacity()), (length, room));
            assert_eq!(output[..at], before[..at]);
            assert_eq!(output[at..next], *source);
            assert_eq!(output[next..], before[next..]);
        }
    }

    #[test]
    fn a_slice_longer_than_the_output_has_left_is_appended_where_it_runs_out() {
        let source = [quad(1.0), text(2.0, "two"), styled(3.0, "three"), quad(4.0)];
        // Nothing to write over, one primitive, and all but the last two.
        for (length, at) in [(2, 2), (2, 1), (3, 1), (0, 0)] {
            let mut output: Vec<Primitive> = (0..length).map(|x| quad(10.0 + x as f32)).collect();
            let before = output.clone();
            let mut next = at;
            place_slice(&source, &mut output, &mut next);
            assert_eq!(next, at + source.len());
            assert_eq!(output.len(), at + source.len());
            assert_eq!(output[..at], before[..at]);
            assert_eq!(output[at..], source);
        }
    }

    #[test]
    fn room_is_kept_when_it_fits_and_given_up_when_it_is_far_too_much_or_too_little() {
        // The text needs 21 bytes and the room is 64: the allocation stays.
        let mut into = String::with_capacity(64);
        into.push_str("old text");
        let (room, allocation) = (into.capacity(), into.as_ptr());
        refill(&mut into, &"a text of some length".to_owned());
        assert_eq!(into, "a text of some length");
        assert_eq!((into.capacity(), into.as_ptr()), (room, allocation));

        // A long text left room for 4096 bytes and the next one needs 5.
        let mut into = String::with_capacity(4096);
        into.push_str(&"long ".repeat(500));
        refill(&mut into, &"short".to_owned());
        assert_eq!(into, "short");
        assert!(
            into.capacity() <= 2 * 5 + 32,
            "kept room for {}",
            into.capacity()
        );

        // Too little room for the text that comes.
        let mut into = String::from("ab");
        refill(&mut into, &"a text that does not fit".to_owned());
        assert_eq!(into, "a text that does not fit");

        // Nothing to put, and nothing to put it in.
        let mut into = String::new();
        refill(&mut into, &String::new());
        assert_eq!(into, "");
    }

    /// A small generator, so that a failure comes again.
    struct Dice(u64);

    impl Dice {
        fn below(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % bound as u64) as usize
        }
    }

    /// Chunks that change from one assembly to the next the way a tree's do,
    /// each primitive told apart from every other by its position.
    struct Forest {
        dice: Dice,
        next: f32,
        /// Chunks that left the tree and may come back to it.
        gone: Vec<Rc<PaintChunk>>,
    }

    impl Forest {
        fn primitives(&mut self) -> Vec<Primitive> {
            (0..self.dice.below(4))
                .map(|_| {
                    self.next += 1.0;
                    let length = self.dice.below(40);
                    match self.dice.below(8) {
                        0 => text(self.next, &"t".repeat(length)),
                        1 => styled(self.next, &"s".repeat(length)),
                        which => plain(self.next)[which - 2].clone(),
                    }
                })
                .collect()
        }

        fn grow(&mut self, depth: usize) -> Rc<PaintChunk> {
            if self.dice.below(4) == 0 && !self.gone.is_empty() {
                let back = self.dice.below(self.gone.len());
                return self.gone.swap_remove(back);
            }
            let children = if depth == 0 { 0 } else { self.dice.below(4) };
            let children = (0..children).map(|_| self.grow(depth - 1)).collect();
            let (before, after) = (self.primitives(), self.primitives());
            chunk(before, children, after)
        }

        /// What follows `old`: itself, or a new chunk that keeps, changes,
        /// loses and gains children.
        fn follow(&mut self, old: &Rc<PaintChunk>) -> Rc<PaintChunk> {
            if self.dice.below(2) == 0 {
                return old.clone();
            }
            let mut children: Vec<_> = old
                .children
                .iter()
                .map(|child| self.follow(child))
                .collect();
            if !children.is_empty() && self.dice.below(3) == 0 {
                let lost = children.remove(self.dice.below(children.len()));
                self.gone.push(lost);
            }
            if self.dice.below(3) == 0 {
                let at = self.dice.below(children.len() + 1);
                let gained = self.grow(2);
                children.insert(at, gained);
            }
            let (before, after) = if self.dice.below(2) == 0 {
                let (before, after) = old.primitives.split_at(old.split);
                (before.to_vec(), after.to_vec())
            } else {
                (self.primitives(), self.primitives())
            };
            chunk(before, children, after)
        }
    }

    /// Every chunk counts the primitives that flattening it gives.
    fn counted(chunk: &PaintChunk) {
        let mut flat = Vec::new();
        chunk.append_to(&mut flat);
        assert_eq!(chunk.count(), flat.len());
        for child in &chunk.children {
            counted(child);
        }
    }

    #[test]
    fn placed_chunks_are_the_flattened_ones_whatever_the_buffer_and_the_history_were() {
        for seed in 1..=30u64 {
            let mut forest = Forest {
                dice: Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
                next: 0.0,
                gone: Vec::new(),
            };
            let count = forest.dice.below(3) + 1;
            let mut roots: Vec<_> = (0..count).map(|_| forest.grow(3)).collect();
            // Something in the buffer before the first assembly.
            let mut buffer = forest.primitives();
            let mut assembly = 1;
            for step in 0..100 {
                roots = roots.iter().map(|root| forest.follow(root)).collect();
                match forest.dice.below(10) {
                    // The same chunk twice in one scene.
                    0 => roots.push(roots[0].clone()),
                    1 if roots.len() > 1 => {
                        roots.pop();
                    }
                    _ => {}
                }
                match forest.dice.below(10) {
                    // A frame that does not paint.
                    0 => continue,
                    // A pass that stopped part of the way and left no scene.
                    1 => {
                        assembly += 1;
                        let mut at = 0;
                        let mut lost = core::mem::take(&mut buffer);
                        let reached = forest.dice.below(roots.len() + 1);
                        for root in roots.iter().take(reached) {
                            root.place(&mut lost, &mut at, assembly);
                        }
                        continue;
                    }
                    _ => {}
                }
                assembly += 1;
                let placed = assemble(&mut buffer, &roots, assembly);
                let flat = flattened(&roots);
                let at = format!("seed {seed}, step {step}");
                assert_eq!(buffer, flat, "{at}");
                let bits = |scene: &[Primitive]| scene.iter().map(words).collect::<Vec<_>>();
                assert_eq!(bits(&buffer), bits(&flat), "{at}");
                assert_eq!(placed, flat.len(), "{at}");
                if step % 10 == 0 {
                    assert_eq!(format!("{buffer:?}"), format!("{flat:?}"), "{at}");
                    roots.iter().for_each(|root| counted(root));
                }
                // What the overlays draw after the tree.
                for tail in 0..forest.dice.below(5) {
                    buffer.push(quad(-1.0 - tail as f32));
                }
            }
        }
    }

    #[test]
    fn the_chunks_of_a_painted_tree_count_what_they_flatten_to() {
        let mut context = Context::new();
        let view = Style::default()
            .width(Length::Px(100.0))
            .height(Length::Px(30.0));
        context.run(
            Size::new(300.0, 200.0),
            &mut crate::MonospaceMeasurer,
            |ui| {
                ui.keyed("view", |ui| {
                    ui.scroll(view, |ui| {
                        for row in 0..3 {
                            ui.keyed(format!("row{row}"), |ui| ui.label("row"));
                        }
                    })
                });
            },
        );
        let root = context.nodes[&context.root].paint_cache.clone().unwrap();
        // Nothing is drawn over the tree, so the scene is the root's chunk.
        assert_eq!(root.count(), context.scene.primitives.len());
        counted(&root);

        // A row draws a clip, its text and the pop, which is 3. The view draws
        // its clip, the rows, the thumb of its scroll bar and the pop: 12. The
        // root has a clip and a pop around the view: 14.
        let report = context.memory();
        assert_eq!((report.nodes, report.cached_nodes), (5, 5));
        assert_eq!(report.cached_primitives, 14 + 12 + 3 * 3);
        let mut flattened = 0;
        for node in context.nodes.values() {
            let mut flat = Vec::new();
            node.paint_cache.as_ref().unwrap().append_to(&mut flat);
            flattened += flat.len();
        }
        assert_eq!(report.cached_primitives, flattened);
    }

    /// A leaf of the application's own that draws `count` quads. The clip
    /// around them and its pop make its chunk `count + 2` primitives.
    fn quads(count: usize) -> CustomWidget {
        CustomWidget {
            size: Size::new(40.0, 20.0),
            primitives: (0..count).map(|at| quad(at as f32)).collect(),
            ..CustomWidget::default()
        }
    }

    /// The chunk that the node at `path` was painted into.
    fn painted(context: &Context, path: &[&str]) -> Rc<PaintChunk> {
        let id = context.find(path).unwrap();
        context.nodes[&id].paint_cache.clone().unwrap()
    }

    #[test]
    fn a_node_that_draws_eight_primitives_or_fewer_holds_them_in_a_vector_of_eight() {
        let mut context = Context::new();
        context.run(
            Size::new(300.0, 200.0),
            &mut crate::MonospaceMeasurer,
            |ui| {
                ui.keyed("button", |ui| ui.button("Button"));
                for count in [0, 2, 4, 5, 6] {
                    ui.keyed(format!("quads{count}"), |ui| {
                        ui.custom(Style::default(), quads(count), ())
                    });
                }
                // A layer and its pop are two primitives more: four quads draw
                // eight.
                ui.keyed("layered", |ui| {
                    ui.custom(Style::default().opacity(0.5), quads(4), ())
                });
            },
        );
        for (path, drawn) in [
            ("button", 4),
            ("quads0", 2),
            ("quads2", 4),
            ("quads4", 6),
            ("quads5", 7),
            ("quads6", 8),
            ("layered", 8),
        ] {
            let chunk = painted(&context, &[path]);
            assert_eq!(chunk.primitives.len(), drawn, "{path}");
            // Room for eight whatever the node draws: the vector did not start
            // with less, grow to what it holds or shrink to it afterwards.
            assert_eq!(chunk.primitives.capacity(), 8, "{path}");
        }
    }

    #[test]
    fn a_widget_that_draws_many_primitives_has_room_for_its_pops_and_not_for_as_many_again() {
        let mut context = Context::new();
        context.run(
            Size::new(300.0, 200.0),
            &mut crate::MonospaceMeasurer,
            |ui| {
                ui.keyed("plain", |ui| ui.custom(Style::default(), quads(100), ()));
                ui.keyed("layered", |ui| {
                    ui.custom(Style::default().opacity(0.5), quads(100), ())
                });
            },
        );
        for (path, drawn) in [("plain", 102), ("layered", 104)] {
            let chunk = painted(&context, &[path]);
            assert_eq!(chunk.primitives.len(), drawn, "{path}");
            // Extending a vector makes it just big enough for what is added,
            // and a pop pushed after that would move all of it to one twice as
            // big.
            let room = chunk.primitives.capacity();
            assert!(
                room < drawn + drawn / 2,
                "{path} draws {drawn} primitives and has room for {room}"
            );
        }
    }

    /// A chunk of two primitives inside another, which assembly 2 put at the
    /// index 1 of a scene of four. Those two are overwritten with marks, so
    /// that a later assembly shows by what it leaves of them whether it wrote
    /// them again.
    fn marked_scene() -> (Rc<PaintChunk>, Rc<PaintChunk>, Vec<Primitive>) {
        let inner = chunk(vec![quad(1.0)], vec![], vec![quad(2.0)]);
        let outer = around(vec![quad(0.0)], &inner);
        let mut scene = Vec::new();
        assemble(&mut scene, core::slice::from_ref(&outer), 2);
        assert_eq!(scene.len(), 4);
        scene[1] = quad(99.0);
        scene[2] = quad(99.0);
        (outer, inner, scene)
    }

    fn around(before: Vec<Primitive>, inner: &Rc<PaintChunk>) -> Rc<PaintChunk> {
        chunk(before, vec![inner.clone()], vec![quad(3.0)])
    }

    #[test]
    fn a_chunk_is_left_alone_only_where_the_assembly_before_put_it() {
        let marks = [quad(99.0), quad(99.0)];

        // The next assembly puts it at the same index again.
        let (_, inner, mut scene) = marked_scene();
        assemble(&mut scene, &[around(vec![quad(0.0)], &inner)], 3);
        assert_eq!(scene[1..3], marks, "written again");
        assert_eq!((scene[0].clone(), scene[3].clone()), (quad(0.0), quad(3.0)));

        // So does a chunk that is put again whole.
        let (outer, _, mut scene) = marked_scene();
        scene[0] = quad(98.0);
        assemble(&mut scene, &[outer], 3);
        assert_eq!(scene[..3], [quad(98.0), quad(99.0), quad(99.0)]);

        // And the one after, for as long as it stays where it is.
        let (outer, _, mut scene) = marked_scene();
        scene[0] = quad(98.0);
        for assembly in 3..6 {
            assemble(&mut scene, core::slice::from_ref(&outer), assembly);
            assert_eq!(scene[..3], [quad(98.0), quad(99.0), quad(99.0)]);
        }

        // The assembly before was not the one that put it there.
        let (_, inner, mut scene) = marked_scene();
        let outer = around(vec![quad(0.0)], &inner);
        assemble(&mut scene, core::slice::from_ref(&outer), 4);
        assert_eq!(scene, flattened(&[outer]), "left alone");

        // It lands at another index.
        let (_, inner, mut scene) = marked_scene();
        let shifted = around(vec![quad(0.0), quad(0.5)], &inner);
        assemble(&mut scene, core::slice::from_ref(&shifted), 3);
        assert_eq!(scene, flattened(&[shifted]), "left alone");

        // The scene is too short to hold it where it was put.
        let (_, inner, mut scene) = marked_scene();
        scene.truncate(2);
        let outer = around(vec![quad(0.0)], &inner);
        assemble(&mut scene, core::slice::from_ref(&outer), 3);
        assert_eq!(scene, flattened(&[outer]), "left alone");

        // There is no scene, as after a pass that did not finish.
        let (_, inner, _) = marked_scene();
        let outer = around(vec![quad(0.0)], &inner);
        let mut scene = Vec::new();
        assemble(&mut scene, core::slice::from_ref(&outer), 3);
        assert_eq!(scene, flattened(&[outer]), "left alone");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn the_check_of_a_placed_chunk_accepts_what_was_placed_even_with_a_nan_in_it() {
        let nan = Primitive::Quad {
            rect: Rect {
                x: f32::NAN,
                ..rect(0.0)
            },
            color: color(1.0),
            radius: f32::NAN,
        };
        let tree = chunk(
            vec![quad(1.0)],
            vec![chunk(vec![nan], vec![], vec![quad(2.0)])],
            vec![quad(3.0)],
        );
        let mut scene = Vec::new();
        assemble(&mut scene, core::slice::from_ref(&tree), 2);
        assert_eq!(tree.assert_placed(&scene, 0), 4);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "of the scene, where")]
    fn the_check_of_a_placed_chunk_finds_a_primitive_that_is_not_where_it_was_placed() {
        let tree = chunk(
            vec![quad(1.0)],
            vec![chunk(vec![quad(2.0)], vec![], vec![])],
            vec![quad(3.0)],
        );
        let mut scene = Vec::new();
        assemble(&mut scene, core::slice::from_ref(&tree), 2);
        scene[1] = quad(9.0);
        tree.assert_placed(&scene, 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "of the scene, where")]
    fn the_check_of_a_placed_chunk_finds_a_scene_that_ends_before_it_does() {
        let tree = chunk(vec![quad(1.0), quad(2.0)], vec![], vec![quad(3.0)]);
        let mut scene = Vec::new();
        assemble(&mut scene, core::slice::from_ref(&tree), 2);
        scene.pop();
        tree.assert_placed(&scene, 0);
    }

    #[test]
    fn a_chunk_nothing_else_holds_is_written_over_whole() {
        let inner = chunk(vec![quad(1.0)], vec![], vec![]);
        let spare = chunk(
            vec![quad(0.0), text(2.0, "text")],
            vec![inner],
            vec![quad(3.0)],
        );
        let mut scene = Vec::new();
        assemble(&mut scene, core::slice::from_ref(&spare), 2);
        assert_eq!(spare.placed.get(), (2, 0));
        assert_eq!(spare.total, 4);
        let at = Rc::as_ptr(&spare);

        let written = PaintChunk::new(vec![quad(5.0)], 1, vec![]).put_in(Some(spare));
        // The same allocation holds a chunk that has nothing of the old one:
        // not its primitives, its children, its total or where it was placed.
        assert_eq!(Rc::as_ptr(&written), at);
        assert_eq!(written.primitives, [quad(5.0)]);
        assert_eq!((written.split, written.total), (1, 1));
        assert!(written.children.is_empty());
        assert_eq!(written.placed.get(), (0, 0));
    }

    #[test]
    fn a_chunk_that_something_else_holds_is_left_as_it_is() {
        let spare = chunk(vec![quad(0.0)], vec![], vec![quad(3.0)]);
        let held = spare.clone();
        let mut scene = Vec::new();
        assemble(&mut scene, core::slice::from_ref(&spare), 2);
        let at = Rc::as_ptr(&spare);

        let written = PaintChunk::new(vec![quad(5.0)], 1, vec![]).put_in(Some(spare));
        assert_ne!(Rc::as_ptr(&written), at);
        assert_eq!(written.primitives, [quad(5.0)]);
        assert_eq!(written.placed.get(), (0, 0));
        assert_eq!(held.primitives, [quad(0.0), quad(3.0)]);
        assert_eq!((held.split, held.total), (1, 2));
        assert_eq!(held.placed.get(), (2, 0));

        let alone = PaintChunk::new(vec![quad(6.0)], 0, vec![]).put_in(None);
        assert_eq!(alone.primitives, [quad(6.0)]);
    }

    #[test]
    fn the_room_of_a_chunk_is_taken_and_its_children_are_let_go_of() {
        let inner = chunk(vec![quad(1.0)], vec![], vec![]);
        let mut spare = chunk(
            vec![
                quad(0.0),
                text(1.0, "the first text"),
                styled(2.0, "second"),
            ],
            vec![inner.clone()],
            vec![quad(3.0)],
        );
        assert_eq!(Rc::strong_count(&inner), 2);
        let room = spare.primitives.capacity();

        let (primitives, taken) = Rc::get_mut(&mut spare).unwrap().take_room();
        assert!(primitives.is_empty());
        assert_eq!(primitives.capacity(), room);
        let taken = taken.expect("a text was taken");
        assert_eq!(taken, "the first text");
        // Nothing holds the chunk of the child but the one that has it.
        assert_eq!(Rc::strong_count(&inner), 1);
        assert!(spare.children.is_empty());

        // A chunk with no text has no buffer to give.
        let mut bare = chunk(vec![quad(0.0)], vec![], vec![quad(3.0)]);
        let (_, taken) = Rc::get_mut(&mut bare).unwrap().take_room();
        assert_eq!(taken, None);
    }

    fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
        context.run(Size::new(300.0, 200.0), &mut crate::MonospaceMeasurer, view);
    }

    fn ids(context: &Context) -> Vec<u64> {
        let mut ids: Vec<u64> = context.spare_chunks.keys().map(|id| id.0).collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn a_node_whose_paint_is_invalidated_is_not_cached_while_its_chunk_waits_to_be_written_over() {
        let view = |ui: &mut Ui<'_>| {
            ui.keyed("a", |ui| ui.button("A"));
            ui.keyed("b", |ui| ui.button("B"));
        };
        let mut context = Context::new();
        frame(&mut context, view);
        assert!(ids(&context).is_empty());
        let cached = context.memory();
        let (a, root) = (context.find(&["a"]).unwrap(), context.root);

        context.invalidate_paint(a);
        // What the node and the root drew waits, and neither is cached.
        let mut waiting = vec![a.0, root.0];
        waiting.sort_unstable();
        assert_eq!(ids(&context), waiting);
        assert!(context.nodes[&a].paint_cache.is_none());
        assert!(context.nodes[&root].paint_cache.is_none());
        let invalidated = context.memory();
        assert_eq!(invalidated.cached_nodes, cached.cached_nodes - 2);
        assert!(invalidated.cached_primitives < cached.cached_primitives);
        // The node is found invalid again, and what waits is not touched.
        context.invalidate_paint(a);
        assert_eq!(ids(&context), waiting);
        assert_eq!(context.memory(), invalidated);

        frame(&mut context, view);
        assert!(ids(&context).is_empty());
        assert_eq!(context.memory(), cached);
    }

    #[test]
    fn the_chunk_of_a_node_that_is_removed_before_it_is_painted_again_is_not_kept() {
        let mut context = Context::new();
        frame(&mut context, |ui| {
            ui.keyed("a", |ui| ui.button("A"));
            ui.keyed("b", |ui| ui.button("B"));
        });
        let b = context.find(&["b"]).unwrap();
        context.invalidate_paint(b);
        assert!(ids(&context).contains(&b.0));
        frame(&mut context, |ui| {
            ui.keyed("a", |ui| ui.button("A"));
        });
        assert!(context.find(&["b"]).is_none());
        assert!(ids(&context).is_empty());
    }

    /// The room a node's chunk has for primitives, and how many it holds.
    fn primitives_room(context: &Context, path: &[&str]) -> (usize, usize) {
        let chunk = painted(context, path);
        (chunk.primitives.capacity(), chunk.primitives.len())
    }

    #[test]
    fn a_node_painted_again_has_the_room_its_primitives_had_but_not_much_more_than_it_needs() {
        let mut context = Context::new();
        let mut paint = |count| {
            frame(&mut context, |ui| {
                ui.keyed("quads", |ui| ui.custom(Style::default(), quads(count), ()));
            });
            primitives_room(&context, &["quads"])
        };
        // The clip and its pop are two more than the quads.
        assert_eq!(paint(20), (22, 22));
        // Fewer primitives go in the room there is.
        assert_eq!(paint(10), (22, 12));
        assert_eq!(paint(11), (22, 13));
        assert_eq!(paint(100), (102, 102));
        // A node that is down to a few has room for eight, and not for a hundred.
        let (room, drawn) = paint(2);
        assert_eq!(drawn, 4);
        assert!((8..=32).contains(&room), "{room}");
    }

    /// Whether the label at `key` draws its text as a styled text, and the room
    /// the buffer of the text has.
    fn text_room(context: &Context, key: &str) -> (bool, usize) {
        painted(context, &[key])
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                Primitive::Text { text, .. } => Some((false, text.capacity())),
                Primitive::StyledText { text, .. } => Some((true, text.capacity())),
                _ => None,
            })
            .expect("the label draws its text")
    }

    #[test]
    fn a_label_painted_again_has_the_room_its_text_had_but_not_much_more_than_it_needs() {
        let mut context = Context::new();
        let mut paint = |text: &str| {
            frame(&mut context, |ui| {
                ui.keyed("label", |ui| ui.label(text.to_owned()));
                // A bold label draws its text as a styled text.
                ui.keyed("bold", |ui| {
                    ui.label(crate::Label::new(text).style(Style::default().font_weight(700)))
                });
            });
            [text_room(&context, "label"), text_room(&context, "bold")]
        };
        let both = |room| [(false, room), (true, room)];
        assert_eq!(paint("a text of twenty chars"), both(22));
        // A shorter one goes in the room there is, and so does one as long.
        assert_eq!(paint("ten chars!"), both(22));
        assert_eq!(paint("another twenty chars!!"), both(22));
        // A longer one needs another, and a short one after it does not keep it.
        assert_eq!(paint(&"long ".repeat(200)), both(1000));
        assert_eq!(paint("ab"), both(2));
    }

    /// A list of `len` ids that has room for `room`.
    fn id_list(len: usize, room: usize) -> Vec<NodeId> {
        let mut list = Vec::with_capacity(room);
        list.extend((0..len as u64).map(|id| NodeId(id + 1)));
        list
    }

    #[test]
    fn a_list_of_ids_is_kept_emptied_and_only_while_it_is_small_and_few_are_kept() {
        let mut context = Context::new();
        // One with no room, and one with too much, are let go.
        context.give_ids(Vec::new());
        context.give_ids(id_list(4097, 4097));
        assert!(context.id_spare.is_empty());
        // The largest is kept, emptied.
        context.give_ids(id_list(4096, 4096));
        assert_eq!(context.id_spare.len(), 1);
        assert!(context.id_spare[0].is_empty() && context.id_spare[0].capacity() >= 4096);
        for _ in 0..400 {
            context.give_ids(id_list(3, 4));
        }
        assert_eq!(context.id_spare.len(), 256);
        assert!(context.id_spare.iter().all(Vec::is_empty));
    }

    // Placing the children of a node takes a list and gives it back, so the
    // lists kept are those in use at the deepest place, not one for each
    // node that has children.
    #[test]
    fn placing_the_children_of_many_nodes_keeps_a_list_for_each_level() {
        let mut context = Context::new();
        frame(&mut context, |ui| {
            for at in 0..300 {
                ui.keyed(format!("column{at}"), |ui| {
                    ui.column(Style::default(), |ui| {
                        ui.label("one");
                        ui.label("two");
                    })
                });
            }
        });
        let kept = context.id_spare.len();
        assert!((1..=4).contains(&kept), "{kept}");
    }

    #[test]
    fn the_lists_kept_for_copying_children_hold_nothing_whatever_the_frames_were() {
        let mut context = Context::new();
        let kept = |context: &Context| {
            assert!(context.id_spare.len() <= 256);
            assert!(context.id_spare.iter().all(|list| list.is_empty()));
            assert!(context.id_spare.iter().all(|list| list.capacity() <= 4096));
            context.id_spare.len()
        };
        fn nested(ui: &mut Ui<'_>, depth: usize, width: usize) {
            for at in 0..width {
                ui.keyed(format!("label{at}"), |ui| ui.label("a label"));
            }
            if depth > 0 {
                ui.keyed("deeper", |ui| {
                    ui.column(Style::default(), |ui| nested(ui, depth - 1, width))
                });
            }
        }
        for (depth, width) in [(10, 3), (10, 30), (4, 1), (0, 0), (10, 3)] {
            frame(&mut context, |ui| nested(ui, depth, width));
            frame(&mut context, |ui| nested(ui, depth, width));
            kept(&context);
        }
        assert!(kept(&context) > 0);
        // A container with more children than a list is kept at, then the
        // same with few, then with many again, each placed afresh.
        for count in [5000, 5, 5000] {
            frame(&mut context, |ui| {
                for at in 0..count {
                    ui.keyed(format!("child{at}"), |ui| ui.label("x"));
                }
            });
            kept(&context);
        }
    }
}
