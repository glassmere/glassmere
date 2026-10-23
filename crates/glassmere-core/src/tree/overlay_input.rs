use super::*;
use crate::{Axis, panes::TOOLTIP_DELAY};

impl Context {
    /// Supplies the host's monotonic clock. Timed behavior (tooltips) is
    /// evaluated on the next declaration.
    pub fn set_now(&mut self, now: Duration) {
        self.now = now;
    }

    /// The clock the host last supplied through `set_now`.
    pub fn now(&self) -> Duration {
        self.now
    }

    /// When the context next wants to be run without input, on the `set_now` clock.
    pub fn next_wake(&self) -> Option<Duration> {
        let tooltip = self.hover_since.and_then(|(id, since)| {
            (self.tooltip.is_none() && self.tooltip_text(id).is_some())
                .then_some(since + TOOLTIP_DELAY)
        });
        [
            tooltip,
            self.toast_wake(),
            self.touch_wake(),
            self.grid_wake(),
            self.track_wake(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// The node's tooltip, or for an editor the diagnostic under the pointer.
    fn tooltip_text(&self, id: NodeId) -> Option<String> {
        let node = self.nodes.get(&id)?;
        node.tooltip.clone().or_else(|| self.area_diagnostic(id))
    }

    /// Tracks hover dwell after input; hides a visible tooltip when the pointer
    /// leaves its owner or any press or key arrives.
    pub(super) fn track_hover(&mut self, event: &Event) {
        let hides = matches!(
            event,
            Event::PointerDown
                | Event::PointerDownWithShift
                | Event::KeyDown { .. }
                | Event::FocusLost
                | Event::PointerLeft
                | Event::Scroll { .. }
        );
        if hides {
            self.hover_since = None;
            self.hide_tooltip();
            return;
        }
        // Only movement re-arms the dwell; a release over the same widget does not.
        if !matches!(event, Event::PointerMoved(_)) {
            return;
        }
        // Inside an editor the text under the pointer changes as it moves.
        let editor = self.hovered.is_some_and(|id| {
            self.nodes
                .get(&id)
                .is_some_and(|node| node.kind == Kind::TextArea)
        });
        if editor || self.hover_since.map(|(id, _)| id) != self.hovered {
            self.hover_since = self.hovered.map(|id| (id, self.now));
            if editor
                || self
                    .tooltip
                    .is_some_and(|owner| Some(owner) != self.hovered)
            {
                self.hide_tooltip();
            }
        }
    }

    fn hide_tooltip(&mut self) {
        if self.tooltip.take().is_some() {
            self.pending |= Dirty::PAINT;
        }
    }

    /// Shows a due tooltip. Called during declaration, after reconciliation.
    pub(super) fn update_tooltip(&mut self) {
        let Some((id, since)) = self.hover_since else {
            return;
        };
        if self.tooltip.is_some() || self.now < since + TOOLTIP_DELAY {
            return;
        }
        if self.tooltip_text(id).is_some() {
            self.tooltip = Some(id);
            self.pending |= Dirty::PAINT;
        }
    }

    /// Paints the visible tooltip after all content and overlays.
    pub(super) fn paint_tooltip(&mut self, scene: &mut Scene, measurer: &mut impl TextMeasurer) {
        let Some(owner) = self.tooltip else {
            return;
        };
        let Some(node) = self.nodes.get(&owner) else {
            self.tooltip = None;
            return;
        };
        let Some(text) = self.tooltip_text(owner) else {
            return;
        };
        let style = TextStyle {
            font_size: 13.0,
            family: self.theme.font_family.clone(),
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let padding = 8.0;
        let measured = measurer.measure_styled(&text, &style, Some(280.0));
        let viewport = self.nodes[&self.root].bounds;
        let width = (measured.width + padding * 2.0).min(viewport.width);
        let height = measured.height + padding * 2.0;
        let mut rect = Rect {
            x: if node.resolved.rtl() {
                node.bounds.x + node.bounds.width - width
            } else {
                node.bounds.x
            },
            y: node.bounds.y + node.bounds.height + 6.0,
            width,
            height,
        };
        // Editor messages belong to a spot in the text, not to the whole editor.
        let anchor = self.pointer.filter(|_| node.kind == Kind::TextArea);
        if let Some(pointer) = anchor {
            rect.x = pointer.x;
            rect.y = pointer.y + 18.0;
        }
        rect.x = rect.x.clamp(0.0, (viewport.width - width).max(0.0));
        if rect.y + height > viewport.height {
            rect.y = (anchor.map_or(node.bounds.y, |pointer| pointer.y) - height - 6.0).max(0.0);
        }
        self.painted_overlay(rect);
        scene.primitives.push(Primitive::Quad {
            rect,
            color: self.theme.popup,
            radius: 6.0,
        });
        scene.primitives.push(Primitive::StyledText {
            rect: Rect {
                x: rect.x + padding,
                y: rect.y + padding,
                width: measured.width,
                height: measured.height,
            },
            text,
            style,
            color: self.theme.text,
        });
    }

    /// All open popovers in tree order.
    pub(super) fn popovers(&self) -> Vec<NodeId> {
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        order.retain(|id| self.nodes[id].kind == Kind::Popover);
        order
    }

    fn sibling_tabs(&self, tab: NodeId) -> Vec<NodeId> {
        let Some(strip) = self.nodes[&tab].parent else {
            return Vec::new();
        };
        self.nodes[&strip]
            .children
            .iter()
            .copied()
            .filter(|id| self.nodes[id].kind == Kind::Tab && self.nodes[id].enabled)
            .collect()
    }

    /// Selects a tab: queues the choice on the tabs owner and moves focus to it.
    pub(super) fn select_tab(&mut self, tab: NodeId) -> bool {
        let Some(node) = self.nodes.get(&tab).filter(|node| node.enabled) else {
            return false;
        };
        let Control::Tab { index, .. } = node.control else {
            return false;
        };
        let Some(owner) = node.parent.and_then(|strip| self.nodes[&strip].parent) else {
            return false;
        };
        self.queue_update(owner, ControlUpdate::Choice(index));
        self.focus_control(tab);
        self.pending |= Dirty::ACCESSIBILITY;
        true
    }

    pub(super) fn tab_key(&mut self, tab: NodeId, key: Key) -> Option<bool> {
        let siblings = self.sibling_tabs(tab);
        let current = siblings.iter().position(|id| *id == tab)?;
        // In a right-to-left strip the next tab is on the left.
        let key = match key {
            Key::ArrowLeft if self.nodes[&tab].resolved.rtl() => Key::ArrowRight,
            Key::ArrowRight if self.nodes[&tab].resolved.rtl() => Key::ArrowLeft,
            other => other,
        };
        let target = match key {
            Key::ArrowLeft | Key::ArrowUp => {
                siblings[(current + siblings.len() - 1) % siblings.len()]
            }
            Key::ArrowRight | Key::ArrowDown => siblings[(current + 1) % siblings.len()],
            Key::Home => siblings[0],
            Key::End => siblings[siblings.len() - 1],
            Key::Delete => {
                let Control::Tab {
                    index,
                    closable: true,
                    ..
                } = self.nodes[&tab].control
                else {
                    return Some(false);
                };
                let strip = self.nodes[&tab].parent?;
                let close = self.nodes[&strip]
                    .children_by_key
                    .get(&WidgetKey::Named(format!("$close-{index}")))
                    .copied()?;
                self.queue_click(close);
                self.invalidate_paint(close);
                return Some(true);
            }
            _ => return None,
        };
        if target == tab {
            return Some(false);
        }
        Some(self.select_tab(target))
    }

    /// Sets a split's fraction from its divider, clamped by the pane minimums.
    pub(super) fn set_fraction(&mut self, divider: NodeId, fraction: f32) -> bool {
        let Some(node) = self.nodes.get(&divider).filter(|node| node.enabled) else {
            return false;
        };
        let Control::Divider {
            axis,
            fraction: current,
            min_first,
            min_second,
        } = node.control
        else {
            return false;
        };
        let Some(owner) = node.parent else {
            return false;
        };
        let split = self.nodes[&owner].bounds;
        let extent = match axis {
            Axis::Horizontal => split.width,
            Axis::Vertical => split.height,
        } - node.bounds.width.min(node.bounds.height);
        let (low, high) = if extent > 0.0 {
            (
                (min_first / extent).min(1.0),
                (1.0 - min_second / extent).max(0.0),
            )
        } else {
            (0.0, 1.0)
        };
        let fraction = if fraction.is_finite() {
            fraction
        } else {
            current
        };
        let next = if low <= high {
            fraction.clamp(low, high)
        } else {
            0.5
        };
        if (next - current).abs() < f32::EPSILON {
            return false;
        }
        if let Control::Divider { fraction, .. } =
            &mut self.nodes.get_mut(&divider).unwrap().control
        {
            *fraction = next;
        }
        self.queue_update(owner, ControlUpdate::Number(f64::from(next)));
        self.pending |= Dirty::ACCESSIBILITY;
        self.invalidate_paint(divider);
        true
    }

    /// Asks the declaration for a new size; the widget that owns the sizer applies it.
    pub(super) fn set_size(&mut self, sizer: NodeId, size: f32) -> bool {
        let Some(node) = self.nodes.get(&sizer).filter(|node| node.enabled) else {
            return false;
        };
        let Control::Sizer {
            size: current,
            min,
            max,
            ..
        } = node.control
        else {
            return false;
        };
        let next = if size.is_finite() { size } else { current }.clamp(min, max.max(min));
        if (next - current).abs() < 0.5 {
            return false;
        }
        self.queue_update(sizer, ControlUpdate::Number(f64::from(next)));
        true
    }

    pub(super) fn divider_pointer(&mut self, divider: NodeId, position: Point) -> bool {
        let node = &self.nodes[&divider];
        if let Control::Sizer { size, .. } = node.control {
            // The handle sits on the edge it moves, so the distance from its
            // middle to the pointer is how much the size is off.
            let off = position.x - (node.bounds.x + node.bounds.width / 2.0);
            let off = if node.resolved.rtl() { -off } else { off };
            return self.set_size(divider, size + off);
        }
        let Control::Divider { axis, .. } = node.control else {
            return false;
        };
        let Some(owner) = node.parent else {
            return false;
        };
        let split = self.nodes[&owner].bounds;
        let thickness = node.bounds.width.min(node.bounds.height);
        let (offset, extent) = match axis {
            // The first pane of a right-to-left split is on the right.
            Axis::Horizontal if node.resolved.rtl() => {
                (split.x + split.width - position.x, split.width)
            }
            Axis::Horizontal => (position.x - split.x, split.width),
            Axis::Vertical => (position.y - split.y, split.height),
        };
        let usable = (extent - thickness).max(1.0);
        self.set_fraction(divider, (offset - thickness / 2.0) / usable)
    }

    pub(super) fn divider_key(&mut self, divider: NodeId, key: Key) -> Option<bool> {
        let node = &self.nodes[&divider];
        let Control::Divider { axis, fraction, .. } = node.control else {
            return None;
        };
        let owner = node.parent?;
        let split = self.nodes[&owner].bounds;
        let extent = match axis {
            Axis::Horizontal => split.width,
            Axis::Vertical => split.height,
        }
        .max(1.0);
        let step = 10.0 / extent;
        // Arrows move the divider the way they point.
        let step = if axis == Axis::Horizontal && node.resolved.rtl() {
            -step
        } else {
            step
        };
        let next = match (axis, key) {
            (Axis::Horizontal, Key::ArrowLeft) | (Axis::Vertical, Key::ArrowUp) => fraction - step,
            (Axis::Horizontal, Key::ArrowRight) | (Axis::Vertical, Key::ArrowDown) => {
                fraction + step
            }
            (_, Key::Home) => 0.0,
            (_, Key::End) => 1.0,
            _ => return None,
        };
        Some(self.set_fraction(divider, next))
    }

    /// Opens the context menu of the deepest enabled area containing `point`.
    pub(super) fn open_context_menu_at(&mut self, point: Point) -> bool {
        if let Some(opened) = self.open_strip_menu_at(point) {
            return opened;
        }
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        let Some(owner) = order.into_iter().rev().find(|id| {
            let node = &self.nodes[id];
            matches!(node.control, Control::ContextMenu { .. })
                && node.enabled
                && node.clip.contains(point)
        }) else {
            return false;
        };
        self.open_popup_at(owner, Some(point))
    }

    /// Opens the context menu around the focused node from the keyboard.
    pub(super) fn open_context_menu_for_focus(&mut self) -> bool {
        let Some(focused) = self.focused else {
            return false;
        };
        if let Some(menu) = self.strip_menu(focused) {
            let anchor = self.nodes[&focused].bounds;
            return self.open_popup_at(
                menu,
                Some(Point {
                    x: anchor.x,
                    y: anchor.y + anchor.height,
                }),
            );
        }
        let mut next = Some(focused);
        while let Some(id) = next {
            let node = &self.nodes[&id];
            if matches!(node.control, Control::ContextMenu { .. }) && node.enabled {
                let anchor = self.nodes[&focused].bounds;
                return self.open_popup_at(
                    id,
                    Some(Point {
                        x: anchor.x,
                        y: anchor.y + anchor.height,
                    }),
                );
            }
            next = node.parent;
        }
        false
    }

    /// Focuses the first control of popovers created by this declaration.
    pub(super) fn focus_new_popovers(&mut self) {
        for popover in std::mem::take(&mut self.new_popovers) {
            if !self.nodes.contains_key(&popover) {
                continue;
            }
            let inside = self
                .focused
                .is_some_and(|focused| self.within(focused, popover));
            if inside {
                continue;
            }
            let mut order = Vec::new();
            self.order(popover, &mut order);
            if let Some(target) = order
                .into_iter()
                .find(|id| self.nodes[id].focusable() && self.nodes[id].kind != Kind::Choice)
            {
                self.focus_control(target);
            }
        }
    }

    /// Dismisses popovers on presses outside them (and outside their anchor).
    pub(super) fn dismiss_outside_popovers(&mut self, hit: Option<NodeId>) {
        for popover in self.popovers() {
            let anchor = self.nodes[&popover].parent;
            let inside = hit.is_some_and(|hit| self.within(hit, popover) || Some(hit) == anchor);
            if !inside {
                self.queue_update(popover, ControlUpdate::Dismiss);
                self.invalidate_paint(popover);
                self.pending |= Dirty::ACCESSIBILITY;
            }
        }
    }

    /// The popover around the focus, for Escape.
    pub(super) fn focused_popover(&self) -> Option<NodeId> {
        let mut next = self.focused;
        while let Some(id) = next {
            let node = self.nodes.get(&id)?;
            if node.kind == Kind::Popover {
                return Some(id);
            }
            next = node.parent;
        }
        None
    }

    pub(super) fn dismiss_popover(&mut self, popover: NodeId) -> bool {
        if self
            .nodes
            .get(&popover)
            .is_none_or(|node| node.kind != Kind::Popover)
        {
            return false;
        }
        self.queue_update(popover, ControlUpdate::Dismiss);
        self.invalidate_paint(popover);
        self.pending |= Dirty::ACCESSIBILITY;
        true
    }
}
