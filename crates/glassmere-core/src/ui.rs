use crate::collections::HashMap;
use crate::collections::HashSet;
use crate::collections::KeyState;
use crate::controls::{Control, ControlUpdate};
use crate::window::caption;
use crate::{Component, Context, CustomWidget, NodeId, Style};
#[cfg(feature = "std")]
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
#[cfg(feature = "std")]
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::panic::Location;
use glassmere_scene::Color;
mod builders;
pub use builders::{
    Button, Calendar, Checkbox, Collapsible, ComboBox, DatePicker, Label, Link, Progress, Radio,
    RangeSlider, Spinner, Switch, TabStyle, TextInput,
};
mod calendar;
mod collections;
mod controls;
mod dock;
mod fields;
pub use fields::{Autocomplete, suggest};
mod image;
mod overlays;
pub use overlays::CommandPalette;
mod property_grid;
pub use property_grid::PropertyGrid;
mod areas;
mod basic;
pub use areas::{AreaBuilder, DragSourceBuilder, DropTargetBuilder, GestureBuilder};
mod dialog;
mod rich_editor;
mod rich_text;
#[cfg(feature = "std")]
mod text_area;
mod toasts;
mod window;
pub(crate) use toasts::ToastOutbox;
pub use window::WindowButtons;

pub use dock::{DockOptions, DockPanel};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WidgetKey {
    Named(String),
    /// Where in the source the widget was declared, and how many siblings
    /// declared from the same place came before it.
    Site(&'static Location<'static>, u32),
    /// The content of a dock panel, named by the panel's id. It is identified
    /// by the dock that shows it, not by the group it is in, so it follows
    /// the panel when the dock moves it.
    Panel(String),
}

/// Equal keys hash alike under one state. A call site is hashed by its
/// content, never its address, because two `Location`s of one place are equal
/// keys. It is made of integers only, which is cheap and needs no allocation:
/// a call site is the key of every widget that has no name. A name is chosen
/// by the application, so its bytes are given to the hasher whole, for the
/// keyed hash of a `KeyState`.
impl core::hash::Hash for WidgetKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Named(name) => {
                state.write_u8(0);
                state.write(name.as_bytes());
            }
            Self::Site(site, nth) => {
                state.write_u8(1);
                state.write_u32(site.line());
                state.write_u32(site.column());
                state.write_u32(*nth);
                let (words, rest) = site.file().as_bytes().as_chunks::<8>();
                state.write_usize(site.file().len());
                for word in words {
                    state.write_u64(u64::from_le_bytes(*word));
                }
                if !rest.is_empty() {
                    let mut word = [0u8; 8];
                    word[..rest.len()].copy_from_slice(rest);
                    state.write_u64(u64::from_le_bytes(word));
                }
            }
            Self::Panel(name) => {
                state.write_u8(2);
                state.write(name.as_bytes());
            }
        }
    }
}

impl WidgetKey {
    pub(crate) fn named(key: impl Into<String>) -> Self {
        Self::Named(key.into())
    }

    /// The key as a path segment: the name, or `file.rs:line:column`, with
    /// `#n` for the n-th repeat of one site such as a loop body.
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Named(name) => name.clone(),
            Self::Site(site, nth) => {
                let file = site.file().rsplit(['/', '\\']).next().unwrap_or_default();
                let at = format!("{file}:{}:{}", site.line(), site.column());
                if *nth == 0 { at } else { format!("{at}#{nth}") }
            }
            Self::Panel(id) => format!("$panel:{id}"),
        }
    }

    /// Whether two keys are equal, which is what `==` says too, with the
    /// cheap fields compared before the file path of a call site.
    fn same(&self, other: &Self) -> bool {
        #[cfg(all(test, feature = "std"))]
        KEY_COMPARISONS.with(|count| count.set(count.get() + 1));
        match (self, other) {
            (Self::Named(a), Self::Named(b)) | (Self::Panel(a), Self::Panel(b)) => a == b,
            (Self::Site(a, n), Self::Site(b, m)) => n == m && same_site(a, b),
            _ => false,
        }
    }
}

#[cfg(all(test, feature = "std"))]
std::thread_local! {
    /// How many times `WidgetKey::same` ran on this thread.
    static KEY_COMPARISONS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// How many source locations a scope counts in an array before it builds a
/// table for them.
const SITES_INLINE: usize = 8;

/// How many widgets each source location has declared in one scope.
///
/// A scope declares its widgets from a handful of lines, so the counts sit
/// in a short array, and only a scope that declares from more lines than
/// that pays for a table.
#[derive(Default)]
pub(crate) struct Sites {
    inline: [Option<(&'static Location<'static>, u32)>; SITES_INLINE],
    /// Every count so far, once the array was full; then the array is not
    /// read again.
    spilled: Option<crate::collections::FastMap<&'static Location<'static>, u32>>,
}

/// Whether two locations are the same place in the source, which is what
/// `==` says too. It looks at the line and column first, because the file
/// path is long and most locations of one file differ before it.
fn same_site(a: &'static Location<'static>, b: &'static Location<'static>) -> bool {
    core::ptr::eq(a, b)
        || (a.line() == b.line() && a.column() == b.column() && a.file() == b.file())
}

impl Sites {
    /// How many widgets `site` has declared before this one, counting this
    /// one from now on.
    fn next(&mut self, site: &'static Location<'static>) -> u32 {
        let counts = match &mut self.spilled {
            Some(counts) => counts,
            None => {
                for slot in &mut self.inline {
                    match slot {
                        Some((seen, count)) if same_site(seen, site) => {
                            let before = *count;
                            *count += 1;
                            return before;
                        }
                        Some(_) => {}
                        None => {
                            *slot = Some((site, 1));
                            return 0;
                        }
                    }
                }
                self.spilled
                    .insert(self.inline.iter().flatten().copied().collect())
            }
        };
        let count = counts.entry(site).or_default();
        let before = *count;
        *count += 1;
        before
    }
}

/// How many keys a scope compares a new one against, one by one, before it
/// keeps a table of their hashes.
const KEYS_SCANNED: usize = 8;

/// Which keys of one scope are taken.
///
/// The keys of the siblings declared so far are in the scope's `elements`
/// already, so a scope with few of them compares against those and keeps
/// nothing of its own; only a bigger one keeps a table of hashes, and no key
/// is copied into it.
#[derive(Default)]
pub(crate) struct Keys {
    /// The hash of every key that is taken, once the scope is big enough to
    /// want it, and the state that made them. Equal keys have equal hashes,
    /// so a hash that is not here belongs to a key that is free; one that is
    /// here is only a candidate.
    index: Option<(KeyState, crate::collections::FastSet<u64>)>,
    /// How many of the first `elements` have their hash in `index`.
    indexed: usize,
    /// The keys of elements `take_declared` took back out, which stay taken.
    retired: Vec<WidgetKey>,
}

/// The hash a scope's table holds a key by.
fn digest_of(state: &KeyState, key: &WidgetKey) -> u64 {
    use core::hash::BuildHasher;
    state.hash_one(key)
}

impl Keys {
    /// Whether `key` is free among the siblings in `elements`, and takes it
    /// if so. `elements` are the siblings before the element, which stays
    /// right behind them after a `true`.
    fn claim(&mut self, elements: &[Element], key: &WidgetKey) -> bool {
        self.claim_with(elements, key, digest_of)
    }

    /// `claim` with the hash chosen, so that a test can make keys collide.
    fn claim_with(
        &mut self,
        elements: &[Element],
        key: &WidgetKey,
        digest: fn(&KeyState, &WidgetKey) -> u64,
    ) -> bool {
        if self.index.is_none() && elements.len() + self.retired.len() <= KEYS_SCANNED {
            return !elements.iter().any(|element| element.key.same(key))
                && !self.retired.iter().any(|retired| retired.same(key));
        }
        let (state, index) = match &mut self.index {
            Some(index) => index,
            None => {
                let state = KeyState::default();
                let index = self.retired.iter().map(|key| digest(&state, key)).collect();
                self.index.insert((state, index))
            }
        };
        // The siblings from before the table was built, and any added
        // without a claim, such as a popup's, are not in it yet.
        for element in &elements[self.indexed.min(elements.len())..] {
            index.insert(digest(state, &element.key));
        }
        self.indexed = elements.len();
        // Another key can have the same hash, so a hash that is there is
        // looked for among the keys themselves.
        if !index.insert(digest(state, key))
            && (elements.iter().any(|element| element.key.same(key))
                || self.retired.iter().any(|retired| retired.same(key)))
        {
            return false;
        }
        self.indexed = elements.len() + 1;
        true
    }

    /// Keeps `key` taken after its element, the last of the scope, was taken
    /// back out and `elements_len` are left.
    fn retire(&mut self, key: WidgetKey, elements_len: usize) {
        self.retire_with(key, elements_len, digest_of);
    }

    /// `retire` with the hash chosen, to go with `claim_with`.
    fn retire_with(
        &mut self,
        key: WidgetKey,
        elements_len: usize,
        digest: fn(&KeyState, &WidgetKey) -> u64,
    ) {
        if let Some((state, index)) = &mut self.index {
            index.insert(digest(state, &key));
        }
        self.indexed = self.indexed.min(elements_len);
        self.retired.push(key);
    }
}

/// What a declaration hands to the host besides the tree.
#[derive(Default)]
pub(crate) struct HostOutbox {
    #[cfg(feature = "std")]
    pub watched: Vec<std::path::PathBuf>,
    /// The native menu and tray icon as the view last declared them.
    pub menu: Option<crate::Menu>,
    /// Whether what is being declared sits in a title bar.
    pub in_title_bar: bool,
    pub tray: Option<Option<crate::Tray>>,
    /// Commands a menu the framework drew itself chose.
    pub commands: Vec<String>,
    #[cfg(feature = "std")]
    pub images: Vec<std::path::PathBuf>,
    pub toasts: ToastOutbox,
    /// Misuses a release build skipped, with the parent they were declared in.
    pub misuse: Vec<(Option<crate::NodeId>, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Column,
    Row,
    Grid,
    Container,
    FocusScope,
    Dialog,
    Tabs,
    TabList,
    Tab,
    TabPanel,
    Split,
    SplitRow,
    Divider,
    Dock,
    DockGroup,
    DockFloat,
    DockHandle,
    TextArea,
    CodeEditor,
    RichEditor,
    RichText,
    Image,
    /// The stack of toasts in the window corner, and one toast in it.
    Toasts,
    Toast,
    ContextArea,
    GestureArea,
    /// A container that watches what reaches the widgets inside it.
    EventArea,
    /// A container that keeps its subtree when the state it was declared from
    /// has not changed.
    Memo,
    Popover,
    Label,
    Button,
    Scroll,
    Custom,
    TextInput,
    Checkbox,
    Switch,
    Disclosure,
    Link,
    Radio,
    Slider,
    RangeSlider,
    Progress,
    Spinner,
    ComboBox,
    MenuButton,
    /// The days of a calendar, which take the keys together.
    Calendar,
    Popup,
    Choice,
    Collection,
    CollectionRow,
    DragSource,
    DropTarget,
}

/// Width and height of a switch's track.
pub(crate) const SWITCH_TRACK: (f32, f32) = (40.0, 22.0);
/// The side of a checkbox's box and of a radio button's ring.
pub(crate) const MARK: f32 = 18.0;

impl Kind {
    /// Whether the node only holds others, so its box is laid out by its
    /// padding and border alone: what `Style::padding_sides` and
    /// `Style::border_sides` apply to. The rest draw their own content at the
    /// one `padding` and `border`.
    pub(crate) fn holds_nodes(self) -> bool {
        matches!(
            self,
            Self::Column
                | Self::Row
                | Self::Grid
                | Self::Container
                | Self::FocusScope
                | Self::EventArea
                | Self::Memo
                | Self::ContextArea
                | Self::GestureArea
        )
    }

    pub(crate) fn text_leaf(self) -> bool {
        matches!(
            self,
            Self::Label
                | Self::Button
                | Self::Checkbox
                | Self::Switch
                | Self::Disclosure
                | Self::Link
                | Self::Radio
                | Self::Choice
                | Self::MenuButton
                | Self::ComboBox
                | Self::Tab
                | Self::RichText
        )
    }

    /// Width of what a control draws before its text, along the start edge,
    /// counting the gap to the text.
    pub(crate) fn indicator_width(self) -> f32 {
        match self {
            Self::Switch => SWITCH_TRACK.0 + 10.0,
            Self::Disclosure => 20.0,
            Self::Checkbox | Self::Radio => MARK + 10.0,
            _ => 0.0,
        }
    }
}

/// How a view asks the host's text measurer for the size of some text.
pub(crate) type MeasureFn<'a> =
    dyn Fn(&str, &glassmere_scene::TextStyle, Option<f32>) -> glassmere_scene::Size + 'a;

pub(crate) struct Element {
    /// Where among the declaration's handlers this element's is.
    pub handler: Option<u32>,
    /// Which element of the declaration this is, from 1; 0 for one that did
    /// not go through `Ui::push`.
    pub seq: u32,
    pub key: WidgetKey,
    pub kind: Kind,
    pub text: String,
    pub style: Style,
    pub enabled: bool,
    pub children: Vec<Element>,
    pub custom: Option<CustomWidget>,
    pub tooltip: Option<Box<crate::Tooltip>>,
    pub control: Control,
}

impl Element {
    /// An enabled element with no text, children, handler or control. The
    /// builders that run once per widget (`leaf`, `label`, `group`,
    /// `control` and `text_field`) write every field themselves, so that no
    /// element is built only to have most of it replaced; they spell out
    /// these defaults and must be kept in step with them.
    pub(crate) fn new(key: WidgetKey, kind: Kind) -> Self {
        Element {
            handler: None,
            seq: 0,
            key,
            kind,
            text: String::new(),
            style: Style::default(),
            enabled: true,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control: Control::None,
        }
    }
}

/// What a widget reports about itself. Reading [`Response::hovered`],
/// [`Response::pressed`] or [`Response::focused`] tells the framework the view depends on that state,
/// so a change in it declares the view again; a change in a state nobody
/// read only repaints.
#[derive(Clone, Copy, Default)]
pub struct Response<'a> {
    hovered: bool,
    pressed: bool,
    focused: bool,
    observer: Option<(&'a Context, Observed)>,
}

/// The node a response is about, or the element that will become it.
#[derive(Clone, Copy)]
enum Observed {
    Node(NodeId),
    Element(u32),
}

impl core::fmt::Debug for Response<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Response")
            .field("hovered", &self.hovered)
            .field("pressed", &self.pressed)
            .field("focused", &self.focused)
            .finish()
    }
}

impl Response<'_> {
    /// Whether the pointer is over the widget; the view is declared again
    /// when that changes.
    pub fn hovered(&self) -> bool {
        self.observe();
        self.hovered
    }

    /// Whether the widget is being held down, by pointer, touch or keyboard;
    /// the view is declared again when that changes.
    pub fn pressed(&self) -> bool {
        self.observe();
        self.pressed
    }

    /// Whether the widget has the keyboard; the view is declared again when
    /// that changes.
    pub fn focused(&self) -> bool {
        self.observe();
        self.focused
    }

    /// The same response, held for as long as `context` is: for one
    /// declared in a nested scope and returned from the outer one.
    pub(crate) fn rebind(self, context: &Context) -> Response<'_> {
        Response {
            hovered: self.hovered,
            pressed: self.pressed,
            focused: self.focused,
            observer: self.observer.map(|(_, observed)| (context, observed)),
        }
    }

    /// Marks the node as one whose hover, focus and press the view reads.
    pub(crate) fn observe(&self) {
        match self.observer {
            Some((context, Observed::Node(id))) => context.observe_node(id),
            Some((context, Observed::Element(seq))) => context.observe_element(seq),
            None => {}
        }
    }
}

/// Declarations are rebuilt on input; matching nodes and layout caches persist.
///
/// A widget is identified by where it is declared: the source line that
/// declares a button declares the same button next frame, whatever was
/// declared conditionally before it. Repeats of one line, such as a loop
/// body, are told apart by order, and [`Ui::keyed`] names them when items
/// move. Keys are scoped to the parent.
pub struct Ui<'a, M: 'static = ()> {
    pub(crate) context: &'a Context,
    /// Measures text with the measurer this frame's layout uses, so that the
    /// answer is the size the text will have.
    pub(crate) measure: &'a MeasureFn<'a>,
    pub(crate) parent: Option<NodeId>,
    /// The name [`Ui::keyed`] gave the next element declared, taken by the
    /// first one that asks for a key.
    pub(crate) pending_key: Option<String>,
    /// Clicks and changes that reached widgets without a handler: the
    /// framework's own composites, which read them while declaring.
    pub(crate) clicks: &'a mut HashSet<NodeId>,
    /// The widgets declared in this scope, in order. Only `push` adds to it,
    /// apart from the popup a context menu puts first. `take_declared` takes
    /// one out and leaves its key taken; `push` takes out the one it just
    /// added when its key was taken already, which leaves nothing to retire.
    /// No key is changed in place, because `keys` reads the siblings' keys
    /// from here.
    pub(crate) elements: Vec<Element>,
    /// Which keys of `elements` and of the ones taken back out are taken. It
    /// reads the siblings from `elements`, so a widget enters it by being
    /// pushed there.
    pub(crate) keys: Keys,
    /// Where in `elements` the last declaration went, or `None` when it was
    /// skipped as a misuse.
    pub(crate) declared: Option<usize>,
    /// How many widgets each source location has declared here so far.
    pub(crate) sites: Sites,
    pub(crate) updates: &'a mut HashMap<NodeId, ControlUpdate>,
    pub(crate) warnings: &'a mut Vec<String>,
    /// The store `Ui::observe` asked for, read after the declaration so the
    /// graph shows what this frame read, whenever `observe` was called.
    pub(crate) observed: &'a mut Option<crate::Store>,
    pub(crate) redraw: &'a mut bool,
    pub(crate) focus: &'a mut Option<(Option<NodeId>, WidgetKey)>,
    pub(crate) host: &'a mut HostOutbox,
    /// Dialog layers, appended after normal content as root children.
    pub(crate) layers: &'a mut Vec<Element>,
    pub(crate) enabled: bool,
    /// What the widgets of this declaration send, in the order they were
    /// declared; an element names its own by index.
    pub(crate) handlers: &'a mut Vec<crate::app::Handler<M>>,
    /// Where this list starts among the declaration's handlers. Nonzero
    /// inside `Ui::map`, whose handlers are converted and appended at its end.
    pub(crate) handler_base: u32,
}

impl<'a, M: 'static> Ui<'a, M> {
    /// Design tokens of the active theme, for application-owned surfaces.
    #[track_caller]
    pub fn theme(&self) -> &crate::Theme {
        self.context.theme()
    }

    /// How big `text` is in `style`, on one line, as the layout will measure
    /// it: the same font system the host draws with, so a control you paint
    /// yourself can size itself to its label with no table of glyph widths.
    ///
    /// ```ignore
    /// let style = ui.text_style();
    /// let label = ui.measure_text("Save changes", &style);
    /// let size = Size::new(label.width + 24.0, label.height + 12.0);
    /// ```
    ///
    /// Shaping costs time, so measure once and keep the answer when the text
    /// does not change.
    pub fn measure_text(
        &self,
        text: &str,
        style: &glassmere_scene::TextStyle,
    ) -> glassmere_scene::Size {
        (self.measure)(text, style, None)
    }

    /// How big `text` is when it wraps at `width`.
    pub fn measure_text_wrapped(
        &self,
        text: &str,
        style: &glassmere_scene::TextStyle,
        width: f32,
    ) -> glassmere_scene::Size {
        (self.measure)(text, style, Some(width))
    }

    /// The text style the theme gives plain text: its size, family, weight
    /// and direction. Change what you need before measuring.
    pub fn text_style(&self) -> glassmere_scene::TextStyle {
        self.context
            .base_text()
            .text_style(glassmere_scene::TextWrap::None)
    }

    /// Files the operating system asked the application to open, from the
    /// command line or from an association. Empty when it was started
    /// without any.
    #[cfg(feature = "std")]
    #[track_caller]
    pub fn opened_files(&self) -> &[std::path::PathBuf] {
        self.context.opened_files()
    }

    /// Shows a reactive store's dependency graph in the inspector: what its
    /// values are, what each derived one read, and which moved last. The
    /// store is otherwise none of the framework's business, so nothing is
    /// observed unless this is called.
    #[track_caller]
    pub fn observe(&mut self, store: &crate::Store) {
        *self.observed = Some(store.clone());
    }

    /// Asks the host for another frame after this one, for work that spreads
    /// over frames without an input event or animation driving it.
    #[track_caller]
    pub fn request_redraw(&mut self) {
        *self.redraw = true;
    }

    /// Content of a file the host keeps current: it is loaded after the first
    /// declaration that asks for it, reloaded when it changes on disk, and a
    /// redraw follows each change. `None` until the first load; a failed
    /// reload keeps the last good content (see `Context::watch_error`).
    #[cfg(feature = "std")]
    #[track_caller]
    pub fn watched_bytes(&mut self, path: impl AsRef<std::path::Path>) -> Option<Arc<[u8]>> {
        let path = path.as_ref();
        if !self.host.watched.iter().any(|known| known == path) {
            self.host.watched.push(path.to_owned());
        }
        self.context.watched_bytes(path)
    }

    /// `watched_bytes` for UTF-8 text files; `None` also for invalid UTF-8.
    #[cfg(feature = "std")]
    #[track_caller]
    pub fn watched_text(&mut self, path: impl AsRef<std::path::Path>) -> Option<Arc<str>> {
        let path = path.as_ref();
        self.watched_bytes(path)?;
        self.context.watched_text(path)
    }

    /// Moves keyboard focus to the widget declared with `key` in this same
    /// container, once this declaration has been reconciled: for the
    /// framework's own composites, whose focus follows what they show.
    #[track_caller]
    pub(crate) fn request_focus(&mut self, key: impl Into<String>) {
        *self.focus = Some((self.parent, WidgetKey::Named(key.into())));
    }

    /// A named class from the installed stylesheet. Unknown names return the
    /// default style and are reported through `Context::style_warnings`.
    #[track_caller]
    pub fn class(&mut self, name: &str) -> Style {
        match self.context.stylesheet().class(name) {
            Some(style) => style.clone(),
            None => {
                let warning = format!("unknown style class `{name}`");
                if !self.warnings.contains(&warning) {
                    self.warnings.push(warning);
                }
                Style::default()
            }
        }
    }

    #[track_caller]
    fn push(&mut self, element: Element) -> Response<'a> {
        // The element goes into its place before anything borrows, reads or
        // changes it, and is finished there: an argument that is touched
        // first is copied once more on its way. It is in the list from here
        // on, so a way out before `declared` is set must pop it first, as a
        // duplicate does.
        let index = self.elements.len();
        self.elements.push(element);
        let (siblings, slot) = self.elements.split_at_mut(index);
        let element = &mut slot[0];
        element.enabled &= self.enabled;
        // A background file becomes its image once the host has delivered it,
        // and that style change is what repaints the node.
        #[cfg(feature = "std")]
        if let Some(background) = &mut element.style.background_image
            && let crate::ImageSource::File(path) = &background.source
        {
            if !self.host.images.contains(path) {
                self.host.images.push(path.clone());
            }
            if let (Some(image), _) = self.context.image_file(path) {
                background.source = crate::ImageSource::Image(image);
            }
        }
        if !self.keys.claim(siblings, &element.key) {
            let message = format!("duplicate widget key among siblings: {:?}", element.key);
            // It leaves the list before `misuse`, which can panic, so that
            // it is dropped where a widget that was never added is.
            let duplicate = self.elements.pop();
            self.misuse(message);
            drop(duplicate);
            return Response::default();
        }
        if let Some(crate::FontFamily::Named(name)) = &element.style.font_family {
            self.context.note_family(name);
        }
        element.seq = self.context.count_element(
            element.text.len()
                + match &element.key {
                    WidgetKey::Named(key) | WidgetKey::Panel(key) => key.len(),
                    WidgetKey::Site(..) => 0,
                },
        );
        let existing = self.context.child(self.parent, &element.key, element.kind);
        let context: &'a Context = self.context;
        let response = match existing {
            Some(id) => Response {
                hovered: context.hovered() == Some(id),
                pressed: context.pressed(id),
                focused: context.focused() == Some(id),
                observer: Some((context, Observed::Node(id))),
            },
            None => Response {
                observer: Some((context, Observed::Element(element.seq))),
                ..Response::default()
            },
        };
        self.declared = Some(index);
        response
    }

    /// A use of the API the framework does not support. A debug build stops
    /// with `message`; a release build leaves out what was being declared
    /// and reports a [`crate::DiagnosticKind::Misuse`] diagnostic instead.
    #[track_caller]
    pub(crate) fn misuse(&mut self, message: String) {
        assert!(!self.context.misuse_panics(), "{message}");
        self.declared = None;
        self.host.misuse.push((self.parent, message));
    }

    /// The element the last declaration added, unless it was skipped.
    #[track_caller]
    pub(crate) fn declared(&mut self) -> Option<&mut Element> {
        self.declared.and_then(|index| self.elements.get_mut(index))
    }

    /// Takes the element the last declaration added back out. Its key stays
    /// taken.
    #[track_caller]
    pub(crate) fn take_declared(&mut self) -> Option<Element> {
        let index = self.declared.take()?;
        if index + 1 != self.elements.len() {
            return None;
        }
        let element = self.elements.pop()?;
        self.keys.retire(element.key.clone(), self.elements.len());
        Some(element)
    }

    /// Gives the element just declared what it sends.
    pub(crate) fn set_handler(&mut self, handler: crate::app::Handler<M>) {
        let index = self.handler_base + self.handlers.len() as u32;
        if let Some(element) = self.declared() {
            element.handler = Some(index);
            self.handlers.push(handler);
        }
    }

    /// Gives the element just declared the control that makes it what it is.
    #[track_caller]
    pub(crate) fn set_control(&mut self, control: Control) {
        if let Some(element) = self.declared() {
            element.control = control;
        }
    }

    #[track_caller]
    fn leaf(
        &mut self,
        key: WidgetKey,
        kind: Kind,
        text: String,
        style: Style,
        enabled: bool,
    ) -> Response<'a> {
        self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind,
            text,
            style,
            enabled,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control: Control::None,
        })
    }

    /// What the next element is identified by: the name [`Ui::keyed`] gave
    /// it, or failing that where it is being declared.
    #[track_caller]
    pub(crate) fn next_key(&mut self) -> WidgetKey {
        if let Some(key) = self.pending_key.take() {
            return WidgetKey::Named(key);
        }
        let site = Location::caller();
        let nth = self.sites.next(site);
        WidgetKey::Site(site, nth)
    }

    /// Names the next widget declared inside, so the framework can follow it
    /// when its siblings change.
    ///
    /// A widget is normally identified by the line that declares it, which
    /// survives conditional siblings. A line that runs in a loop declares its
    /// widgets in order, though, and when items are inserted, removed or
    /// reordered that order stops meaning the same thing from one frame to
    /// the next: focus, scroll offset, selection and undo history follow the
    /// identity, so they end up on the wrong row. A key taken from the data
    /// fixes that. A helper of the application's that declares widgets can be
    /// `#[track_caller]`, so its widgets are told apart by where it is called.
    ///
    /// ```ignore
    /// for pane in &panes {
    ///     ui.keyed(pane.id.to_string(), |ui| {
    ///         ui.column(Style::default(), |ui| ui.label(&pane.title));
    ///     });
    /// }
    /// ```
    ///
    /// The name is scoped to the parent, and two siblings may not share one.
    #[track_caller]
    pub fn keyed<R>(
        &mut self,
        key: impl Into<String>,
        content: impl FnOnce(&mut Ui<'a, M>) -> R,
    ) -> R {
        let outer = self.pending_key.replace(key.into());
        let result = content(self);
        self.pending_key = outer;
        result
    }

    /// A piece of text. `ui.label("Ready")` is the whole of it, and
    /// [`Label::new`] carries a style when one is wanted.
    #[track_caller]
    pub fn label(&mut self, label: impl Into<Label>) {
        let label = label.into();
        let key = self.next_key();
        self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind: Kind::Label,
            text: label.text,
            style: label.style.unwrap_or_default(),
            enabled: true,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control: if label.selectable {
                Control::SelectableText
            } else {
                Control::None
            },
        });
    }

    /// A label that shows a [`Signal`](crate::Signal) or [`Memo`](crate::Memo)
    /// of text and follows it. When the source changes, the label's text is
    /// replaced in the tree, measured and repainted without the view running
    /// again; see [`App::view_state`](crate::App::view_state) for how a view is kept from running
    /// after a message.
    ///
    /// The view reads the text when it declares, so a view that does run
    /// shows the same text. A label declared this way without the source
    /// next frame is an ordinary label again.
    #[track_caller]
    pub fn bound_label<S: crate::TextSource + Clone + 'static>(&mut self, source: &S) {
        let version = source.version();
        let before = self.elements.len();
        self.label(source.text());
        // A duplicate key leaves nothing behind to follow the source.
        if let Some(element) = self.elements.get(before)
            && element.kind == Kind::Label
        {
            self.context
                .bind_element(element.seq, Rc::new(source.clone()), version);
        }
    }

    /// A button. `ui.button("Save")` is the whole of it, and
    /// [`Button::new`] carries a style, a disabled state, and with
    /// [`Button::on_click`] the message a click sends.
    #[track_caller]
    pub fn button(&mut self, button: impl Into<Button<M>>) -> Response<'a> {
        let button = button.into();
        let key = self.next_key();
        let response = self.leaf(
            key,
            Kind::Button,
            button.text,
            button.style.unwrap_or_else(Style::button),
            button.enabled,
        );
        if let Some(message) = button.on_click {
            self.set_handler(crate::app::Handler::Message(message));
        }
        response
    }

    /// Declares `content`, whose widgets send `N`, as part of this view:
    /// each message they send reaches this one's as `f(message)`. Declares
    /// no node of its own.
    #[track_caller]
    pub fn map<N: Clone + 'static>(
        &mut self,
        f: impl Fn(N) -> M + 'static,
        content: impl FnOnce(&mut Ui<'_, N>),
    ) {
        let mut handlers = Vec::new();
        let mut inner = Ui {
            context: self.context,
            measure: self.measure,
            parent: self.parent,
            pending_key: self.pending_key.take(),
            clicks: &mut *self.clicks,
            elements: core::mem::take(&mut self.elements),
            keys: core::mem::take(&mut self.keys),
            declared: self.declared,
            sites: core::mem::take(&mut self.sites),
            updates: &mut *self.updates,
            warnings: &mut *self.warnings,
            observed: &mut *self.observed,
            redraw: &mut *self.redraw,
            focus: &mut *self.focus,
            host: &mut *self.host,
            layers: &mut *self.layers,
            enabled: self.enabled,
            handlers: &mut handlers,
            handler_base: self.handler_base + self.handlers.len() as u32,
        };
        content(&mut inner);
        self.pending_key = inner.pending_key;
        self.elements = inner.elements;
        self.keys = inner.keys;
        self.declared = inner.declared;
        self.sites = inner.sites;
        if !handlers.is_empty() {
            let f: alloc::rc::Rc<dyn Fn(N) -> M> = alloc::rc::Rc::new(f);
            self.handlers
                .extend(handlers.into_iter().map(|handler| handler.map(&f)));
        }
    }

    /// A `Ui` that declares the children of `parent`, sharing everything
    /// else with this one.
    pub(crate) fn scope(&mut self, parent: Option<NodeId>) -> Ui<'_, M> {
        Ui {
            context: self.context,
            measure: self.measure,
            parent,
            pending_key: None,
            clicks: self.clicks,
            elements: self.context.take_spare(),
            keys: Default::default(),
            declared: None,
            sites: Default::default(),
            updates: self.updates,
            warnings: self.warnings,
            observed: self.observed,
            redraw: self.redraw,
            focus: self.focus,
            host: self.host,
            layers: self.layers,
            enabled: self.enabled,
            handlers: self.handlers,
            handler_base: self.handler_base,
        }
    }

    #[track_caller]
    fn group(
        &mut self,
        key: WidgetKey,
        kind: Kind,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let parent = self.context.child(self.parent, &key, kind);
        let mut child = self.scope(parent);
        content(&mut child);
        let children = child.elements;
        self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind,
            text: String::new(),
            style,
            enabled: true,
            children,
            custom: None,
            tooltip: None,
            control: Control::None,
        });
    }

    /// A column whose content is declared again only when `state` differs from
    /// the last frame, or when something inside it has input waiting. The
    /// retained subtree is kept as it was otherwise: it still paints, scrolls,
    /// animates and takes part in layout, so this costs nothing in appearance.
    ///
    /// `state` must cover everything the content reads, and the content's
    /// `Response` values are not delivered on a frame that was skipped. Use it
    /// for a large subtree over stable data, not for a handful of widgets.
    /// Returns whether the content ran.
    #[track_caller]
    pub fn memo<S: core::hash::Hash>(
        &mut self,
        state: &S,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) -> bool {
        use core::hash::Hasher;
        let key = self.next_key();
        let mut hasher = crate::collections::StableHasher::new();
        state.hash(&mut hasher);
        let hash = hasher.finish();
        let existing = self.context.child(self.parent, &key, Kind::Memo);
        // Input waiting inside the subtree has to reach the widget that owns
        // it, so the frame that carries it is never skipped.
        let declare = existing.is_none_or(|id| {
            self.context.memo_hash(id) != Some(hash) || self.context.awaits_declaration(id)
        });
        if declare {
            self.group(key, Kind::Memo, style, content);
        } else {
            self.group(key, Kind::Memo, style, |_| {});
            self.context.count_skipped();
        }
        self.set_control(Control::Memo(hash));
        declare
    }

    /// A column that tells its content how much room it had on the last
    /// frame, so that a view can change with the space it is given: a wide
    /// window shows two panes, a narrow one shows tabs.
    ///
    /// The width is the content box, inside the padding, and it is zero on
    /// the frame the container appears, because nothing has been laid out
    /// yet. A view that changes its structure at a threshold therefore
    /// settles on the second frame. The width is returned as well.
    #[track_caller]
    pub fn responsive(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>, f32)) -> f32 {
        let key = self.next_key();
        let inside = self
            .context
            .child(self.parent, &key, Kind::Container)
            .and_then(|id| self.context.node(id))
            .map_or(0.0, |node| {
                (node.bounds.width - 2.0 * style.inset()).max(0.0)
            });
        self.group(key, Kind::Container, style, |ui| content(ui, inside));
        inside
    }

    /// Children one under the other.
    #[track_caller]
    pub fn column(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>)) {
        let key = self.next_key();
        self.group(key, Kind::Column, style, content);
    }

    /// Children side by side.
    #[track_caller]
    pub fn row(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>)) {
        let key = self.next_key();
        self.group(key, Kind::Row, style, content);
    }

    /// Children fill `style.columns` cells in row-major order, wrapping into
    /// implicit rows sized by `auto_rows`; a child's `area` places it explicitly,
    /// and its `area_name` places it in one of the container's `areas`.
    /// Items without a width stretch to their cell. Percentage widths resolve
    /// against the grid container, not the cell, so prefer `Length::Auto` to fill.
    #[track_caller]
    pub fn grid(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>)) {
        let key = self.next_key();
        if let Err(message) = crate::style::template_areas(&style.areas) {
            self.misuse(message);
            return;
        }
        self.group(key, Kind::Grid, style, content);
    }

    /// A vertical group whose children form a focus region: Tab cycles inside
    /// when `scope.trap` is set, focus returns to the previously focused node
    /// when a `scope.restore` group disappears, and re-entering by Tab lands on
    /// the last focused child. `scope.arrows` adds geometric arrow-key navigation.
    #[track_caller]
    pub fn focus_scope(
        &mut self,
        scope: crate::FocusScope,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let key = self.next_key();
        self.group(key, Kind::FocusScope, style, content);
        self.set_control(Control::FocusScope { scope });
    }

    /// A vertical group with its own background, padding, and rectangular clip.
    #[track_caller]
    pub fn container(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>)) {
        let key = self.next_key();
        self.group(key, Kind::Container, style, content);
    }

    /// A vertical scroll viewport. Give it a constrained height. Positive wheel
    /// deltas move toward the end; offsets survive keyed reconciliation.
    #[track_caller]
    pub fn scroll(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>)) {
        let key = self.next_key();
        self.group(key, Kind::Scroll, style, content);
    }

    /// Disables everything declared in `content` without changing widget identities.
    #[track_caller]
    pub fn enabled(&mut self, enabled: bool, content: impl FnOnce(&mut Ui<'_, M>)) {
        let previous = self.enabled;
        self.enabled &= enabled;
        content(self);
        self.enabled = previous;
    }
}

impl<M: 'static> Ui<'_, M> {
    /// A part of the application with a message of its own, in a container
    /// of its own: its messages reach this view's as `f(message)`. The
    /// parent forwards them back to [`Component::update`].
    #[track_caller]
    pub fn component<C: Component>(
        &mut self,
        style: Style,
        component: &C,
        f: impl Fn(C::Message) -> M + 'static,
    ) {
        let key = self.next_key();
        self.group(key, Kind::Container, style, |ui| {
            ui.map(f, |ui| component.view(ui));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Handler;
    use crate::collections::{FastMap, FastSet};
    use crate::{Button, Label, Length, Link, MonospaceMeasurer};
    use alloc::vec;
    use glassmere_scene::{Size, TextWrap};

    #[track_caller]
    fn here() -> &'static Location<'static> {
        Location::caller()
    }

    /// Places that differ by line only, by column only, by both and by file
    /// only, more of them than a scope counts in its array.
    fn places() -> Vec<&'static Location<'static>> {
        let mut places = vec![
            here(),
            here(),
            here(),
            here(),
            here(),
            here(),
            here(),
            here(),
            here(),
            here(),
        ];
        places.extend([here(), here(), here(), here()]);
        // Line 1, column 1 of two files.
        places.extend([
            include!("ui/places/first.rs"),
            include!("ui/places/second.rs"),
        ]);
        places
    }

    fn next_random(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn places_are_the_same_exactly_when_they_are_equal() {
        let places = places();
        assert!(places.len() > SITES_INLINE);
        let differ = |by: fn(&Location<'_>, &Location<'_>) -> bool| {
            places.iter().any(|a| places.iter().any(|b| by(a, b)))
        };
        assert!(differ(
            |a, b| a.line() != b.line() && a.column() == b.column()
        ));
        assert!(differ(
            |a, b| a.line() == b.line() && a.column() != b.column()
        ));
        assert!(differ(
            |a, b| a.line() != b.line() && a.column() != b.column()
        ));
        assert!(differ(|a, b| {
            a.line() == b.line() && a.column() == b.column() && a.file() != b.file()
        }));
        for a in &places {
            for b in &places {
                assert_eq!(same_site(a, b), a == b, "{a} and {b}");
            }
        }
    }

    #[test]
    fn calls_from_one_line_are_counted_in_order() {
        let mut sites = Sites::default();
        for expected in 0..20 {
            assert_eq!(sites.next(here()), expected);
        }
    }

    #[test]
    fn the_counts_of_the_first_places_carry_over_to_the_table() {
        let places = places();
        let mut sites = Sites::default();
        // The array holds the first few places, one more call each than the
        // one before.
        for (index, place) in places.iter().take(SITES_INLINE).enumerate() {
            for expected in 0..=index as u32 {
                assert_eq!(sites.next(place), expected);
            }
        }
        assert!(sites.spilled.is_none());
        // The next place does not fit and moves the counts into a table.
        for place in &places[SITES_INLINE..] {
            assert_eq!(sites.next(place), 0);
            assert_eq!(sites.next(place), 1);
        }
        assert!(sites.spilled.is_some());
        for (index, place) in places.iter().take(SITES_INLINE).enumerate() {
            assert_eq!(sites.next(place), index as u32 + 1);
        }
    }

    #[test]
    fn sites_number_calls_like_a_table_of_counts() {
        let places = places();
        let mut state = 0x9e37_79b9_7f4a_7c15;
        for round in 0..400 {
            let distinct = 1 + round % places.len();
            let mut sites = Sites::default();
            let mut table: FastMap<&Location<'_>, u32> = FastMap::default();
            for call in 0..next_random(&mut state) % 200 {
                let place = places[(next_random(&mut state) % distinct as u64) as usize];
                let count = table.entry(place).or_default();
                *count += 1;
                assert_eq!(
                    sites.next(place),
                    *count - 1,
                    "call {call} of round {round}"
                );
            }
        }
    }

    /// The first `size` keys of a family of names, panels and call sites.
    /// A name and a panel share their text, and a call site comes back with
    /// a higher count once the places are used up.
    fn alphabet(size: usize) -> Vec<WidgetKey> {
        let places = places();
        (0..size)
            .map(|index| {
                let at = index / 3;
                match index % 3 {
                    0 => WidgetKey::Named(format!("k{at}")),
                    1 => WidgetKey::Panel(format!("k{at}")),
                    _ => WidgetKey::Site(places[at % places.len()], (at / places.len()) as u32),
                }
            })
            .collect()
    }

    /// The hashes a scope is tried with: its own, one that is the same for
    /// every key, and one that tells only four keys apart.
    const DIGESTS: [fn(&KeyState, &WidgetKey) -> u64; 3] =
        [digest_of, |_, _| 0, |state, key| digest_of(state, key) & 3];

    /// What `Ui::push` does with a key: take it, and put its element in the
    /// scope if it was free.
    fn claim(
        keys: &mut Keys,
        elements: &mut Vec<Element>,
        key: &WidgetKey,
        digest: fn(&KeyState, &WidgetKey) -> u64,
    ) -> bool {
        let free = keys.claim_with(elements, key, digest);
        if free {
            elements.push(Element::new(key.clone(), Kind::Label));
        }
        free
    }

    #[test]
    fn keys_are_the_same_exactly_when_they_are_equal() {
        let keys = alphabet(90);
        for a in &keys {
            for b in &keys {
                assert_eq!(a.same(b), a == b, "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn a_call_site_hashes_by_its_content() {
        use core::hash::BuildHasher;
        // The same place at another address.
        let one = here();
        let other: &'static Location<'static> = Box::leak(Box::new(*one));
        assert_eq!(one, other);
        assert!(!core::ptr::eq(one, other), "the places are one location");
        let state = KeyState::default();
        let hash = |key: WidgetKey| state.hash_one(key);
        assert_eq!(
            hash(WidgetKey::Site(one, 3)),
            hash(WidgetKey::Site(other, 3))
        );
        let places = places();
        let base = WidgetKey::Site(places[0], 0);
        let mut seen = vec![hash(base.clone())];
        for key in [
            WidgetKey::Site(places[0], 1),
            WidgetKey::Site(places[1], 0),
            WidgetKey::Site(places[SITES_INLINE + 6], 0),
            WidgetKey::Site(places[SITES_INLINE + 7], 0),
        ] {
            assert_ne!(key, base);
            seen.push(hash(key));
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 5);
    }

    #[test]
    fn names_hash_by_their_kind_and_all_their_bytes() {
        use core::hash::BuildHasher;
        let state = KeyState::default();
        let hash = |key: WidgetKey| state.hash_one(key);
        assert_ne!(
            hash(WidgetKey::Named("a".into())),
            hash(WidgetKey::Panel("a".into()))
        );
        assert_ne!(
            hash(WidgetKey::Named("a".into())),
            hash(WidgetKey::Named("a\0".into()))
        );
        assert_ne!(
            hash(WidgetKey::Panel("abcdefgh".into())),
            hash(WidgetKey::Panel("abcdefgh\0\0".into()))
        );
    }

    #[test]
    fn a_scope_of_names_chosen_to_collide_costs_a_few_comparisons_each() {
        use crate::collections::tests::{one_bucket, one_hash, padded, pairs};
        use alloc::string::ToString;
        let rows = (0..10_000).map(|n| format!("row-{n}")).collect();
        let ids = (100_000_000..100_010_000u32)
            .map(|n| n.to_string())
            .collect();
        for (label, names) in [
            ("one hash", one_hash(10_000)),
            ("pairs", pairs(10_000)),
            ("one bucket", one_bucket(10_000)),
            ("padded", padded(10_000)),
            ("rows", rows),
            ("ids", ids),
        ] {
            let mut keys = Keys::default();
            let mut elements = Vec::new();
            KEY_COMPARISONS.with(|count| count.set(0));
            for name in names {
                assert!(claim(
                    &mut keys,
                    &mut elements,
                    &WidgetKey::Named(name),
                    digest_of
                ));
            }
            let compared = KEY_COMPARISONS.with(|count| count.get());
            assert!(compared < 20_000, "{label}: {compared} comparisons");
        }
    }

    #[test]
    fn keys_are_taken_exactly_like_the_members_of_a_set() {
        let mut state = 0x2545_f491_4f6c_dd1d;
        let (mut indexed, mut retired) = (0, 0);
        for digest in DIGESTS {
            for round in 0..300 {
                let alphabet = alphabet(1 + round % 120);
                let pick = |state: &mut u64| {
                    alphabet[(next_random(state) % alphabet.len() as u64) as usize].clone()
                };
                let mut keys = Keys::default();
                let mut elements: Vec<Element> = Vec::new();
                let mut set: FastSet<WidgetKey> = FastSet::default();
                let mut declared = None;
                for step in 0..next_random(&mut state) % 160 {
                    match next_random(&mut state) % 10 {
                        0 | 1 => {
                            // `Ui::take_declared`.
                            if let Some(index) = declared.take()
                                && index + 1 == elements.len()
                                && let Some(element) = elements.pop()
                            {
                                keys.retire_with(element.key, elements.len(), digest);
                            }
                        }
                        2 if step == 0 || next_random(&mut state).is_multiple_of(4) => {
                            // An element that no claim saw, like a popup's.
                            let key = pick(&mut state);
                            set.insert(key.clone());
                            elements.push(Element::new(key, Kind::Label));
                            declared = Some(elements.len() - 1);
                        }
                        _ => {
                            let key = pick(&mut state);
                            let free = set.insert(key.clone());
                            let taken = claim(&mut keys, &mut elements, &key, digest);
                            assert_eq!(taken, free, "{key:?} at step {step} of round {round}");
                            declared = free.then(|| elements.len() - 1);
                        }
                    }
                }
                // Everything the set holds is still taken, and nothing else.
                for key in &alphabet {
                    let free = set.insert(key.clone());
                    assert_eq!(claim(&mut keys, &mut elements, key, digest), free);
                }
                indexed += usize::from(keys.index.is_some());
                retired += usize::from(keys.retired.len() > KEYS_SCANNED);
            }
        }
        // The scopes were big enough to build a table, and to take many back.
        assert!(indexed > 300 && retired > 100, "{indexed} {retired}");
    }

    #[test]
    fn keys_whose_hashes_are_equal_are_told_apart_by_comparing_them() {
        let all = alphabet(40);
        let mut keys = Keys::default();
        let mut elements = Vec::new();
        for key in &all {
            assert!(claim(&mut keys, &mut elements, key, |_, _| 0), "{key:?}");
        }
        for key in &all {
            assert!(!claim(&mut keys, &mut elements, key, |_, _| 0), "{key:?}");
        }
        assert_eq!(elements.len(), all.len());
    }

    /// Whether the next claim in a scope of `elements` that no claim saw and
    /// `retired` taken back builds a table.
    fn builds_a_table(elements: usize, retired: usize) -> bool {
        let all = alphabet(elements + retired + 1);
        let mut keys = Keys::default();
        let scope: Vec<Element> = all[..elements]
            .iter()
            .map(|key| Element::new(key.clone(), Kind::Label))
            .collect();
        for key in &all[elements..elements + retired] {
            keys.retire(key.clone(), elements);
        }
        assert!(keys.index.is_none());
        assert!(keys.claim(&scope, &all[elements + retired]));
        keys.index.is_some()
    }

    #[test]
    fn a_table_is_built_when_the_elements_and_the_retired_keys_pass_the_limit() {
        assert!(!builds_a_table(KEYS_SCANNED, 0));
        assert!(builds_a_table(KEYS_SCANNED + 1, 0));
        assert!(!builds_a_table(0, KEYS_SCANNED));
        assert!(builds_a_table(0, KEYS_SCANNED + 1));
        assert!(!builds_a_table(3, KEYS_SCANNED - 3));
        assert!(builds_a_table(3, KEYS_SCANNED - 2));
        assert!(builds_a_table(2, 100));
    }

    /// Declares one widget with `declare` and compares the element it added
    /// with the one `Element::new` gives for the same key and kind once
    /// `sets` has written what the widget sets: every other field is its
    /// default. `children` is how many elements the widget holds, `handled`
    /// whether it was given a handler, and `seen` how many elements the
    /// declaration had before.
    fn declared_as(
        ui: &mut Ui<'_>,
        seen: &mut u32,
        (kind, children, handled): (Kind, usize, bool),
        sets: impl FnOnce(&mut Element),
        declare: impl FnOnce(&mut Ui<'_>),
    ) {
        let handler = ui.handlers.len() as u32;
        declare(ui);
        // A widget is counted after what it holds.
        *seen += children as u32 + 1;
        let element = ui.elements.last().unwrap();
        let mut want = Element::new(element.key.clone(), kind);
        want.handler = handled.then_some(handler);
        want.seq = *seen;
        sets(&mut want);
        want.enabled &= ui.enabled;
        let which = format!("{kind:?} with the scope enabled: {}", ui.enabled);
        assert_eq!(element.handler, want.handler, "{which}");
        assert_eq!(element.seq, want.seq, "{which}");
        assert_eq!(element.key, want.key, "{which}");
        assert_eq!(element.kind, want.kind, "{which}");
        assert_eq!(element.text, want.text, "{which}");
        assert_eq!(element.style, want.style, "{which}");
        assert_eq!(element.enabled, want.enabled, "{which}");
        assert_eq!(element.children.len(), children, "{which}");
        assert_eq!(element.custom, want.custom, "{which}");
        assert_eq!(element.tooltip, want.tooltip, "{which}");
        assert_eq!(element.control, want.control, "{which}");
    }

    /// One of each widget that is built in one of the five builders.
    fn declare_every_builder(ui: &mut Ui<'_>) {
        let own = Style::default().width(Length::Px(300.0)).gap(3.0);
        let mut seen = 0;
        let seen = &mut seen;
        declared_as(
            ui,
            seen,
            (Kind::Label, 0, false),
            |want| want.text = "plain".into(),
            |ui| ui.label("plain"),
        );
        declared_as(
            ui,
            seen,
            (Kind::Label, 0, false),
            |want| {
                want.text = "styled".into();
                want.style = own.clone();
            },
            |ui| ui.label(Label::new("styled").style(own.clone())),
        );
        declared_as(
            ui,
            seen,
            (Kind::Label, 0, false),
            |want| {
                want.text = "selectable".into();
                want.control = Control::SelectableText;
            },
            |ui| ui.label(Label::new("selectable").selectable()),
        );
        declared_as(
            ui,
            seen,
            (Kind::Button, 0, false),
            |want| {
                want.text = "Save".into();
                want.style = Style::button();
            },
            |ui| {
                ui.button("Save");
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Button, 0, true),
            |want| {
                want.text = "Send".into();
                want.style = own.clone();
            },
            |ui| {
                ui.button(Button::new("Send").style(own.clone()).on_click(()));
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Button, 0, false),
            |want| {
                want.text = "Off".into();
                want.style = Style::button();
                want.enabled = false;
            },
            |ui| {
                ui.button(Button::new("Off").enabled(false));
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Link, 0, false),
            |want| {
                want.text = "Docs".into();
                want.style = Style {
                    padding: 3.0,
                    radius: 3.0,
                    wrap: TextWrap::None,
                    ..Style::default()
                };
            },
            |ui| {
                ui.link("Docs");
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Link, 0, false),
            |want| {
                want.text = "Gone".into();
                want.style = Style {
                    wrap: TextWrap::None,
                    ..own.clone()
                };
                want.enabled = false;
            },
            |ui| {
                ui.link(Link::new("Gone").style(own.clone()).enabled(false));
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Checkbox, 0, true),
            |want| {
                want.text = "Agree".into();
                want.style = Style::button();
                want.control = Control::Checkbox {
                    label: "Agree".into(),
                    checked: true,
                };
            },
            |ui| {
                ui.checkbox("Agree", true, |_| ());
            },
        );
        // The checkbox of a property grid shows no text and has no handler.
        declared_as(
            ui,
            seen,
            (Kind::Checkbox, 0, false),
            |want| {
                want.text = "shown".into();
                want.style = Style::button();
                want.control = Control::Checkbox {
                    label: "Named".into(),
                    checked: false,
                };
            },
            |ui| {
                ui.checkbox_shown("Named", "shown", false, None);
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Radio, 0, true),
            |want| {
                want.text = "One".into();
                want.style = Style::button();
                want.control = Control::Radio {
                    label: "One".into(),
                    selected: false,
                };
            },
            |ui| {
                ui.radio("One", 1, 0, |_| ());
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Slider, 0, true),
            |want| {
                want.text = "Volume: 5.00".into();
                want.style = Style {
                    width: Length::Px(280.0),
                    height: Length::Px(64.0),
                    padding: 10.0,
                    ..Style::button()
                };
                want.control = Control::Slider {
                    label: "Volume".into(),
                    value: 5.0,
                    min: 0.0,
                    max: 10.0,
                    step: 1.0,
                    span: None,
                };
            },
            |ui| {
                ui.slider("Volume", 5.0, 0.0..=10.0, 1.0, |_| ());
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::TextInput, 0, true),
            |want| {
                want.style = Style {
                    wrap: TextWrap::None,
                    ..Ui::<()>::field_style()
                };
                want.control = Control::TextInput {
                    label: "Name".into(),
                    value: "ada".into(),
                    placeholder: String::new(),
                    number: None,
                    suggestions: None,
                    draft: false,
                    password: false,
                };
            },
            |ui| {
                ui.text_input("Name", "ada", |_| ());
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Row, 2, false),
            |want| want.style = own.clone(),
            |ui| {
                ui.row(own.clone(), |ui| {
                    ui.label("a");
                    ui.label("b");
                })
            },
        );
        declared_as(
            ui,
            seen,
            (Kind::Column, 1, false),
            |want| want.style = own.clone(),
            |ui| ui.column(own.clone(), |ui| ui.label("c")),
        );
        declared_as(
            ui,
            seen,
            (Kind::Container, 0, false),
            |_| {},
            |ui| ui.container(Style::default(), |_| {}),
        );
        declared_as(
            ui,
            seen,
            (Kind::Scroll, 1, false),
            |want| want.style = own.clone(),
            |ui| ui.scroll(own.clone(), |ui| ui.label("d")),
        );
    }

    // A widget left out for its key is not in the list and is not counted,
    // and what is declared after it is the one the calls that follow reach.
    #[test]
    fn a_duplicate_is_left_out_and_the_calls_after_it_reach_the_next_widget() {
        let mut context = Context::new();
        context.set_misuse_panics(false);
        let stats = context.run(Size::new(400.0, 400.0), &mut MonospaceMeasurer, |ui| {
            ui.keyed("a", |ui| ui.label("first"));
            assert_eq!(ui.declared, Some(0));
            ui.keyed("a", |ui| ui.label("again"));
            assert_eq!(ui.declared, None);
            assert!(ui.declared().is_none());
            assert_eq!(ui.elements.len(), 1);
            // There is nothing to give a handler or a control to.
            ui.set_handler(Handler::Message(()));
            ui.set_control(Control::SelectableText);
            assert!(ui.handlers.is_empty());
            assert_eq!(ui.elements[0].handler, None);
            assert_eq!(ui.elements[0].control, Control::None);
            assert!(ui.take_declared().is_none());

            ui.keyed("b", |ui| ui.label("second"));
            assert_eq!(ui.declared, Some(1));
            assert_eq!(ui.elements.len(), 2);
            assert_eq!(ui.elements[1].text, "second");
            // The duplicate was not counted.
            assert_eq!((ui.elements[0].seq, ui.elements[1].seq), (1, 2));
            ui.set_handler(Handler::Message(()));
            ui.set_control(Control::SelectableText);
            assert_eq!(ui.elements[0].handler, None);
            assert_eq!(ui.elements[0].control, Control::None);
            assert_eq!(ui.elements[1].handler, Some(0));
            assert_eq!(ui.elements[1].control, Control::SelectableText);

            let taken = ui.take_declared().unwrap();
            assert_eq!(taken.text, "second");
            assert_eq!(ui.elements.len(), 1);
            assert_eq!(ui.elements[0].text, "first");
            assert!(ui.declared.is_none());
        });
        assert_eq!(stats.elements, 2);
    }

    #[test]
    fn the_builders_that_write_every_field_give_the_base_element_with_what_they_set() {
        for scope in [None, Some(true), Some(false)] {
            let mut context = Context::new();
            context.run(
                Size::new(400.0, 4000.0),
                &mut MonospaceMeasurer,
                |ui| match scope {
                    None => declare_every_builder(ui),
                    Some(enabled) => ui.enabled(enabled, declare_every_builder),
                },
            );
        }
    }
}
