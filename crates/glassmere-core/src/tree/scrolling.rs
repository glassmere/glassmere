//! Scroll offsets on both axes, wheel routing and revealing a node.

use super::*;

impl Context {
    pub(super) fn set_scroll(&mut self, id: NodeId, offset: f32) -> bool {
        if !offset.is_finite() {
            return false;
        }
        let Some(node) = self.nodes.get_mut(&id) else {
            return false;
        };
        if !matches!(
            node.kind,
            Kind::Scroll | Kind::Popup | Kind::Collection | Kind::TextArea | Kind::RichEditor
        ) {
            return false;
        }
        let offset = offset.clamp(0.0, node.scroll.maximum);
        if node.scroll.offset == offset {
            return false;
        }
        node.scroll.offset = offset;
        self.scrolled(id);
        true
    }

    /// Horizontal offset from the start side.
    pub(super) fn set_scroll_x(&mut self, id: NodeId, offset: f32) -> bool {
        if !offset.is_finite() {
            return false;
        }
        let Some(node) = self.nodes.get_mut(&id) else {
            return false;
        };
        if !matches!(node.kind, Kind::Scroll | Kind::Collection | Kind::TextArea) {
            return false;
        }
        let offset = offset.clamp(0.0, node.scroll.maximum_x);
        if node.scroll.offset_x == offset {
            return false;
        }
        node.scroll.offset_x = offset;
        self.scrolled(id);
        true
    }

    /// Moves the view by physical pixels: positive is to the right, which in a
    /// right-to-left container is back toward the start.
    pub(super) fn scroll_x_by(&mut self, id: NodeId, delta: f32) -> bool {
        let Some(node) = self.nodes.get(&id) else {
            return false;
        };
        if delta == 0.0 {
            return false;
        }
        // A grid scrolls its own columns, so that frozen ones stay put and
        // only the ones in view exist; it is told instead of being moved.
        if matches!(
            node.control,
            Control::Collection {
                kind: crate::collection::CollectionKind::Grid,
                ..
            }
        ) {
            return self.grid_scroll_x(id, delta);
        }
        let delta = if node.scrolls_x_from_end() {
            -delta
        } else {
            delta
        };
        self.set_scroll_x(id, node.scroll.offset_x + delta)
    }

    /// After an offset changed: children move, and the pointer may be over
    /// something else now.
    pub(super) fn scrolled(&mut self, id: NodeId) {
        self.invalidate_paint(id);
        self.geometry_dirty = true;
        self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        self.refresh_bounds(self.scene.size);
        let hovered = self.pointer.and_then(|position| self.hit_test(position));
        for id in [self.hovered, hovered].into_iter().flatten() {
            self.invalidate_paint(id);
        }
        self.hovered = hovered;
    }

    /// The innermost container under the pointer that can still move sideways.
    pub(super) fn scroll_x_at_pointer(&mut self, delta: f32) -> bool {
        let Some(pointer) = self.pointer else {
            return false;
        };
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        order.into_iter().rev().any(|id| {
            let node = &self.nodes[&id];
            matches!(node.kind, Kind::Scroll | Kind::Collection | Kind::TextArea)
                && node.clip.contains(pointer)
                && self.scroll_x_by(id, delta)
        })
    }

    pub(super) fn scroll_at_pointer(&mut self, delta: f32) -> bool {
        let Some(pointer) = self.pointer else {
            return false;
        };
        if let Some(popup) = self.popup
            && let Some(panel) = self.nodes[&popup.owner].children.first().copied()
            && self.nodes[&panel].clip.contains(pointer)
        {
            return self.set_scroll(panel, self.nodes[&panel].scroll.offset + delta);
        }
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        for id in order.into_iter().rev() {
            let node = &self.nodes[&id];
            if matches!(
                node.kind,
                Kind::Scroll | Kind::Popup | Kind::Collection | Kind::TextArea | Kind::RichEditor
            ) && node.clip.contains(pointer)
                && self.set_scroll(id, node.scroll.offset + delta)
            {
                return true;
            }
        }
        false
    }

    pub(super) fn scroll_into_view(&mut self, target: NodeId) -> bool {
        let mut parent = self.nodes.get(&target).and_then(|node| node.parent);
        let mut changed = false;
        while let Some(id) = parent {
            let node = &self.nodes[&id];
            if matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection) {
                let bounds = self.nodes[&target].bounds;
                let top = node.bounds.y + node.style.padding.max(0.0);
                let bottom = node.bounds.y + node.bounds.height - node.style.padding.max(0.0);
                let delta = if bounds.y < top {
                    bounds.y - top
                } else if bounds.y + bounds.height > bottom {
                    bounds.y + bounds.height - bottom
                } else {
                    0.0
                };
                changed |= self.set_scroll(id, node.scroll.offset + delta);
                // The same for the other axis; the start edge wins when the
                // target is wider than the view.
                let node = &self.nodes[&id];
                let bounds = self.nodes[&target].bounds;
                let padding = node.style.padding.max(0.0);
                let (left, right) = (
                    node.bounds.x + padding,
                    node.bounds.x + node.bounds.width - padding,
                );
                let (before, after) = (bounds.x - left, bounds.x + bounds.width - right);
                let sideways = if node.resolved.rtl() {
                    if after > 0.0 { after } else { before.min(0.0) }
                } else if before < 0.0 {
                    before
                } else {
                    after.max(0.0)
                };
                changed |= self.scroll_x_by(id, sideways);
                if self.nodes[&id].kind == Kind::Popup {
                    break;
                }
            }
            parent = self.nodes[&id].parent;
        }
        changed
    }
}
