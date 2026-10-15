use super::*;

impl<'a, M: 'static> Ui<'a, M> {
    /// Drags inside the window are delivered to a `drop_target`. A host that
    /// can hands a drag that leaves the window to the operating system (files
    /// only today); how that ended is returned here, once.
    #[track_caller]
    pub fn drag_source(
        &mut self,
        label: impl Into<String>,
        payload: crate::DragPayload,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) -> Option<crate::DragOutcome> {
        let key = self.next_key();
        let update = self
            .context
            .child(self.parent, &key, Kind::DragSource)
            .and_then(|id| self.updates.remove(&id));
        self.group(key, Kind::DragSource, style, content);
        self.set_control(Control::DragSource {
            label: label.into(),
            payload,
        });
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
    #[track_caller]
    pub fn gesture_area(
        &mut self,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) -> crate::GestureResponse {
        let key = self.next_key();
        let id = self.context.child(self.parent, &key, Kind::GestureArea);
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

    #[track_caller]
    pub fn drop_target(
        &mut self,
        label: impl Into<String>,
        accepts: crate::DropKind,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) -> Option<crate::DragPayload> {
        let key = self.next_key();
        let update = self
            .context
            .child(self.parent, &key, Kind::DropTarget)
            .and_then(|id| self.updates.remove(&id));
        let payload = match update {
            Some(ControlUpdate::Drop(payload)) if self.enabled && payload.kind() == accepts => {
                Some(payload)
            }
            _ => None,
        };
        self.group(key, Kind::DropTarget, style, content);
        self.set_control(Control::DropTarget {
            label: label.into(),
            accepts,
        });
        payload
    }

    /// A column that watches the events of the widgets inside it, and may
    /// stop them. What reached it since the last declaration comes back in
    /// the order it arrived.
    ///
    /// A capturing area sees an event before the widgets inside, so consuming
    /// it keeps it from them; an ordinary one sees what they did not take, on
    /// the way up. Areas nest: outermost first going down, innermost first
    /// coming up, and a consumed event stops there.
    #[track_caller]
    pub fn event_area(
        &mut self,
        area: crate::EventArea,
        style: Style,
        content: impl FnOnce(&mut Ui<'_, M>),
    ) -> Vec<crate::UiEvent> {
        let key = self.next_key();
        let arrived = self
            .context
            .child(self.parent, &key, Kind::EventArea)
            .map(|id| self.context.take_area_events(id))
            .unwrap_or_default();
        self.group(key, Kind::EventArea, style, content);
        self.set_control(Control::EventArea(area));
        arrived
    }

    /// Tells the areas above this point something. The event travels up from
    /// the container being declared and arrives on the next declaration, like
    /// every other input.
    #[track_caller]
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
    #[track_caller]
    pub fn widget(
        &mut self,
        style: Style,
        build: impl FnOnce(&crate::WidgetInput) -> CustomWidget,
    ) -> Response<'a> {
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
            input.events = self
                .context
                .take_area_events(id)
                .into_iter()
                .map(|event| crate::WidgetInput::local(event, bounds))
                .collect();
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
        // `build` was handed the hover, focus and press, so a change in them
        // declares again.
        response.observe();
        response
    }

    #[track_caller]
    pub fn custom(&mut self, style: Style, widget: CustomWidget) -> Response<'a> {
        if let Err(message) = widget.validate() {
            self.misuse(message);
            return Response::default();
        }
        let key = self.next_key();
        self.push(Element {
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
        })
    }
}
