use super::*;
use crate::{Axis, Length, Tooltip, TooltipPlacement, panes::TOOLTIP_DELAY};
use std::borrow::Cow;

/// How soon after a tooltip goes away the next one is shown without its full
/// delay, and the delay it then has at most: someone moving along a row of
/// buttons wants each one's help at once.
const FOLLOW_WINDOW: Duration = Duration::from_millis(300);
const FOLLOW_DELAY: Duration = Duration::from_millis(100);

/// The space between a tooltip and the widget it is for.
const TOOLTIP_GAP: f32 = 6.0;

/// What brought a tooltip up, which decides what takes it away again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Dwell {
    /// The pointer rested on the widget.
    Pointer,
    /// The keyboard moved focus to the widget.
    Focus,
}

/// The side of a widget a tooltip is on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Above,
    Below,
    Left,
    Right,
}

impl Side {
    fn opposite(self) -> Self {
        match self {
            Self::Above => Self::Below,
            Self::Below => Self::Above,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// Where a tooltip of `size` goes against `anchor`, inside `window`: on the
/// side `placement` prefers, on the opposite one when that side has no room,
/// and in any case within the window.
fn place_tooltip(
    placement: TooltipPlacement,
    rtl: bool,
    anchor: Rect,
    size: Size,
    window: Rect,
) -> Rect {
    let side = match placement {
        TooltipPlacement::Auto | TooltipPlacement::Below => Side::Below,
        TooltipPlacement::Above => Side::Above,
        TooltipPlacement::Before if rtl => Side::Right,
        TooltipPlacement::Before => Side::Left,
        TooltipPlacement::After if rtl => Side::Left,
        TooltipPlacement::After => Side::Right,
    };
    let (right, bottom) = (anchor.x + anchor.width, anchor.y + anchor.height);
    // Above and below line up with the start edge; the sides are centred.
    let start = if rtl { right - size.width } else { anchor.x };
    let middle = anchor.y + (anchor.height - size.height) / 2.0;
    let at = |side: Side| match side {
        Side::Below => (start, bottom + TOOLTIP_GAP),
        Side::Above => (start, anchor.y - size.height - TOOLTIP_GAP),
        Side::Left => (anchor.x - size.width - TOOLTIP_GAP, middle),
        Side::Right => (right + TOOLTIP_GAP, middle),
    };
    let fits = |side: Side| {
        let (x, y) = at(side);
        match side {
            Side::Below => y + size.height <= window.y + window.height,
            Side::Above => y >= window.y,
            Side::Right => x + size.width <= window.x + window.width,
            Side::Left => x >= window.x,
        }
    };
    let (x, y) = at(if fits(side) { side } else { side.opposite() });
    Rect {
        x: x.clamp(window.x, window.x + (window.width - size.width).max(0.0)),
        y: y.clamp(window.y, window.y + (window.height - size.height).max(0.0)),
        width: size.width,
        height: size.height,
    }
}

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
        let tooltip = [
            self.hover_since.map(|dwell| (dwell, Dwell::Pointer)),
            self.focus_since.map(|dwell| (dwell, Dwell::Focus)),
        ]
        .into_iter()
        .flatten()
        .filter_map(|((id, since), by)| self.tooltip_due(id, since, by))
        .min();
        [
            tooltip,
            self.toast_wake(),
            self.touch_wake(),
            self.grid_wake(),
            self.track_wake(),
            self.edge_scroll_wake(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// The tooltip `by` brings up for the node, or for an editor under the
    /// pointer the diagnostic there. Windows shows its own over a title bar's
    /// buttons, after its own delay, so that it does not come before the snap
    /// layouts.
    fn tooltip_for(&self, id: NodeId, by: Dwell) -> Option<Cow<'_, Tooltip>> {
        let node = self.nodes.get(&id)?;
        if cfg!(windows)
            && matches!(
                node.control,
                Control::MinimizeButton | Control::MaximizeButton | Control::CloseButton
            )
        {
            return None;
        }
        if let Some(tooltip) = node
            .tooltip
            .as_deref()
            .filter(|tooltip| !tooltip.text.trim().is_empty())
        {
            return Some(Cow::Borrowed(tooltip));
        }
        // A spot in the text is something only the pointer can point at.
        let message = self.area_diagnostic(id).filter(|_| by == Dwell::Pointer)?;
        Some(Cow::Owned(Tooltip::new(message)))
    }

    /// When the tooltip a dwell on `id` since `since` waits for is due, if
    /// there is one still to show.
    fn tooltip_due(&self, id: NodeId, since: Duration, by: Dwell) -> Option<Duration> {
        // It is on show already, or the pointer's is.
        if self
            .tooltip
            .is_some_and(|(owner, shown)| owner == id || shown == Dwell::Pointer)
        {
            return None;
        }
        let delay = self.tooltip_for(id, by)?.delay.unwrap_or(TOOLTIP_DELAY);
        let following = self
            .tooltip_left
            .is_some_and(|left| since <= left + FOLLOW_WINDOW);
        Some(
            since
                + if following {
                    delay.min(FOLLOW_DELAY)
                } else {
                    delay
                },
        )
    }

    /// Tracks hover dwell after input; hides a visible tooltip when the pointer
    /// leaves its owner or any press or key arrives.
    pub(super) fn track_hover(&mut self, event: &Event) {
        // A press, a scroll and losing the window end every tooltip.
        if matches!(
            event,
            Event::PointerDown
                | Event::PointerDownWithShift
                | Event::FocusLost
                | Event::Scroll { .. }
        ) {
            self.hover_since = None;
            self.focus_since = None;
            self.hide_tooltip(false);
            return;
        }
        // A key, or the pointer leaving the window, ends the pointer's only.
        if matches!(event, Event::KeyDown { .. } | Event::PointerLeft) {
            self.hover_since = None;
            self.hide_pointer_tooltip(false);
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
                    .is_some_and(|(owner, _)| Some(owner) != self.hovered)
            {
                self.hide_pointer_tooltip(true);
            }
        }
    }

    /// Takes the tooltip off. `left` when it went because its widget was left,
    /// which makes the next tooltip quick.
    fn hide_tooltip(&mut self, left: bool) {
        if self.tooltip.take().is_some() {
            if left {
                self.tooltip_left = Some(self.now);
            }
            self.pending |= Dirty::PAINT;
        }
    }

    fn hide_pointer_tooltip(&mut self, left: bool) {
        if self
            .tooltip
            .is_some_and(|(_, shown)| shown == Dwell::Pointer)
        {
            self.hide_tooltip(left);
        }
    }

    /// Focus moved: the tooltip the keyboard brought up, or was about to, was
    /// for the widget that had it.
    pub(super) fn blur_tooltip(&mut self) {
        self.focus_since = None;
        if self.tooltip.is_some_and(|(_, shown)| shown == Dwell::Focus) {
            self.hide_tooltip(true);
        }
    }

    /// Tooltips for the keyboard, after `event` has been handled: a key that
    /// moved focus starts the delay of the tooltip of the widget it moved to,
    /// unless it also acted on that widget (changed its value, pressed it),
    /// which takes the tooltip away, as a press does. `before` is the focus
    /// before the event and `queued` how much was queued by then.
    pub(super) fn track_focus(&mut self, event: &Event, before: Option<NodeId>, queued: usize) {
        if !matches!(
            event,
            Event::KeyDown { .. } | Event::KeyUp(_) | Event::Text(_)
        ) {
            return;
        }
        let acted = self
            .queued
            .get(queued..)
            .is_some_and(|new| new.iter().any(|(id, _)| Some(*id) == self.focused));
        if acted {
            self.focus_since = None;
            if self.tooltip.is_some_and(|(_, shown)| shown == Dwell::Focus) {
                self.hide_tooltip(false);
            }
        } else if self.noted_focus != before
            // Escape leaves what it dismissed, and focus goes back to where it was.
            && matches!(event, Event::KeyDown { key, .. } if *key != Key::Escape)
        {
            self.focus_since = self.noted_focus.map(|id| (id, self.now));
        }
    }

    /// Escape takes a tooltip the keyboard brought up away and does nothing
    /// else, as it does for the innermost popup, popover or dialog. True when
    /// it did.
    pub(super) fn escape_tooltip(&mut self, event: &Event) -> bool {
        if !matches!(
            event,
            Event::KeyDown {
                key: Key::Escape,
                ..
            }
        ) {
            return false;
        }
        self.focus_since = None;
        if self.tooltip.is_none_or(|(_, shown)| shown != Dwell::Focus) {
            return false;
        }
        self.hide_tooltip(false);
        true
    }

    /// Shows a due tooltip. Called during declaration, after reconciliation.
    pub(super) fn update_tooltip(&mut self) {
        let due = |context: &Self, dwell: Option<(NodeId, Duration)>, by| {
            dwell
                .and_then(|(id, since)| Some((id, context.tooltip_due(id, since, by)?)))
                .filter(|(_, due)| context.now >= *due)
                .map(|(id, _)| id)
        };
        // The pointer's goes first, and replaces one the keyboard brought up.
        if let Some(id) = due(self, self.hover_since, Dwell::Pointer) {
            self.tooltip = Some((id, Dwell::Pointer));
            self.focus_since = None;
        } else if let Some(id) = due(self, self.focus_since, Dwell::Focus) {
            self.tooltip = Some((id, Dwell::Focus));
        } else {
            return;
        }
        self.pending |= Dirty::PAINT;
    }

    /// Paints the visible tooltip after all content and overlays.
    pub(super) fn paint_tooltip(&mut self, scene: &mut Scene, measurer: &mut impl TextMeasurer) {
        let Some((owner, by)) = self.tooltip else {
            return;
        };
        let Some(node) = self.nodes.get(&owner) else {
            self.tooltip = None;
            return;
        };
        let (anchor, rtl, in_editor) = (
            node.bounds,
            node.resolved.rtl(),
            node.kind == Kind::TextArea,
        );
        let Some(tooltip) = self.tooltip_for(owner, by).map(Cow::into_owned) else {
            return;
        };
        let custom = tooltip.style.as_deref();
        let positive = |value: Option<f32>, default: f32| {
            value.filter(|value| *value > 0.0).unwrap_or(default)
        };
        let padding = positive(custom.map(|style| style.padding), 8.0);
        let radius = positive(custom.map(|style| style.radius), 6.0);
        let style = TextStyle {
            font_size: custom.and_then(|style| style.font_size).unwrap_or(13.0),
            family: custom
                .and_then(|style| style.font_family.clone())
                .unwrap_or_else(|| self.theme.font_family.clone()),
            weight: custom.and_then(|style| style.font_weight).unwrap_or(400),
            italic: custom.and_then(|style| style.italic).unwrap_or(false),
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let wrap = match custom.map(|style| style.max_width) {
            Some(Length::Px(width)) => (width - padding * 2.0).max(1.0),
            _ => 280.0,
        };
        let measured = measurer.measure_styled(&tooltip.text, &style, Some(wrap));
        let window = self.nodes[&self.root].bounds;
        let size = Size::new(
            (measured.width + padding * 2.0).min(window.width),
            measured.height + padding * 2.0,
        );
        // Editor messages belong to a spot in the text, not to the whole
        // editor, unless a placement was asked for.
        let pointer = self.pointer.filter(|_| {
            by == Dwell::Pointer && in_editor && tooltip.placement == TooltipPlacement::Auto
        });
        let rect = match pointer {
            Some(pointer) => {
                let y = pointer.y + 18.0;
                Rect {
                    x: pointer.x.clamp(0.0, (window.width - size.width).max(0.0)),
                    y: if y + size.height > window.height {
                        (pointer.y - size.height - TOOLTIP_GAP).max(0.0)
                    } else {
                        y
                    },
                    width: size.width,
                    height: size.height,
                }
            }
            None => place_tooltip(tooltip.placement, rtl, anchor, size, window),
        };
        self.painted_overlay(rect);
        scene.primitives.push(Primitive::Quad {
            rect,
            color: custom
                .and_then(|style| style.background)
                .unwrap_or(self.theme.popup),
            radius,
        });
        scene.primitives.push(Primitive::StyledText {
            rect: Rect {
                x: rect.x + padding,
                y: rect.y + padding,
                width: measured.width,
                height: measured.height,
            },
            text: tooltip.text,
            style,
            color: custom
                .and_then(|style| style.foreground)
                .unwrap_or(self.theme.text),
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
