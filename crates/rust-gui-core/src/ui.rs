use crate::controls::{Control, ControlUpdate};
use crate::window::caption;
use crate::{Component, Context, CustomWidget, NodeId, Style};
use rust_gui_scene::Color;
use std::collections::HashMap;
use std::collections::HashSet;
use std::panic::Location;
use std::sync::Arc;
mod builders;
pub use builders::{Button, ComboBox, Label, TextInput};
mod collections;
mod controls;
mod dock;
mod fields;
pub use fields::suggest;
mod image;
mod overlays;
mod property_grid;
pub use property_grid::{Properties, PropertyGrid, PropertyGridMsg};
mod areas;
pub use areas::{AreaBuilder, DragSourceBuilder, DropTargetBuilder, GestureBuilder};
mod dialog;
mod rich_editor;
mod rich_text;
mod text_area;
mod toasts;
mod window;
pub(crate) use toasts::ToastOutbox;

pub use dock::{DockOptions, DockPanel, DockResponse};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WidgetKey {
    Named(String),
    /// Where in the source the widget was declared, and how many siblings
    /// declared from the same place came before it.
    Site(&'static Location<'static>, u32),
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
        }
    }
}

/// What a declaration hands to the host besides the tree.
#[derive(Default)]
pub(crate) struct HostOutbox {
    /// Custom events `Ui::emit` sent, with the node they came from.
    pub emitted: Vec<(crate::NodeId, crate::UiEvent)>,
    pub watched: Vec<std::path::PathBuf>,
    /// What `Ui::copy` put on the clipboard, and whether `Ui::request_paste`
    /// asked for what is on it.
    pub clip: Option<crate::Clip>,
    pub paste: bool,
    /// A file dialog to put up, notifications to post, and the native menu
    /// and tray icon as the view last declared them.
    pub files: Option<crate::FileRequest>,
    pub notifications: Vec<crate::Notification>,
    pub menu: Option<crate::Menu>,
    pub tray: Option<Option<crate::Tray>>,
    /// Commands a menu the framework drew itself chose.
    pub commands: Vec<String>,
    /// What a title bar of the application's own asked the window to do.
    pub window_commands: Vec<crate::WindowCommand>,
    pub images: Vec<std::path::PathBuf>,
    pub toasts: ToastOutbox,
    /// A new message source, or `Some(None)` to remove the current one.
    pub messages: Option<Option<Box<dyn crate::Messages>>>,
    /// New locale-aware formatting, or `Some(None)` to remove it.
    pub format: Option<Option<Box<dyn crate::LocaleFormat>>>,
    pub hot_state: Option<String>,
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
    Radio,
    Slider,
    ComboBox,
    MenuButton,
    Popup,
    Choice,
    Collection,
    CollectionRow,
    DragSource,
    DropTarget,
}

impl Kind {
    pub(crate) fn text_leaf(self) -> bool {
        matches!(
            self,
            Self::Label
                | Self::Button
                | Self::Checkbox
                | Self::Radio
                | Self::Choice
                | Self::MenuButton
                | Self::ComboBox
                | Self::Tab
                | Self::RichText
        )
    }
}

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
    pub tooltip: Option<String>,
    pub control: Control,
}

/// What a widget reports about itself. Reading [`Response::hovered`] or
/// [`Response::focused`] tells the framework the view depends on that state,
/// so a change in it declares the view again; a change in a state nobody
/// read only repaints.
#[derive(Clone, Copy, Default)]
pub struct Response<'a> {
    clicked: bool,
    pub hovered: bool,
    pub focused: bool,
    pub changed: bool,
    pub submitted: bool,
    observer: Option<(&'a Context, Observed)>,
}

/// The node a response is about, or the element that will become it.
#[derive(Clone, Copy)]
enum Observed {
    Node(NodeId),
    Element(u32),
}

impl std::fmt::Debug for Response<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("clicked", &self.clicked)
            .field("hovered", &self.hovered)
            .field("focused", &self.focused)
            .field("changed", &self.changed)
            .field("submitted", &self.submitted)
            .finish()
    }
}

impl Response<'_> {
    pub fn clicked(self) -> bool {
        self.clicked
    }

    /// Whether the pointer is over the widget; the view is declared again
    /// when that changes.
    pub fn hovered(&self) -> bool {
        self.observe();
        self.hovered
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
            clicked: self.clicked,
            hovered: self.hovered,
            focused: self.focused,
            changed: self.changed,
            submitted: self.submitted,
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
    pub(crate) parent: Option<NodeId>,
    /// The name [`Ui::keyed`] gave the next element declared, taken by the
    /// first one that asks for a key.
    pub(crate) pending_key: Option<String>,
    pub(crate) clicks: &'a mut HashSet<NodeId>,
    pub(crate) elements: Vec<Element>,
    pub(crate) keys: HashSet<WidgetKey>,
    /// Where in `elements` the last declaration went, or `None` when it was
    /// skipped as a misuse.
    pub(crate) declared: Option<usize>,
    /// How many widgets each source location has declared here so far.
    pub(crate) sites: HashMap<&'static Location<'static>, u32>,
    pub(crate) updates: &'a mut HashMap<NodeId, ControlUpdate>,
    pub(crate) submitted: &'a mut HashSet<NodeId>,
    pub(crate) warnings: &'a mut Vec<String>,
    /// The store `Ui::observe` asked for, read after the declaration so the
    /// graph shows what this frame read, whenever `observe` was called.
    pub(crate) observed: &'a mut Option<crate::Store>,
    pub(crate) theme_request: &'a mut Option<crate::Theme>,
    /// Text a copy or cut produced, handed to the host after the declaration.
    pub(crate) clipboard: &'a mut Option<String>,
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

    /// Switches the theme after this declaration; the host declares once more
    /// so token reads made earlier in the view see the new theme too.
    #[track_caller]
    pub fn set_theme(&mut self, theme: crate::Theme) {
        *self.theme_request = Some(theme);
    }

    /// Puts something on the clipboard once this declaration is over. The
    /// text goes to the system clipboard, the image with it where the
    /// platform can carry one, and any private bytes stay in this
    /// application.
    #[track_caller]
    pub fn copy(&mut self, clip: crate::Clip) {
        self.host.clip = Some(clip);
    }

    /// Asks for what is on the clipboard. It arrives on the next
    /// declaration, through `Ui::pasted`, because reading a clipboard is the
    /// host's business and can be slow.
    #[track_caller]
    pub fn request_paste(&mut self) {
        self.host.paste = true;
    }

    /// What the host last read from the clipboard, after
    /// [`Ui::request_paste`]. It stays readable until the next request, so a
    /// view can paste on the frame it asked and read it again later.
    #[track_caller]
    pub fn pasted(&self) -> Option<&crate::Clip> {
        self.context.pasted()
    }

    /// Puts up a file dialog once this declaration is over. The answer
    /// arrives through [`Ui::chosen_files`] on a later frame.
    #[track_caller]
    pub fn choose_file(&mut self, request: crate::FileRequest) {
        self.host.files = Some(request);
    }

    /// What the last file dialog answered: the paths chosen, or an empty
    /// slice when it was cancelled. `None` until one has been answered. It
    /// stays readable until the next dialog.
    #[track_caller]
    pub fn chosen_files(&self) -> Option<&[std::path::PathBuf]> {
        self.context.chosen_files()
    }

    /// Posts a message to the desktop's notification area.
    #[track_caller]
    pub fn notify(&mut self, notification: crate::Notification) {
        self.host.notifications.push(notification);
    }

    /// Files the operating system asked the application to open, from the
    /// command line or from an association. Empty when it was started
    /// without any.
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

    /// A named class from the installed stylesheet. Unknown names return the
    /// default style and are reported through `Context::style_warnings`.
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
    #[track_caller]
    pub fn watched_bytes(&mut self, path: impl AsRef<std::path::Path>) -> Option<Arc<[u8]>> {
        let path = path.as_ref();
        if !self.host.watched.iter().any(|known| known == path) {
            self.host.watched.push(path.to_owned());
        }
        self.context.watched_bytes(path)
    }

    /// `watched_bytes` for UTF-8 text files; `None` also for invalid UTF-8.
    #[track_caller]
    pub fn watched_text(&mut self, path: impl AsRef<std::path::Path>) -> Option<Arc<str>> {
        let path = path.as_ref();
        self.watched_bytes(path)?;
        self.context.watched_text(path)
    }

    /// Registers how to save the app's state for a development restart. The
    /// closure runs only when the host is about to restart the app; the text
    /// it returns is handed back by the host's `hot::saved_state` on the next
    /// start.
    #[track_caller]
    pub fn hot_state(&mut self, save: impl FnOnce() -> String) {
        if self.context.hot_save_requested() {
            self.host.hot_state = Some(save());
        }
    }

    /// Moves keyboard focus to the widget declared with `key` in this same
    /// container, once this declaration has been reconciled.
    #[track_caller]
    pub fn request_focus(&mut self, key: impl Into<String>) {
        *self.focus = Some((self.parent, WidgetKey::Named(key.into())));
    }

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
    fn push(&mut self, mut element: Element) -> Response<'a> {
        element.enabled &= self.enabled;
        // A background file becomes its image once the host has delivered it,
        // and that style change is what repaints the node.
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
        if !self.keys.insert(element.key.clone()) {
            self.misuse(format!(
                "duplicate widget key among siblings: {:?}",
                element.key
            ));
            return Response::default();
        }
        if let Some(crate::FontFamily::Named(name)) = &element.style.font_family {
            self.context.note_family(name);
        }
        element.seq = self.context.count_element(
            element.text.len()
                + match &element.key {
                    WidgetKey::Named(key) => key.len(),
                    WidgetKey::Site(..) => 0,
                },
        );
        let existing = self.context.child(self.parent, &element.key, element.kind);
        let context: &'a Context = self.context;
        let response = match existing {
            Some(id) => Response {
                clicked: self.clicks.remove(&id) && element.enabled,
                hovered: context.hovered() == Some(id),
                focused: context.focused() == Some(id),
                changed: false,
                submitted: element.enabled && self.submitted.remove(&id),
                observer: Some((context, Observed::Node(id))),
            },
            None => Response {
                observer: Some((context, Observed::Element(element.seq))),
                ..Response::default()
            },
        };
        self.declared = Some(self.elements.len());
        self.elements.push(element);
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

    /// Takes the element the last declaration added back out.
    #[track_caller]
    pub(crate) fn take_declared(&mut self) -> Option<Element> {
        let index = self.declared.take()?;
        (index + 1 == self.elements.len()).then(|| self.elements.pop())?
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
        let nth = self.sites.entry(site).or_default();
        *nth += 1;
        WidgetKey::Site(site, *nth - 1)
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
        self.leaf(
            key,
            Kind::Label,
            label.text,
            label.style.unwrap_or_default(),
            true,
        );
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
            parent: self.parent,
            pending_key: self.pending_key.take(),
            clicks: &mut *self.clicks,
            elements: std::mem::take(&mut self.elements),
            keys: std::mem::take(&mut self.keys),
            declared: self.declared,
            sites: std::mem::take(&mut self.sites),
            updates: &mut *self.updates,
            submitted: &mut *self.submitted,
            warnings: &mut *self.warnings,
            observed: &mut *self.observed,
            theme_request: &mut *self.theme_request,
            clipboard: &mut *self.clipboard,
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
            let f: std::rc::Rc<dyn Fn(N) -> M> = std::rc::Rc::new(f);
            self.handlers
                .extend(handlers.into_iter().map(|handler| handler.map(&f)));
        }
    }

    /// A `Ui` that declares the children of `parent`, sharing everything
    /// else with this one.
    pub(crate) fn scope(&mut self, parent: Option<NodeId>) -> Ui<'_, M> {
        Ui {
            context: self.context,
            parent,
            pending_key: None,
            clicks: self.clicks,
            elements: self.context.take_spare(),
            keys: HashSet::new(),
            declared: None,
            sites: HashMap::new(),
            updates: self.updates,
            submitted: self.submitted,
            warnings: self.warnings,
            observed: self.observed,
            theme_request: self.theme_request,
            clipboard: self.clipboard,
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
    pub fn memo<S: std::hash::Hash>(
        &mut self,
        state: &S,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) -> bool {
        use std::hash::Hasher;
        let key = self.next_key();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
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
                (node.bounds.width - 2.0 * style.padding.max(0.0)).max(0.0)
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
    /// implicit rows sized by `auto_rows`; a child's `area` places it explicitly.
    /// Items without a width stretch to their cell. Percentage widths resolve
    /// against the grid container, not the cell, so prefer `Length::Auto` to fill.
    #[track_caller]
    pub fn grid(&mut self, style: Style, content: impl FnOnce(&mut Ui<'_, M>)) {
        let key = self.next_key();
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

    // Disables an entire declaration scope without changing widget identities.
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
