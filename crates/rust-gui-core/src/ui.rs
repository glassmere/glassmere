use crate::controls::{Control, ControlUpdate};
use crate::{Component, Context, CustomWidget, NodeId, Style};
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
            elements: Vec::new(),
            keys: HashSet::new(),
            updates: self.updates,
            submitted: self.submitted,
            warnings: self.warnings,
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
            elements: Vec::new(),
            keys: HashSet::new(),
            updates: self.updates,
            submitted: self.submitted,
            warnings: self.warnings,
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
