//! Lists and trees whose rows differ in height: the model gives the heights
//! that are not the fixed one, and layout, scrolling, paging and the
//! accessibility bounds follow them.

use glassmere_core::{
    App, Effects, GridColumn, GridModel, GridMsg, GridState, ItemId, Key, ListModel, ListState,
    NodeId, RowHeights, SortDirection, TreeItem, TreeModel, Ui, VirtualListOptions,
};
use glassmere_scene::{Point, Rect, Size};
use glassmere_testing::Harness;
use std::cell::Cell;

const FIXED: f32 = 30.0;
const VIEW: f32 = 300.0;

/// Every seventh row is 80 high, every third 50, the others the fixed 30.
fn height_of(index: usize) -> Option<f32> {
    if index % 7 == 6 {
        Some(80.0)
    } else if index % 3 == 2 {
        Some(50.0)
    } else {
        None
    }
}

/// Where row `index` starts, summed row by row.
fn top_of(heights: &RowHeights, index: usize) -> f32 {
    (0..index)
        .map(|row| f64::from(heights.get(row).unwrap_or(FIXED)))
        .sum::<f64>() as f32
}

/// The row at `offset`, found by walking the rows.
fn row_at(heights: &RowHeights, offset: f32) -> usize {
    (0..heights.len())
        .find(|row| top_of(heights, row + 1) > offset)
        .unwrap_or(heights.len())
}

struct Mixed {
    rows: usize,
    /// `None` is a model that does not override `row_heights`.
    heights: Option<RowHeights>,
    /// Rows the framework asked the model about.
    calls: Cell<usize>,
    columns: Vec<GridColumn>,
}

impl Mixed {
    fn new(rows: usize) -> Self {
        Self {
            rows,
            heights: Some(RowHeights::new((0..rows).map(height_of))),
            calls: Cell::new(0),
            columns: vec![GridColumn::new("Name", 120.0)],
        }
    }
    fn heights(&self) -> &RowHeights {
        self.heights.as_ref().unwrap()
    }
}

impl ListModel for Mixed {
    fn len(&self) -> usize {
        self.rows
    }
    fn item_id(&self, index: usize) -> ItemId {
        self.calls.set(self.calls.get() + 1);
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        self.calls.set(self.calls.get() + 1);
        ((id.0 as usize) < self.rows).then_some(id.0 as usize)
    }
    fn label(&self, index: usize) -> String {
        self.calls.set(self.calls.get() + 1);
        format!("Row {index}")
    }
    fn row_heights(&self) -> Option<&RowHeights> {
        self.heights.as_ref()
    }
}

impl GridModel for Mixed {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, _: usize) -> String {
        format!("Cell {row}")
    }
    fn sort(&mut self, _: usize, _: SortDirection) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
enum Msg {
    Select(GridMsg),
}

struct Lists {
    model: Mixed,
    state: ListState,
}

impl App for Lists {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        let Msg::Select(message) = message;
        match message {
            GridMsg::Select(id) => self.state.select(Some(id)),
            GridMsg::Deselect => self.state.select(None),
            _ => {}
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("list", |ui| {
            ui.virtual_list(&self.model, &self.state, options(), Msg::Select)
        });
    }
}

fn options() -> VirtualListOptions {
    VirtualListOptions {
        height: VIEW,
        row_height: FIXED,
        ..VirtualListOptions::default()
    }
}

fn lists(model: Mixed) -> Harness<Lists> {
    Harness::app(
        Size::new(640.0, 480.0),
        Lists {
            model,
            state: ListState::default(),
        },
    )
}

fn select(harness: &mut Harness<Lists>, row: usize) {
    harness.send(Msg::Select(GridMsg::Select(ItemId(row as u64))));
}

fn list(harness: &Harness<Lists>) -> NodeId {
    harness.find(&["list"])
}

fn bounds(harness: &Harness<Lists>, id: NodeId) -> Rect {
    harness.context().node(id).unwrap().bounds
}

fn row(harness: &Harness<Lists>, index: usize) -> Option<NodeId> {
    harness.context().find(&["list", &format!("item-{index}")])
}

fn offset(harness: &Harness<Lists>) -> f32 {
    harness
        .context()
        .node(list(harness))
        .unwrap()
        .scroll
        .unwrap()
        .offset
}

fn selected(harness: &Harness<Lists>) -> Option<ItemId> {
    harness.state().state.selected()
}

#[test]
fn rows_lay_out_at_the_offsets_their_heights_add_up_to() {
    let harness = lists(Mixed::new(1000));
    let top = bounds(&harness, list(&harness)).y;
    for index in 0..9 {
        let shown = bounds(&harness, row(&harness, index).expect("in the viewport"));
        assert_eq!(
            shown.y - top,
            top_of(harness.state().model.heights(), index)
        );
        assert_eq!(shown.height, height_of(index).unwrap_or(FIXED));
    }
    assert!(
        row(&harness, 12).is_none(),
        "rows past the view are not declared"
    );
}

/// The row a hit landed in: the row itself, or something inside it.
fn row_of(harness: &Harness<Lists>, mut hit: Option<NodeId>) -> Option<NodeId> {
    let list = list(harness);
    while let Some(id) = hit {
        let parent = harness.context().node(id)?.parent;
        if parent == Some(list) {
            return Some(id);
        }
        hit = parent;
    }
    None
}

#[test]
fn accessibility_bounds_and_hits_follow_the_real_heights() {
    let harness = lists(Mixed::new(1000));
    let tall = row(&harness, 2).unwrap();
    let node = harness
        .context()
        .snapshots()
        .into_iter()
        .find(|node| node.id == tall)
        .unwrap();
    assert_eq!(node.bounds.height, 50.0);
    // A point inside the 50 high row is in it, and just past it is the next.
    let shown = node.bounds;
    let at = |y| {
        let hit = harness.context().hit_test(Point {
            x: shown.x + 12.0,
            y,
        });
        row_of(&harness, hit)
    };
    assert_eq!(at(shown.y + 45.0), Some(tall));
    assert_eq!(at(shown.y + 55.0), row(&harness, 3));
}

#[test]
fn selecting_a_far_row_scrolls_it_into_view() {
    let mut harness = lists(Mixed::new(10_000));
    select(&mut harness, 5000);
    let heights = harness.state().model.heights();
    let (first, last) = (top_of(heights, 5000), top_of(heights, 5001));
    let shown = bounds(&harness, row(&harness, 5000).expect("revealed"));
    let view = bounds(&harness, list(&harness));
    assert!(shown.y >= view.y && shown.y + shown.height <= view.y + view.height + 0.5);
    // Revealed at the bottom edge, from where it was above the view.
    assert_eq!(offset(&harness), last - VIEW);
    assert_eq!(shown.height, last - first);
    // And back to the first row.
    select(&mut harness, 0);
    assert_eq!(offset(&harness), 0.0);
    assert!(row(&harness, 0).is_some());
}

#[test]
fn the_scroll_range_is_the_sum_of_the_heights() {
    let harness = lists(Mixed::new(10_000));
    let node = harness.context().node(list(&harness)).unwrap();
    let total = top_of(harness.state().model.heights(), 10_000);
    assert_eq!(node.scroll.unwrap().maximum, total - VIEW);
}

#[test]
fn page_keys_move_by_the_viewport() {
    let mut harness = lists(Mixed::new(1000));
    let owner = list(&harness);
    harness.focus(owner);
    harness.press(Key::Home);
    assert_eq!(selected(&harness), Some(ItemId(0)));

    let mut from = 0;
    for _ in 0..4 {
        harness.press(Key::PageDown);
        let heights = harness.state().model.heights();
        let target = row_at(heights, top_of(heights, from) + VIEW);
        assert_eq!(selected(&harness), Some(ItemId(target as u64)));
        // The selected row is one view further down, and in sight.
        assert!(top_of(heights, target) >= top_of(heights, from) + VIEW - 80.0);
        let shown = bounds(&harness, row(&harness, target).unwrap());
        let view = bounds(&harness, owner);
        assert!(shown.y >= view.y && shown.y + shown.height <= view.y + view.height + 0.5);
        from = target;
    }
    // Ten rows of the fixed height would be a page of a uniform list.
    assert_ne!(from, 40);
    for _ in 0..4 {
        harness.press(Key::PageUp);
        let heights = harness.state().model.heights();
        let target = row_at(heights, (top_of(heights, from) - VIEW).max(0.0));
        assert_eq!(selected(&harness), Some(ItemId(target as u64)));
        from = target;
    }
    assert_eq!(from, 0);
}

#[test]
fn a_model_of_100_000_rows_declares_only_the_viewport() {
    let mut harness = lists(Mixed::new(100_000));
    let stats = harness.run();
    assert!(stats.elements < 100, "{} elements", stats.elements);
    assert!(harness.context().node_count() < 60);

    let owner = list(&harness);
    harness.focus(owner);
    for step in [Key::End, Key::PageUp, Key::Home, Key::PageDown] {
        let stats = harness.press(step);
        assert!(stats.elements < 150, "{} elements", stats.elements);
    }
    select(&mut harness, 77_777);
    let stats = harness.run();
    assert!(stats.elements < 150, "{} elements", stats.elements);
    assert!(row(&harness, 77_777).is_some());
    let heights = harness.state().model.heights();
    let total = top_of(heights, 100_000);
    assert_eq!(
        harness
            .context()
            .node(owner)
            .unwrap()
            .scroll
            .unwrap()
            .maximum,
        total - VIEW
    );
    // Rows were asked for by the viewport, not by the length: a walk would be
    // 100,000 calls on every frame.
    assert!(
        harness.state().model.calls.get() < 2000,
        "{} calls",
        harness.state().model.calls.get()
    );
}

#[test]
fn patching_a_height_moves_the_rows_below_it() {
    let mut harness = lists(Mixed::new(1000));
    let before = bounds(&harness, row(&harness, 4).unwrap()).y;
    let maximum = |harness: &Harness<Lists>| {
        harness
            .context()
            .node(list(harness))
            .unwrap()
            .scroll
            .unwrap()
            .maximum
    };
    let was = maximum(&harness);

    let heights = harness.app_mut().model.heights.as_mut().unwrap();
    heights.set(1, Some(100.0));
    harness.run();
    assert_eq!(bounds(&harness, row(&harness, 1).unwrap()).height, 100.0);
    assert_eq!(bounds(&harness, row(&harness, 4).unwrap()).y, before + 70.0);
    assert_eq!(maximum(&harness), was + 70.0);

    let heights = harness.app_mut().model.heights.as_mut().unwrap();
    heights.set(1, None);
    harness.run();
    assert_eq!(bounds(&harness, row(&harness, 4).unwrap()).y, before);
    assert_eq!(maximum(&harness), was);
}

/// A list with no heights set is laid out as a list that does not override
/// `row_heights`: the same tree, bounds and paint, at the top and scrolled.
#[test]
fn heights_that_are_all_unset_lay_out_as_the_default() {
    let plain = || {
        let mut model = Mixed::new(1000);
        model.heights = None;
        lists(model)
    };
    let unset = || {
        let mut model = Mixed::new(1000);
        model.heights = Some(RowHeights::new((0..1000).map(|_| None)));
        lists(model)
    };
    let (mut a, mut b) = (plain(), unset());
    assert_eq!(a.snapshot_layout(), b.snapshot_layout());
    assert_eq!(a.snapshot_scene(), b.snapshot_scene());
    for harness in [&mut a, &mut b] {
        let owner = list(harness);
        harness.focus(owner);
        for key in [
            Key::Home,
            Key::PageDown,
            Key::PageDown,
            Key::End,
            Key::PageUp,
        ] {
            harness.press(key);
        }
    }
    assert_eq!(selected(&a), selected(&b));
    assert_eq!(selected(&a), Some(ItemId(989)));
    assert_eq!(a.snapshot_layout(), b.snapshot_layout());
    assert_eq!(a.snapshot_scene(), b.snapshot_scene());
    assert_eq!(a.snapshot_accessibility(), b.snapshot_accessibility());
}

#[test]
#[should_panic(expected = "one entry per row")]
fn heights_for_another_number_of_rows_are_misuse() {
    let mut model = Mixed::new(10);
    model.heights = Some(RowHeights::new((0..9).map(height_of)));
    lists(model);
}

struct Grid {
    model: Mixed,
    state: GridState,
}

impl App for Grid {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("grid", |ui| {
            ui.data_grid(&self.model, &self.state, options(), Msg::Select)
        });
    }
}

#[test]
fn a_grid_keeps_one_height_whatever_its_model_says() {
    let mut model = Mixed::new(10);
    model.heights = Some(RowHeights::new((0..10).map(|_| Some(55.0))));
    let harness = Harness::app(
        Size::new(640.0, 480.0),
        Grid {
            model,
            state: GridState::default(),
        },
    );
    let shown = harness
        .context()
        .find(&["grid", "$rows", "item-3"])
        .unwrap();
    assert_eq!(harness.context().node(shown).unwrap().bounds.height, FIXED);
}

struct Outline {
    model: TreeModel,
    state: ListState,
}

impl App for Outline {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        let Msg::Select(message) = message;
        if let GridMsg::Expand { row, open } = message {
            self.model.set_expanded(row, open);
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("tree", |ui| {
            ui.tree_view(&self.model, &self.state, options(), Msg::Select)
        });
    }
}

#[test]
fn tree_heights_follow_their_items_as_the_tree_expands() {
    let item = |id, parent: Option<u64>| TreeItem {
        id: ItemId(id),
        parent: parent.map(ItemId),
        label: format!("Item {id}"),
    };
    let mut model = TreeModel::new(vec![
        item(1, None),
        item(2, Some(1)),
        item(3, None),
        item(4, Some(3)),
    ])
    .unwrap();
    model.set_row_height(ItemId(1), Some(60.0));
    model.set_row_height(ItemId(3), Some(45.0));
    model.set_row_height(ItemId(99), Some(10.0));
    let mut harness = Harness::app(
        Size::new(640.0, 480.0),
        Outline {
            model,
            state: ListState::default(),
        },
    );
    let at = |harness: &Harness<Outline>, id: u64| {
        harness
            .context()
            .find(&["tree", &format!("item-{id}")])
            .map(|row| harness.context().node(row).unwrap().bounds)
    };
    let top = harness
        .context()
        .node(harness.find(&["tree"]))
        .unwrap()
        .bounds
        .y;
    let shape = |harness: &Harness<Outline>, id| {
        let bounds = at(harness, id).unwrap();
        (bounds.y - top, bounds.height)
    };
    assert_eq!(shape(&harness, 1), (0.0, 60.0));
    assert_eq!(shape(&harness, 3), (60.0, 45.0));

    harness.send(Msg::Select(GridMsg::Expand {
        row: ItemId(1),
        open: true,
    }));
    assert_eq!(shape(&harness, 1), (0.0, 60.0));
    assert_eq!(shape(&harness, 2), (60.0, FIXED));
    assert_eq!(shape(&harness, 3), (90.0, 45.0));

    // A height set on a row that is not shown waits for it.
    harness
        .app_mut()
        .model
        .set_row_height(ItemId(4), Some(70.0));
    harness.send(Msg::Select(GridMsg::Expand {
        row: ItemId(3),
        open: true,
    }));
    assert_eq!(shape(&harness, 4), (135.0, 70.0));

    harness.app_mut().model.set_row_height(ItemId(1), None);
    harness.run();
    assert_eq!(shape(&harness, 1), (0.0, FIXED));
    assert_eq!(shape(&harness, 3), (60.0, 45.0));
}
