//! Hit testing, routing an event to the nodes on its path, and the input
//! events themselves.

use super::*;

impl Context {
    /// What has reached an event area since its last declaration.
    pub(crate) fn take_area_events(&self, id: NodeId) -> Vec<crate::UiEvent> {
        self.pending_area_events
            .borrow_mut()
            .remove(&id)
            .unwrap_or_default()
    }

    /// Whether the area on this node may keep pointer events from what is
    /// under it. A widget of the application's own may not: it is what is
    /// under it, and focus, hover and press are services to it rather than
    /// competitors for the event.
    pub(super) fn may_consume_pointer(&self, id: NodeId) -> bool {
        !self
            .nodes
            .get(&id)
            .is_some_and(|node| node.custom.is_some())
    }

    /// The path from the root down to `id`.
    pub(super) fn path_to(&self, id: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut next = Some(id);
        while let Some(current) = next {
            path.push(current);
            next = self.nodes.get(&current).and_then(|node| node.parent);
        }
        path.reverse();
        path
    }

    /// Offers an event to the areas along a path, first going down to the
    /// node it concerns and then coming back up. True when one consumed it,
    /// and the widgets under it must not see it.
    pub(super) fn travel(&mut self, event: crate::UiEvent, path: &[NodeId]) -> bool {
        let mut arrived: Vec<(NodeId, bool)> = Vec::new();
        let mut consumed = false;
        let consumable = |context: &Self, id: NodeId| match &event {
            // The frame keeps these two, so nothing can trap the keyboard.
            crate::UiEvent::Key {
                key: Key::Tab | Key::Escape,
                ..
            } => false,
            event if event.pointer() => context.may_consume_pointer(id),
            _ => true,
        };
        // Down first: an area that captures may take the event away from the
        // widgets inside it.
        for id in path {
            let Some(area) = self.areas.get(id).copied() else {
                continue;
            };
            if area.capture && area.watches(&event) {
                let consuming = area.consume && consumable(self, *id);
                arrived.push((*id, consuming));
                if consuming {
                    consumed = true;
                    break;
                }
            }
        }
        if !consumed {
            // Then up, innermost first, over what the widgets left.
            for id in path.iter().rev() {
                let Some(area) = self.areas.get(id).copied() else {
                    continue;
                };
                // Consuming here keeps the event from the areas further out,
                // not from the widgets: they have not acted yet.
                if !area.capture && area.watches(&event) {
                    arrived.push((*id, area.consume));
                    if area.consume {
                        break;
                    }
                }
            }
        }
        if arrived.is_empty() {
            return false;
        }
        let mut pending = self.pending_area_events.borrow_mut();
        for (id, _) in &arrived {
            pending.entry(*id).or_default().push(event.clone());
        }
        drop(pending);
        for (id, _) in &arrived {
            if !self.queued.contains(&(*id, Queued::Area)) {
                self.queued.push((*id, Queued::Area));
            }
            self.stale_unless_handled(*id);
        }
        consumed
    }

    /// The innermost node a point is inside, whatever it is. Areas contain
    /// things rather than taking the pointer themselves, so this is not the
    /// hit test.
    pub(super) fn deepest_at(&self, point: Point) -> Option<NodeId> {
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        order
            .into_iter()
            .rev()
            .find(|id| self.nodes[id].clip.contains(point))
    }

    /// Where an event goes: what it is over, or what has the focus.
    pub(super) fn travelled(&mut self, event: &Event) -> bool {
        if self.areas.is_empty() {
            return false;
        }
        let pointer = self.pointer;
        let (carried, target) = match event {
            Event::PointerDown | Event::PointerDownWithShift => (
                pointer.map(crate::UiEvent::PointerDown),
                pointer.and_then(|at| self.deepest_at(at)),
            ),
            // A press holds the pointer, so a drag goes on reaching what it
            // started on however far it wanders.
            Event::PointerUp => (
                pointer.map(crate::UiEvent::PointerUp),
                self.captured
                    .or_else(|| pointer.and_then(|at| self.deepest_at(at))),
            ),
            Event::PointerMoved(at) => (
                Some(crate::UiEvent::PointerMoved(*at)),
                self.captured.or_else(|| self.deepest_at(*at)),
            ),
            Event::Scroll { delta_x, delta_y } => (
                Some(crate::UiEvent::Scroll {
                    delta_x: *delta_x,
                    delta_y: *delta_y,
                }),
                pointer.and_then(|at| self.deepest_at(at)),
            ),
            Event::KeyDown { key, shift, .. } => (
                Some(crate::UiEvent::Key {
                    key: *key,
                    shift: *shift,
                }),
                self.focused,
            ),
            Event::Text(TextEdit::Insert(text)) => {
                (Some(crate::UiEvent::Text(text.clone())), self.focused)
            }
            _ => (None, None),
        };
        let Some(carried) = carried else {
            return false;
        };
        // Without a node under it, an event still belongs to the window.
        let path = target.map_or_else(|| vec![self.root], |id| self.path_to(id));
        let consumed = self.travel(carried, &path);
        // Where the pointer is remains the window's business: an area may
        // keep a move from the widgets, but not from the context.
        if consumed && let Event::PointerMoved(at) = event {
            self.pointer = Some(*at);
        }
        consumed
    }

    pub fn hit_test(&self, position: Point) -> Option<NodeId> {
        // A capturing area that consumes pointer events stands between the
        // pointer and everything under it, like a scrim: nothing there is
        // hovered, pressed or dragged while it is up.
        if !self.areas.is_empty()
            && let Some(deepest) = self.deepest_at(position)
            && self.path_to(deepest).iter().any(|id| {
                matches!(self.areas.get(id), Some(area) if area.capture && area.consume && area.pointer)
                    && self.may_consume_pointer(*id)
            })
        {
            return None;
        }
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        for popover in self.popovers() {
            self.order(popover, &mut order);
        }
        if let Some(popup) = self.popup
            && let Some(panel) = self
                .nodes
                .get(&popup.owner)
                .and_then(|node| node.children.first())
        {
            self.order(*panel, &mut order);
        }
        order.into_iter().rev().find(|id| {
            let node = &self.nodes[id];
            let dialog_surface = match node.kind {
                Kind::Dialog => matches!(node.control, Control::Dialog { modal: true, .. }),
                Kind::Container => node
                    .parent
                    .is_some_and(|parent| self.nodes[&parent].kind == Kind::Dialog),
                _ => false,
            };
            (node.takes_pointer()
                // Floating groups block clicks to whatever lies under them.
                || matches!(
                    node.kind,
                    Kind::Popup | Kind::Popover | Kind::DockFloat | Kind::Toast
                )
                || dialog_surface)
                && node.clip.contains(position)
        })
    }

    /// Returns true if a declaration/redraw is needed. Pointer capture survives
    /// leaving the window, but activation requires release over the same button.
    pub fn event(&mut self, event: Event) -> bool {
        // Areas see an event before and after the widgets, and may keep it
        // from them.
        if self.travelled(&event) {
            self.note_focus();
            return true;
        }
        match event {
            Event::Touch {
                id,
                phase,
                position,
                force,
                pen,
            } => {
                let handled = self.touch_event(id, phase, position, force, pen);
                self.note_focus();
                self.view_stale |= handled;
                return handled;
            }
            Event::Zoom(factor) => {
                let handled = self.pointer_gesture(factor, 0.0);
                self.view_stale |= handled;
                return handled;
            }
            Event::Rotate(radians) => {
                let handled = self.pointer_gesture(1.0, radians);
                self.view_stale |= handled;
                return handled;
            }
            Event::FocusLost => self.stop_touches(),
            // Any other way of scrolling or pressing ends a fling.
            Event::PointerDown | Event::PointerDownWithShift | Event::Scroll { .. } => {
                self.fling = None;
            }
            _ => {}
        }
        let queued = self.queued.len();
        let handled = self.handle_event(event.clone());
        self.track_hover(&event);
        self.note_focus();
        // Moving the pointer changes what is hovered, which only repaints
        // unless the view observed it; holding something while moving it
        // (a scrollbar, a drag) changes what the view reads.
        let moved = matches!(event, Event::PointerMoved(_) | Event::PointerLeft)
            && self.queued.len() == queued
            && self.captured.is_none()
            && self.drag.is_none()
            && self.dock_drag.is_none();
        // Typing a draft into a number field that sends messages is not a
        // change the view reads: the draft waits for Enter or for leaving.
        let draft = matches!(event, Event::Text(_))
            && self.focused.is_some_and(|id| self.sends_numbers(id));
        self.view_stale |= handled && !moved && !draft;
        handled
    }

    pub(super) fn handle_event(&mut self, event: Event) -> bool {
        if let Some(handled) = self.drag_event(&event) {
            return handled;
        }
        if let Some(handled) = self.dock_event(&event) {
            return handled;
        }
        if let Some(handled) = self.rich_editor_event(&event) {
            return handled;
        }
        if let Some(handled) = self.text_area_event(&event) {
            return handled;
        }
        if let Some(handled) = self.rich_text_event(&event) {
            return handled;
        }
        if let Event::Command(wanted) = &event {
            return self
                .command_target(wanted)
                .is_some_and(|id| self.activate(id));
        }
        if let Event::Shortcut(shortcut) = &event {
            return self
                .shortcut_target(shortcut)
                .is_some_and(|id| self.activate(id));
        }
        if let Some(handled) = self.collection_event(&event) {
            return handled;
        }
        if let Some(handled) = self.control_event(&event) {
            return handled;
        }
        if let Some(handled) = self.dialog_event(&event) {
            return handled;
        }
        if let Event::Scroll { delta_x, delta_y } = event {
            return self.wheel(delta_x, delta_y);
        }
        let before = (
            self.hovered,
            self.focused,
            self.captured,
            self.keyboard_pressed,
        );
        let mut activated = false;
        match event {
            Event::PointerMoved(position) => {
                self.pointer = Some(position);
                self.hovered = self.hit_test(position);
            }
            Event::PointerLeft => {
                self.pointer = None;
                self.hovered = None;
            }
            Event::PointerDown | Event::PointerDownWithShift => {
                self.captured = self.hovered;
                self.focused = self.hovered.filter(|id| self.nodes[id].focusable());
                self.keyboard_pressed = None;
                self.press_window_frame();
            }
            Event::PointerUp => {
                if let Some(id) = self
                    .captured
                    .take()
                    .filter(|id| self.hovered == Some(*id) && self.nodes[id].activatable())
                {
                    activated = self.activate(id);
                    if activated {
                        self.pointer_clicks.insert(id);
                    }
                }
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } => {
                self.captured = None;
                self.keyboard_pressed = None;
            }
            Event::KeyDown {
                key: key @ (Key::Enter | Key::Space),
                repeat: false,
                ..
            } => {
                if self.keyboard_pressed.is_none() {
                    self.keyboard_pressed = self
                        .focused
                        .filter(|id| self.nodes[id].activatable())
                        .map(|id| (id, key));
                }
            }
            Event::KeyDown { key, .. }
                if matches!(
                    key,
                    Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Home
                        | Key::End
                ) =>
            {
                if self.scroll_by_key(key) {
                    return true;
                }
            }
            Event::KeyDown { .. } => {}
            Event::KeyUp(key) => {
                if let Some((id, pressed_key)) = self.keyboard_pressed
                    && pressed_key == key
                {
                    self.keyboard_pressed = None;
                    if self.focused == Some(id) {
                        activated = self.activate(id);
                    }
                }
            }
            Event::FocusLost => {
                self.captured = None;
                self.keyboard_pressed = None;
                self.hovered = None;
                self.pointer = None;
                self.focused = None;
                self.clicks.clear();
                self.pointer_clicks.clear();
            }
            Event::Scroll { .. } => unreachable!(),
            Event::Text(_)
            | Event::Shortcut(_)
            | Event::Command(_)
            | Event::DropAt { .. }
            | Event::ContextMenu(_)
            | Event::Editor(_)
            // Handled in `event`: contacts as pointer events, the others by
            // the gesture area under the pointer.
            | Event::Touch { .. }
            | Event::Zoom(_)
            | Event::Rotate(_) => {}
        }
        let after = (
            self.hovered,
            self.focused,
            self.captured,
            self.keyboard_pressed,
        );
        self.interaction_changed(before, after, activated)
    }

    /// The innermost enabled command with that id, wherever the focus is: a
    /// menu belongs to the window, not to what has the keyboard.
    fn command_target(&self, wanted: &str) -> Option<NodeId> {
        let mut best: Option<(usize, NodeId)> = None;
        for (id, node) in &self.nodes {
            let Control::Command { id: declared, .. } = &node.control else {
                continue;
            };
            if !node.enabled || declared != wanted {
                continue;
            }
            let depth = self.depth(*id);
            if best.is_none_or(|(deepest, _)| depth > deepest) {
                best = Some((depth, *id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// The command a shortcut activates. The innermost scope that holds the
    /// focus wins, so a panel may take a shortcut the window also has.
    fn shortcut_target(&self, shortcut: &Shortcut) -> Option<NodeId> {
        let mut best: Option<(usize, NodeId)> = None;
        for (id, node) in &self.nodes {
            let Control::Command {
                shortcut: Some(binding),
                while_editing,
                scoped,
                ..
            } = &node.control
            else {
                continue;
            };
            if !node.enabled || binding != shortcut || !(*while_editing || !self.accepts_text()) {
                continue;
            }
            let scope = self.scope_of(*id);
            let inside = match (scope, self.focused) {
                (Some(scope), Some(focused)) => self.within(focused, scope),
                // A command outside every scope belongs to the window.
                (None, _) => true,
                (Some(_), None) => false,
            };
            if *scoped && !inside {
                continue;
            }
            let depth = if inside { self.depth(*id) } else { 0 };
            if best.is_none_or(|(deepest, _)| depth > deepest) {
                best = Some((depth, *id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Steps a number field or scrolls what is under the pointer.
    fn wheel(&mut self, delta_x: f32, delta_y: f32) -> bool {
        // The wheel steps a number field only while it has the keyboard
        // and the pointer, so scrolling a page never edits values in passing.
        if let Some(id) = self.focused.filter(|id| self.hovered == Some(*id))
            && delta_y != 0.0
            && self.step_number(id, if delta_y < 0.0 { 1.0 } else { -1.0 })
        {
            return true;
        }
        let vertical = delta_y != 0.0 && self.scroll_at_pointer(delta_y);
        let horizontal = delta_x != 0.0 && self.scroll_x_at_pointer(delta_x);
        vertical || horizontal
    }

    /// Scrolls the nearest scrolling ancestor of the focus by a navigation
    /// key. Returns whether anything moved.
    fn scroll_by_key(&mut self, key: Key) -> bool {
        let mut target = self.focused;
        while let Some(id) = target {
            let node = &self.nodes[&id];
            if node.kind == Kind::Scroll && matches!(key, Key::ArrowLeft | Key::ArrowRight) {
                let step = if key == Key::ArrowRight { 40.0 } else { -40.0 };
                if self.scroll_x_by(id, step) {
                    return true;
                }
            } else if node.kind == Kind::Scroll {
                let offset = match key {
                    Key::ArrowUp => node.scroll.offset - 40.0,
                    Key::ArrowDown => node.scroll.offset + 40.0,
                    Key::PageUp => node.scroll.offset - node.bounds.height * 0.9,
                    Key::PageDown => node.scroll.offset + node.bounds.height * 0.9,
                    Key::Home => 0.0,
                    Key::End => node.scroll.maximum,
                    _ => unreachable!(),
                };
                if self.set_scroll(id, offset) {
                    return true;
                }
            }
            target = self.nodes[&id].parent;
        }
        false
    }

    /// Repaints whatever gained or lost hover, focus, capture or a keyboard
    /// press, and says whether the event changed anything.
    fn interaction_changed(
        &mut self,
        before: Interaction,
        after: Interaction,
        activated: bool,
    ) -> bool {
        if before.1 != after.1 {
            self.cancel_compositions_except(self.focused);
            self.pending |= Dirty::ACCESSIBILITY;
            if let Some(id) = self.focused {
                self.scroll_into_view(id);
            }
        }
        if before != after || activated {
            for id in [
                before.0,
                before.1,
                before.2,
                before.3.map(|(id, _)| id),
                after.0,
                after.1,
                after.2,
                after.3.map(|(id, _)| id),
            ]
            .into_iter()
            .flatten()
            {
                self.invalidate_paint(id);
            }
            self.pending |= Dirty::PAINT;
            true
        } else {
            false
        }
    }
}

/// Hovered, focused, captured, and pressed from the keyboard.
type Interaction = (
    Option<NodeId>,
    Option<NodeId>,
    Option<NodeId>,
    Option<(NodeId, Key)>,
);
