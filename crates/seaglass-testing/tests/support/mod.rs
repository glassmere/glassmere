use seaglass_core::{App, NodeId};
use seaglass_testing::Harness;

/// The node with this key anywhere in the tree.
pub fn find_keyed<A: App>(harness: &Harness<A>, key: &str) -> NodeId {
    let context = harness.context();
    let mut pending = vec![context.root()];
    while let Some(id) = pending.pop() {
        if context
            .details(id)
            .is_some_and(|details| details.key == key)
        {
            return id;
        }
        if let Some(node) = context.node(id) {
            pending.extend(node.children);
        }
    }
    panic!("no node keyed {key:?}");
}
