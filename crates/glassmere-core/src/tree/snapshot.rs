//! What the tree says about its nodes to tests, the inspector and
//! accessibility.

use super::*;

impl Context {
    /// Parent-first snapshots for platform accessibility adapters and inspectors.
    pub fn snapshots(&self) -> Vec<NodeSnapshot> {
        let mut order = Vec::with_capacity(self.nodes.len());
        self.order(self.root, &mut order);
        order.into_iter().filter_map(|id| self.node(id)).collect()
    }

    pub fn node(&self, id: NodeId) -> Option<NodeSnapshot> {
        self.nodes.get(&id).map(|node| NodeSnapshot {
            id,
            parent: node.parent,
            children: node.children.clone(),
            bounds: node.bounds,
            semantics: {
                let mut semantics = node.semantics();
                let key = match node.control {
                    Control::Divider { .. } => Some("glassmere-splitter"),
                    Control::DockFloat => Some("glassmere-dock-floating"),
                    Control::DockHandle { resize: false, .. } => Some("glassmere-dock-move"),
                    Control::DockHandle { resize: true, .. } => Some("glassmere-dock-resize"),
                    _ => None,
                };
                if let Some(key) = key {
                    semantics.label = self.tr(key, &[]);
                }
                semantics
            },
            focused: self.focused == Some(id),
            clip: node.clip,
            scroll: matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection).then_some(node.scroll),
            value: match &node.control {
                // What the field shows, so a draft typed since the last
                // declaration is read as it is typed.
                // A password is read as its bullets.
                Control::TextInput { value, .. } => Some(node.editor.as_ref().map_or_else(
                    || value.clone(),
                    |editor| {
                        if editor.masked {
                            editor.shown()
                        } else {
                            editor.text.clone()
                        }
                    },
                )),
                // Assistive technology reads the caret's line.
                Control::TextArea {
                    lines,
                    first_line,
                    primary,
                    ..
                } => primary
                    .0
                    .checked_sub(*first_line)
                    .and_then(|index| lines.get(index))
                    .map(|line| line.text.clone()),
                Control::RichEditor { blocks, cursor, .. } => {
                    blocks.get(cursor.block).map(|block| block.text.clone())
                }
                Control::Combo {
                    items, selected, ..
                } => items.get(*selected).map(|item| item.label.clone()),
                // A widget of the application's own says what it holds.
                _ => node
                    .custom
                    .as_ref()
                    .and_then(|widget| widget.semantics.value.clone()),
            },
            checked: match node.control {
                Control::Checkbox { checked, .. } => Some(checked),
                _ => None,
            },
            selected: match node.control {
                Control::Radio { selected, .. }
                | Control::Tab { selected, .. }
                | Control::CollectionRow { selected, .. }
                | Control::Choice {
                    selected,
                    menu: false,
                    ..
                } => Some(selected),
                _ => None,
            },
            numeric: match node.control {
                Control::Slider {
                    value,
                    min,
                    max,
                    step,
                    ..
                } => Some(NumericValue {
                    value,
                    min,
                    max,
                    step,
                }),
                Control::TextInput { number, .. } => number,
                Control::Sizer { size, min, max, .. } => Some(NumericValue {
                    value: f64::from(size),
                    min: f64::from(min),
                    max: f64::from(max),
                    step: 10.0,
                }),
                Control::Divider { fraction, .. } => Some(NumericValue {
                    value: f64::from(fraction) * 100.0,
                    min: 0.0,
                    max: 100.0,
                    step: 1.0,
                }),
                _ => node.custom.as_ref().and_then(|widget| widget.semantics.numeric),
            },
            live: match node.control {
                Control::Toast {
                    assertive: true, ..
                } => Some(Live::Assertive),
                Control::Toast { .. } => Some(Live::Polite),
                _ => None,
            },
            expanded: if let Control::CollectionRow { expanded, .. } = node.control { expanded } else {
                let suggests = matches!(node.control, Control::TextInput { suggestions: Some(_), .. });
                // An autocomplete list is open only while it has rows to show.
                (suggests || matches!(node.kind, Kind::ComboBox | Kind::MenuButton)).then_some(
                    self.popup.is_some_and(|popup| popup.owner == id)
                        && (!suggests || !node.children.is_empty()),
                )
            },
            active_descendant: self
                .popup
                .filter(|popup| popup.owner == id)
                .and_then(|popup| {
                    node.children
                        .first()
                        .and_then(|panel| self.nodes[panel].children.get(popup.highlighted))
                        .copied()
                }).or_else(|| if let Control::Collection { selected: Some(selected), .. } = node.control {
                    node.children.iter().find(|id| matches!(self.nodes[id].control, Control::CollectionRow { item, .. } if item == selected)).copied()
                } else { None }),
            text_selection: node.editor.as_ref().map(|editor| editor.selection),
            editor_text: self.editor_text(id),
            is_popup: matches!(node.kind, Kind::Popup | Kind::Popover),
            modal: matches!(node.control, Control::Dialog { modal: true, .. }),
            description: node.tooltip.clone(),
            orientation: match node.control {
                Control::Divider { axis, .. } => Some(axis),
                Control::Sizer { .. } => Some(crate::Axis::Horizontal),
                _ => None,
            },
            drag: match node.control {
                Control::DragSource { .. } => Some(crate::DragInfo { source: node.enabled, can_drop: false, dragging: self.drag.as_ref().is_some_and(|drag| drag.source == id && drag.active) }),
                Control::DropTarget { accepts, .. } => Some(crate::DragInfo { source: false, can_drop: node.enabled && self.drag.as_ref().is_some_and(|drag| drag.active && drag.payload.kind() == accepts), dragging: false }),
                _ => None,
            },
            collection: match node.control {
                Control::Collection { count, .. } => Some(CollectionInfo { item: None, index: 0, total: count, level: 0, column: None, columns: None, sort: None }),
                Control::CollectionRow { item, index, total, depth, .. } => Some(CollectionInfo { item: Some(item), index, total, level: depth + 1, column: None, columns: None, sort: None }),
                Control::Grid { rows, columns, .. } => Some(CollectionInfo { item: None, index: 0, total: rows, level: 0, column: None, columns: Some(columns), sort: None }),
                Control::GridCell { row, column, sort, .. } => Some(CollectionInfo { item: None, index: row, total: 0, level: 0, column: Some(column), columns: None, sort }),
                _ => None,
            },
        })
    }

    /// Key, kind, style and change facts for inspectors and test harnesses.
    pub fn details(&self, id: NodeId) -> Option<NodeDetails> {
        self.nodes.get(&id).map(|node| NodeDetails {
            key: node.key.describe(),
            explicit: matches!(node.key, WidgetKey::Named(_)),
            kind: format!("{:?}", node.kind),
            text: node.text.clone(),
            style: node.style.clone(),
            resolved: node.resolved.clone(),
            changed: if node.changed_frame == self.frame {
                node.changed
            } else {
                Dirty::empty()
            },
            painted: node.painted_frame == self.frame,
        })
    }

    /// Find explicit keys relative to the root, e.g. &["main", "save"].
    pub fn find(&self, path: &[&str]) -> Option<NodeId> {
        let mut id = self.root;
        for key in path {
            id = *self.nodes[&id]
                .children_by_key
                .get(&WidgetKey::Named((*key).into()))?;
        }
        Some(id)
    }
}
