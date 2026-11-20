//! Hit testing, routing an event to the nodes on its path, and the input
//! events themselves.

use super::*;
use alloc::vec;
use alloc::vec::Vec;

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
        let popovers = self.popovers();
        let panels = self.popup_panels();
        let mut hit = |_, node: &Node| {
            node.clip.contains(position) && {
                let dialog_surface = match node.kind {
                    Kind::Dialog => matches!(node.control, Control::Dialog { modal: true, .. }),
                    Kind::Container => node
                        .parent
                        .is_some_and(|parent| self.nodes[&parent].kind == Kind::Dialog),
                    _ => false,
                };
                node.takes_pointer()
                    // Floating groups block clicks to whatever lies under them.
                    || matches!(
                        node.kind,
                        Kind::Popup | Kind::Popover | Kind::DockFloat | Kind::Toast
                    )
                    || dialog_surface
            }
        };
        // Popovers are over the root, and a submenu is over the menu it
        // opened from: the last panel is searched first.
        panels
            .iter()
            .rev()
            .chain(popovers.iter().rev())
            .chain(core::iter::once(&self.root))
            .find_map(|top| self.last_in_order(*top, &mut hit))
    }

    /// Returns true if a declaration/redraw is needed. Pointer capture survives
    /// leaving the window, but activation requires release over the same button.
    pub fn event(&mut self, event: Event) -> bool {
        if self.escape_tooltip(&event) {
            return true;
        }
        // Areas see an event before and after the widgets, and may keep it
        // from them.
        let waiting = |context: &Self| -> usize {
            let pending = context.pending_area_events.borrow();
            pending.values().map(Vec::len).sum()
        };
        let waited = waiting(self);
        if self.travelled(&event) {
            self.note_focus();
            return true;
        }
        // What reached an area is handed over at the next declaration, so
        // there has to be one even when no widget made anything of the event.
        let arrived = waiting(self) > waited;
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
        let focus = self.noted_focus;
        let menus = self.popup_levels().count();
        let mut handled = self.handle_event(event.clone());
        // A press outside a menu closes it and still goes on to what is under
        // it, which may change nothing else, and the menu has to be redrawn.
        handled |= self.popup_levels().count() < menus;
        self.track_hover(&event);
        self.note_focus();
        self.track_focus(&event, focus, queued);
        // Moving the pointer changes what is hovered, which only repaints
        // unless the view observed it; holding something while moving it
        // (a scrollbar, a drag) changes what the view reads.
        let moved = matches!(event, Event::PointerMoved(_) | Event::PointerLeft)
            && self.queued.len() == queued
            && self.captured.is_none()
            && self.scrollbar_grab.is_none()
            && self.drag.is_none()
            && self.dock_drag.is_none();
        // Typing a draft into a number field that sends messages is not a
        // change the view reads: the draft waits for Enter or for leaving.
        let draft = matches!(event, Event::Text(_))
            && self.focused.is_some_and(|id| self.sends_numbers(id));
        self.view_stale |= handled && !moved && !draft;
        handled || arrived
    }

    pub(super) fn handle_event(&mut self, event: Event) -> bool {
        if let Some(handled) = self.scrollbar_event(&event) {
            return handled;
        }
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
            return match self.shortcut_target(shortcut) {
                Some(id) => self.activate(id),
                None => self.date_picker_shortcut(shortcut),
            };
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
                self.focused = self
                    .hovered
                    // A press on a reveal button leaves the keyboard with
                    // its password field, so typing goes on.
                    .map(|id| {
                        self.reveal_field(id)
                            .or_else(|| self.calendar_focus_of(id))
                            .unwrap_or(id)
                    })
                    .filter(|id| self.nodes[id].focusable());
                self.quiet_focus = self.focused.filter(|&id| self.in_title_bar(id));
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
                        // A link answers Enter, as one on a page does, and not Space.
                        .filter(|id| {
                            self.nodes[id].activatable()
                                && !(key == Key::Space && self.nodes[id].kind == Kind::Link)
                        })
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
            if !node.enabled || declared != wanted || !self.attached(*id) {
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
            if !node.enabled
                || binding != shortcut
                || !(*while_editing || !self.accepts_text())
                || !self.attached(*id)
            {
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
        // A wheel that only turns up and down moves a tab strip sideways.
        if delta_x == 0.0 && delta_y != 0.0 && self.scroll_strip_at_pointer(delta_y) {
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
        before: EventState,
        after: EventState,
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
type EventState = (
    Option<NodeId>,
    Option<NodeId>,
    Option<NodeId>,
    Option<(NodeId, Key)>,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Button, DialogOptions, DockFloat, DockLayout, DockNode, DockOptions, DockPanel, DockTabs,
        Label, Length, MenuItem, MonospaceMeasurer, PopoverOptions, Style, Toast, Ui,
    };
    use alloc::collections::BTreeSet;
    use glassmere_scene::Size;

    const WINDOW: Size = Size::new(320.0, 260.0);

    fn sized(width: f32, height: f32) -> Style {
        Style::default()
            .width(Length::Px(width))
            .height(Length::Px(height))
    }

    fn button(ui: &mut Ui<'_>, key: &'static str, style: Style, enabled: bool) {
        ui.keyed(key, |ui| {
            ui.button(Button::new(key).enabled(enabled).style(style))
        });
    }

    /// Overlapping siblings, nested scroll views, two popovers (one over the
    /// anchor of the next), a context menu, a floating dock group and, when
    /// asked, a modal dialog or a dialog that is not modal.
    fn page(ui: &mut Ui<'_>, dialog: Dialog) {
        ui.keyed("page", |ui| {
            ui.column(Style::default().gap(0.0), |ui| {
                ui.keyed("row", |ui| {
                    ui.row(Style::default().gap(0.0), |ui| {
                        button(ui, "a", sized(80.0, 40.0), true);
                        button(ui, "b", sized(80.0, 40.0).offset(-30.0, 0.0), true);
                        button(ui, "c", sized(80.0, 40.0).offset(-40.0, 10.0), false);
                    })
                });
                ui.keyed("outer", |ui| {
                    ui.scroll(sized(260.0, 120.0), |ui| {
                        ui.keyed("inner", |ui| {
                            ui.scroll(sized(200.0, 70.0), |ui| {
                                for key in ["i1", "i2", "i3", "i4"] {
                                    button(ui, key, sized(180.0, 30.0), true);
                                }
                            })
                        });
                        ui.keyed("label", |ui| {
                            ui.label(Label::new("text").style(sized(90.0, 20.0)))
                        });
                        for key in ["o1", "o2", "o3"] {
                            button(ui, key, sized(240.0, 40.0), true);
                        }
                    })
                });
                ui.keyed("menu", |ui| {
                    ui.context_menu(
                        &[MenuItem::new("Copy"), MenuItem::new("Paste")],
                        sized(120.0, 40.0),
                        |_: usize| (),
                        |ui| button(ui, "inside", sized(60.0, 20.0), true),
                    );
                });
                ui.keyed("anchor", |ui| ui.button("Anchor"));
                ui.keyed("pop", |ui| {
                    ui.popover(
                        true,
                        PopoverOptions::default(),
                        |_| (),
                        |ui| {
                            button(ui, "p1", sized(100.0, 30.0), true);
                        },
                    );
                });
                ui.keyed("later", |ui| ui.button("Later"));
                ui.keyed("dock", |ui| {
                    let layout = DockLayout {
                        root: Some(DockNode::Tabs(DockTabs {
                            panels: vec!["main".into()],
                            active: 0,
                        })),
                        floating: vec![DockFloat {
                            x: 60.0,
                            y: 20.0,
                            width: 140.0,
                            height: 90.0,
                            tabs: DockTabs {
                                panels: vec!["tools".into()],
                                active: 0,
                            },
                        }],
                        windows: Vec::new(),
                    };
                    ui.dock(
                        &layout,
                        &[
                            DockPanel::new("main", "Main"),
                            DockPanel::new("tools", "Tools"),
                        ],
                        DockOptions {
                            style: sized(260.0, 150.0).offset(0.0, 0.0),
                            ..DockOptions::default()
                        },
                        |_| (),
                        |ui, id| ui.label(id),
                    );
                });
                ui.keyed("pop2", |ui| {
                    ui.popover(
                        true,
                        PopoverOptions::default(),
                        |_| (),
                        |ui| {
                            button(ui, "p2", sized(120.0, 50.0), true);
                        },
                    );
                });
            })
        });
        ui.dialog(
            "confirm",
            dialog == Dialog::Modal,
            DialogOptions::new("Sure?").modal(true),
            |_| (),
            |ui| button(ui, "yes", sized(60.0, 24.0), true),
        );
        ui.dialog(
            "note",
            dialog == Dialog::Plain,
            DialogOptions::new("Note").modal(false),
            |_| (),
            |ui| button(ui, "ok", sized(50.0, 24.0), true),
        );
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Dialog {
        None,
        Modal,
        Plain,
    }

    /// The node a point is over, found the way a flat list of the whole tree
    /// is searched from the back: the root, then the popovers, then the
    /// panels.
    fn reference(context: &Context, position: Point) -> Option<NodeId> {
        let mut order = Vec::new();
        context.order(context.root, &mut order);
        for popover in context.popovers() {
            context.order(popover, &mut order);
        }
        for panel in context.popup_panels() {
            context.order(panel, &mut order);
        }
        order.into_iter().rev().find(|id| {
            let node = &context.nodes[id];
            let dialog_surface = match node.kind {
                Kind::Dialog => matches!(node.control, Control::Dialog { modal: true, .. }),
                Kind::Container => node
                    .parent
                    .is_some_and(|parent| context.nodes[&parent].kind == Kind::Dialog),
                _ => false,
            };
            (node.takes_pointer()
                || matches!(
                    node.kind,
                    Kind::Popup | Kind::Popover | Kind::DockFloat | Kind::Toast
                )
                || dialog_surface)
                && node.clip.contains(position)
        })
    }

    fn grid() -> Vec<Point> {
        let mut points = vec![
            Point {
                x: f32::NAN,
                y: 10.0,
            },
            Point {
                x: 10.0,
                y: f32::NAN,
            },
            Point {
                x: f32::INFINITY,
                y: 0.0,
            },
            Point { x: -5.0, y: -5.0 },
            Point { x: 400.0, y: 300.0 },
        ];
        for row in 0..30 {
            for column in 0..38 {
                points.push(Point {
                    x: column as f32 * 9.0 - 6.0,
                    y: row as f32 * 9.0 - 6.0,
                });
            }
        }
        points
    }

    /// The grid, and a point just inside each corner and at the middle of
    /// every node, so that thin or small nodes are tried too.
    fn probes(context: &Context) -> Vec<Point> {
        let mut points = grid();
        for node in context.nodes.values() {
            let rect = node.bounds;
            for (x, y) in [
                (rect.x + 1.0, rect.y + 1.0),
                (rect.x + rect.width - 1.0, rect.y + rect.height - 1.0),
                (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0),
            ] {
                points.push(Point { x, y });
            }
        }
        points
    }

    /// Hit tests the probes against the flat search, and returns how many
    /// different nodes were found and whether one of each of `kinds` was.
    fn agrees(context: &Context, kinds: &[Kind]) -> usize {
        let mut found = BTreeSet::new();
        for position in probes(context) {
            let hit = context.hit_test(position);
            assert_eq!(hit, reference(context, position), "at {position:?}");
            found.insert(hit.map(|id| id.0));
        }
        for kind in kinds {
            assert!(
                found
                    .iter()
                    .flatten()
                    .any(|id| context.nodes[&NodeId(*id)].kind == *kind),
                "no point is over a {kind:?}"
            );
        }
        found.len()
    }

    fn run(context: &mut Context, dialog: Dialog) {
        context.run(WINDOW, &mut MonospaceMeasurer, |ui| page(ui, dialog));
    }

    #[test]
    fn hit_test_finds_what_a_search_of_the_flattened_tree_finds() {
        let mut context = Context::new();
        run(&mut context, Dialog::None);
        let inner = context.find(&["page", "outer", "inner"]).unwrap();
        let outer = context.find(&["page", "outer"]).unwrap();
        context.set_scroll(inner, 25.0);
        context.set_scroll(outer, 40.0);
        run(&mut context, Dialog::None);
        context.push_toast(Toast::new("Saved"));
        run(&mut context, Dialog::None);
        // Open a menu over the page, so that its panel is searched first.
        let area = context.find(&["page", "menu"]).unwrap();
        let area = context.nodes[&area].bounds;
        context.event(Event::ContextMenu(Point {
            x: area.x + 10.0,
            y: area.y + 10.0,
        }));
        run(&mut context, Dialog::None);
        assert!(!context.popup_panels().is_empty());
        assert_eq!(context.popovers().len(), 2);
        assert!(agrees(&context, &[Kind::DockFloat, Kind::Toast]) > 12);
        // A modal dialog stands over all of it.
        run(&mut context, Dialog::Modal);
        assert!(agrees(&context, &[Kind::Dialog, Kind::Container]) > 3);
        // A dialog that is not modal leaves the page under it to the pointer.
        run(&mut context, Dialog::Plain);
        assert!(agrees(&context, &[Kind::DockFloat, Kind::Container]) > 12);
    }
    /// How many nodes of each kind the editors' counters should hold.
    fn recounted(context: &Context) -> (usize, usize) {
        let count = |kind| {
            context
                .nodes
                .values()
                .filter(|node| node.kind == kind)
                .count()
        };
        (count(Kind::TextArea), count(Kind::RichEditor))
    }

    #[test]
    fn the_editor_counters_follow_the_nodes_that_come_and_go() {
        use crate::{CodeEditorOptions, RichDocument, RichEditorOptions, TextDocument};
        let text = TextDocument::from_text("one\ntwo");
        let notes = RichDocument::new();
        // Which of two text areas and one rich editor are declared, and
        // whether a label stands in the first one's place.
        let declare =
            |context: &mut Context, first: bool, second: bool, rich: bool, label: bool| {
                context.run(WINDOW, &mut MonospaceMeasurer, |ui| {
                    if label {
                        ui.keyed("first", |ui| ui.label("first"));
                    }
                    if first {
                        ui.keyed("first", |ui| {
                            ui.text_area(&text, CodeEditorOptions::default(), |_| ())
                        });
                    }
                    if second {
                        ui.keyed("second", |ui| {
                            ui.code_editor(&text, CodeEditorOptions::default(), |_| ())
                        });
                    }
                    if rich {
                        ui.keyed("rich", |ui| {
                            ui.rich_editor(&notes, RichEditorOptions::default(), |_| ());
                        });
                    }
                });
                (context.text_area_count, context.rich_editor_count)
            };
        let mut context = Context::new();
        for (first, second, rich, label, expected) in [
            (false, false, false, false, (0, 0)),
            (true, false, false, false, (1, 0)),
            (true, true, true, false, (2, 1)),
            (true, true, true, false, (2, 1)),
            (false, true, true, true, (1, 1)),
            (true, true, false, false, (2, 0)),
            (false, false, true, false, (0, 1)),
            (false, false, false, false, (0, 0)),
        ] {
            let counts = declare(&mut context, first, second, rich, label);
            assert_eq!(counts, expected);
            assert_eq!(counts, recounted(&context));
        }
    }
}
