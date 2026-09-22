use crate::controls::{Control, ControlUpdate};
use crate::window::caption;
use crate::{Component, Context, CustomWidget, NodeId, Style};
use rust_gui_scene::Color;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
mod collections;
mod controls;
mod dock;
mod fields;
pub use fields::suggest;
mod image;
mod overlays;
mod property_grid;
pub use property_grid::{Properties, PropertyGrid};
mod rich_editor;
mod rich_text;
mod text_area;
mod toasts;
pub(crate) use toasts::ToastOutbox;

pub use dock::{DockOptions, DockPanel, DockResponse};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WidgetKey {
    Named(String),
    Position(usize),
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

#[derive(Clone, Copy, Debug, Default)]
pub struct Response {
    clicked: bool,
    pub hovered: bool,
    pub focused: bool,
    pub changed: bool,
    pub submitted: bool,
}

impl Response {
    pub fn clicked(self) -> bool {
        self.clicked
    }
}

/// Declarations are rebuilt on input; matching nodes and layout caches persist.
/// Explicit keys are scoped to the parent. Use them for conditional or reordered children.
pub struct Ui<'a> {
    pub(crate) context: &'a Context,
    pub(crate) parent: Option<NodeId>,
    pub(crate) clicks: &'a mut HashSet<NodeId>,
    pub(crate) elements: Vec<Element>,
    pub(crate) keys: HashSet<WidgetKey>,
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
}

impl Ui<'_> {
    /// Design tokens of the active theme, for application-owned surfaces.
    pub fn theme(&self) -> &crate::Theme {
        self.context.theme()
    }

    /// Switches the theme after this declaration; the host declares once more
    /// so token reads made earlier in the view see the new theme too.
    pub fn set_theme(&mut self, theme: crate::Theme) {
        *self.theme_request = Some(theme);
    }

    /// Puts something on the clipboard once this declaration is over. The
    /// text goes to the system clipboard, the image with it where the
    /// platform can carry one, and any private bytes stay in this
    /// application.
    pub fn copy(&mut self, clip: crate::Clip) {
        self.host.clip = Some(clip);
    }

    /// Asks for what is on the clipboard. It arrives on the next
    /// declaration, through `Ui::pasted`, because reading a clipboard is the
    /// host's business and can be slow.
    pub fn request_paste(&mut self) {
        self.host.paste = true;
    }

    /// What the host last read from the clipboard, after
    /// [`Ui::request_paste`]. It stays readable until the next request, so a
    /// view can paste on the frame it asked and read it again later.
    pub fn pasted(&self) -> Option<&crate::Clip> {
        self.context.pasted()
    }

    /// Puts up a file dialog once this declaration is over. The answer
    /// arrives through [`Ui::chosen_files`] on a later frame.
    pub fn choose_file(&mut self, request: crate::FileRequest) {
        self.host.files = Some(request);
    }

    /// What the last file dialog answered: the paths chosen, or an empty
    /// slice when it was cancelled. `None` until one has been answered. It
    /// stays readable until the next dialog.
    pub fn chosen_files(&self) -> Option<&[std::path::PathBuf]> {
        self.context.chosen_files()
    }

    /// Posts a message to the desktop's notification area.
    pub fn notify(&mut self, notification: crate::Notification) {
        self.host.notifications.push(notification);
    }

    /// The window's native menu bar. Declaring the same menu again changes
    /// nothing, so a view may set it every frame. Choosing an item sends
    /// `Event::Command` with that command's id, which activates the command
    /// the view declared, exactly as its button would.
    pub fn set_menu(&mut self, menu: crate::Menu) {
        self.host.menu = Some(menu);
    }

    /// Asks the window to move, resize, minimize, maximize or close itself.
    /// `Ui::title_bar`, `Ui::window_buttons` and `Ui::resize_frame` are made
    /// of these; this is for a frame of an entirely different shape.
    pub fn window_command(&mut self, command: crate::WindowCommand) {
        self.host.window_commands.push(command);
    }

    /// What the host says about the window: whether it is maximized and
    /// whether it has the keyboard. Of the last frame, like everything a
    /// declaration is given.
    pub fn window(&self) -> crate::WindowInfo {
        self.context.window_info()
    }

    /// The window's logical size, which is what `Ui::resize_frame` places
    /// its grips by.
    pub fn window_size(&self) -> rust_gui_scene::Size {
        self.context.size()
    }

    /// A strip that moves the window, for a window with the system's title
    /// bar turned off. Pressing it starts a move; pressing it twice
    /// maximizes or restores. Whatever is declared inside it keeps working,
    /// because a press that lands on a control there is that control's.
    ///
    /// ```ignore
    /// ui.title_bar("bar", Style::default().padding(6.0), |ui| {
    ///     ui.label("My application");
    ///     ui.window_buttons("buttons");
    /// });
    /// ```
    pub fn title_bar(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) {
        // A row, so that a title, a menu and the buttons sit side by side
        // the way a title bar's do.
        self.row(key.into(), style, content);
        self.elements.last_mut().unwrap().control = Control::TitleBar;
    }

    /// Minimize, maximize or restore, and close, as three buttons. The
    /// middle one says which it is from `Ui::window`.
    pub fn window_buttons(&mut self, key: impl Into<String>) {
        let maximized = self.window().maximized;
        let foreground = self.theme().text;
        // A wash of the text colour, which is how a desktop marks a caption
        // button: light over a dark title bar, dark over a light one.
        let wash = |alpha: u8| Color(foreground.0, foreground.1, foreground.2, alpha);
        let (hover, pressed) = (wash(caption::WASH_HOVER), wash(caption::WASH_PRESSED));
        self.row(key.into(), Style::default(), |ui| {
            for (key, glyph, name, command) in [
                (
                    "minimize",
                    caption::MINIMIZE,
                    "rust-gui-minimize",
                    crate::WindowCommand::Minimize,
                ),
                (
                    "maximize",
                    if maximized {
                        caption::RESTORE
                    } else {
                        caption::MAXIMIZE
                    },
                    if maximized {
                        "rust-gui-restore"
                    } else {
                        "rust-gui-maximize"
                    },
                    crate::WindowCommand::ToggleMaximize,
                ),
                (
                    "close",
                    caption::CLOSE,
                    "rust-gui-close",
                    crate::WindowCommand::Close,
                ),
            ] {
                // Closing is the one that is dangerous, and every desktop
                // says so the same way: red under the pointer, white on it.
                let closing = matches!(command, crate::WindowCommand::Close);
                let over = |background: Color| crate::Paint {
                    background: Some(background),
                    foreground: closing.then_some(Color::WHITE),
                    ..crate::Paint::default()
                };
                let style = Style {
                    width: crate::Length::Px(caption::WIDTH),
                    height: crate::Length::Px(caption::HEIGHT),
                    // The glyph sits in the middle of the box, which is the
                    // size the shell's own buttons are.
                    justify_content: crate::Justify::Center,
                    align_items: crate::Align::Center,
                    padding: 0.0,
                    radius: 0.0,
                    background: Some(Color(0, 0, 0, 0)),
                    foreground: Some(foreground),
                    font_family: caption::FONT
                        .map(|name| rust_gui_scene::FontFamily::Named((*name).to_owned())),
                    font_size: Some(caption::SIZE),
                    ..Style::default()
                }
                .on_hover(over(if closing { caption::CLOSE_HOVER } else { hover }))
                .on_pressed(over(if closing {
                    caption::CLOSE_PRESSED
                } else {
                    pressed
                }));
                let clicked = ui.button_styled(key, glyph, true, style).clicked();
                // The glyph is what is drawn; the name is what is announced.
                let label = ui.tr(name);
                ui.tooltip(label);
                if matches!(command, crate::WindowCommand::ToggleMaximize) {
                    // Windows asks where this one is, to offer its snap
                    // layouts when the pointer rests over it.
                    ui.elements.last_mut().unwrap().control = Control::MaximizeButton;
                }
                if clicked {
                    ui.window_command(command);
                }
            }
        });
    }

    /// The eight edges and corners of a window that has no system border,
    /// as narrow strips over everything else. Declare it once, at the end of
    /// the view, so it lies above the content; a press on one starts a
    /// resize. `thickness` is in logical pixels, and six is comfortable.
    ///
    /// The grips are placed from the size the window had last frame, so a
    /// resize moves them a frame later, which nothing can see while the
    /// platform is drawing the drag.
    pub fn resize_frame(&mut self, key: impl Into<String>, thickness: f32) {
        let size = self.window_size();
        let side = thickness.max(1.0);
        if size.width <= side * 2.0 || size.height <= side * 2.0 {
            return;
        }
        // Out of flow and over the whole window, so that the grips sit on
        // top of the content and displace none of it.
        let frame = Style {
            offset: Some((0.0, 0.0)),
            width: crate::Length::Px(size.width),
            height: crate::Length::Px(size.height),
            ..Style::default()
        };
        self.group(key.into(), Kind::Container, frame, |ui| {
            for edge in crate::ResizeEdge::ALL {
                use crate::ResizeEdge::{
                    East, North, NorthEast, NorthWest, South, SouthEast, SouthWest, West,
                };
                let (x, y, width, height) = match edge {
                    North => (side, 0.0, size.width - side * 2.0, side),
                    South => (side, size.height - side, size.width - side * 2.0, side),
                    West => (0.0, side, side, size.height - side * 2.0),
                    East => (size.width - side, side, side, size.height - side * 2.0),
                    NorthWest => (0.0, 0.0, side, side),
                    NorthEast => (size.width - side, 0.0, side, side),
                    SouthWest => (0.0, size.height - side, side, side),
                    SouthEast => (size.width - side, size.height - side, side, side),
                };
                ui.group(
                    edge.name().to_owned(),
                    Kind::Container,
                    Style {
                        offset: Some((x, y)),
                        width: crate::Length::Px(width.max(0.0)),
                        height: crate::Length::Px(height.max(0.0)),
                        ..Style::default()
                    },
                    |_| {},
                );
                ui.elements.last_mut().unwrap().control = Control::ResizeGrip(edge);
            }
        });
    }

    /// Draws `menu` as a menu bar, for the platforms where a menu belongs
    /// to the window rather than to the application. Choosing an item
    /// activates that command, exactly as the platform's own menu does, so a
    /// view handles both the same way: through the command it declared.
    ///
    /// Two levels are shown. A submenu inside a submenu is drawn as a
    /// heading with its items under it, because the framework's own menus
    /// do not nest.
    pub fn menu_bar(&mut self, key: impl Into<String>, menu: &crate::Menu) {
        let style = Style {
            gap: 4.0,
            ..Style::default()
        };
        let mut chosen = None;
        self.row(key.into(), style, |ui| {
            for (index, entry) in menu.entries.iter().enumerate() {
                let crate::MenuEntry::Submenu { label, entries } = entry else {
                    continue;
                };
                let mut flat = Vec::new();
                crate::platform::flatten(entries, 0, &mut flat);
                let items: Vec<crate::MenuItem> = flat
                    .iter()
                    .map(|(depth, command)| match command {
                        Some(command) => crate::MenuItem {
                            label: format!("{}{}", "  ".repeat(*depth), command.label),
                            enabled: command.enabled,
                            hint: command
                                .shortcut
                                .as_ref()
                                .map(crate::Shortcut::describe)
                                .unwrap_or_default(),
                        },
                        None => crate::MenuItem {
                            label: "─".repeat(12),
                            enabled: false,
                            hint: String::new(),
                        },
                    })
                    .collect();
                if let Some(picked) = ui.menu_styled(
                    format!("menu{index}"),
                    label.clone(),
                    &items,
                    Style {
                        padding: 8.0,
                        ..Style::button()
                    },
                    crate::Length::Px(260.0),
                ) && let Some((_, Some(command))) = flat.get(picked)
                {
                    chosen = Some(command.id.clone());
                }
            }
        });
        if let Some(id) = chosen {
            self.host.commands.push(id);
        }
    }

    /// The application's tray icon, or `None` to take it away.
    pub fn set_tray(&mut self, tray: Option<crate::Tray>) {
        self.host.tray = Some(tray);
    }

    /// Files the operating system asked the application to open, from the
    /// command line or from an association. Empty when it was started
    /// without any.
    pub fn opened_files(&self) -> &[std::path::PathBuf] {
        self.context.opened_files()
    }

    /// Shows a reactive store's dependency graph in the inspector: what its
    /// values are, what each derived one read, and which moved last. The
    /// store is otherwise none of the framework's business, so nothing is
    /// observed unless this is called.
    pub fn observe(&mut self, store: &crate::Store) {
        *self.observed = Some(store.clone());
    }

    /// A named class from the installed stylesheet. Unknown names return the
    /// default style and are reported through `Context::style_warnings`.
    /// Asks the host for another frame after this one, for work that spreads
    /// over frames without an input event or animation driving it.
    pub fn request_redraw(&mut self) {
        *self.redraw = true;
    }

    /// Content of a file the host keeps current: it is loaded after the first
    /// declaration that asks for it, reloaded when it changes on disk, and a
    /// redraw follows each change. `None` until the first load; a failed
    /// reload keeps the last good content (see `Context::watch_error`).
    pub fn watched_bytes(&mut self, path: impl AsRef<std::path::Path>) -> Option<Arc<[u8]>> {
        let path = path.as_ref();
        if !self.host.watched.iter().any(|known| known == path) {
            self.host.watched.push(path.to_owned());
        }
        self.context.watched_bytes(path)
    }

    /// `watched_bytes` for UTF-8 text files; `None` also for invalid UTF-8.
    pub fn watched_text(&mut self, path: impl AsRef<std::path::Path>) -> Option<Arc<str>> {
        let path = path.as_ref();
        self.watched_bytes(path)?;
        self.context.watched_text(path)
    }

    /// Registers how to save the app's state for a development restart. The
    /// closure runs only when the host is about to restart the app; the text
    /// it returns is handed back by the host's `hot::saved_state` on the next
    /// start.
    pub fn hot_state(&mut self, save: impl FnOnce() -> String) {
        if self.context.hot_save_requested() {
            self.host.hot_state = Some(save());
        }
    }

    /// Moves keyboard focus to the widget declared with `key` in this same
    /// container, once this declaration has been reconciled.
    pub fn request_focus(&mut self, key: impl Into<String>) {
        *self.focus = Some((self.parent, WidgetKey::Named(key.into())));
    }

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

    /// Drags inside the window are delivered to a `drop_target`. A host that
    /// can hands a drag that leaves the window to the operating system (files
    /// only today); how that ended is returned here, once.
    pub fn drag_source(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        payload: crate::DragPayload,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) -> Option<crate::DragOutcome> {
        let key = key.into();
        let update = self
            .context
            .child(
                self.parent,
                &WidgetKey::Named(key.clone()),
                Kind::DragSource,
            )
            .and_then(|id| self.updates.remove(&id));
        self.group(key, Kind::DragSource, style, content);
        self.elements.last_mut().unwrap().control = Control::DragSource {
            label: label.into(),
            payload,
        };
        // Reported once, after a drag that left the window has ended.
        match update {
            Some(ControlUpdate::DragEnded(outcome)) => Some(outcome),
            _ => None,
        }
    }

    /// A surface that fingers move, scale and turn: a canvas, a map, a photo.
    /// Reports what happened since the last declaration; the app applies it
    /// to its own view state. Controls inside are pressed as anywhere else.
    /// A pinch on a trackpad and Ctrl with the wheel arrive as `scale`.
    pub fn gesture_area(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) -> crate::GestureResponse {
        let key = key.into();
        let id = self.context.child(
            self.parent,
            &WidgetKey::Named(key.clone()),
            Kind::GestureArea,
        );
        let mut response = match id.and_then(|id| self.updates.remove(&id)) {
            Some(ControlUpdate::Gesture(gesture)) if self.enabled => gesture,
            _ => crate::GestureResponse::default(),
        };
        if let Some(id) = id {
            (response.active, response.pressure) = self.context.gesture_contacts(id);
        }
        self.group(key, Kind::GestureArea, style, content);
        response
    }

    pub fn drop_target(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        accepts: crate::DropKind,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) -> Option<crate::DragPayload> {
        let key = key.into();
        let update = self
            .context
            .child(
                self.parent,
                &WidgetKey::Named(key.clone()),
                Kind::DropTarget,
            )
            .and_then(|id| self.updates.remove(&id));
        let payload = match update {
            Some(ControlUpdate::Drop(payload)) if self.enabled && payload.kind() == accepts => {
                Some(payload)
            }
            _ => None,
        };
        self.group(key, Kind::DropTarget, style, content);
        self.elements.last_mut().unwrap().control = Control::DropTarget {
            label: label.into(),
            accepts,
        };
        payload
    }
    /// One command declaration per window. Its shortcut and button share the
    /// same enabled check and one-shot clicked response.
    pub fn command_button(&mut self, key: impl Into<String>, command: &crate::Command) -> Response {
        let command = self.command(command);
        let response = self.button_styled(key, &command.label, command.enabled, Style::button());
        self.elements.last_mut().unwrap().control = Control::Command {
            id: command.id,
            shortcut: command.shortcut,
            while_editing: command.while_editing,
            scoped: command.scoped,
        };
        response
    }

    /// The command as the user's bindings leave it. `command_button` applies
    /// this itself; menus and palettes go through it to show the shortcut the
    /// user will actually press.
    pub fn command(&self, command: &crate::Command) -> crate::Command {
        match self.context.binding(&command.id) {
            Some(shortcut) => crate::Command {
                shortcut: shortcut.clone(),
                ..command.clone()
            },
            None => command.clone(),
        }
    }
    fn push(&mut self, mut element: Element) -> Response {
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
        assert!(
            self.keys.insert(element.key.clone()),
            "duplicate widget key among siblings: {:?}",
            element.key
        );
        if let Some(crate::FontFamily::Named(name)) = &element.style.font_family {
            self.context.note_family(name);
        }
        self.context.count_element(
            element.text.len()
                + match &element.key {
                    WidgetKey::Named(key) => key.len(),
                    WidgetKey::Position(_) => 0,
                },
        );
        let existing = self.context.child(self.parent, &element.key, element.kind);
        let response = existing
            .map(|id| Response {
                clicked: self.clicks.remove(&id) && element.enabled,
                hovered: self.context.hovered() == Some(id),
                focused: self.context.focused() == Some(id),
                changed: false,
                submitted: element.enabled && self.submitted.remove(&id),
            })
            .unwrap_or_default();
        self.elements.push(element);
        response
    }

    fn leaf(
        &mut self,
        key: WidgetKey,
        kind: Kind,
        text: String,
        style: Style,
        enabled: bool,
    ) -> Response {
        self.push(Element {
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

    pub fn label(&mut self, text: impl Into<String>) {
        self.leaf(
            WidgetKey::Position(self.elements.len()),
            Kind::Label,
            text.into(),
            Style::default(),
            true,
        );
    }

    pub fn label_keyed(&mut self, key: impl Into<String>, text: impl Into<String>) {
        self.label_styled(key, text, Style::default());
    }

    pub fn label_styled(&mut self, key: impl Into<String>, text: impl Into<String>, style: Style) {
        self.leaf(
            WidgetKey::Named(key.into()),
            Kind::Label,
            text.into(),
            style,
            true,
        );
    }

    pub fn button(&mut self, text: impl Into<String>) -> Response {
        self.leaf(
            WidgetKey::Position(self.elements.len()),
            Kind::Button,
            text.into(),
            Style::button(),
            true,
        )
    }

    pub fn button_keyed(&mut self, key: impl Into<String>, text: impl Into<String>) -> Response {
        self.button_styled(key, text, true, Style::button())
    }

    pub fn button_styled(
        &mut self,
        key: impl Into<String>,
        text: impl Into<String>,
        enabled: bool,
        style: Style,
    ) -> Response {
        self.leaf(
            WidgetKey::Named(key.into()),
            Kind::Button,
            text.into(),
            style,
            enabled,
        )
    }

    fn group(&mut self, key: String, kind: Kind, style: Style, content: impl FnOnce(&mut Ui<'_>)) {
        let key = WidgetKey::Named(key);
        let parent = self.context.child(self.parent, &key, kind);
        let mut child = Ui {
            context: self.context,
            parent,
            clicks: self.clicks,
            elements: self.context.take_spare(),
            keys: HashSet::new(),
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
        };
        content(&mut child);
        let children = child.elements;
        self.push(Element {
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
    pub fn memo<S: std::hash::Hash>(
        &mut self,
        key: impl Into<String>,
        state: &S,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) -> bool {
        use std::hash::Hasher;
        let key = key.into();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut hasher);
        let hash = hasher.finish();
        let existing = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::Memo);
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
        self.elements.last_mut().unwrap().control = Control::Memo(hash);
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
    pub fn responsive(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>, f32),
    ) -> f32 {
        let key = key.into();
        let inside = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::Container)
            .and_then(|id| self.context.node(id))
            .map_or(0.0, |node| {
                (node.bounds.width - 2.0 * style.padding.max(0.0)).max(0.0)
            });
        self.group(key, Kind::Container, style, |ui| content(ui, inside));
        inside
    }

    /// A column that watches the events of the widgets inside it, and may
    /// stop them. What reached it since the last declaration comes back in
    /// the order it arrived.
    ///
    /// A capturing area sees an event before the widgets inside, so consuming
    /// it keeps it from them; an ordinary one sees what they did not take, on
    /// the way up. Areas nest: outermost first going down, innermost first
    /// coming up, and a consumed event stops there.
    pub fn event_area(
        &mut self,
        key: impl Into<String>,
        area: crate::EventArea,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) -> Vec<crate::UiEvent> {
        let key = key.into();
        let arrived = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::EventArea)
            .map(|id| self.context.take_area_events(id))
            .unwrap_or_default();
        self.group(key, Kind::EventArea, style, content);
        self.elements.last_mut().unwrap().control = Control::EventArea(area);
        arrived
    }

    /// Tells the areas above this point something. The event travels up from
    /// the container being declared and arrives on the next declaration, like
    /// every other input.
    pub fn emit(&mut self, name: impl Into<String>, data: impl Into<String>) {
        if let Some(parent) = self.parent {
            self.host.emitted.push((
                parent,
                crate::UiEvent::Custom {
                    name: name.into(),
                    data: data.into(),
                },
            ));
        }
    }

    pub fn column(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) {
        self.group(key.into(), Kind::Column, style, content);
    }

    pub fn row(&mut self, key: impl Into<String>, style: Style, content: impl FnOnce(&mut Ui<'_>)) {
        self.group(key.into(), Kind::Row, style, content);
    }

    /// Children fill `style.columns` cells in row-major order, wrapping into
    /// implicit rows sized by `auto_rows`; a child's `area` places it explicitly.
    /// Items without a width stretch to their cell. Percentage widths resolve
    /// against the grid container, not the cell, so prefer `Length::Auto` to fill.
    pub fn grid(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) {
        self.group(key.into(), Kind::Grid, style, content);
    }

    /// A vertical group whose children form a focus region: Tab cycles inside
    /// when `scope.trap` is set, focus returns to the previously focused node
    /// when a `scope.restore` group disappears, and re-entering by Tab lands on
    /// the last focused child. `scope.arrows` adds geometric arrow-key navigation.
    pub fn focus_scope(
        &mut self,
        key: impl Into<String>,
        scope: crate::FocusScope,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) {
        self.group(key.into(), Kind::FocusScope, style, content);
        self.elements.last_mut().unwrap().control = Control::FocusScope { scope };
    }

    /// A dialog layer over the whole window while `*open`. Modal dialogs paint a
    /// scrim, block pointer and wheel input behind them and trap Tab; focus
    /// moves inside on open and returns to the previously focused node on close.
    /// Escape or a scrim click clears `*open` and reports `dismissed` once.
    /// `content` receives the open flag so buttons inside can close the dialog.
    /// Dialog keys are unique across the whole view, and later dialogs stack on top.
    pub fn dialog(
        &mut self,
        key: impl Into<String>,
        open: &mut bool,
        options: crate::DialogOptions,
        content: impl FnOnce(&mut Ui<'_>, &mut bool),
    ) -> crate::DialogResponse {
        let key = WidgetKey::Named(format!("$dialog:{}", key.into()));
        let layer = self
            .context
            .child(Some(self.context.root()), &key, Kind::Dialog);
        let dismissed = self.enabled
            && layer
                .is_some_and(|id| matches!(self.updates.remove(&id), Some(ControlUpdate::Dismiss)));
        if dismissed {
            *open = false;
        }
        if !*open {
            return crate::DialogResponse { dismissed };
        }
        let popup = self.context.theme().popup;
        let mut panel = options.style;
        panel.width = options.width;
        if panel.background.is_none() {
            panel.background = Some(popup);
        }
        if panel.radius == 0.0 {
            panel.radius = 12.0;
        }
        if panel.padding == 0.0 {
            panel.padding = 20.0;
        }
        let index = self.layers.len();
        let mut child = Ui {
            context: self.context,
            parent: layer,
            clicks: self.clicks,
            elements: self.context.take_spare(),
            keys: HashSet::new(),
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
        };
        let title = options.title.clone();
        child.group("$panel".into(), Kind::Container, panel, |ui| {
            if !title.is_empty() {
                ui.label_styled(
                    "$title",
                    title.clone(),
                    Style {
                        font_size: Some(20.0),
                        font_weight: Some(700),
                        ..Style::default()
                    },
                );
            }
            content(ui, open);
        });
        let element = Element {
            key,
            kind: Kind::Dialog,
            text: String::new(),
            style: Style {
                width: crate::Length::Percent(1.0),
                height: crate::Length::Percent(1.0),
                align_items: crate::Align::Center,
                justify_content: crate::Justify::Center,
                gap: 0.0,
                ..Style::default()
            },
            enabled: self.enabled,
            children: child.elements,
            custom: None,
            tooltip: None,
            control: Control::Dialog {
                title: options.title,
                modal: options.modal,
                dismissible: options.dismissible,
            },
        };
        // Nested dialogs declared inside `content` were pushed meanwhile; keep
        // this one below them so children stack on top of their parents.
        self.layers.insert(index, element);
        crate::DialogResponse { dismissed }
    }

    /// A vertical group with its own background, padding, and rectangular clip.
    pub fn container(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) {
        self.group(key.into(), Kind::Container, style, content);
    }

    /// A vertical scroll viewport. Give it a constrained height. Positive wheel
    /// deltas move toward the end; offsets survive keyed reconciliation.
    pub fn scroll(
        &mut self,
        key: impl Into<String>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) {
        self.group(key.into(), Kind::Scroll, style, content);
    }

    pub fn component(
        &mut self,
        key: impl Into<String>,
        style: Style,
        component: &mut impl Component,
    ) {
        self.group(key.into(), Kind::Container, style, |ui| component.ui(ui));
    }

    /// A widget of the application's own, with the input, state, style and
    /// semantics a built-in one has. The returned `CustomWidget` says how big
    /// it is, what it paints, what it is for and what it listens to; what it
    /// heard last time is handed to `build` first, with pointer positions in
    /// the widget's own coordinates.
    ///
    /// The widget keeps its state where the application keeps the rest: in a
    /// struct of its own, passed in. Styling is `ui.theme()` and
    /// `ui.class(..)` like anywhere else, and the `style` here is the node's,
    /// so padding, size and background work as they do on a container.
    pub fn widget(
        &mut self,
        key: impl Into<String>,
        style: Style,
        build: impl FnOnce(&crate::WidgetInput) -> CustomWidget,
    ) -> Response {
        let key = WidgetKey::Named(key.into());
        let mut input = crate::WidgetInput {
            enabled: self.enabled,
            ..crate::WidgetInput::default()
        };
        if let Some(id) = self.context.child(self.parent, &key, Kind::Custom) {
            let bounds = self
                .context
                .node(id)
                .map_or_else(rust_gui_scene::Rect::default, |node| node.bounds);
            input.size = rust_gui_scene::Size::new(bounds.width, bounds.height);
            input.hovered = self.context.hovered() == Some(id);
            input.focused = self.context.focused() == Some(id);
            input.pressed = self.context.pressed(id);
            input.events = self
                .context
                .take_area_events(id)
                .into_iter()
                .map(|event| crate::WidgetInput::local(event, bounds))
                .collect();
        }
        let widget = build(&input);
        widget.validate();
        self.push(Element {
            key,
            kind: Kind::Custom,
            text: String::new(),
            style,
            enabled: !widget.semantics.disabled,
            children: Vec::new(),
            custom: Some(widget),
            tooltip: None,
            control: Control::None,
        })
    }

    pub fn custom(
        &mut self,
        key: impl Into<String>,
        style: Style,
        widget: CustomWidget,
    ) -> Response {
        widget.validate();
        self.push(Element {
            key: WidgetKey::Named(key.into()),
            kind: Kind::Custom,
            text: String::new(),
            style,
            enabled: !widget.semantics.disabled,
            children: Vec::new(),
            custom: Some(widget),
            tooltip: None,
            control: Control::None,
        })
    }

    /// Disables an entire declaration scope without changing widget identities.
    pub fn enabled(&mut self, enabled: bool, content: impl FnOnce(&mut Ui<'_>)) {
        let previous = self.enabled;
        self.enabled &= enabled;
        content(self);
        self.enabled = previous;
    }
}
