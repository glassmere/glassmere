mod support;

use rust_gui_core::app::Action as Message;
use rust_gui_core::{
    Action, Event, GridColumn, GridColumns, GridModel, GridMsg, GridState, ItemId, Key, ListModel,
    NodeId, Role, SortDirection, Ui, VirtualListOptions,
};
use rust_gui_scene::{Point, Size};
use support::{Driven, act};

const ROOT: Size = Size::new(900.0, 500.0);

struct People {
    columns: Vec<GridColumn>,
    rows: Vec<[String; 4]>,
}

impl People {
    fn new() -> Self {
        let column = |label: &str, width| GridColumn {
            label: label.into(),
            width,
            sortable: true,
            editor: None,
            filterable: false,
        };
        Self {
            columns: vec![
                column("Name", 160.0),
                column("City", 120.0),
                column("Age", 80.0),
                column("Notes", 200.0),
            ],
            rows: (0..50)
                .map(|row| {
                    [
                        format!("Person {row}"),
                        format!("City {}", row % 7),
                        format!("{}", 20 + row % 40),
                        format!("note {row}"),
                    ]
                })
                .collect(),
        }
    }
}

impl ListModel for People {
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        Some(id.0 as usize).filter(|index| *index < self.rows.len())
    }
    fn label(&self, index: usize) -> String {
        self.rows[index][0].clone()
    }
}

impl GridModel for People {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        self.rows[row][column].clone()
    }
    fn sort(&mut self, _: usize, _: SortDirection) -> bool {
        false
    }
}

/// What the application owns: the model and the grid's state, which
/// `GridMsg`s change in `update`.
struct Grid {
    model: People,
    grid: GridState,
}

fn grid_msg(message: GridMsg) -> Message<Grid> {
    act(move |s: &mut Grid| {
        s.grid.apply(&message);
        if let GridMsg::Sort(column, direction) = &message {
            s.model.sort(*column, *direction);
        }
    })
}

fn grid_view(state: &Grid, ui: &mut Ui<'_, Message<Grid>>) {
    ui.keyed("grid", |ui| {
        ui.data_grid(
            &state.model,
            &state.grid,
            VirtualListOptions::default(),
            grid_msg,
        )
    });
    ui.button("after");
}

struct App {
    driven: Driven<Grid>,
}

impl App {
    fn new() -> Self {
        Self {
            driven: Driven::new(
                ROOT,
                Grid {
                    model: People::new(),
                    grid: GridState::default(),
                },
                grid_view,
            ),
        }
    }

    fn frame(&mut self) {
        self.driven.frame();
    }

    fn header(&self, column: usize, part: &[&str]) -> NodeId {
        let column = column.to_string();
        let mut path = vec![
            "grid",
            "$header",
            "$scrolling",
            "$strip",
            column.as_str(),
            "$cell",
        ];
        path.extend(part);
        self.driven.context.find(&path).unwrap()
    }

    fn sort_button(&self, column: usize) -> NodeId {
        self.header(column, &["$drop", "$drag", "$sort"])
    }

    /// Labels of the header cells, left to right.
    fn headers(&self) -> Vec<String> {
        let mut cells: Vec<(f32, String)> = (0..self.driven.state().model.columns.len())
            .filter_map(|column| {
                let column = column.to_string();
                let path = [
                    "grid",
                    "$header",
                    "$scrolling",
                    "$strip",
                    column.as_str(),
                    "$cell",
                    "$drop",
                    "$drag",
                    "$sort",
                ];
                let button = self.driven.context.node(self.driven.context.find(&path)?)?;
                Some((button.bounds.x, button.semantics.label.clone()))
            })
            .collect();
        cells.sort_by(|a, b| a.0.total_cmp(&b.0));
        cells.into_iter().map(|(_, label)| label).collect()
    }

    fn centre(&self, id: NodeId) -> Point {
        let bounds = self.driven.context.node(id).unwrap().bounds;
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    }

    fn drag(&mut self, from: Point, to: Point) {
        self.driven.context.event(Event::PointerMoved(from));
        self.driven.context.event(Event::PointerDown);
        // In steps, as a hand would: drags start after a threshold.
        for step in 1..=4 {
            let t = step as f32 / 4.0;
            self.driven.context.event(Event::PointerMoved(Point {
                x: from.x + (to.x - from.x) * t,
                y: from.y + (to.y - from.y) * t,
            }));
            self.frame();
        }
        self.driven.context.event(Event::PointerUp);
        self.frame();
        self.frame();
    }

    /// Opens a header's menu from the keyboard and activates the entry with `label`.
    fn menu(&mut self, column: usize, label: &str) {
        let button = self.sort_button(column);
        self.driven.context.action(button, Action::Focus);
        self.driven.context.event(Event::KeyDown {
            key: Key::ContextMenu,
            shift: false,
            repeat: false,
        });
        self.frame();
        let popup = self
            .driven
            .context
            .find(&[
                "grid",
                "$header",
                "$scrolling",
                "$strip",
                &column.to_string(),
                "$popup",
            ])
            .unwrap();
        let entry = self
            .driven
            .context
            .node(popup)
            .unwrap()
            .children
            .iter()
            .copied()
            .find(|entry| self.driven.context.node(*entry).unwrap().semantics.label == label)
            .unwrap_or_else(|| panic!("no menu entry `{label}`"));
        assert!(
            self.driven.context.action(entry, Action::Activate),
            "{label}"
        );
        self.frame();
        self.frame();
    }
}

#[test]
fn column_state_arranges_moves_hides_and_survives_a_changed_model() {
    let mut columns = GridColumns::default();
    assert_eq!(columns.visible(4), [0, 1, 2, 3]);
    columns.move_before(4, 3, Some(0));
    assert_eq!(columns.visible(4), [3, 0, 1, 2]);
    columns.move_before(4, 3, None);
    assert_eq!(columns.visible(4), [0, 1, 2, 3]);
    assert!(columns.shift(4, 1, 1));
    assert_eq!(columns.visible(4), [0, 2, 1, 3]);
    assert!(!columns.shift(4, 0, -1), "already first");
    assert!(!columns.shift(4, 3, 1), "already last");

    // Shifting steps over hidden columns; the last visible one cannot be hidden.
    assert!(columns.hide(4, 2));
    assert_eq!(columns.visible(4), [0, 1, 3]);
    assert!(columns.shift(4, 0, 1));
    assert_eq!(columns.visible(4), [1, 0, 3]);
    assert!(columns.hide(4, 1) && columns.hide(4, 0));
    assert!(!columns.hide(4, 3));
    assert_eq!(columns.visible(4), [3]);
    columns.show_all();
    assert_eq!(columns.visible(4).len(), 4);

    // A model that lost or gained columns still gets a sane arrangement.
    columns.order = vec![9, 2, 2, 0];
    assert_eq!(columns.arranged(3), [2, 0, 1]);
    assert_eq!(columns.arranged(5), [2, 0, 1, 3, 4]);

    columns.set_width(1, 5.0);
    columns.set_width(2, f32::NAN);
    assert_eq!(columns.width(1, 100.0), GridColumns::MIN_WIDTH);
    assert_eq!(columns.width(2, 100.0), 100.0);
    columns.reset();
    assert_eq!(columns, GridColumns::default());
}

#[test]
fn dragging_a_header_edge_resizes_its_column_and_the_cells_follow() {
    let mut app = App::new();
    let sizer = app.header(0, &["$size"]);
    let node = app.driven.context.node(sizer).unwrap();
    assert_eq!(node.semantics.role, Role::Splitter);
    assert_eq!(node.semantics.label, "Resize Name");
    assert!(!node.semantics.focusable, "the menu is the keyboard path");
    assert_eq!(node.numeric.unwrap().value, 160.0);

    let grab = app.centre(sizer);
    app.drag(
        grab,
        Point {
            x: grab.x + 60.0,
            ..grab
        },
    );
    assert_eq!(app.driven.state().grid.columns.width(0, 160.0), 220.0);
    let cell = app
        .driven
        .context
        .find(&[
            "grid",
            "$rows",
            "item-0",
            "$cells",
            "$scrolling",
            "$strip",
            "0",
        ])
        .unwrap();
    assert_eq!(app.driven.context.node(cell).unwrap().bounds.width, 220.0);
    let city = app
        .driven
        .context
        .find(&[
            "grid",
            "$rows",
            "item-0",
            "$cells",
            "$scrolling",
            "$strip",
            "1",
        ])
        .unwrap();
    assert_eq!(app.driven.context.node(city).unwrap().bounds.x, 220.0);

    // Far past the minimum stops at the minimum; assistive technology can set it too.
    let grab = app.centre(app.header(0, &["$size"]));
    app.drag(grab, Point { x: -500.0, ..grab });
    assert_eq!(
        app.driven.state().grid.columns.width(0, 160.0),
        GridColumns::MIN_WIDTH
    );
    let sizer = app.header(0, &["$size"]);
    assert!(app.driven.context.action(sizer, Action::SetNumber(300.0)));
    app.frame();
    assert_eq!(app.driven.state().grid.columns.width(0, 160.0), 300.0);
    // The other columns kept the model's widths.
    assert_eq!(app.driven.state().grid.columns.widths.len(), 1);
}

#[test]
fn dragging_a_header_onto_another_moves_the_column_and_a_click_still_sorts() {
    let mut app = App::new();
    assert_eq!(app.headers(), ["Name", "City", "Age", "Notes"]);
    // Toward the end: Name lands after Age.
    let from = app.centre(app.sort_button(0));
    let to = app.centre(app.sort_button(2));
    app.drag(from, to);
    assert_eq!(app.headers(), ["City", "Age", "Name", "Notes"]);
    assert_eq!(app.driven.state().grid.columns.order, [1, 2, 0, 3]);
    // Toward the start: Notes lands before City. Cells follow the headers.
    let from = app.centre(app.sort_button(3));
    let to = app.centre(app.sort_button(1));
    app.drag(from, to);
    assert_eq!(app.headers(), ["Notes", "City", "Age", "Name"]);
    let first = app
        .driven
        .context
        .find(&[
            "grid",
            "$rows",
            "item-0",
            "$cells",
            "$scrolling",
            "$strip",
            "3",
        ])
        .unwrap();
    assert_eq!(app.driven.context.node(first).unwrap().bounds.x, 0.0);
    assert_eq!(
        app.driven.context.node(first).unwrap().semantics.label,
        "note 0"
    );

    // A press and release without movement is still a click on the sort button.
    let button = app.sort_button(1);
    let at = app.centre(button);
    app.driven.context.event(Event::PointerMoved(at));
    app.driven.context.event(Event::PointerDown);
    app.driven.context.event(Event::PointerUp);
    assert!(app.driven.context.node(button).is_some());
    app.frame();
    assert_eq!(
        app.driven.state().grid.columns.order,
        [3, 1, 2, 0],
        "no move happened"
    );
}

#[test]
fn every_column_operation_is_in_the_header_menu_for_the_keyboard() {
    let mut app = App::new();
    app.menu(0, "Move right");
    assert_eq!(app.headers(), ["City", "Name", "Age", "Notes"]);
    app.menu(0, "Move left");
    assert_eq!(app.headers(), ["Name", "City", "Age", "Notes"]);
    app.menu(1, "Wider");
    app.menu(1, "Wider");
    app.menu(1, "Narrower");
    assert_eq!(app.driven.state().grid.columns.width(1, 120.0), 140.0);
    app.menu(2, "Hide Age");
    assert_eq!(app.headers(), ["Name", "City", "Notes"]);
    let grid = app
        .driven
        .context
        .node(app.driven.context.find(&["grid"]).unwrap())
        .unwrap();
    assert_eq!(grid.collection.unwrap().columns, Some(3));
    app.menu(3, "Show all columns");
    assert_eq!(app.headers().len(), 4);
    app.menu(3, "Reset columns");
    assert_eq!(app.driven.state().grid.columns, GridColumns::default());

    // Entries that cannot apply are disabled rather than missing.
    let button = app.sort_button(0);
    app.driven.context.action(button, Action::Focus);
    app.driven.context.event(Event::KeyDown {
        key: Key::ContextMenu,
        shift: false,
        repeat: false,
    });
    app.frame();
    let popup = app
        .driven
        .context
        .find(&["grid", "$header", "$scrolling", "$strip", "0", "$popup"])
        .unwrap();
    let disabled: Vec<String> = app
        .driven
        .context
        .node(popup)
        .unwrap()
        .children
        .iter()
        .map(|entry| app.driven.context.node(*entry).unwrap().semantics.clone())
        .filter(|semantics| semantics.disabled)
        .map(|semantics| semantics.label)
        .collect();
    assert_eq!(
        disabled,
        ["Move left", "Show all columns", "Unfreeze columns"]
    );
}

#[test]
fn the_header_stays_one_tab_stop_per_column() {
    let mut app = App::new();
    let mut stops = Vec::new();
    for _ in 0..6 {
        app.driven.context.event(Event::KeyDown {
            key: Key::Tab,
            shift: false,
            repeat: false,
        });
        app.frame();
        let focused = app.driven.context.focused().unwrap();
        stops.push(
            app.driven
                .context
                .node(focused)
                .unwrap()
                .semantics
                .label
                .clone(),
        );
    }
    assert_eq!(&stops[..4], ["Name", "City", "Age", "Notes"]);
    assert_ne!(stops[4], stops[3], "then the rows, not more header parts");
}
