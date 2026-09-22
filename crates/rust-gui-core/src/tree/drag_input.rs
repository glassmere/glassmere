use super::*;
use crate::{DragPayload, DropKind, drag::DragState};

impl Context {
    pub(super) fn cancel_drag(&mut self) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        self.invalidate_paint(drag.source);
        if let Some(over) = drag.over {
            self.invalidate_paint(over);
        }
        self.pending |= Dirty::ACCESSIBILITY;
        true
    }
    /// The payload of the drag in progress, for a host that can hand it to the
    /// operating system once the pointer leaves the window.
    pub fn dragging(&self) -> Option<&DragPayload> {
        self.drag
            .as_ref()
            .filter(|drag| drag.active)
            .map(|drag| &drag.payload)
    }
    /// The host has handed the drag to the operating system: it stops being an
    /// in-app drag, and the pointer is free again because the system swallows
    /// the button release. Follow with `finish_exported_drag`.
    pub fn export_drag(&mut self) -> Option<DragPayload> {
        let drag = self.drag.as_ref().filter(|drag| drag.active)?;
        let (source, payload) = (drag.source, drag.payload.clone());
        self.cancel_drag();
        self.captured = None;
        self.exported_drag = Some(source);
        Some(payload)
    }
    /// Takes over a drag that started in another window of the same
    /// application. Nothing here is its source, so no node shows as the one
    /// being dragged, but drop targets light up and a release drops it.
    pub fn adopt_drag(&mut self, payload: DragPayload, at: Point) -> bool {
        if self.drag.is_some() {
            return false;
        }
        self.pointer = Some(at);
        let over = self.drop_at(at, payload.kind());
        self.drag = Some(Box::new(DragState {
            source: self.root,
            payload,
            start: at,
            active: true,
            over,
        }));
        self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        self.invalidate_paint(self.root);
        if let Some(over) = over {
            self.invalidate_paint(over);
        }
        true
    }

    /// Takes an adopted drag back, because the pointer has left this window
    /// for another. Unlike `export_drag` this tells nobody it ended: the
    /// window it started in still owns its source.
    pub fn withdraw_drag(&mut self) -> Option<DragPayload> {
        let payload = self
            .drag
            .as_ref()
            .filter(|drag| drag.active)?
            .payload
            .clone();
        self.cancel_drag();
        Some(payload)
    }

    /// Whether dropping this payload at this point would land on a target
    /// that takes it. A host carrying a drag between windows asks before it
    /// reports the drag as dropped.
    pub fn accepts_drop(&self, at: Point, payload: &DragPayload) -> bool {
        self.drop_at(at, payload.kind()).is_some()
    }

    /// Tells the source of an exported drag how it ended.
    pub fn finish_exported_drag(&mut self, outcome: crate::DragOutcome) {
        if let Some(source) = self.exported_drag.take()
            && self.nodes.contains_key(&source)
        {
            self.updates
                .insert(source, ControlUpdate::DragEnded(outcome));
            self.invalidate_paint(source);
        }
    }
    fn drop_at(&self, point: Point, kind: DropKind) -> Option<NodeId> {
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        order.into_iter().rev().find(|id| {
            let node = &self.nodes[id];
            node.enabled
                && node.clip.contains(point)
                && matches!(node.control, Control::DropTarget { accepts, .. } if accepts == kind)
        })
    }
    fn deliver_drop(&mut self, target: NodeId, payload: DragPayload) -> bool {
        let Some(node) = self.nodes.get(&target).filter(|node| node.enabled) else {
            return false;
        };
        if !matches!(node.control, Control::DropTarget { accepts, .. } if accepts == payload.kind())
        {
            return false;
        }
        // ponytail: hosts report one file per drop event; merge them into one delivery.
        match (self.updates.get_mut(&target), payload) {
            (
                Some(ControlUpdate::Drop(DragPayload::Files(existing))),
                DragPayload::Files(paths),
            ) => existing.extend(paths),
            (_, payload) => {
                self.updates.insert(target, ControlUpdate::Drop(payload));
            }
        }
        self.invalidate_paint(target);
        true
    }
    fn begin_drag(&mut self, target: NodeId, active: bool) -> bool {
        let Some(node) = self.nodes.get(&target).filter(|node| node.enabled) else {
            return false;
        };
        let Control::DragSource { payload, .. } = &node.control else {
            return false;
        };
        let payload = payload.clone();
        self.cancel_drag();
        self.drag = Some(Box::new(DragState {
            source: target,
            payload,
            start: self.pointer.unwrap_or_default(),
            active,
            over: None,
        }));
        self.invalidate_paint(target);
        self.pending |= Dirty::ACCESSIBILITY;
        true
    }
    pub(super) fn drag_action(&mut self, target: NodeId, action: &Action) -> Option<bool> {
        match action {
            Action::StartDrag => Some(self.begin_drag(target, true)),
            Action::Drop => {
                let payload = self
                    .drag
                    .as_ref()
                    .filter(|drag| drag.active)
                    .map(|drag| drag.payload.clone());
                let delivered = payload.is_some_and(|payload| self.deliver_drop(target, payload));
                if delivered {
                    self.cancel_drag();
                }
                Some(delivered)
            }
            _ => None,
        }
    }
    pub(super) fn drag_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::FocusLost => {
                self.cancel_drag();
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if self.drag.is_some() => {
                self.captured = None;
                self.keyboard_pressed = None;
                return Some(self.cancel_drag());
            }
            Event::PointerDown | Event::PointerDownWithShift => {
                let point = self.pointer?;
                let mut order = Vec::new();
                self.order(self.root, &mut order);
                if let Some(id) = order.into_iter().rev().find(|id| {
                    self.nodes[id].kind == Kind::DragSource
                        && self.nodes[id].enabled
                        && self.nodes[id].clip.contains(point)
                }) {
                    self.begin_drag(id, false);
                }
            }
            Event::PointerMoved(point) => {
                let drag = self.drag.as_ref()?;
                let dx = point.x - drag.start.x;
                let dy = point.y - drag.start.y;
                if !drag.active && dx * dx + dy * dy < 36.0 {
                    return None;
                }
                let target = self.drop_at(*point, drag.payload.kind());
                let source = drag.source;
                let old = drag.over;
                let drag = self.drag.as_mut().unwrap();
                drag.active = true;
                drag.over = target;
                self.pointer = Some(*point);
                self.captured = None;
                self.keyboard_pressed = None;
                for id in [Some(source), old, target].into_iter().flatten() {
                    self.invalidate_paint(id);
                }
                self.pending |= Dirty::ACCESSIBILITY;
                return Some(true);
            }
            Event::PointerUp => {
                let drag = self.drag.take()?;
                self.invalidate_paint(drag.source);
                self.pending |= Dirty::ACCESSIBILITY;
                if let Some(target) = drag.over {
                    self.invalidate_paint(target);
                }
                if drag.active {
                    self.captured = None;
                    if let Some(target) = drag.over {
                        self.deliver_drop(target, drag.payload);
                    }
                    return Some(true);
                }
            }
            Event::KeyDown {
                key: Key::Space,
                repeat: false,
                ..
            } if self.drag.is_none() => {
                let id = self.focused?;
                if self.nodes[&id].kind == Kind::DragSource {
                    return Some(self.begin_drag(id, true));
                }
            }
            Event::KeyDown {
                key: Key::Enter,
                repeat: false,
                ..
            } if self.drag.as_ref().is_some_and(|drag| drag.active) => {
                return self.drag_action(self.focused?, &Action::Drop);
            }
            Event::DropAt { position, payload } => {
                return Some(
                    self.drop_at(*position, payload.kind())
                        .is_some_and(|target| self.deliver_drop(target, payload.clone())),
                );
            }
            _ => {}
        }
        None
    }
}
