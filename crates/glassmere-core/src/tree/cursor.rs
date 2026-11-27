//! The pointer a host shows over a node.

use super::*;
use crate::Cursor;

impl Context {
    /// The pointer to show over `id`. A `Style::cursor` on the node or the
    /// nearest ancestor that sets one wins. Otherwise it is what the widget
    /// is: an I-beam over a text field or selectable text, the resize
    /// pointers over a divider, a hand over one that takes a press, and
    /// the ordinary arrow over everything else, a layout container, a
    /// scroll area and a disabled widget included. The window's own
    /// buttons keep the arrow, as the system's do.
    pub fn cursor(&self, id: NodeId) -> Cursor {
        let mut next = Some(id);
        while let Some(at) = next {
            let Some(node) = self.nodes.get(&at) else {
                break;
            };
            if let Some(cursor) = node.style.cursor {
                return cursor;
            }
            next = node.parent;
        }
        let Some(node) = self.nodes.get(&id) else {
            return Cursor::Default;
        };
        if self.caption_buttons.contains(&Some(id)) {
            return Cursor::Default;
        }
        // A disabled field takes no typing, so it does not ask for the I-beam.
        if node.enabled
            && (matches!(node.control, Control::SelectableText)
                || matches!(
                    node.semantics().role,
                    Role::TextInput | Role::MultilineTextInput | Role::PasswordInput
                ))
        {
            return Cursor::Text;
        }
        match node.control {
            Control::Divider {
                axis: crate::Axis::Vertical,
                ..
            } => return Cursor::RowResize,
            Control::Divider { .. } | Control::Sizer { .. } => return Cursor::ColResize,
            _ => {}
        }
        // A scroll container is focusable for the keyboard and is not
        // something to press.
        if node.kind != Kind::Scroll && (node.focus_kind() || node.activatable()) {
            Cursor::Pointer
        } else {
            Cursor::Default
        }
    }
}
