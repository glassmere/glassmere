//! Labels that follow a text source: a change to the source reaches the
//! node without the view running.

use super::*;

impl Context {
    /// Gives the node declared as element `seq` the text source it was
    /// declared with, or takes the one it had if it was declared without.
    /// Only a declaration does this.
    pub(super) fn attach_bound(&mut self, id: NodeId, seq: u32) {
        let declared = {
            let bound = self.bound_new.borrow();
            if bound.is_empty() || seq == 0 {
                None
            } else {
                // Declared in order, so the sequence numbers only rise.
                bound
                    .binary_search_by_key(&seq, |(seq, ..)| *seq)
                    .ok()
                    .map(|at| (bound[at].1.clone(), bound[at].2))
            }
        };
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        match declared {
            Some((source, seen)) => {
                if node.bound.is_none() {
                    self.bound_nodes.push(id);
                }
                node.bound = Some(Box::new(Bound { source, seen }));
            }
            // A stale entry in `bound_nodes` is dropped by the next refresh.
            None => node.bound = None,
        }
    }

    /// Shows the text of every label whose source has moved since it was
    /// shown. Runs after a declaration and on a frame without one, so a
    /// change made by `update` is on screen whether or not the view ran.
    pub(super) fn refresh_bound(&mut self, measurer: &mut impl TextMeasurer) {
        if self.bound_nodes.is_empty() {
            return;
        }
        let mut nodes = core::mem::take(&mut self.bound_nodes);
        nodes.retain(|id| self.refresh_bound_node(*id, measurer));
        // A node that was bound during the refresh cannot happen, but keep
        // whatever was pushed in case it does.
        nodes.append(&mut self.bound_nodes);
        self.bound_nodes = nodes;
    }

    /// Returns whether the node still follows a source.
    fn refresh_bound_node(&mut self, id: NodeId, measurer: &mut impl TextMeasurer) -> bool {
        let Some(node) = self.nodes.get_mut(&id) else {
            return false;
        };
        let Some(bound) = node.bound.as_mut() else {
            return false;
        };
        let version = bound.source.version();
        if version == bound.seen {
            return true;
        }
        bound.seen = version;
        let text = bound.source.text();
        if node.text == text {
            return true;
        }
        node.text = text;
        let text_style = node.text_style();
        let measured = Measurement::text(&node.text, text_style, measurer);
        // `changed_frame` is a declaration's record of what it changed, and
        // what is prepared from it is for kinds this never binds, so it is
        // left alone; `pending` is what makes layout, paint and the
        // accessibility tree follow.
        let layout_id = node.layout_id;
        self.stats.text_measures += 1;
        if self.layout.get_node_context(layout_id) != Some(&measured) {
            self.layout
                .set_node_context(layout_id, Some(measured))
                .unwrap();
            self.pending |= Dirty::LAYOUT;
            self.diagnostic.layout_cause.get_or_insert(id);
        }
        self.pending |= Dirty::TEXT | Dirty::PAINT | Dirty::ACCESSIBILITY;
        self.invalidate_paint(id);
        true
    }
}
