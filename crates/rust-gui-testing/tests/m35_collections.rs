//! Milestone 35, phase 2B: grids, lists and trees send `GridMsg`s through
//! `Harness::app`, and the application applies them in `update`.

use rust_gui_core::{
    Action, App, CellEditor, CellValue, Effects, Event, FILTER_DELAY, GridColumn, GridModel,
    GridMsg, GridState, ItemId, Key, ListModel, ListState, SortDirection, TreeItem, TreeModel, Ui,
    VirtualListOptions,
};
use rust_gui_scene::{Point, Size};
use rust_gui_testing::Harness;
use std::cell::Cell;
use std::time::Duration;

/// Rows of (name, qty). Identity is the row's index in `data`, stable across
/// sorting and filtering.
struct Rows {
    data: Vec<(String, String)>,
    /// Row indices in sorted order.
    order: Vec<usize>,
    /// Row indices shown: `order` narrowed by the name filter.
    shown: Vec<usize>,
    columns: Vec<GridColumn>,
}

impl Rows {
    fn new(data: &[(&str, &str)]) -> Self {
        let data: Vec<(String, String)> = data
            .iter()
            .map(|(name, qty)| (name.to_string(), qty.to_string()))
            .collect();
        let order: Vec<usize> = (0..data.len()).collect();
        Self {
            shown: order.clone(),
            order,
            data,
            columns: vec![
                GridColumn::new("Name", 120.0)
                    .sortable(true)
                    .filterable(true),
                GridColumn::new("Qty", 80.0).editor(CellEditor::Text),
            ],
        }
    }

    fn shown_names(&self) -> Vec<String> {
        self.shown
            .iter()
            .map(|row| self.data[*row].0.clone())
            .collect()
    }
}

impl ListModel for Rows {
    fn len(&self) -> usize {
        self.shown.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(self.shown[index] as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        self.shown.iter().position(|row| *row as u64 == id.0)
    }
    fn label(&self, index: usize) -> String {
        self.data[self.shown[index]].0.clone()
    }
}

impl GridModel for Rows {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        let data = &self.data[self.shown[row]];
        if column == 0 {
            data.0.clone()
        } else {
            data.1.clone()
        }
    }
    fn sort(&mut self, column: usize, direction: SortDirection) -> bool {
        if column != 0 {
            return false;
        }
        self.order
            .sort_by(|a, b| self.data[*a].0.cmp(&self.data[*b].0));
        if direction == SortDirection::Descending {
            self.order.reverse();
        }
        self.shown = self
            .order
            .iter()
            .copied()
            .filter(|row| self.shown.contains(row))
            .collect();
        true
    }
    fn value(&self, row: usize, column: usize) -> CellValue {
        CellValue::Text(self.cell(row, column))
    }
    fn set_cell(&mut self, row: usize, column: usize, value: CellValue) -> Result<(), String> {
        match (column, value) {
            (1, CellValue::Text(text)) if text == "bad" => Err("not a quantity".into()),
            (1, CellValue::Text(text)) => {
                self.data[self.shown[row]].1 = text;
                Ok(())
            }
            _ => Err("read-only".into()),
        }
    }
    fn filter(&mut self, filters: &[(usize, &str)]) -> bool {
        self.shown = self
            .order
            .iter()
            .copied()
            .filter(|row| {
                filters.iter().all(|(column, text)| {
                    *column != 0
                        || self.data[*row]
                            .0
                            .to_lowercase()
                            .contains(&text.to_lowercase())
                })
            })
            .collect();
        true
    }
}

#[derive(Clone, Debug)]
enum Msg {
    Grid(GridMsg),
}

/// Records what `update` did and how often the view declared.
struct Table {
    model: Rows,
    state: GridState,
    views: Cell<usize>,
    filter_calls: usize,
    /// Every message `update` received, in order.
    log: Vec<GridMsg>,
}

impl App for Table {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        let Msg::Grid(grid) = message;
        self.log.push(grid.clone());
        self.state.apply(&grid);
        match &grid {
            GridMsg::Sort(column, direction) => {
                self.model.sort(*column, *direction);
            }
            GridMsg::FiltersApplied => {
                self.filter_calls += 1;
                let filters: Vec<(usize, String)> = self.state.applied_filters().to_vec();
                let pairs: Vec<(usize, &str)> = filters
                    .iter()
                    .map(|(column, text)| (*column, text.as_str()))
                    .collect();
                self.model.filter(&pairs);
            }
            GridMsg::Edit { row, column, value } => {
                if let Some(index) = self.model.index_of(*row) {
                    let _ = self.model.set_cell(index, *column, value.clone());
                }
            }
            _ => {}
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        ui.keyed("grid", |ui| {
            ui.data_grid(
                &self.model,
                &self.state,
                VirtualListOptions::default(),
                Msg::Grid,
            )
        });
    }
}

fn table() -> Harness<Table> {
    let model = Rows::new(&[("Cara", "3"), ("Ada", "1"), ("Bo", "2")]);
    Harness::app(
        Size::new(640.0, 480.0),
        Table {
            model,
            state: GridState::default(),
            views: Cell::new(0),
            filter_calls: 0,
            log: Vec::new(),
        },
    )
}

#[test]
fn sort_click_sends_sort_and_update_resorts() {
    let mut harness = table();
    assert_eq!(harness.state().model.shown_names(), ["Cara", "Ada", "Bo"]);

    let header = harness.by_label("Name");
    harness.click(header);

    assert_eq!(harness.state().model.shown_names(), ["Ada", "Bo", "Cara"]);
    assert_eq!(
        harness.state().state.sort,
        Some((0, SortDirection::Ascending))
    );
}

#[test]
fn one_click_declares_the_view_once() {
    let mut harness = table();
    let before = harness.state().views.get();
    let header = harness.by_label("Name");
    let stats = harness.click(header);

    assert!(stats.declared);
    assert_eq!(stats.messages, 1);
    assert_eq!(harness.state().views.get(), before + 1);
}

#[test]
fn pointer_over_a_header_declares_nothing() {
    let mut harness = table();
    // The first frames ask for another while the rows have no width yet.
    harness.run();
    harness.run();
    let before = harness.state().views.get();
    let header = harness.by_label("Name");
    let stats = harness.hover(header);

    assert!(!stats.declared);
    assert_eq!(harness.state().views.get(), before);
}

#[test]
fn edit_message_is_stored_by_set_cell() {
    let mut harness = table();
    let ada = ItemId(1);
    harness.send(Msg::Grid(GridMsg::Edit {
        row: ada,
        column: 1,
        value: CellValue::Text("7".into()),
    }));

    assert_eq!(harness.state().model.data[1].1, "7");
}

#[test]
fn select_message_selects_the_row() {
    let mut harness = table();
    harness.send(Msg::Grid(GridMsg::Select(ItemId(2))));

    assert_eq!(harness.state().state.rows.selected(), Some(ItemId(2)));
}

#[test]
fn resize_and_move_messages_change_columns_only() {
    let mut harness = table();
    harness.send(Msg::Grid(GridMsg::ResizeColumn(1, 200.0)));
    harness.send(Msg::Grid(GridMsg::MoveColumn(vec![1, 0])));

    let columns = &harness.state().state.columns;
    assert_eq!(columns.width(1, 80.0), 200.0);
    assert_eq!(columns.arranged(2), vec![1, 0]);
    assert_eq!(harness.state().filter_calls, 0);
}

#[test]
fn filter_waits_for_the_delay_then_sends_filters_applied() {
    let mut harness = table();
    harness.set_now(Duration::ZERO);
    let field = harness.by_label("Filter Name");
    harness.focus(field);
    harness.type_text(" a ");
    assert_eq!(
        harness.state().state.filters.get(&0).map(String::as_str),
        Some(" a ")
    );
    // The view noted the pending filter; the next batch starts its delay.
    harness.run();
    assert_eq!(harness.state().filter_calls, 0);
    assert_eq!(harness.state().model.shown_names().len(), 3);

    harness.set_now(FILTER_DELAY / 2);
    harness.run();
    assert_eq!(harness.state().filter_calls, 0, "the delay has not passed");

    harness.set_now(FILTER_DELAY + Duration::from_millis(1));
    harness.run();
    assert_eq!(
        harness.state().filter_calls,
        1,
        "FiltersApplied is sent once"
    );
    harness.run();
    assert_eq!(
        harness.state().filter_calls,
        1,
        "and not again on the next frame"
    );
    assert_eq!(harness.state().model.shown_names(), ["Cara", "Ada"]);
    assert_eq!(
        harness.state().state.applied_filters().to_vec(),
        vec![(0, "a".to_owned())]
    );

    harness.set_now(FILTER_DELAY * 3);
    harness.run();
    assert_eq!(
        harness.state().filter_calls,
        1,
        "no filter is pending, so no second FiltersApplied"
    );
}

/// The rows of the grid, focused, with nothing selected.
fn focused_rows(harness: &mut Harness<Table>) -> rust_gui_core::NodeId {
    let rows = harness.find(&["grid", "$rows"]);
    harness.focus(rows);
    // Two frames: the first knows the width of the rows.
    harness.run();
    rows
}

fn selected(harness: &Harness<Table>) -> Option<ItemId> {
    harness.state().state.rows.selected()
}

fn key(harness: &mut Harness<Table>, key: Key) {
    harness.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

#[test]
fn keyboard_moves_the_selection_with_select_messages() {
    let mut harness = table();
    focused_rows(&mut harness);
    harness.app_mut().log.clear();

    harness.press(Key::ArrowDown);
    assert_eq!(selected(&harness), Some(ItemId(0)));
    harness.press(Key::ArrowDown);
    assert_eq!(selected(&harness), Some(ItemId(1)));
    harness.press(Key::End);
    assert_eq!(selected(&harness), Some(ItemId(2)));
    harness.press(Key::ArrowUp);
    assert_eq!(selected(&harness), Some(ItemId(1)));
    harness.press(Key::Home);
    assert_eq!(selected(&harness), Some(ItemId(0)));
    harness.press(Key::PageDown);
    assert_eq!(
        selected(&harness),
        Some(ItemId(2)),
        "a page is the whole list"
    );
    harness.press(Key::PageUp);
    assert_eq!(selected(&harness), Some(ItemId(0)));

    let selects: Vec<ItemId> = harness
        .state()
        .log
        .iter()
        .filter_map(|message| match message {
            GridMsg::Select(id) => Some(*id),
            _ => None,
        })
        .collect();
    let expected: Vec<ItemId> = [0, 1, 2, 1, 0, 2, 0].map(ItemId).to_vec();
    assert_eq!(selects, expected);
}

#[test]
fn several_messages_for_one_collection_in_one_batch_all_arrive_in_order() {
    let mut harness = table();
    focused_rows(&mut harness);
    harness.app_mut().log.clear();

    // Three keys and no frame between them: one batch.
    key(&mut harness, Key::ArrowDown);
    key(&mut harness, Key::ArrowDown);
    key(&mut harness, Key::ArrowDown);
    let stats = harness.run();

    assert_eq!(stats.messages, 3);
    assert_eq!(
        harness.state().log,
        vec![
            GridMsg::Select(ItemId(0)),
            GridMsg::Select(ItemId(1)),
            GridMsg::Select(ItemId(2)),
        ]
    );
    assert_eq!(selected(&harness), Some(ItemId(2)));
}

/// Whether an editor is open in a cell of the grid.
fn editing(harness: &Harness<Table>) -> bool {
    harness.snapshot_tree().contains("$edit")
}

#[test]
fn typing_in_a_cell_then_enter_sends_edit_and_moves_to_the_next_row() {
    let mut harness = table();
    focused_rows(&mut harness);
    harness.press(Key::ArrowDown);
    harness.press(Key::ArrowRight);
    harness.app_mut().log.clear();

    // Typing replaces the content of the current cell.
    harness.type_text("9");
    assert!(editing(&harness));
    harness.press(Key::Enter);
    // The edit is sent one batch after the key.
    harness.run();

    assert_eq!(
        harness.state().log,
        vec![GridMsg::Edit {
            row: ItemId(0),
            column: 1,
            value: CellValue::Text("9".into()),
        }]
    );
    assert_eq!(harness.state().model.data[0].1, "9");

    // The model took it: the editor closes and the selection moves on.
    harness.run();
    harness.run();
    assert!(!editing(&harness));
    assert_eq!(selected(&harness), Some(ItemId(1)));
}

#[test]
fn f2_opens_the_editor_and_enter_sends_what_was_typed() {
    let mut harness = table();
    focused_rows(&mut harness);
    harness.press(Key::ArrowDown);
    harness.press(Key::ArrowRight);
    harness.app_mut().log.clear();

    harness.press(Key::F2);
    assert!(editing(&harness));
    harness.run();
    harness.type_text("5");
    harness.press(Key::Enter);
    harness.run();

    let [GridMsg::Edit { row, column, value }] = harness.state().log.as_slice() else {
        panic!("one Edit expected, got {:?}", harness.state().log);
    };
    assert_eq!((*row, *column), (ItemId(0), 1));
    let CellValue::Text(text) = value else {
        panic!("a text value, got {value:?}");
    };
    assert!(
        text.contains('5') && text != "5",
        "typed into `3`: {text:?}"
    );
}

#[test]
fn an_edit_left_unchanged_sends_nothing() {
    let mut harness = table();
    focused_rows(&mut harness);
    harness.press(Key::ArrowDown);
    harness.press(Key::ArrowRight);
    harness.app_mut().log.clear();

    harness.press(Key::F2);
    harness.run();
    harness.press(Key::Enter);
    harness.run();
    harness.run();

    assert!(!editing(&harness));
    assert!(
        harness
            .state()
            .log
            .iter()
            .all(|message| !matches!(message, GridMsg::Edit { .. }))
    );
}

#[test]
fn a_refused_edit_keeps_the_editor_open_until_cancelled() {
    let mut harness = table();
    focused_rows(&mut harness);
    harness.press(Key::ArrowDown);
    harness.press(Key::ArrowRight);

    harness.type_text("bad");
    harness.press(Key::Enter);
    harness.run();
    harness.run();
    harness.run();

    // set_cell said no: the value is not stored, and the draft is still shown.
    assert_eq!(harness.state().model.data[0].1, "3");
    assert!(editing(&harness));
    assert!(harness.state().log.contains(&GridMsg::Edit {
        row: ItemId(0),
        column: 1,
        value: CellValue::Text("bad".into()),
    }));

    harness.press(Key::Escape);
    harness.run();
    assert!(!editing(&harness));
    assert_eq!(harness.state().model.data[0].1, "3");
}

#[test]
fn header_menu_pick_sends_the_new_column_layout() {
    let mut harness = table();
    harness.run();
    let sort = harness.by_label("Name");
    harness.focus(sort);
    harness.press(Key::ContextMenu);
    harness.run();
    let popup = harness.find(&["grid", "$header", "$scrolling", "$strip", "0", "$popup"]);
    let entry = harness
        .context()
        .node(popup)
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|entry| harness.context().node(*entry).unwrap().semantics.label == "Wider")
        .expect("a Wider entry");
    harness.app_mut().log.clear();

    assert!(harness.action(entry, Action::Activate));
    harness.run();

    let layout = harness
        .state()
        .log
        .iter()
        .find_map(|message| match message {
            GridMsg::Columns(layout) => Some(layout.clone()),
            _ => None,
        })
        .expect("a Columns message");
    assert_eq!(layout.width(0, 120.0), 140.0);
    assert_eq!(harness.state().state.columns.width(0, 120.0), 140.0);
}

#[test]
fn filter_set_from_code_applies_at_once() {
    let mut harness = table();
    harness.set_now(Duration::ZERO);
    harness.run();
    harness.app_mut().state.filters.insert(0, "a".to_owned());
    // The clock never moves: no delay is waited out.
    harness.run();
    harness.run();

    assert_eq!(harness.state().filter_calls, 1);
    assert_eq!(harness.state().model.shown_names(), ["Cara", "Ada"]);
    harness.run();
    assert_eq!(harness.state().filter_calls, 1, "sent once");
}

/// A tree of one folder with one child, selected and opened by messages.
struct Outline {
    model: TreeModel,
    state: ListState,
    log: Vec<GridMsg>,
}

impl App for Outline {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        let Msg::Grid(grid) = message;
        self.log.push(grid.clone());
        match grid {
            GridMsg::Select(id) => self.state.select(Some(id)),
            GridMsg::Expand { row, open } => {
                self.model.set_expanded(row, open);
            }
            _ => {}
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("tree", |ui| {
            ui.tree_view(
                &self.model,
                &self.state,
                VirtualListOptions::default(),
                Msg::Grid,
            )
        });
    }
}

fn outline() -> Harness<Outline> {
    let item = |id, parent: Option<u64>, label: &str| TreeItem {
        id: ItemId(id),
        parent: parent.map(ItemId),
        label: label.into(),
    };
    Harness::app(
        Size::new(640.0, 480.0),
        Outline {
            model: TreeModel::new(vec![
                item(1, None, "Folder"),
                item(2, Some(1), "Child"),
                item(3, None, "Other"),
            ])
            .unwrap(),
            state: ListState::default(),
            log: Vec::new(),
        },
    )
}

#[test]
fn clicking_a_disclosure_sends_select_and_expand() {
    let mut harness = outline();
    harness.run();
    let label = harness.by_label("▸  Folder");
    let bounds = harness.context().node(label).unwrap().bounds;
    // The disclosure sits at the start of the row.
    let at = Point {
        x: bounds.x + 4.0,
        y: bounds.y + bounds.height / 2.0,
    };
    harness.event(Event::PointerMoved(at));
    harness.event(Event::PointerDown);
    harness.event(Event::PointerUp);
    harness.run();

    assert_eq!(
        harness.state().log,
        vec![
            GridMsg::Select(ItemId(1)),
            GridMsg::Expand {
                row: ItemId(1),
                open: true
            }
        ]
    );
    assert!(harness.query_label("▾  Folder").is_some());
    assert!(harness.query_label("   Child").is_some());
}

#[test]
fn right_and_left_arrows_expand_and_collapse_the_selected_row() {
    let mut harness = outline();
    harness.run();
    let rows = harness.find(&["tree"]);
    harness.focus(rows);
    harness.press(Key::Home);
    harness.app_mut().log.clear();

    harness.press(Key::ArrowRight);
    assert_eq!(
        harness.state().log,
        vec![GridMsg::Expand {
            row: ItemId(1),
            open: true
        }]
    );
    harness.press(Key::ArrowDown);
    harness.press(Key::ArrowLeft);
    assert_eq!(
        harness.state().log.last(),
        Some(&GridMsg::Select(ItemId(1))),
        "Left on a child selects its parent"
    );
}
