use super::*;
use crate::collection::{CollectionInput, CollectionKind};

impl Context {
    fn collection_queue(&mut self, owner: NodeId, input: CollectionInput) -> bool {
        if !self
            .nodes
            .get(&owner)
            .is_some_and(|node| node.enabled && node.kind == Kind::Collection)
        {
            return false;
        }
        let update = self
            .updates
            .entry(owner)
            .or_insert_with(|| ControlUpdate::Collection(Vec::new()));
        if let ControlUpdate::Collection(queue) = update {
            queue.push(input);
        }
        self.invalidate_paint(owner);
        self.pending |= Dirty::ACCESSIBILITY;
        true
    }

    pub(super) fn collection_action(&mut self, target: NodeId, action: &Action) -> Option<bool> {
        let node = self.nodes.get(&target)?;
        let Control::CollectionRow { item, .. } = node.control else {
            return None;
        };
        if !node.enabled {
            return Some(false);
        }
        let owner = node.parent?;
        match action {
            Action::Activate | Action::Focus => {
                self.focus_control(owner);
                Some(self.collection_queue(owner, CollectionInput::Select(item)))
            }
            Action::Expand | Action::Collapse => {
                self.collection_queue(owner, CollectionInput::Select(item));
                Some(self.collection_queue(
                    owner,
                    CollectionInput::Expand(matches!(action, Action::Expand)),
                ))
            }
            _ => None,
        }
    }

    pub(super) fn collection_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::PointerDown | Event::PointerDownWithShift => {
                let id = self.pointer.and_then(|point| self.hit_test(point))?;
                let node = &self.nodes[&id];
                if node.kind == Kind::CollectionRow && node.enabled {
                    self.focus_control(node.parent.unwrap());
                    self.captured = Some(id);
                    return Some(true);
                }
            }
            Event::PointerUp => {
                let id = self.captured?;
                let node = &self.nodes[&id];
                if let Control::CollectionRow {
                    item,
                    kind: CollectionKind::Tree,
                    depth,
                    expanded: Some(_),
                    ..
                } = node.control
                    && self.hovered == Some(id)
                    && self.pointer.is_some_and(|point| {
                        point.x < node.bounds.x + (depth.min(100) * 18 + 28) as f32
                    })
                {
                    let owner = node.parent.unwrap();
                    self.captured = None;
                    return Some(self.collection_queue(owner, CollectionInput::Toggle(item)));
                }
            }
            Event::KeyDown { key, repeat, .. } => {
                let id = self.focused?;
                let node = &self.nodes[&id];
                let Control::Collection {
                    kind,
                    row_height,
                    selected,
                    ..
                } = node.control
                else {
                    return None;
                };
                if !node.enabled {
                    return Some(false);
                }
                let page = (node.bounds.height / row_height).floor().max(1.0) as isize;
                let input = match key {
                    Key::ArrowUp => CollectionInput::Move(-1),
                    Key::ArrowDown => CollectionInput::Move(1),
                    Key::PageUp => CollectionInput::Move(-page),
                    Key::PageDown => CollectionInput::Move(page),
                    Key::Home => CollectionInput::First,
                    Key::End => CollectionInput::Last,
                    Key::Enter if !repeat => CollectionInput::Activate,
                    Key::Space if !repeat && kind == CollectionKind::Tree => {
                        CollectionInput::Toggle(selected?)
                    }
                    Key::ArrowRight if kind == CollectionKind::Tree => {
                        CollectionInput::Expand(true)
                    }
                    Key::ArrowLeft if kind == CollectionKind::Tree => {
                        CollectionInput::Expand(false)
                    }
                    _ => return None,
                };
                return Some(self.collection_queue(id, input));
            }
            _ => {}
        }
        None
    }
}
