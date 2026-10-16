mod support;

use rust_gui_core::app::Action as Message;
use rust_gui_core::{
    Action, Event, GridColumn, GridModel, GridMsg, GridState, ItemId, ItemOrder, Key, ListModel,
    ListState, ModelError, SortDirection, TreeItem, TreeModel, Ui, VirtualListOptions,
};
use rust_gui_scene::{Point, Size};
use std::cell::Cell;
use support::{Driven, act};

const ROOT: Size = Size::new(500.0, 500.0);

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

/// A list's state as the application keeps it: the model and the selection.
struct Lists {
    model: Million,
    list: ListState,
    activated: Vec<ItemId>,
}

fn list_msg(message: GridMsg) -> Message<Lists> {
    act(move |s: &mut Lists| match &message {
        GridMsg::Select(id) => s.list.select(Some(*id)),
        GridMsg::Deselect => s.list.select(None),
        GridMsg::Activate(id) => s.activated.push(*id),
        _ => {}
    })
}

fn list_view(state: &Lists, ui: &mut Ui<'_, Message<Lists>>) {
    ui.keyed("list", |ui| {
        ui.virtual_list_msg(&state.model, &state.list, options(), list_msg)
    });
}

fn key<S: 'static>(driven: &mut Driven<S>, key: Key) {
    driven.context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

#[test]
fn million_items_realize_only_the_viewport_and_can_jump_to_last_item() {
    let mut driven = Driven::new(
        ROOT,
        Lists {
            model: model(1_000_000),
            list: ListState::default(),
            activated: Vec::new(),
        },
        list_view,
    );
    assert!(driven.context.find(&["list", "item-0"]).is_some());
    assert!(driven.context.find(&["list", "item-12"]).is_none());
    assert!(driven.context.node_count() < 35);
    assert!(driven.state().model.labels.get() < 30);
    let owner = driven.context.find(&["list"]).unwrap();
    driven.context.action(owner, Action::Focus);
    key(&mut driven, Key::End);
    driven.frame();
    assert_eq!(driven.state().list.selected(), Some(ItemId(999_999)));
    assert!(driven.context.find(&["list", "item-999999"]).is_some());
    assert!(driven.context.node_count() < 35);
    assert_eq!(
        driven.context.node(owner).unwrap().scroll.unwrap().maximum,
        29_999_700.0
    );
    let row = driven.context.find(&["list", "item-999999"]).unwrap();
    let bounds = driven.context.node(row).unwrap().bounds;
    assert!(bounds.y >= 0.0 && bounds.y + bounds.height <= 301.0);
    assert_eq!(
        driven.context.hit_test(Point {
            x: 12.0,
            y: bounds.y + 12.0
        }),
        Some(row)
    );
    assert_eq!(driven.context.focused(), Some(owner));
    // Enter activates the selected row, once.
    assert!(driven.state().activated.is_empty());
    key(&mut driven, Key::Enter);
    driven.frame();
    assert_eq!(driven.state().activated, [ItemId(999_999)]);
}

#[test]
fn enter_activates_only_a_selected_row() {
    let mut driven = Driven::new(
        ROOT,
        Lists {
            model: model(10),
            list: ListState::default(),
            activated: Vec::new(),
        },
        list_view,
    );
    let owner = driven.context.find(&["list"]).unwrap();
    driven.context.action(owner, Action::Focus);
    key(&mut driven, Key::Enter);
    driven.frame();
    assert!(driven.state().activated.is_empty());
    key(&mut driven, Key::ArrowDown);
    key(&mut driven, Key::Enter);
    driven.frame();
    assert_eq!(driven.state().activated, [ItemId(0)]);
}

#[test]
fn a_selection_the_model_lost_is_cleared_by_a_message() {
    let mut driven = Driven::new(
        ROOT,
        Lists {
            model: model(10),
            list: ListState::default(),
            activated: Vec::new(),
        },
        list_view,
    );
    driven.state_mut().list.select(Some(ItemId(8)));
    driven.frame();
    assert_eq!(driven.state().list.selected(), Some(ItemId(8)));
    driven.state_mut().model.count = 5;
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().list.selected(), None);
}

#[test]
fn pointer_capture_survives_the_intervening_frame_and_keyboard_events_accumulate() {
    let mut driven = Driven::new(
        ROOT,
        Lists {
            model: model(1000),
            list: ListState::default(),
            activated: Vec::new(),
        },
        list_view,
    );
    driven
        .context
        .event(Event::PointerMoved(Point { x: 15.0, y: 75.0 }));
    driven.context.event(Event::PointerDown);
    driven.frame();
    assert!(driven.context.captured().is_some());
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert_eq!(driven.state().list.selected(), Some(ItemId(2)));
    for _ in 0..7 {
        key(&mut driven, Key::ArrowDown);
    }
    driven.frame();
    assert_eq!(driven.state().list.selected(), Some(ItemId(9)));
    driven
        .context
        .event(Event::PointerMoved(Point { x: 15.0, y: 75.0 }));
    driven.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 450.0,
    });
    driven.frame();
    assert!(driven.context.find(&["list", "item-12"]).is_none());
    assert!(driven.context.find(&["list", "item-13"]).is_some());
}

#[test]
fn empty_lists_and_disabled_collections_reject_selection() {
    let mut driven = Driven::new(
        ROOT,
        Lists {
            model: model(0),
            list: ListState::default(),
            activated: Vec::new(),
        },
        list_view,
    );
    let owner = driven.context.find(&["list"]).unwrap();
    driven.context.action(owner, Action::Focus);
    key(&mut driven, Key::End);
    driven.frame();
    assert!(driven.context.find(&["list", "item-0"]).is_none());
    driven.state_mut().model.count = 100;
    driven.set_view(|state, ui| {
        ui.enabled(false, |ui| {
            ui.keyed("list", |ui| {
                ui.virtual_list_msg(&state.model, &state.list, options(), list_msg)
            });
        })
    });
    driven.frame();
    assert!(!driven.context.action(owner, Action::Focus));
    assert!(!driven.context.action(
        driven.context.find(&["list", "item-0"]).unwrap(),
        Action::Activate
    ));
}

/// A tree's state as the application keeps it: the model and the selection.
struct Trees {
    tree: TreeModel,
    list: ListState,
}

fn tree_msg(message: GridMsg) -> Message<Trees> {
    act(move |s: &mut Trees| match &message {
        GridMsg::Select(id) => s.list.select(Some(*id)),
        GridMsg::Expand { row, open } => {
            s.tree.set_expanded(*row, *open);
        }
        _ => {}
    })
}

fn tree_view(state: &Trees, ui: &mut Ui<'_, Message<Trees>>) {
    ui.keyed("tree", |ui| {
        ui.tree_view_msg(&state.tree, &state.list, options(), tree_msg)
    });
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
    let tree = TreeModel::new(vec![
        item(1, None),
        item(2, Some(1)),
        item(3, Some(2)),
        item(4, None),
    ])
    .unwrap();
    let mut driven = Driven::new(
        ROOT,
        Trees {
            tree,
            list: ListState::default(),
        },
        tree_view,
    );
    assert_eq!(driven.state().tree.len(), 2);
    driven
        .context
        .action(driven.context.find(&["tree"]).unwrap(), Action::Focus);
    key(&mut driven, Key::Home);
    key(&mut driven, Key::ArrowRight);
    driven.frame();
    assert_eq!(driven.state().tree.len(), 3);
    key(&mut driven, Key::ArrowRight);
    driven.frame();
    assert_eq!(driven.state().list.selected(), Some(ItemId(2)));
    key(&mut driven, Key::ArrowRight);
    driven.frame();
    assert_eq!(driven.state().tree.len(), 4);
    key(&mut driven, Key::ArrowLeft);
    driven.frame();
    assert_eq!(driven.state().tree.len(), 3);
    key(&mut driven, Key::ArrowLeft);
    driven.frame();
    assert_eq!(driven.state().list.selected(), Some(ItemId(1)));
    driven.context.action(
        driven.context.find(&["tree", "item-1"]).unwrap(),
        Action::Collapse,
    );
    driven.frame();
    assert_eq!(driven.state().tree.len(), 2);
}

struct Table {
    rows: Million,
    columns: Vec<GridColumn>,
    grid: GridState,
}
impl ListModel for Table {
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
impl GridModel for Table {
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

fn grid_msg(message: GridMsg) -> Message<Table> {
    act(move |s: &mut Table| {
        s.grid.apply(&message);
        if let GridMsg::Sort(column, direction) = &message {
            s.sort(*column, *direction);
        }
    })
}

fn grid_view(state: &Table, ui: &mut Ui<'_, Message<Table>>) {
    ui.keyed("grid", |ui| {
        ui.data_grid_msg(state, &state.grid, options(), grid_msg)
    });
}

#[test]
fn grid_realizes_bounded_cells_and_sorting_preserves_selected_identity() {
    let table = Table {
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
        grid: GridState::default(),
    };
    let mut driven = Driven::new(ROOT, table, grid_view);
    driven.state_mut().grid.rows.select(Some(ItemId(7)));
    driven.frame();
    // About thirteen realized rows of six nodes each (the row, its line, the
    // window onto the scrolling columns, their strip and two cells) plus the
    // header: bounded by the viewport, not by the hundred thousand rows.
    assert!(driven.context.node_count() < 100);
    let header = driven
        .context
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
    driven.context.action(header, Action::Activate);
    driven.frame();
    driven.context.action(header, Action::Activate);
    driven.frame();
    assert_eq!(
        driven.state().grid.sort,
        Some((0, SortDirection::Descending))
    );
    assert_eq!(driven.state().grid.rows.selected(), Some(ItemId(7)));
    assert!(driven.context.node_count() < 100);
    let info = driven
        .context
        .node(driven.context.find(&["grid"]).unwrap())
        .unwrap()
        .collection
        .unwrap();
    assert_eq!(info.total, 100_000);
    assert_eq!(info.columns, Some(2));
}

#[test]
fn stable_selection_outlives_unrealized_nodes_and_reorder_preserves_anchor() {
    let mut driven = Driven::new(
        ROOT,
        Lists {
            model: model(1000),
            list: ListState::default(),
            activated: Vec::new(),
        },
        list_view,
    );
    let old_node = driven.context.find(&["list", "item-4"]).unwrap();
    driven.context.action(old_node, Action::Activate);
    driven.frame();
    let owner = driven.context.find(&["list"]).unwrap();
    let offset =
        |driven: &Driven<Lists>| driven.context.node(owner).unwrap().scroll.unwrap().offset;
    driven.context.action(owner, Action::ScrollTo(15000.0));
    driven.frame();
    assert!(driven.context.node(old_node).is_none());
    assert_eq!(driven.state().list.selected(), Some(ItemId(4)));
    assert_eq!(offset(&driven), 15000.0);
    // The row on top (500) is where the reversed model puts it.
    driven.state_mut().model.reverse = true;
    driven.frame();
    assert_eq!(offset(&driven), 499.0 * 30.0);
    assert_eq!(driven.state().list.selected(), Some(ItemId(4)));
    driven.state_mut().list.reveal(ItemId(4));
    driven.frame();
    assert_ne!(driven.context.find(&["list", "item-4"]).unwrap(), old_node);
    assert!(!driven.context.action(old_node, Action::Activate));
    driven.state_mut().model.count = 3;
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().list.selected(), None);
    assert_eq!(offset(&driven), 0.0);
}
