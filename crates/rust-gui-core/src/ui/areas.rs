use super::*;

impl<'a, M: 'static> Ui<'a, M> {
    /// A widget of the application's own, with the input, state, style and
    /// semantics a built-in one has. The `CustomWidget` that `widget`'s
    /// build function returns says how big it is, what it paints, what it is
    /// for and what it listens to. With [`crate::WidgetBuilder::on_event`],
    /// each event that reaches it is handed to that function when it is
    /// resolved, in the widget's own coordinates, and what it returns goes
    /// to `update`.
    ///
    /// The widget keeps its state where the application keeps the rest: in a
    /// struct of its own, passed in. Styling is `ui.theme()` and
    /// `ui.class(..)` like anywhere else, and the `style` here is the node's,
    /// so padding, size and background work as they do on a container.
    #[track_caller]
    pub fn widget<F>(&mut self, style: Style, widget: crate::WidgetBuilder<F, M>) -> Response<'a>
    where
        F: FnOnce(&crate::WidgetInput) -> CustomWidget,
    {
        let crate::WidgetBuilder { build, on_event } = widget;
        let key = self.next_key();
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
        }
        let widget = build(&input);
        if let Err(message) = widget.validate() {
            self.misuse(message);
            return Response::default();
        }
        let response = self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind: Kind::Custom,
            text: String::new(),
            style,
            enabled: !widget.semantics.disabled,
            children: Vec::new(),
            custom: Some(widget),
            tooltip: None,
            control: Control::None,
        });
        // What reaches a widget that listens is taken when it is resolved,
        // so a widget without `on_event` still has a handler that drops it.
        let listens = self
            .declared()
            .and_then(|element| element.custom.as_ref())
            .is_some_and(|widget| widget.input != crate::EventArea::default());
        match on_event {
            Some(on_event) => self.set_handler(crate::app::Handler::Event(on_event)),
            None if listens => self.set_handler(crate::app::Handler::Event(Box::new(|_| None))),
            None => {}
        }
        // `build` was handed the hover, focus and press, so a change in them
        // declares again.
        response.observe();
        response
    }

    /// A leaf the application paints, which sends `on_click` when it is
    /// activated: a click, or Enter or Space on it, or an accessibility
    /// Activate. A leaf that only paints is a [`Ui::widget`].
    #[track_caller]
    pub fn custom(&mut self, style: Style, widget: CustomWidget, on_click: M) -> Response<'a> {
        if let Err(message) = widget.validate() {
            self.misuse(message);
            return Response::default();
        }
        let key = self.next_key();
        let response = self.push(Element {
            handler: None,
            seq: 0,
            key,
            kind: Kind::Custom,
            text: String::new(),
            style,
            enabled: !widget.semantics.disabled,
            children: Vec::new(),
            custom: Some(widget),
            tooltip: None,
            control: Control::None,
        });
        self.set_handler(crate::app::Handler::Message(on_click));
        response
    }
}

/// A drag source as [`Ui::drag_source`] declares it: what it drags, and
/// the message sent when a drag it started ends outside the window.
pub struct DragSourceBuilder<M = ()> {
    label: String,
    payload: crate::DragPayload,
    on_drag_end: Option<Box<dyn Fn(crate::DragOutcome) -> M>>,
}

impl<M> DragSourceBuilder<M> {
    pub fn new(label: impl Into<String>, payload: crate::DragPayload) -> Self {
        Self {
            label: label.into(),
            payload,
            on_drag_end: None,
        }
    }

    /// The message a drag that left the window sends once it has ended:
    /// `Dropped` when another application took it, `Cancelled` otherwise.
    pub fn on_drag_end(mut self, message: impl Fn(crate::DragOutcome) -> M + 'static) -> Self {
        self.on_drag_end = Some(Box::new(message));
        self
    }
}

/// A drop target as [`Ui::drop_target`] declares it.
pub struct DropTargetBuilder<M = ()> {
    label: String,
    accepts: crate::DropKind,
    on_drop: Option<Box<dyn Fn(crate::DragPayload) -> M>>,
}

impl<M> DropTargetBuilder<M> {
    pub fn new(label: impl Into<String>, accepts: crate::DropKind) -> Self {
        Self {
            label: label.into(),
            accepts,
            on_drop: None,
        }
    }

    /// The message a payload of the accepted kind, dropped on the target,
    /// sends.
    pub fn on_drop(mut self, message: impl Fn(crate::DragPayload) -> M + 'static) -> Self {
        self.on_drop = Some(Box::new(message));
        self
    }
}

/// A gesture area as [`Ui::gesture_area`] declares it.
pub struct GestureBuilder<M = ()> {
    on_gesture: Option<Box<dyn Fn(crate::GestureResponse) -> M>>,
}

impl<M> Default for GestureBuilder<M> {
    fn default() -> Self {
        Self { on_gesture: None }
    }
}

impl<M> GestureBuilder<M> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The message sent for what the fingers, the pointer or the trackpad did
    /// since the last one. Several changes in one batch arrive as one message.
    pub fn on_gesture(mut self, message: impl Fn(crate::GestureResponse) -> M + 'static) -> Self {
        self.on_gesture = Some(Box::new(message));
        self
    }
}

/// An event area as [`Ui::event_area`] declares it: what it watches, and
/// the message its events send.
pub struct AreaBuilder<M = ()> {
    area: crate::EventArea,
    on_event: Option<Box<dyn Fn(crate::UiEvent) -> Option<M>>>,
}

impl<M> AreaBuilder<M> {
    pub fn new(area: crate::EventArea) -> Self {
        Self {
            area,
            on_event: None,
        }
    }

    /// The message an event that reached the area sends, or none for one it
    /// does not take. Pointer positions are in the area's own coordinates.
    pub fn on_event(mut self, on_event: impl Fn(crate::UiEvent) -> Option<M> + 'static) -> Self {
        self.on_event = Some(Box::new(on_event));
        self
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// Content that drags `source`'s payload. Drags inside the window are
    /// delivered to a `drop_target`. A host that can hands a drag that leaves
    /// the window to the operating system (files only today); how that ended
    /// is sent through [`DragSourceBuilder::on_drag_end`].
    #[track_caller]
    pub fn drag_source(
        &mut self,
        source: DragSourceBuilder<M>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let DragSourceBuilder {
            label,
            payload,
            on_drag_end,
        } = source;
        let key = self.next_key();
        self.group(key, Kind::DragSource, style, content);
        self.set_control(Control::DragSource { label, payload });
        if let Some(on_drag_end) = on_drag_end {
            self.set_handler(crate::app::Handler::Map(Box::new(
                move |update| match update {
                    ControlUpdate::DragEnded(outcome) => Some(on_drag_end(outcome)),
                    _ => None,
                },
            )));
        }
    }

    /// A surface that fingers move, scale and turn: a canvas, a map, a photo.
    /// What happened is sent through [`GestureBuilder::on_gesture`]; the app
    /// applies it to its own view state. Controls inside are pressed as
    /// anywhere else. A pinch on a trackpad and Ctrl with the wheel arrive as
    /// `scale`.
    #[track_caller]
    pub fn gesture_area(
        &mut self,
        gesture: GestureBuilder<M>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let key = self.next_key();
        self.group(key, Kind::GestureArea, style, content);
        if let Some(on_gesture) = gesture.on_gesture {
            self.set_handler(crate::app::Handler::Map(Box::new(
                move |update| match update {
                    ControlUpdate::Gesture(gesture) => Some(on_gesture(gesture)),
                    _ => None,
                },
            )));
        }
    }

    /// Content that takes drops of `target`'s kind; a drop sends what
    /// [`DropTargetBuilder::on_drop`] makes of the payload.
    #[track_caller]
    pub fn drop_target(
        &mut self,
        target: DropTargetBuilder<M>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let DropTargetBuilder {
            label,
            accepts,
            on_drop,
        } = target;
        let key = self.next_key();
        self.group(key, Kind::DropTarget, style, content);
        self.set_control(Control::DropTarget { label, accepts });
        if let Some(on_drop) = on_drop {
            self.set_handler(crate::app::Handler::Map(Box::new(
                move |update| match update {
                    ControlUpdate::Drop(payload) => Some(on_drop(payload)),
                    _ => None,
                },
            )));
        }
    }

    /// A column that watches the events of the widgets inside it, and may
    /// stop them; each one that reaches it sends what
    /// [`AreaBuilder::on_event`] makes of it, when it is resolved.
    ///
    /// A capturing area sees an event before the widgets inside, so consuming
    /// it keeps it from them; an ordinary one sees what they did not take, on
    /// the way up. Areas nest: outermost first going down, innermost first
    /// coming up, and a consumed event stops there.
    #[track_caller]
    pub fn event_area(
        &mut self,
        area: AreaBuilder<M>,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let key = self.next_key();
        self.group(key, Kind::EventArea, style, content);
        self.set_control(Control::EventArea(area.area));
        // Always a handler, so that what reaches the area is taken at resolve
        // time even when nothing is sent.
        let on_event: Box<dyn Fn(crate::UiEvent) -> Option<M>> = match area.on_event {
            Some(on_event) => on_event,
            None => Box::new(|_| None),
        };
        self.set_handler(crate::app::Handler::Event(on_event));
    }
}
