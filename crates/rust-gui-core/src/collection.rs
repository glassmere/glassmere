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
    /// Grids: the row and model column whose edited value the model accepted
    /// this frame.
    pub edited: Option<(ItemId, usize)>,
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
    /// Grids: a press landed on this cell of this row.
    Cell {
        item: ItemId,
        position: usize,
    },
    /// Grids: move the current cell sideways by display positions.
    Column(isize),
    /// Grids: F2 or Space on the current cell.
    Edit,
    /// Grids: text typed while the grid itself has focus.
    Type(String),
    /// Grids: Enter, Tab or Escape while an editor in a cell has focus.
    EditKey {
        key: crate::Key,
        shift: bool,
    },
    /// Grids: scroll the unfrozen columns by physical pixels, right positive.
    ScrollX(f32),
}

impl CollectionInput {
    /// Inputs the grid handles itself; the list below it never sees them.
    pub(crate) fn for_grid(&self) -> bool {
        matches!(
            self,
            Self::Cell { .. }
                | Self::Column(_)
                | Self::Edit
                | Self::Type(_)
                | Self::EditKey { .. }
                | Self::ScrollX(_)
        )
    }
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
                // The grid took these out before the rows saw the rest.
                CollectionInput::Cell { .. }
                | CollectionInput::Column(_)
                | CollectionInput::Edit
                | CollectionInput::Type(_)
                | CollectionInput::EditKey { .. }
                | CollectionInput::ScrollX(_) => None,
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
            edited: None,
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
/// What a cell holds, as its editor sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Text(String),
    Number(f64),
    Bool(bool),
    /// An index into the column's choices.
    Choice(usize),
}

/// How the cells of a column are edited in place.
#[derive(Clone, Debug, PartialEq)]
pub enum CellEditor {
    Text,
    /// Typed as text; committing parses, clamps and snaps it like a number
    /// input. Boxed because the options carry a whole style; see `number`.
    Number(Box<crate::NumberOptions>),
    /// No editor opens: editing toggles the value.
    Bool,
    Choice(Vec<String>),
}

impl CellEditor {
    pub fn number(options: crate::NumberOptions) -> Self {
        Self::Number(Box::new(options))
    }
}

#[derive(Clone, Debug)]
pub struct GridColumn {
    pub label: String,
    pub width: f32,
    pub sortable: bool,
    /// `None` makes the column read-only.
    pub editor: Option<CellEditor>,
    /// Shows a filter field under the header; see `GridModel::filter`.
    pub filterable: bool,
}

impl GridColumn {
    /// A read-only, unsortable column.
    pub fn new(label: impl Into<String>, width: f32) -> Self {
        Self {
            label: label.into(),
            width,
            sortable: false,
            editor: None,
            filterable: false,
        }
    }

    pub fn filterable(mut self, filterable: bool) -> Self {
        self.filterable = filterable;
        self
    }

    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    pub fn editor(mut self, editor: CellEditor) -> Self {
        self.editor = Some(editor);
        self
    }
}

pub trait GridModel: ListModel {
    fn columns(&self) -> &[GridColumn];
    fn cell(&self, row: usize, column: usize) -> String;
    /// Return true after actually changing the application-owned order.
    fn sort(&mut self, column: usize, direction: SortDirection) -> bool;
    /// What an editor starts from. Editable number, bool and choice columns
    /// override this; text columns edit their displayed text.
    fn value(&self, row: usize, column: usize) -> CellValue {
        CellValue::Text(self.cell(row, column))
    }
    /// Stores an edited value. An error keeps the editor open and is shown
    /// to the user, so say what is wrong with the value.
    fn set_cell(&mut self, row: usize, column: usize, value: CellValue) -> Result<(), String> {
        let _ = (row, column, value);
        Err("This grid is read-only.".into())
    }
    /// Narrows the rows to the ones matching every `(column, text)` pair; an
    /// empty slice shows everything again. The model filters its own data, as
    /// it sorts it, so this works for indexes and remote sources, and item
    /// identities stay stable. Texts arrive trimmed and never empty. Return
    /// true after actually changing the rows. `text_matches` is the usual test.
    fn filter(&mut self, filters: &[(usize, &str)]) -> bool {
        let _ = filters;
        false
    }
}

/// Whether `cell` contains `filter`, ignoring case: the usual test for
/// `GridModel::filter`.
pub fn text_matches(cell: &str, filter: &str) -> bool {
    cell.to_lowercase().contains(&filter.to_lowercase())
}

/// How long the filter fields wait after the last keystroke before the model
/// filters, so that typing a word filters once.
pub const FILTER_DELAY: std::time::Duration = std::time::Duration::from_millis(200);
#[derive(Clone, Debug, Default)]
pub struct GridState {
    pub rows: ListState,
    pub sort: Option<(usize, SortDirection)>,
    /// What the user did to the columns.
    pub columns: GridColumns,
    /// Model column of the current cell; its row is the selected row.
    pub cursor: usize,
    /// How far the unfrozen columns are scrolled, from the start side. The
    /// grid clamps it and moves it to keep the current cell in view.
    pub scroll_x: f32,
    /// Text of the filter fields by model column. Changing it from code
    /// filters at the next declaration, without the typing delay.
    pub filters: std::collections::BTreeMap<usize, String>,
    /// What the model was last asked to filter by.
    pub(crate) applied_filters: Vec<(usize, String)>,
    /// When typed filter text is due to reach the model.
    pub(crate) filter_due: Option<std::time::Duration>,
    pub(crate) edit: Option<GridEdit>,
    /// The next declaration gives the keyboard back to the rows.
    pub(crate) refocus: bool,
}

/// A cell being edited in place.
#[derive(Clone, Debug)]
pub(crate) struct GridEdit {
    pub item: ItemId,
    pub column: usize,
    /// Draft of a text or number editor.
    pub text: String,
    /// Draft of a choice editor.
    pub choice: usize,
    pub error: Option<String>,
    /// The editor has had keyboard focus, so losing it means leaving the cell.
    pub focused: bool,
}

impl GridState {
    /// The row and model column being edited.
    pub fn editing(&self) -> Option<(ItemId, usize)> {
        self.edit.as_ref().map(|edit| (edit.item, edit.column))
    }

    /// Why the model refused the value of the open editor.
    pub fn edit_error(&self) -> Option<&str> {
        self.edit.as_ref()?.error.as_deref()
    }

    /// Closes the editor without storing anything.
    pub fn cancel_edit(&mut self) {
        self.edit = None;
    }
}

/// A grid's column layout as the user arranged it: the model describes the
/// data and its default widths, this holds the order, the widths and what is
/// hidden. It is plain data, so it can be kept with the rest of a workspace,
/// and two grids over one model can differ.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GridColumns {
    /// Model column indices in display order. Columns of the model that are
    /// missing here follow in model order; indices the model no longer has
    /// are ignored.
    pub order: Vec<usize>,
    /// Widths the user chose, by model column index.
    pub widths: std::collections::BTreeMap<usize, f32>,
    /// Hidden model column indices.
    pub hidden: std::collections::BTreeSet<usize>,
    /// How many of the leading visible columns stay in place while the rest
    /// scroll sideways.
    pub frozen: usize,
}

impl GridColumns {
    pub const MIN_WIDTH: f32 = 40.0;
    pub const MAX_WIDTH: f32 = 2000.0;

    /// All `count` model columns in display order, hidden ones included.
    pub fn arranged(&self, count: usize) -> Vec<usize> {
        let mut arranged: Vec<usize> = Vec::with_capacity(count);
        for column in self.order.iter().copied().chain(0..count) {
            if column < count && !arranged.contains(&column) {
                arranged.push(column);
            }
        }
        arranged
    }

    /// The columns on screen, in display order. Never empty for a model with
    /// columns: hiding the last one is refused by `hide`, and a state that
    /// hides everything anyway shows the first.
    pub fn visible(&self, count: usize) -> Vec<usize> {
        let arranged = self.arranged(count);
        let visible: Vec<usize> = arranged
            .iter()
            .copied()
            .filter(|column| !self.hidden.contains(column))
            .collect();
        if visible.is_empty() {
            arranged.into_iter().take(1).collect()
        } else {
            visible
        }
    }

    pub fn width(&self, column: usize, default: f32) -> f32 {
        self.widths
            .get(&column)
            .copied()
            .unwrap_or(default)
            .clamp(Self::MIN_WIDTH, Self::MAX_WIDTH)
    }

    pub fn set_width(&mut self, column: usize, width: f32) {
        if width.is_finite() {
            self.widths
                .insert(column, width.clamp(Self::MIN_WIDTH, Self::MAX_WIDTH));
        }
    }

    /// Moves `column` so that it stands where `before` stands now; `None`
    /// moves it to the end.
    pub fn move_before(&mut self, count: usize, column: usize, before: Option<usize>) {
        let mut arranged = self.arranged(count);
        if column >= count || before == Some(column) {
            return;
        }
        arranged.retain(|other| *other != column);
        let at = before
            .and_then(|before| arranged.iter().position(|other| *other == before))
            .unwrap_or(arranged.len());
        arranged.insert(at, column);
        self.order = arranged;
    }

    /// Moves `column` past its visible neighbour: `-1` toward the start,
    /// `1` toward the end. Returns whether it moved.
    pub fn shift(&mut self, count: usize, column: usize, by: isize) -> bool {
        let visible = self.visible(count);
        let Some(at) = visible.iter().position(|other| *other == column) else {
            return false;
        };
        let Some(target) = at
            .checked_add_signed(by)
            .filter(|target| *target < visible.len())
        else {
            return false;
        };
        // Toward the end the column lands after its neighbour, which is
        // before the neighbour's own successor.
        let before = if by < 0 {
            Some(visible[target])
        } else {
            visible.get(target + 1).copied()
        };
        self.move_before(count, column, before);
        true
    }

    /// Hides a column unless it is the last one showing.
    pub fn hide(&mut self, count: usize, column: usize) -> bool {
        let visible = self.visible(count);
        if visible.len() < 2 || !visible.contains(&column) {
            return false;
        }
        self.hidden.insert(column)
    }

    pub fn show_all(&mut self) {
        self.hidden.clear();
    }

    /// Back to the model's order and widths, with everything showing.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
