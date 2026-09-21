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

    pub(super) fn grid_scroll_x(&mut self, rows: NodeId, delta: f32) -> bool {
        self.collection_queue(rows, CollectionInput::ScrollX(delta))
    }

    /// Display position of the grid cell of `row` under `point`.
    fn grid_cell_at(&self, row: NodeId, point: Point) -> Option<usize> {
        let mut pending = self.nodes.get(&row)?.children.clone();
        while let Some(id) = pending.pop() {
            let node = &self.nodes[&id];
            if let Control::GridCell {
                column,
                header: false,
                ..
            } = node.control
            {
                // A cell scrolled under the frozen columns still has bounds
                // there; what can be pressed is what shows.
                if node.clip.contains(point) {
                    return Some(column);
                }
            } else {
                pending.extend(node.children.iter().copied());
            }
        }
        None
    }

    /// The grid's row collection when `focused` is an editor inside one of its cells.
    fn grid_of_editor(&self, focused: NodeId) -> Option<NodeId> {
        let mut next = self.nodes.get(&focused)?.parent;
        while let Some(id) = next {
            let node = self.nodes.get(&id)?;
            if let Control::CollectionRow { kind, .. } = node.control {
                return (kind == CollectionKind::Grid)
                    .then_some(node.parent)
                    .flatten();
            }
            next = node.parent;
        }
        None
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
                let point = self.pointer?;
                let id = self.hit_test(point)?;
                let node = &self.nodes[&id];
                if node.kind == Kind::CollectionRow && node.enabled {
                    let owner = node.parent.unwrap();
                    // In a grid the press also says which cell it landed on.
                    if let Control::CollectionRow {
                        item,
                        kind: CollectionKind::Grid,
                        ..
                    } = node.control
                        && let Some(position) = self.grid_cell_at(id, point)
                    {
                        self.collection_queue(owner, CollectionInput::Cell { item, position });
                    }
                    self.focus_control(owner);
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
                        // The expander sits at the start of the row, after its indent.
                        let reach = (depth.min(100) * 18 + 28) as f32;
                        if node.resolved.rtl() {
                            point.x > node.bounds.x + node.bounds.width - reach
                        } else {
                            point.x < node.bounds.x + reach
                        }
                    })
                {
                    let owner = node.parent.unwrap();
                    self.captured = None;
                    return Some(self.collection_queue(owner, CollectionInput::Toggle(item)));
                }
            }
            Event::KeyDown { key, shift, .. }
                if matches!(key, Key::Enter | Key::Tab | Key::Escape)
                    && self.popup.is_none()
                    && self
                        .focused
                        .is_some_and(|id| self.grid_of_editor(id).is_some()) =>
            {
                // An editor inside a grid cell: these keys end the edit, and
                // the grid decides where the cursor goes next.
                let owner = self.grid_of_editor(self.focused?)?;
                return Some(self.collection_queue(
                    owner,
                    CollectionInput::EditKey {
                        key: *key,
                        shift: *shift,
                    },
                ));
            }
            Event::Text(crate::TextEdit::Insert(text)) => {
                let id = self.focused?;
                let typed = matches!(
                    self.nodes.get(&id)?.control,
                    Control::Collection {
                        kind: CollectionKind::Grid,
                        ..
                    }
                ) && !text.chars().all(char::is_control);
                return typed
                    .then(|| self.collection_queue(id, CollectionInput::Type(text.clone())));
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
                    // Sideways moves the current cell the way the arrow points.
                    Key::ArrowLeft | Key::ArrowRight if kind == CollectionKind::Grid => {
                        let forward = (*key == Key::ArrowRight) != node.resolved.rtl();
                        CollectionInput::Column(if forward { 1 } else { -1 })
                    }
                    Key::F2 | Key::Space if !repeat && kind == CollectionKind::Grid => {
                        CollectionInput::Edit
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
