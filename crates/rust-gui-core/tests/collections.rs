use rust_gui_core::{
    Action, Context, Event, GridColumn, GridModel, GridState, ItemId, ItemOrder, Key, ListModel,
    ListState, ModelError, MonospaceMeasurer, SortDirection, TreeItem, TreeModel,
    VirtualListOptions,
};
use rust_gui_scene::{Point, Size};
use std::cell::Cell;

struct Million {
    count: usize,
    reverse: bool,
    labels: Cell<usize>,
}
impl ListModel for Million {
    fn len(&self) -> usize {
        self.count
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(if self.reverse {
            self.count - index - 1
        } else {
            index
        } as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        ((id.0 as usize) < self.count).then(|| {
            if self.reverse {
                self.count - id.0 as usize - 1
            } else {
                id.0 as usize
            }
        })
    }
    fn label(&self, index: usize) -> String {
        self.labels.set(self.labels.get() + 1);
        format!("Row {}", self.item_id(index).0)
    }
}
fn model(count: usize) -> Million {
    Million {
        count,
        reverse: false,
        labels: Cell::new(0),
    }
}
fn options() -> VirtualListOptions {
    VirtualListOptions {
        height: 300.0,
        row_height: 30.0,
        ..Default::default()
    }
}
fn frame(
    context: &mut Context,
    model: &mut impl ListModel,
    state: &mut ListState,
) -> rust_gui_core::ListResponse {
    let mut response = Default::default();
    context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, |ui| {
        response = ui.keyed("list", |ui| ui.virtual_list(model, state, options()))
    });
    response
}
fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

#[test]
fn million_items_realize_only_the_viewport_and_can_jump_to_last_item() {
    let mut context = Context::new();
    let mut model = model(1_000_000);
    let mut state = ListState::default();
    let first = frame(&mut context, &mut model, &mut state);
    assert_eq!(first.realized, 0..12);
    assert!(context.node_count() < 35);
    assert!(model.labels.get() < 30);
    let owner = context.find(&["list"]).unwrap();
    context.action(owner, Action::Focus);
    key(&mut context, Key::End);
    let last = frame(&mut context, &mut model, &mut state);
    assert!(last.changed);
    assert_eq!(state.selected(), Some(ItemId(999_999)));
    assert_eq!(last.realized.end, 1_000_000);
    assert!(context.node_count() < 35);
    assert_eq!(
        context.node(owner).unwrap().scroll.unwrap().maximum,
        29_999_700.0
    );
    let row = context.find(&["list", "item-999999"]).unwrap();
    let bounds = context.node(row).unwrap().bounds;
    assert!(bounds.y >= 0.0 && bounds.y + bounds.height <= 301.0);
    assert_eq!(
        context.hit_test(Point {
            x: 12.0,
            y: bounds.y + 12.0
        }),
        Some(row)
    );
    assert_eq!(context.focused(), Some(owner));
    key(&mut context, Key::Enter);
    assert_eq!(
        frame(&mut context, &mut model, &mut state).activated,
        Some(ItemId(999_999))
    );
    assert_eq!(frame(&mut context, &mut model, &mut state).activated, None);
}

#[test]
fn stable_selection_outlives_unrealized_nodes_and_reorder_preserves_anchor() {
    let mut context = Context::new();
    let mut model = model(1000);
    let mut state = ListState::default();
    frame(&mut context, &mut model, &mut state);
    let old_node = context.find(&["list", "item-4"]).unwrap();
    context.action(old_node, Action::Activate);
    frame(&mut context, &mut model, &mut state);
    let owner = context.find(&["list"]).unwrap();
    context.action(owner, Action::ScrollTo(15000.0));
    frame(&mut context, &mut model, &mut state);
    assert!(context.node(old_node).is_none());
    assert_eq!(state.selected(), Some(ItemId(4)));
    assert_eq!(state.scroll_offset(), 15000.0);
    model.reverse = true;
    frame(&mut context, &mut model, &mut state);
    assert_eq!(state.scroll_offset(), 499.0 * 30.0);
    assert_eq!(state.selected(), Some(ItemId(4)));
    state.reveal(ItemId(4));
    frame(&mut context, &mut model, &mut state);
    assert_ne!(context.find(&["list", "item-4"]).unwrap(), old_node);
    assert!(!context.action(old_node, Action::Activate));
    model.count = 3;
    frame(&mut context, &mut model, &mut state);
    assert_eq!(state.selected(), None);
    assert_eq!(state.scroll_offset(), 0.0);
}

#[test]
fn pointer_capture_survives_the_intervening_frame_and_keyboard_events_accumulate() {
    let mut context = Context::new();
    let mut model = model(1000);
    let mut state = ListState::default();
    frame(&mut context, &mut model, &mut state);
    context.event(Event::PointerMoved(Point { x: 15.0, y: 75.0 }));
    context.event(Event::PointerDown);
    frame(&mut context, &mut model, &mut state);
    assert!(context.captured().is_some());
    context.event(Event::PointerUp);
    frame(&mut context, &mut model, &mut state);
    assert_eq!(state.selected(), Some(ItemId(2)));
    for _ in 0..7 {
        key(&mut context, Key::ArrowDown);
    }
    frame(&mut context, &mut model, &mut state);
    assert_eq!(state.selected(), Some(ItemId(9)));
    context.event(Event::PointerMoved(Point { x: 15.0, y: 75.0 }));
    context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 450.0,
    });
    let response = frame(&mut context, &mut model, &mut state);
    assert_eq!(response.realized.start, 13);
}

#[test]
fn empty_lists_and_disabled_collections_reject_selection() {
    let mut context = Context::new();
    let mut model = model(0);
    let mut state = ListState::default();
    state.select(Some(ItemId(4)));
    frame(&mut context, &mut model, &mut state);
    assert_eq!(state.selected(), None);
    let owner = context.find(&["list"]).unwrap();
    context.action(owner, Action::Focus);
    key(&mut context, Key::End);
    assert!(
        frame(&mut context, &mut model, &mut state)
            .realized
            .is_empty()
    );
    model.count = 100;
    context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, |ui| {
        ui.enabled(false, |ui| {
            ui.keyed("list", |ui| {
                ui.virtual_list(&mut model, &mut state, options())
            });
        })
    });
    assert!(!context.action(owner, Action::Focus));
    assert!(!context.action(context.find(&["list", "item-0"]).unwrap(), Action::Activate));
}

#[test]
fn tree_validation_and_expansion_preserve_logical_identity() {
    assert!(matches!(
        ItemOrder::new([ItemId(1), ItemId(1)]),
        Err(ModelError::Duplicate(ItemId(1)))
    ));
    let item = |id, parent: Option<u64>| TreeItem {
        id: ItemId(id),
        parent: parent.map(ItemId),
        label: format!("Node {id}"),
    };
    assert!(matches!(
        TreeModel::new(vec![item(1, Some(2))]),
        Err(ModelError::MissingParent(ItemId(2)))
    ));
    assert!(matches!(
        TreeModel::new(vec![item(1, Some(2)), item(2, Some(1))]),
        Err(ModelError::Cycle(_))
    ));
    let mut tree = TreeModel::new(vec![
        item(1, None),
        item(2, Some(1)),
        item(3, Some(2)),
        item(4, None),
    ])
    .unwrap();
    let mut context = Context::new();
    let mut state = ListState::default();
    let frame = |context: &mut Context, tree: &mut TreeModel, state: &mut ListState| {
        context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, |ui| {
            ui.keyed("tree", |ui| ui.tree_view(tree, state, options()));
        });
    };
    frame(&mut context, &mut tree, &mut state);
    assert_eq!(tree.len(), 2);
    context.action(context.find(&["tree"]).unwrap(), Action::Focus);
    key(&mut context, Key::Home);
    key(&mut context, Key::ArrowRight);
    frame(&mut context, &mut tree, &mut state);
    assert_eq!(tree.len(), 3);
    key(&mut context, Key::ArrowRight);
    frame(&mut context, &mut tree, &mut state);
    assert_eq!(state.selected(), Some(ItemId(2)));
    key(&mut context, Key::ArrowRight);
    frame(&mut context, &mut tree, &mut state);
    assert_eq!(tree.len(), 4);
    key(&mut context, Key::ArrowLeft);
    frame(&mut context, &mut tree, &mut state);
    assert_eq!(tree.len(), 3);
    key(&mut context, Key::ArrowLeft);
    frame(&mut context, &mut tree, &mut state);
    assert_eq!(state.selected(), Some(ItemId(1)));
    context.action(context.find(&["tree", "item-1"]).unwrap(), Action::Collapse);
    frame(&mut context, &mut tree, &mut state);
    assert_eq!(tree.len(), 2);
}

struct Grid {
    rows: Million,
    columns: Vec<GridColumn>,
}
impl ListModel for Grid {
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        self.rows.item_id(index)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        self.rows.index_of(id)
    }
    fn label(&self, index: usize) -> String {
        self.rows.label(index)
    }
}
impl GridModel for Grid {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        format!("{} / {column}", self.item_id(row).0)
    }
    fn sort(&mut self, _: usize, direction: SortDirection) -> bool {
        self.rows.reverse = direction == SortDirection::Descending;
        true
    }
}
#[test]
fn grid_realizes_bounded_cells_and_sorting_preserves_selected_identity() {
    let mut grid = Grid {
        rows: model(100_000),
        columns: vec![
            GridColumn {
                label: "ID".into(),
                width: 120.0,
                sortable: true,
                editor: None,
                filterable: false,
            },
            GridColumn {
                label: "Value".into(),
                width: 200.0,
                sortable: false,
                editor: None,
                filterable: false,
            },
        ],
    };
    let mut context = Context::new();
    let mut state = GridState::default();
    state.rows.select(Some(ItemId(7)));
    let frame = |context: &mut Context, grid: &mut Grid, state: &mut GridState| {
        context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, |ui| {
            ui.keyed("grid", |ui| ui.data_grid(grid, state, options()));
        });
    };
    frame(&mut context, &mut grid, &mut state);
    // About thirteen realized rows of six nodes each (the row, its line, the
    // window onto the scrolling columns, their strip and two cells) plus the
    // header: bounded by the viewport, not by the hundred thousand rows.
    assert!(context.node_count() < 100);
    let header = context
        .find(&[
            "grid",
            "$header",
            "$scrolling",
            "$strip",
            "0",
            "$cell",
            "$drop",
            "$drag",
            "$sort",
        ])
        .unwrap();
    context.action(header, Action::Activate);
    frame(&mut context, &mut grid, &mut state);
    context.action(header, Action::Activate);
    frame(&mut context, &mut grid, &mut state);
    assert_eq!(state.sort, Some((0, SortDirection::Descending)));
    assert_eq!(state.rows.selected(), Some(ItemId(7)));
    assert!(context.node_count() < 100);
    let info = context
        .node(context.find(&["grid"]).unwrap())
        .unwrap()
        .collection
        .unwrap();
    assert_eq!(info.total, 100_000);
    assert_eq!(info.columns, Some(2));
}
