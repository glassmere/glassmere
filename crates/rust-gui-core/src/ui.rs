use crate::controls::{Control, ControlUpdate};
use crate::{Component, Context, CustomWidget, NodeId, Style};
use std::collections::HashMap;
use std::collections::HashSet;
mod collections;
mod controls;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WidgetKey {
    Named(String),
    Position(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Column,
    Row,
    Container,
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
    pub(crate) enabled: bool,
}

impl Ui<'_> {
    pub fn drag_source(
        &mut self,
        key: impl Into<String>,
        label: impl Into<String>,
        payload: crate::DragPayload,
        style: Style,
        content: impl FnOnce(&mut Ui<'_>),
    ) {
        self.group(key.into(), Kind::DragSource, style, content);
        self.elements.last_mut().unwrap().control = Control::DragSource {
            label: label.into(),
            payload,
        };
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
        let response = self.button_styled(key, &command.label, command.enabled, Style::button());
        self.elements.last_mut().unwrap().control = Control::Command {
            shortcut: command.shortcut.clone(),
            while_editing: command.while_editing,
        };
        response
    }
    fn push(&mut self, mut element: Element) -> Response {
        element.enabled &= self.enabled;
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
            control: Control::None,
        });
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
