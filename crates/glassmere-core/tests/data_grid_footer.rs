mod support;

use glassmere_core::app::Action as Message;
use glassmere_core::{
    Action, Event, GridColumn, GridModel, GridMsg, GridState, ItemId, Length, ListModel, NodeId,
    Role, SortDirection, Style, Ui, VirtualListOptions,
};
use glassmere_scene::{Point, Rect, Size};
use support::{Driven, act, click};

const ROOT: Size = Size::new(900.0, 500.0);
const ROWS: usize = 40;

/// Eight 100 px columns whose cells are the sum of their coordinates, with
/// whatever footer the test gives it.
struct Sums {
    columns: Vec<GridColumn>,
    footer: Vec<Vec<String>>,
}

impl ListModel for Sums {
    fn len(&self) -> usize {
        ROWS
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        Some(id.0 as usize).filter(|index| *index < ROWS)
    }
    fn label(&self, index: usize) -> String {
        format!("row {index}")
    }
}

impl GridModel for Sums {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        format!("r{row}c{column}")
    }
    fn sort(&mut self, _: usize, _: SortDirection) -> bool {
        false
    }
    fn footer(&self) -> Vec<Vec<String>> {
        self.footer.clone()
    }
}

struct Grid {
    model: Sums,
    grid: GridState,
}

fn grid_msg(message: GridMsg) -> Message<Grid> {
    act(move |s: &mut Grid| s.grid.apply(&message))
}

fn grid_view(state: &Grid, ui: &mut Ui<'_, Message<Grid>>) {
    ui.keyed("grid", |ui| {
        let options = VirtualListOptions {
            style: Style::default().width(Length::Px(450.0)),
            ..VirtualListOptions::default()
        };
        ui.data_grid(&state.model, &state.grid, options, grid_msg)
    });
}

fn footer(rows: &[&[&str]]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
        .collect()
}

fn driven(rows: &[&[&str]]) -> Driven<Grid> {
    let model = Sums {
        columns: (0..8)
            .map(|column| GridColumn::new(format!("C{column}"), 100.0))
            .collect(),
        footer: footer(rows),
    };
    let mut driven = Driven::new(
        ROOT,
        Grid {
            model,
            grid: GridState::default(),
        },
        grid_view,
    );
    // The first declaration learned the width, the second uses it.
    driven.state_mut();
    driven.frame();
    driven.state_mut();
    driven.frame();
    driven
}

fn bounds(driven: &Driven<Grid>, path: &[&str]) -> Rect {
    let id = driven
        .context
        .find(path)
        .unwrap_or_else(|| panic!("{path:?}"));
    driven.context.node(id).unwrap().bounds
}

fn node(driven: &Driven<Grid>, path: &[&str]) -> NodeId {
    driven
        .context
        .find(path)
        .unwrap_or_else(|| panic!("{path:?}"))
}

#[test]
fn footer_rows_stay_under_a_scrolling_body_in_the_same_columns() {
    let mut driven = driven(&[&["Total", "", "", "", "", "", "", "T7"], &["Mean"]]);
    driven.state_mut().grid.columns.frozen = 2;
    driven.frame();
    let rows_id = node(&driven, &["grid", "$rows"]);
    driven.context.action(rows_id, Action::ScrollByX(320.0));
    driven.frame();
    let rows = bounds(&driven, &["grid", "$rows"]);

    // Header, body and two footer rows fill the height the grid was given.
    let header = bounds(&driven, &["grid", "$header"]);
    let first = bounds(&driven, &["grid", "$footer", "0"]);
    let second = bounds(&driven, &["grid", "$footer", "1"]);
    assert_eq!(rows.height, 320.0 - 32.0 - 2.0 * 32.0 - 6.0);
    assert_eq!(first.y, rows.y + rows.height);
    assert_eq!(second.y, first.y + 32.0);
    assert_eq!(header.y + 320.0 - 6.0, second.y + 32.0);

    // The frozen pair sits at the start, the rest scrolls with the body.
    let body = |cells: &[&str]| {
        bounds(
            &driven,
            &[&["grid", "$rows", "item-0", "$cells"], cells].concat(),
        )
    };
    let pinned = |cells: &[&str]| bounds(&driven, &[&["grid", "$footer", "0"], cells].concat());
    for column in ["0", "1"] {
        assert_eq!(pinned(&[column]).x, body(&[column]).x);
        assert_eq!(
            pinned(&[column]).x,
            rows.x + if column == "0" { 0.0 } else { 100.0 }
        );
    }
    let scrolled = ["$scrolling", "$strip", "5"];
    assert_eq!(pinned(&scrolled).x, body(&scrolled).x);
    assert_eq!(pinned(&scrolled).x, rows.x + 200.0 - 20.0);

    // What the model said is what shows; a short row leaves the rest empty.
    let text = |driven: &Driven<Grid>, path: &[&str]| {
        driven
            .context
            .node(node(driven, path))
            .unwrap()
            .semantics
            .label
            .clone()
    };
    assert_eq!(text(&driven, &["grid", "$footer", "0", "0"]), "Total");
    assert_eq!(text(&driven, &["grid", "$footer", "1", "0"]), "Mean");
    assert_eq!(text(&driven, &["grid", "$footer", "1", "1"]), "");

    // The body scrolls away under them: they do not move.
    driven.context.event(Event::PointerMoved(Point {
        x: rows.x + 50.0,
        y: rows.y + 50.0,
    }));
    assert!(driven.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 200.0
    }));
    driven.frame();
    assert!(driven.context.find(&["grid", "$rows", "item-0"]).is_none());
    assert_eq!(bounds(&driven, &["grid", "$footer", "0"]), first);

    // No footer, no rows of it, and the body has the height back.
    driven.state_mut().model.footer = Vec::new();
    driven.frame();
    assert!(driven.context.find(&["grid", "$footer"]).is_none());
    assert_eq!(
        bounds(&driven, &["grid", "$rows"]).height,
        320.0 - 32.0 - 6.0
    );
}

#[test]
fn footer_cells_are_cells_after_the_rows_and_cannot_be_chosen() {
    let mut driven = driven(&[&["Total", "sum"], &["Mean", "avg"]]);
    let grid = driven.context.node(node(&driven, &["grid"])).unwrap();
    assert_eq!(grid.collection.unwrap().total, ROWS + 2);

    let total = driven
        .context
        .node(node(
            &driven,
            &["grid", "$footer", "0", "$scrolling", "$strip", "0"],
        ))
        .unwrap();
    assert_eq!(total.semantics.role, Role::Cell);
    assert_eq!(total.semantics.label, "Total");
    let at = total.collection.unwrap();
    assert_eq!((at.index, at.column), (ROWS, Some(0)));
    let mean = driven
        .context
        .node(node(
            &driven,
            &["grid", "$footer", "1", "$scrolling", "$strip", "1"],
        ))
        .unwrap();
    assert_eq!(mean.collection.unwrap().index, ROWS + 1);

    // A press on one selects nothing, and the body keeps its selection.
    let at = total.bounds;
    click(
        &mut driven.context,
        Point {
            x: at.x + at.width / 2.0,
            y: at.y + at.height / 2.0,
        },
    );
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().grid.rows.selected(), None);
    driven.state_mut().grid.rows.select(Some(ItemId(3)));
    driven.frame();
    let at = bounds(
        &driven,
        &["grid", "$footer", "1", "$scrolling", "$strip", "0"],
    );
    click(
        &mut driven.context,
        Point {
            x: at.x + 10.0,
            y: at.y + 10.0,
        },
    );
    driven.frame();
    driven.frame();
    assert_eq!(driven.state().grid.rows.selected(), Some(ItemId(3)));
}
