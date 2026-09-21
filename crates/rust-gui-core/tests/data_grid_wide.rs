use rust_gui_core::{
    Action, Context, Direction, Event, GridColumn, GridModel, GridState, ItemId, Key, Length,
    ListModel, MonospaceMeasurer, NodeId, SortDirection, Style, VirtualListOptions,
};
use rust_gui_scene::{Point, Rect, Size};

const ROOT: Size = Size::new(900.0, 500.0);
const COLUMNS: usize = 30;

/// Thirty 100 px columns of generated text.
struct Wide(Vec<GridColumn>);

impl ListModel for Wide {
    fn len(&self) -> usize {
        40
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        Some(id.0 as usize).filter(|index| *index < 40)
    }
    fn label(&self, index: usize) -> String {
        format!("row {index}")
    }
}

impl GridModel for Wide {
    fn columns(&self) -> &[GridColumn] {
        &self.0
    }
    fn cell(&self, row: usize, column: usize) -> String {
        format!("r{row}c{column}")
    }
    fn sort(&mut self, _: usize, _: SortDirection) -> bool {
        false
    }
}

struct App {
    context: Context,
    model: Wide,
    state: GridState,
}

impl App {
    fn new(direction: Direction) -> Self {
        let mut context = Context::new();
        context.set_direction(direction);
        let mut app = Self {
            context,
            model: Wide(
                (0..COLUMNS)
                    .map(|column| {
                        GridColumn::new(format!("C{column}"), 100.0).filterable(column == 1)
                    })
                    .collect(),
            ),
            state: GridState::default(),
        };
        // The first declaration learns the width, the second uses it.
        app.frame();
        app.frame();
        app
    }

    fn frame(&mut self) {
        let (model, state) = (&mut self.model, &mut self.state);
        self.context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            let options = VirtualListOptions {
                style: Style::default().width(Length::Px(450.0)),
                ..VirtualListOptions::default()
            };
            ui.data_grid("grid", model, state, options);
        });
    }

    fn rows(&self) -> NodeId {
        self.context.find(&["grid", "$rows"]).unwrap()
    }

    /// Where a column's cell is in the header, the filter row and the first
    /// body row; `None` where it is not declared.
    fn column(&self, column: usize, frozen: bool) -> [Option<Rect>; 3] {
        let column = column.to_string();
        let lines: [&[&str]; 3] = [
            &["grid", "$header"],
            &["grid", "$filters"],
            &["grid", "$rows", "item-0", "$cells"],
        ];
        lines.map(|line| {
            let mut path = line.to_vec();
            if !frozen {
                path.extend(["$scrolling", "$strip"]);
            }
            path.push(&column);
            self.context
                .find(&path)
                .map(|id| self.context.node(id).unwrap().bounds)
        })
    }

    fn xs(&self, column: usize, frozen: bool) -> Vec<f32> {
        self.column(column, frozen)
            .iter()
            .map(|bounds| bounds.expect("declared").x)
            .collect()
    }
}

#[test]
fn only_the_columns_in_view_exist_and_header_filters_and_rows_scroll_together() {
    let mut app = App::new(Direction::Ltr);
    // 450 px of view over thirty 100 px columns: five exist, the fifth half shown.
    assert!(app.column(4, false)[2].is_some());
    assert!(app.column(5, false)[2].is_none());
    let cells = app
        .context
        .find(&["grid", "$rows", "item-0", "$cells", "$scrolling", "$strip"])
        .unwrap();
    assert_eq!(app.context.node(cells).unwrap().children.len(), 5);
    // The engine does not scroll the rows itself; the grid is told instead.
    let native = app.context.node(app.rows()).unwrap().scroll.unwrap();
    assert_eq!(native.maximum_x, 0.0);

    let rows = app.context.node(app.rows()).unwrap().bounds;
    app.context.event(Event::PointerMoved(Point {
        x: rows.x + 50.0,
        y: rows.y + 50.0,
    }));
    assert!(app.context.event(Event::Scroll {
        delta_x: 250.0,
        delta_y: 0.0
    }));
    app.frame();
    assert_eq!(app.state.scroll_x, 250.0);
    // Columns 2 to 6 now, the first of them half scrolled out, identically in all three lines.
    assert!(app.column(1, false)[2].is_none() && app.column(7, false)[2].is_none());
    assert_eq!(app.xs(2, false), [rows.x - 50.0; 3]);
    assert_eq!(app.xs(6, false), [rows.x + 350.0; 3]);
    // What sticks out is clipped at the edge of the grid.
    let shown = app
        .context
        .find(&[
            "grid",
            "$rows",
            "item-0",
            "$cells",
            "$scrolling",
            "$strip",
            "2",
        ])
        .unwrap();
    assert_eq!(app.context.node(shown).unwrap().clip.x, rows.x);

    // The end is the end; assistive technology scrolls the same way.
    let rows_id = app.rows();
    app.context.action(rows_id, Action::ScrollByX(1.0e6));
    app.frame();
    assert_eq!(app.state.scroll_x, 3000.0 - 450.0);
    assert!(app.column(29, false)[2].is_some());
    app.context.action(rows_id, Action::ScrollByX(-1.0e6));
    app.frame();
    assert_eq!(app.state.scroll_x, 0.0);
}

#[test]
fn frozen_columns_stay_put_and_the_cursor_brings_columns_into_view() {
    let mut app = App::new(Direction::Ltr);
    app.state.columns.frozen = 2;
    app.state.scroll_x = 320.0;
    app.frame();
    let rows = app.context.node(app.rows()).unwrap().bounds;
    // The frozen pair sits at the start in every line, whatever the offset.
    assert_eq!(app.xs(0, true), [rows.x; 3]);
    assert_eq!(app.xs(1, true), [rows.x + 100.0; 3]);
    // The window onto the rest starts after them: 250 px, from column 5 on.
    assert!(app.column(4, false)[2].is_none());
    assert_eq!(app.xs(5, false), [rows.x + 200.0 - 20.0; 3]);
    let window = app
        .context
        .find(&["grid", "$rows", "item-0", "$cells", "$scrolling"])
        .unwrap();
    let window = app.context.node(window).unwrap().bounds;
    assert_eq!((window.x, window.width), (rows.x + 200.0, 250.0));

    // The scrollbar lies under the scrolling part only.
    let thumb = app
        .context
        .find(&["grid", "$hbar", "$track", "$thumb"])
        .unwrap();
    let thumb = app.context.node(thumb).unwrap().bounds;
    assert!(thumb.x >= rows.x + 200.0 && thumb.x + thumb.width <= rows.x + 450.0);

    // Moving the cell cursor scrolls just enough to show the whole column.
    app.state.rows.select(Some(ItemId(0)));
    let rows_id = app.rows();
    app.context.action(rows_id, Action::Focus);
    app.frame();
    for _ in 0..9 {
        app.context.event(Event::KeyDown {
            key: Key::ArrowRight,
            shift: false,
            repeat: false,
        });
        app.frame();
    }
    assert_eq!(app.state.cursor, 9);
    let cell = app.column(9, false)[2].unwrap();
    assert!(cell.x >= rows.x + 200.0 && cell.x + cell.width <= rows.x + 450.0 + 0.01);
    // Back to a frozen column: every column passed on the way came into view.
    for _ in 0..9 {
        app.context.event(Event::KeyDown {
            key: Key::ArrowLeft,
            shift: false,
            repeat: false,
        });
        app.frame();
    }
    assert_eq!(app.state.cursor, 0);
    let first_unfrozen = app.column(2, false)[2].unwrap();
    assert_eq!(first_unfrozen.x, rows.x + 200.0);
}

#[test]
fn the_header_menu_freezes_and_unfreezes() {
    let mut app = App::new(Direction::Ltr);
    let cell = app.column(1, false)[0].unwrap();
    // The sort button is disabled here, so open the menu where the header is.
    app.context.event(Event::ContextMenu(Point {
        x: cell.x + 10.0,
        y: cell.y + 10.0,
    }));
    app.frame();
    let popup = app
        .context
        .find(&["grid", "$header", "$scrolling", "$strip", "1", "$popup"])
        .unwrap();
    let entry = app
        .context
        .node(popup)
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|entry| app.context.node(*entry).unwrap().semantics.label == "Freeze up to C1")
        .unwrap();
    app.context.action(entry, Action::Activate);
    app.frame();
    app.frame();
    assert_eq!(app.state.columns.frozen, 2);
    assert!(
        app.column(1, true)[0].is_some(),
        "now among the frozen cells"
    );
}

#[test]
fn a_right_to_left_grid_scrolls_from_the_right() {
    let mut app = App::new(Direction::Rtl);
    app.state.columns.frozen = 1;
    app.frame();
    let rows = app.context.node(app.rows()).unwrap().bounds;
    let right = rows.x + rows.width;
    // The frozen column is at the right edge, the first scrolling one beside it.
    let frozen = app.column(0, true)[2].unwrap();
    assert_eq!(frozen.x + frozen.width, right);
    let first = app.column(1, false)[2].unwrap();
    assert_eq!(first.x + first.width, right - 100.0);
    // Scrolling physically left moves toward the later columns.
    let rows_id = app.rows();
    app.context.action(rows_id, Action::ScrollByX(-150.0));
    app.frame();
    assert_eq!(app.state.scroll_x, 150.0);
    let second = app.column(2, false)[2].unwrap();
    assert_eq!(second.x + second.width, right - 100.0 + 50.0);
}
