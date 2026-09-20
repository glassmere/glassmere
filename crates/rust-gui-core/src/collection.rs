use crate::{Length, Style};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

/// Application identity. Independent of a realized widget's short-lived NodeId.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

/// Random access to application-owned data. These methods should be O(1).
/// Item IDs must be unique within the model and stable across reorder/filtering.
pub trait ListModel {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn item_id(&self, index: usize) -> ItemId;
    fn index_of(&self, id: ItemId) -> Option<usize>;
    fn label(&self, index: usize) -> String;
    fn depth(&self, _index: usize) -> usize {
        0
    }
    fn expanded(&self, _index: usize) -> Option<bool> {
        None
    }
    fn parent(&self, _id: ItemId) -> Option<ItemId> {
        None
    }
    fn set_expanded(&mut self, _id: ItemId, _expanded: bool) -> bool {
        false
    }
}

/// An indexed order for existing application data. Rebuild only on model changes.
#[derive(Clone, Debug, Default)]
pub struct ItemOrder {
    ids: Vec<ItemId>,
    indices: HashMap<ItemId, usize>,
}

impl ItemOrder {
    pub fn new(ids: impl IntoIterator<Item = ItemId>) -> Result<Self, ModelError> {
        let ids: Vec<_> = ids.into_iter().collect();
        let mut indices = HashMap::with_capacity(ids.len());
        for (index, id) in ids.iter().copied().enumerate() {
            if indices.insert(id, index).is_some() {
                return Err(ModelError::Duplicate(id));
            }
        }
        Ok(Self { ids, indices })
    }
    pub fn ids(&self) -> &[ItemId] {
        &self.ids
    }
    pub fn index_of(&self, id: ItemId) -> Option<usize> {
        self.indices.get(&id).copied()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    Duplicate(ItemId),
    MissingParent(ItemId),
    Cycle(ItemId),
}
impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid collection model: {self:?}")
    }
}
impl std::error::Error for ModelError {}

#[derive(Clone, Debug)]
pub struct VirtualListOptions {
    pub label: String,
    pub height: f32,
    pub row_height: f32,
    pub overscan: usize,
    /// Width, colors and text styling. Height, padding and gap are managed by the list.
    pub style: Style,
}
impl Default for VirtualListOptions {
    fn default() -> Self {
        Self {
            label: "Items".into(),
            height: 320.0,
            row_height: 32.0,
            overscan: 2,
            style: Style {
                width: Length::Percent(1.0),
                ..Style::default()
            },
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ListState {
    selected: Option<ItemId>,
    reveal: Option<ItemId>,
    pub(crate) offset: f32,
    pub(crate) anchor: Option<(ItemId, f32)>,
}
impl ListState {
    pub fn selected(&self) -> Option<ItemId> {
        self.selected
    }
    pub fn select(&mut self, id: Option<ItemId>) {
        self.selected = id;
        self.reveal = id;
    }
    pub fn reveal(&mut self, id: ItemId) {
        self.reveal = Some(id);
    }
    pub fn scroll_offset(&self) -> f32 {
        self.offset
    }
}

#[derive(Clone, Debug, Default)]
pub struct ListResponse {
    pub changed: bool,
    pub activated: Option<ItemId>,
    /// Realized indices, including bounded overscan.
    pub realized: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CollectionKind {
    List,
    Tree,
    Grid,
}

#[derive(Clone, Debug)]
pub(crate) enum CollectionInput {
    Select(ItemId),
    Move(isize),
    First,
    Last,
    Activate,
    Expand(bool),
    Toggle(ItemId),
}

impl ListState {
    pub(crate) fn update(
        &mut self,
        model: &mut impl ListModel,
        input: Vec<CollectionInput>,
        offset: Option<f32>,
        options: &VirtualListOptions,
    ) -> ListResponse {
        assert!(
            options.height.is_finite()
                && options.height > 0.0
                && options.row_height.is_finite()
                && options.row_height >= 1.0,
            "virtual lists require finite positive height and row height >= 1"
        );
        assert!(
            options.overscan <= 128,
            "virtual list overscan must be bounded (<= 128)"
        );
        let old = self.selected;
        let native_offset = offset.unwrap_or(self.offset);
        // Preserve the top logical item across model reorder when the user has
        // not independently scrolled since the previous declaration.
        self.offset = if native_offset == self.offset {
            self.anchor
                .and_then(|(id, within)| {
                    model
                        .index_of(id)
                        .map(|index| index as f32 * options.row_height + within)
                })
                .unwrap_or(native_offset)
        } else {
            native_offset
        };
        if self.selected.is_some_and(|id| model.index_of(id).is_none()) {
            self.selected = None;
        }
        let mut activated = None;
        for action in input {
            let current = self.selected.and_then(|id| model.index_of(id));
            let index = match action {
                CollectionInput::Select(id) => model.index_of(id),
                CollectionInput::Move(delta) => {
                    if model.is_empty() {
                        None
                    } else {
                        Some(
                            current.map_or(if delta < 0 { model.len() - 1 } else { 0 }, |index| {
                                index.saturating_add_signed(delta).min(model.len() - 1)
                            }),
                        )
                    }
                }
                CollectionInput::First => (!model.is_empty()).then_some(0),
                CollectionInput::Last => model.len().checked_sub(1),
                CollectionInput::Activate => {
                    activated = self.selected;
                    None
                }
                CollectionInput::Expand(expand) => {
                    if let Some(index) = current {
                        let id = model.item_id(index);
                        match (expand, model.expanded(index)) {
                            (true, Some(false)) => {
                                model.set_expanded(id, true);
                            }
                            (true, Some(true)) if index + 1 < model.len() => {
                                self.selected = Some(model.item_id(index + 1));
                            }
                            (false, Some(true)) => {
                                model.set_expanded(id, false);
                            }
                            (false, _) => {
                                if let Some(parent) = model.parent(id) {
                                    self.selected = Some(parent);
                                }
                            }
                            _ => {}
                        }
                        self.reveal = self.selected;
                    }
                    None
                }
                CollectionInput::Toggle(id) => {
                    if let Some(index) = model.index_of(id)
                        && let Some(expanded) = model.expanded(index)
                    {
                        model.set_expanded(id, !expanded);
                        self.selected = Some(id);
                        self.reveal = Some(id);
                    }
                    None
                }
            };
            if let Some(index) = index {
                self.selected = Some(model.item_id(index));
                self.reveal = self.selected;
            }
        }
        let maximum = (model.len() as f64 * f64::from(options.row_height)
            - f64::from(options.height))
        .max(0.0);
        assert!(
            maximum <= 1_000_000_000.0,
            "virtual content exceeds supported logical extent"
        );
        if let Some(index) = self.reveal.take().and_then(|id| model.index_of(id)) {
            let top = index as f32 * options.row_height;
            let bottom = top + options.row_height;
            if top < self.offset {
                self.offset = top;
            }
            if bottom > self.offset + options.height {
                self.offset = bottom - options.height;
            }
        }
        self.offset = self.offset.clamp(0.0, maximum as f32);
        let first = (self.offset / options.row_height).floor() as usize;
        self.anchor = (first < model.len()).then(|| {
            (
                model.item_id(first),
                self.offset - first as f32 * options.row_height,
            )
        });
        let start = first.saturating_sub(options.overscan).min(model.len());
        let end = (((self.offset + options.height) / options.row_height).ceil() as usize)
            .saturating_add(options.overscan)
            .min(model.len());
        ListResponse {
            changed: old != self.selected,
            activated,
            realized: start..end,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TreeItem {
    pub id: ItemId,
    pub parent: Option<ItemId>,
    pub label: String,
}

/// Indexed forest. Structural input is validated once; expansion rebuilds only
/// the visible order. Widget declarations still visit only the viewport range.
pub struct TreeModel {
    items: HashMap<ItemId, TreeItem>,
    children: HashMap<Option<ItemId>, Vec<ItemId>>,
    expanded: HashSet<ItemId>,
    order: ItemOrder,
    depths: Vec<usize>,
}
impl TreeModel {
    pub fn new(items: Vec<TreeItem>) -> Result<Self, ModelError> {
        let mut map = HashMap::with_capacity(items.len());
        let mut children: HashMap<_, Vec<_>> = HashMap::new();
        for item in items {
            let id = item.id;
            children.entry(item.parent).or_default().push(id);
            if map.insert(id, item).is_some() {
                return Err(ModelError::Duplicate(id));
            }
        }
        for item in map.values() {
            if let Some(parent) = item.parent
                && !map.contains_key(&parent)
            {
                return Err(ModelError::MissingParent(parent));
            }
        }
        let mut done = HashSet::new();
        for id in map.keys().copied() {
            let mut chain = HashSet::new();
            let mut next = Some(id);
            while let Some(id) = next {
                if done.contains(&id) {
                    break;
                }
                if !chain.insert(id) {
                    return Err(ModelError::Cycle(id));
                }
                next = map[&id].parent;
            }
            done.extend(chain);
        }
        let mut model = Self {
            items: map,
            children,
            expanded: HashSet::new(),
            order: ItemOrder::default(),
            depths: Vec::new(),
        };
        model.rebuild();
        Ok(model)
    }
    fn rebuild(&mut self) {
        let mut stack: Vec<_> = self
            .children
            .get(&None)
            .into_iter()
            .flatten()
            .rev()
            .map(|id| (*id, 0))
            .collect();
        let mut ids = Vec::new();
        self.depths.clear();
        while let Some((id, depth)) = stack.pop() {
            ids.push(id);
            self.depths.push(depth);
            if self.expanded.contains(&id) {
                stack.extend(
                    self.children
                        .get(&Some(id))
                        .into_iter()
                        .flatten()
                        .rev()
                        .map(|id| (*id, depth + 1)),
                );
            }
        }
        self.order = ItemOrder::new(ids).expect("validated unique tree identities");
    }
    pub fn item(&self, id: ItemId) -> Option<&TreeItem> {
        self.items.get(&id)
    }
}
impl ListModel for TreeModel {
    fn len(&self) -> usize {
        self.order.ids.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        self.order.ids[index]
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        self.order.index_of(id)
    }
    fn label(&self, index: usize) -> String {
        self.items[&self.item_id(index)].label.clone()
    }
    fn depth(&self, index: usize) -> usize {
        self.depths[index]
    }
    fn expanded(&self, index: usize) -> Option<bool> {
        let id = self.item_id(index);
        self.children
            .contains_key(&Some(id))
            .then(|| self.expanded.contains(&id))
    }
    fn parent(&self, id: ItemId) -> Option<ItemId> {
        self.items.get(&id).and_then(|item| item.parent)
    }
    fn set_expanded(&mut self, id: ItemId, expanded: bool) -> bool {
        if !self.children.contains_key(&Some(id)) {
            return false;
        }
        let changed = if expanded {
            self.expanded.insert(id)
        } else {
            self.expanded.remove(&id)
        };
        if changed {
            self.rebuild();
        }
        changed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}
#[derive(Clone, Debug)]
pub struct GridColumn {
    pub label: String,
    pub width: f32,
    pub sortable: bool,
}
pub trait GridModel: ListModel {
    fn columns(&self) -> &[GridColumn];
    fn cell(&self, row: usize, column: usize) -> String;
    /// Return true after actually changing the application-owned order.
    fn sort(&mut self, column: usize, direction: SortDirection) -> bool;
}
#[derive(Clone, Debug, Default)]
pub struct GridState {
    pub rows: ListState,
    pub sort: Option<(usize, SortDirection)>,
}
