//! Milestone 35, phase 2B: grids, lists and trees send `GridMsg`s through
//! `Harness::app`, and the application applies them in `update`.

use rust_gui_core::{
    App, CellEditor, CellValue, Effects, FILTER_DELAY, GridColumn, GridModel, GridMsg, GridState,
    ItemId, ListModel, SortDirection, Ui, VirtualListOptions,
};
use rust_gui_scene::Size;
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
}

impl App for Table {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        let Msg::Grid(grid) = message;
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
        ui.data_grid_msg(
            &self.model,
            &self.state,
            VirtualListOptions::default(),
            Msg::Grid,
        );
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
    harness.send(Msg::Grid(GridMsg::Filter {
        column: 0,
        text: " a ".into(),
    }));
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
