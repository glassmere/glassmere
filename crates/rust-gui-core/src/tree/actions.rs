//! Semantic actions: what assistive technology and tests ask a node to do.

use super::*;

impl Context {
    pub fn action(&mut self, target: NodeId, action: Action) -> bool {
        let focus = matches!(action, Action::Focus);
        let handled = self.handle_action(target, action);
        self.note_focus();
        // Focus only repaints unless the view observed it.
        self.view_stale |= handled && !focus;
        handled
    }

    pub(super) fn handle_action(&mut self, target: NodeId, action: Action) -> bool {
        if let Some(handled) = self.drag_action(target, &action) {
            return handled;
        }
        if let Some(handled) = self.collection_action(target, &action) {
            return handled;
        }
        if let Some(handled) = self.control_action(target, &action) {
            return handled;
        }
        let Some(node) = self.nodes.get(&target) else {
            return false;
        };
        match action {
            Action::Focus if node.focusable() => {
                if let Some(old) = self.focused {
                    self.invalidate_paint(old);
                }
                self.focused = Some(target);
                self.invalidate_paint(target);
                self.keyboard_pressed = None;
                self.scroll_into_view(target);
                self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
                true
            }
            Action::Activate if node.activatable() => self.activate(target),
            Action::ScrollBy(delta) => self.set_scroll(target, node.scroll.offset + delta),
            Action::ScrollTo(offset) => self.set_scroll(target, offset),
            Action::ScrollByX(delta) => self.scroll_x_by(target, delta),
            Action::ScrollToX(offset) => self.set_scroll_x(target, offset),
            Action::ScrollIntoView => self.scroll_into_view(target),
            _ => false,
        }
    }
}
