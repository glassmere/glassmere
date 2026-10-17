use super::*;
use crate::collection::{
    CollectionInput, CollectionKind, FILTER_DELAY, FilterNote, FilterTimer, GridNode, ListAnchor,
};

impl Context {
    /// Records what a grid's view wants of its filters. The next batch
    /// starts or fires their delay.
    pub(crate) fn note_grid_filters(&self, rows: NodeId, note: FilterNote) {
        self.grid_filters.borrow_mut().push((rows, note));
    }

    /// Starts or restarts the filter delays of the grids that declared
    /// filters since the last batch. Firing is in `due_grid_filters`.
    pub(crate) fn start_grid_timers(&mut self) {
        let now = self.now;
        for (rows, note) in std::mem::take(&mut *self.grid_filters.borrow_mut()) {
            match note {
                FilterNote::Applied => {
                    self.grid_timers.remove(&rows);
                }
                // Sent without a delay: remembered as fired, so a model that
                // stays behind them is not sent them again.
                FilterNote::Sent(filters) => {
                    self.grid_timers.insert(
                        rows,
                        FilterTimer {
                            due: now,
                            filters,
                            fired: true,
                        },
                    );
                }
                // A declaration may still report filters that have just fired;
                // only different filters start a new delay.
                FilterNote::Waiting(filters) => {
                    if self
                        .grid_timers
                        .get(&rows)
                        .is_none_or(|timer| timer.filters != filters)
                    {
                        let due = now + FILTER_DELAY;
                        self.grid_timers.insert(
                            rows,
                            FilterTimer {
                                due,
                                filters,
                                fired: false,
                            },
                        );
                    }
                }
            }
        }
        let nodes = &self.nodes;
        self.grid_timers.retain(|id, _| nodes.contains_key(id));
        self.grid_nodes
            .get_mut()
            .retain(|id, _| nodes.contains_key(id));
        self.revealed
            .get_mut()
            .retain(|id, _| nodes.contains_key(id));
        self.list_anchors
            .get_mut()
            .retain(|id, _| nodes.contains_key(id));
        self.lost_selections
            .get_mut()
            .retain(|id, _| nodes.contains_key(id));
    }

    /// Whether a collection's lost selection is news: `lost` is the selected
    /// id its model no longer has, if any. Each lost id is reported once.
    pub(crate) fn report_lost_selection(&self, rows: NodeId, lost: Option<crate::ItemId>) -> bool {
        let mut reported = self.lost_selections.borrow_mut();
        match lost {
            Some(id) => reported.insert(rows, id) != Some(id),
            None => {
                reported.remove(&rows);
                false
            }
        }
    }

    /// The top row of a message-form list as its last declaration left it.
    pub(crate) fn list_anchor(&self, rows: NodeId) -> Option<ListAnchor> {
        self.list_anchors.borrow().get(&rows).copied()
    }

    pub(crate) fn set_list_anchor(&self, rows: NodeId, anchor: Option<ListAnchor>) {
        let mut anchors = self.list_anchors.borrow_mut();
        match anchor {
            Some(anchor) => drop(anchors.insert(rows, anchor)),
            None => drop(anchors.remove(&rows)),
        }
    }

    /// The kernel state of a message-form grid, for the declaration to change.
    /// `put_grid` gives it back.
    pub(crate) fn take_grid(&self, rows: NodeId) -> GridNode {
        self.grid_nodes
            .borrow_mut()
            .remove(&rows)
            .unwrap_or_default()
    }

    pub(crate) fn put_grid(&self, rows: NodeId, grid: GridNode) {
        self.grid_nodes.borrow_mut().insert(rows, grid);
    }

    /// Input for a collection that the next batch resolves, as if it came
    /// from the user: what a declaration cannot send itself.
    pub(crate) fn queue_grid_input(&self, rows: NodeId, input: CollectionInput) {
        self.grid_pending.borrow_mut().push((rows, input));
    }

    /// Moves what declarations queued into the collections' input.
    pub(super) fn flush_grid_pending(&mut self) {
        let pending = std::mem::take(self.grid_pending.get_mut());
        for (rows, input) in pending {
            self.collection_queue(rows, input);
        }
    }

    /// Whether the user edited this node's text in the batch just resolved.
    pub(crate) fn text_typed(&self, id: NodeId) -> bool {
        self.text_typed.contains(&id)
    }

    /// Whether typed filters of this grid are still waiting out their delay.
    pub(crate) fn grid_filters_waiting(&self, rows: NodeId) -> bool {
        self.grid_timers
            .get(&rows)
            .is_some_and(|timer| !timer.fired)
    }

    /// Whether these filters were already sent to the application, which has
    /// not applied them: they are not sent again.
    pub(crate) fn grid_filters_sent(
        &self,
        rows: NodeId,
        filters: &crate::collection::Filters,
    ) -> bool {
        self.grid_timers
            .get(&rows)
            .is_some_and(|timer| timer.fired && timer.filters == *filters)
    }

    /// Whether `target` is not what this collection last scrolled to; it
    /// is remembered, so a request is honored once.
    pub(crate) fn reveal_selection(
        &self,
        rows: NodeId,
        target: (Option<crate::ItemId>, u32),
    ) -> bool {
        self.revealed.borrow_mut().insert(rows, target) != Some(target)
    }

    /// The grids whose filter delay has passed since it was started, each once.
    /// Called first in a batch, so `FiltersDue` is resolved in that batch.
    pub(crate) fn due_grid_filters(&mut self) -> Vec<NodeId> {
        let now = self.now;
        let mut due = Vec::new();
        for (id, timer) in self.grid_timers.iter_mut() {
            if !timer.fired && timer.due <= now {
                timer.fired = true;
                due.push(*id);
            }
        }
        due
    }

    /// When the earliest filter delay that has not fired is due.
    pub(crate) fn grid_wake(&self) -> Option<Duration> {
        self.grid_timers
            .values()
            .filter(|timer| !timer.fired)
            .map(|timer| timer.due)
            .min()
    }

    pub(crate) fn collection_queue(&mut self, owner: NodeId, input: CollectionInput) -> bool {
        if !self
            .nodes
            .get(&owner)
            .is_some_and(|node| node.enabled && node.kind == Kind::Collection)
        {
            return false;
        }
        // What only a grid understands goes to its kernel state, which its
        // next declaration reads, when it keeps one.
        if (input.for_grid() || matches!(input, CollectionInput::Activate))
            && let Some(grid) = self.grid_nodes.get_mut().get_mut(&owner)
        {
            grid.inbox.push(input);
            self.view_stale = true;
            self.invalidate_paint(owner);
            self.pending |= Dirty::ACCESSIBILITY;
            return true;
        }
        self.note_update(owner);
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
                    expanded: Some(expanded),
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
                    return Some(self.collection_queue(
                        owner,
                        CollectionInput::SetExpanded {
                            item,
                            open: !expanded,
                        },
                    ));
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
