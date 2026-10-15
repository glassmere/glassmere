use super::*;

impl Context {
    pub(super) fn within(&self, id: NodeId, ancestor: NodeId) -> bool {
        let mut next = Some(id);
        while let Some(current) = next {
            if current == ancestor {
                return true;
            }
            next = self.nodes.get(&current).and_then(|node| node.parent);
        }
        false
    }

    /// Scope ancestors of `id`, innermost first.
    fn scopes_of(&self, id: NodeId) -> Vec<NodeId> {
        let mut scopes = Vec::new();
        let mut next = self.nodes.get(&id).and_then(|node| node.parent);
        while let Some(current) = next {
            let node = &self.nodes[&current];
            if node.scope.is_some() {
                scopes.push(current);
            }
            next = node.parent;
        }
        scopes
    }

    /// The innermost focus scope, dialog or popover holding `id`.
    pub(super) fn scope_of(&self, id: NodeId) -> Option<NodeId> {
        self.scopes_of(id).first().copied()
    }

    fn traps(&self, scope: NodeId) -> bool {
        match &self.nodes[&scope].control {
            Control::FocusScope { scope } => scope.trap,
            Control::Dialog { modal, .. } => *modal,
            _ => false,
        }
    }

    /// The topmost modal dialog layer, if one is open.
    pub(super) fn top_modal(&self) -> Option<NodeId> {
        self.nodes[&self.root]
            .children
            .iter()
            .rev()
            .copied()
            .find(|id| {
                matches!(self.nodes[id].control, Control::Dialog { modal: true, .. })
                    && self.nodes[id].enabled
            })
    }

    /// Innermost trapping scope around the focus, or the top modal when nothing
    /// (or something outside it) is focused.
    fn traversal_root(&self) -> NodeId {
        if let Some(focused) = self.focused {
            for scope in self.scopes_of(focused) {
                if self.traps(scope) {
                    return scope;
                }
            }
        }
        self.top_modal().unwrap_or(self.root)
    }

    /// Focusable nodes in declaration order below `root`, with radio groups
    /// collapsed to their selected (or first enabled) option.
    fn tab_candidates(&self, root: NodeId) -> Vec<NodeId> {
        let mut order = Vec::new();
        self.order(root, &mut order);
        order.retain(|id| {
            let node = &self.nodes[id];
            if !node.focusable() || node.kind == Kind::Choice {
                return false;
            }
            if let Control::Radio {
                selected: false, ..
            } = node.control
            {
                let siblings = &self.nodes[&node.parent.unwrap()].children;
                let radios = || {
                    siblings
                        .iter()
                        .filter(|id| self.nodes[id].kind == Kind::Radio && self.nodes[id].enabled)
                };
                return !radios().any(|id| {
                    matches!(
                        self.nodes[id].control,
                        Control::Radio { selected: true, .. }
                    )
                }) && radios().next() == Some(id);
            }
            true
        });
        order
    }

    fn tab(&mut self, shift: bool) {
        let root = self.traversal_root();
        let order = self.tab_candidates(root);
        let current = order.iter().position(|id| Some(*id) == self.focused);
        let Some(mut next) = (!order.is_empty()).then(|| {
            let index = match (current, shift) {
                (Some(index), true) => (index + order.len() - 1) % order.len(),
                (Some(index), false) => (index + 1) % order.len(),
                (None, true) => order.len() - 1,
                (None, false) => 0,
            };
            order[index]
        }) else {
            self.focused = None;
            self.keyboard_pressed = None;
            return;
        };
        // Entering a scope lands on the child it remembers.
        let entered: Vec<_> = self
            .scopes_of(next)
            .into_iter()
            .filter(|scope| {
                !self
                    .focused
                    .is_some_and(|focused| self.within(focused, *scope))
            })
            .collect();
        for scope in entered.into_iter().rev() {
            if let Some(remembered) = self.nodes[&scope]
                .scope
                .as_ref()
                .and_then(|state| state.last_focused)
                && remembered != next
                && order.contains(&remembered)
                && self.within(remembered, scope)
            {
                next = remembered;
                break;
            }
        }
        self.focused = Some(next);
        self.keyboard_pressed = None;
    }

    /// Records focus movement into scopes: what to restore and what to remember.
    pub(super) fn note_focus(&mut self) {
        let focused = self.focused;
        if focused == self.noted_focus {
            return;
        }
        let previous = self.noted_focus;
        self.noted_focus = focused;
        if let Some(old) = previous.filter(|old| Some(*old) != focused) {
            self.queue_blur(old);
        }
        let Some(new) = focused else {
            return;
        };
        for scope in self.scopes_of(new) {
            let outside = previous.is_none_or(|old| !self.within(old, scope));
            let state = self.nodes.get_mut(&scope).unwrap().scope.as_mut().unwrap();
            state.last_focused = Some(new);
            if outside && previous.is_some() {
                state.restore_to = previous;
            }
        }
    }

    /// Keeps focus inside the top modal dialog, choosing its remembered child
    /// or its first focusable when focus is outside or missing.
    pub(super) fn enforce_modal(&mut self) {
        let Some(top) = self.top_modal() else {
            return;
        };
        if self.focused.is_some_and(|id| self.within(id, top)) {
            return;
        }
        let remembered = self.nodes[&top]
            .scope
            .as_ref()
            .and_then(|state| state.last_focused)
            .filter(|id| self.nodes.get(id).is_some_and(Node::focusable) && self.within(*id, top));
        let target = remembered.or_else(|| self.tab_candidates(top).first().copied());
        match target {
            Some(id) => self.focus_control(id),
            None => {
                if let Some(old) = self.focused.take() {
                    self.invalidate_paint(old);
                }
                self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
            }
        }
    }

    fn dismiss(&mut self, dialog: NodeId) -> bool {
        if !matches!(
            self.nodes.get(&dialog).map(|node| &node.control),
            Some(Control::Dialog {
                dismissible: true,
                ..
            })
        ) {
            return false;
        }
        self.queue_update(dialog, ControlUpdate::Dismiss);
        self.invalidate_paint(dialog);
        self.pending |= Dirty::ACCESSIBILITY;
        true
    }

    /// The dialog that should receive Escape: the innermost one around the
    /// focus, else the topmost modal.
    fn escape_target(&self) -> Option<NodeId> {
        self.focused
            .and_then(|focused| {
                self.scopes_of(focused)
                    .into_iter()
                    .find(|scope| matches!(self.nodes[scope].control, Control::Dialog { .. }))
            })
            .or_else(|| self.top_modal())
    }

    fn consumes_arrows(&self, id: NodeId) -> bool {
        let node = &self.nodes[&id];
        self.popup.is_some()
            || matches!(
                node.kind,
                Kind::Slider
                    | Kind::TextInput
                    | Kind::Collection
                    | Kind::CollectionRow
                    | Kind::Radio
                    | Kind::Scroll
                    | Kind::Choice
                    | Kind::Tab
                    | Kind::Divider
                    | Kind::DockHandle
                    | Kind::TextArea
                    | Kind::RichEditor
            )
    }

    fn arrow_navigate(&mut self, key: Key) -> Option<bool> {
        let focused = self.focused?;
        if self.consumes_arrows(focused) {
            return None;
        }
        let scope = self.scopes_of(focused).into_iter().find(|scope| {
            matches!(
                self.nodes[scope].control,
                Control::FocusScope {
                    scope: crate::FocusScope { arrows: true, .. }
                }
            )
        })?;
        let origin = self.nodes[&focused].bounds;
        let center = |rect: Rect| (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let (ox, oy) = center(origin);
        let best = self
            .tab_candidates(scope)
            .into_iter()
            .filter(|id| *id != focused)
            .filter_map(|id| {
                let (cx, cy) = center(self.nodes[&id].bounds);
                let (dx, dy) = (cx - ox, cy - oy);
                let (forward, sideways) = match key {
                    Key::ArrowRight => (dx, dy),
                    Key::ArrowLeft => (-dx, dy),
                    Key::ArrowDown => (dy, dx),
                    Key::ArrowUp => (-dy, dx),
                    _ => return None,
                };
                (forward > 0.5).then(|| (id, forward + 2.0 * sideways.abs()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        match best {
            Some(id) => {
                self.focus_control(id);
                Some(true)
            }
            None => Some(false),
        }
    }

    pub(super) fn dialog_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::KeyDown {
                key: Key::Tab,
                shift,
                ..
            } => {
                let before = self.focused;
                self.tab(*shift);
                for id in [before, self.focused].into_iter().flatten() {
                    self.invalidate_paint(id);
                }
                if let Some(id) = self.focused {
                    self.scroll_into_view(id);
                }
                self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
                Some(true)
            }
            Event::KeyDown {
                key: key @ (Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight),
                ..
            } => self.arrow_navigate(*key),
            Event::KeyDown {
                key: Key::Escape,
                repeat: false,
                ..
            } => {
                if let Some(popover) = self.focused_popover() {
                    self.captured = None;
                    self.keyboard_pressed = None;
                    return Some(self.dismiss_popover(popover));
                }
                let target = self.escape_target()?;
                self.captured = None;
                self.keyboard_pressed = None;
                Some(self.dismiss(target))
            }
            Event::PointerDown | Event::PointerDownWithShift => {
                self.dismiss_outside_popovers(self.hovered);
                let id = self.hovered?;
                if self.nodes[&id].kind != Kind::Dialog {
                    return None;
                }
                self.captured = Some(id);
                self.keyboard_pressed = None;
                Some(true)
            }
            Event::PointerUp => {
                let id = self.captured?;
                if self
                    .nodes
                    .get(&id)
                    .is_none_or(|node| node.kind != Kind::Dialog)
                {
                    return None;
                }
                self.captured = None;
                Some(self.hovered == Some(id) && self.dismiss(id))
            }
            Event::Scroll { .. } => {
                let top = self.top_modal()?;
                let pointer = self.pointer?;
                let hit = self.hit_test(pointer)?;
                (!self.within(hit, top) || hit == top).then_some(false)
            }
            _ => None,
        }
    }
}
