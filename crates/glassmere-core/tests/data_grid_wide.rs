mod support;

use glassmere_core::app::Action as Message;
use glassmere_core::{
    Action, Direction, Event, GridColumn, GridModel, GridMsg, GridState, ItemId, Key, Length,
    ListModel, NodeId, SortDirection, Style, Ui, VirtualListOptions,
};
use glassmere_scene::{Point, Rect, Size};
use std::time::Duration;
use support::{Driven, act};

const ROOT: Size = Size::new(900.0, 500.0);
const COLUMNS: usize = 30;

/// Thirty 100 px columns of text.
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

/// What the application owns: the model and the grid's state.
struct Grid {
    model: Wide,
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
        let options = VirtualListOptions {
            style: Style::default().width(Length::Px(450.0)),
            ..VirtualListOptions::default()
        };
        ui.data_grid(&state.model, &state.grid, options, grid_msg)
    });
}

struct App {
    driven: Driven<Grid>,
}

impl App {
    fn new(direction: Direction) -> Self {
        let model = Wide(
            (0..COLUMNS)
                .map(|column| GridColumn::new(format!("C{column}"), 100.0).filterable(column == 1))
                .collect(),
        );
        let mut driven = Driven::new(
            ROOT,
            Grid {
                model,
                grid: GridState::default(),
            },
            grid_view,
        );
        driven.context.set_direction(direction);
        // The first declaration learned the width, the second uses it.
        driven.state_mut();
        driven.frame();
        driven.state_mut();
        driven.frame();
        Self { driven }
    }

    fn frame(&mut self) {
        self.driven.frame();
    }

    fn rows(&self) -> NodeId {
        self.driven.context.find(&["grid", "$rows"]).unwrap()
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
            self.driven
                .context
                .find(&path)
                .map(|id| self.driven.context.node(id).unwrap().bounds)
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
        .driven
        .context
        .find(&["grid", "$rows", "item-0", "$cells", "$scrolling", "$strip"])
        .unwrap();
    assert_eq!(app.driven.context.node(cells).unwrap().children.len(), 5);
    // The engine does not scroll the rows itself; the grid is told instead.
    let native = app.driven.context.node(app.rows()).unwrap().scroll.unwrap();
    assert_eq!(native.maximum_x, 0.0);

    let rows = app.driven.context.node(app.rows()).unwrap().bounds;
    app.driven.context.event(Event::PointerMoved(Point {
        x: rows.x + 50.0,
        y: rows.y + 50.0,
    }));
    assert!(app.driven.context.event(Event::Scroll {
        delta_x: 250.0,
        delta_y: 0.0
    }));
    app.frame();
    // Columns 2 to 6 now, the first of them half scrolled out, identically in all three lines.
    assert!(app.column(1, false)[2].is_none() && app.column(7, false)[2].is_none());
    assert_eq!(app.xs(2, false), [rows.x - 50.0; 3]);
    assert_eq!(app.xs(6, false), [rows.x + 350.0; 3]);
    // What sticks out is clipped at the edge of the grid.
    let shown = app
        .driven
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
    assert_eq!(app.driven.context.node(shown).unwrap().clip.x, rows.x);

    // The end is the end; assistive technology scrolls the same way.
    let rows_id = app.rows();
    app.driven.context.action(rows_id, Action::ScrollByX(1.0e6));
    app.frame();
    let last = app.column(29, false)[2].unwrap();
    assert_eq!(last.x + last.width, rows.x + 450.0);
    app.driven
        .context
        .action(rows_id, Action::ScrollByX(-1.0e6));
    app.frame();
    assert_eq!(app.xs(0, false), [rows.x; 3]);
}

#[test]
fn frozen_columns_stay_put_and_the_cursor_brings_columns_into_view() {
    let mut app = App::new(Direction::Ltr);
    app.driven.state_mut().grid.columns.frozen = 2;
    app.frame();
    let rows_id = app.rows();
    app.driven.context.action(rows_id, Action::ScrollByX(320.0));
    app.frame();
    let rows = app.driven.context.node(app.rows()).unwrap().bounds;
    // The frozen pair sits at the start in every line, whatever the offset.
    assert_eq!(app.xs(0, true), [rows.x; 3]);
    assert_eq!(app.xs(1, true), [rows.x + 100.0; 3]);
    // The window onto the rest starts after them: 250 px, from column 5 on.
    assert!(app.column(4, false)[2].is_none());
    assert_eq!(app.xs(5, false), [rows.x + 200.0 - 20.0; 3]);
    let window = app
        .driven
        .context
        .find(&["grid", "$rows", "item-0", "$cells", "$scrolling"])
        .unwrap();
    let window = app.driven.context.node(window).unwrap().bounds;
    assert_eq!((window.x, window.width), (rows.x + 200.0, 250.0));

    // The scrollbar lies under the scrolling part only.
    let thumb = app
        .driven
        .context
        .find(&["grid", "$hbar", "$track", "$thumb"])
        .unwrap();
    let thumb = app.driven.context.node(thumb).unwrap().bounds;
    assert!(thumb.x >= rows.x + 200.0 && thumb.x + thumb.width <= rows.x + 450.0);

    // Moving the cell cursor scrolls just enough to show the whole column.
    app.driven.state_mut().grid.rows.select(Some(ItemId(0)));
    let rows_id = app.rows();
    app.driven.context.action(rows_id, Action::Focus);
    app.frame();
    for _ in 0..9 {
        app.driven.context.event(Event::KeyDown {
            key: Key::ArrowRight,
            shift: false,
            repeat: false,
        });
        app.frame();
    }
    let cell = app.column(9, false)[2].unwrap();
    assert!(cell.x >= rows.x + 200.0 && cell.x + cell.width <= rows.x + 450.0 + 0.01);
    // Back to a frozen column: every column passed on the way came into view.
    for _ in 0..9 {
        app.driven.context.event(Event::KeyDown {
            key: Key::ArrowLeft,
            shift: false,
            repeat: false,
        });
        app.frame();
    }
    let first_unfrozen = app.column(2, false)[2].unwrap();
    assert_eq!(first_unfrozen.x, rows.x + 200.0);
}

#[test]
fn the_header_menu_freezes_and_unfreezes() {
    let mut app = App::new(Direction::Ltr);
    let cell = app.column(1, false)[0].unwrap();
    // The sort button is disabled here, so open the menu where the header is.
    app.driven.context.event(Event::ContextMenu(Point {
        x: cell.x + 10.0,
        y: cell.y + 10.0,
    }));
    app.frame();
    let popup = app
        .driven
        .context
        .find(&["grid", "$header", "$scrolling", "$strip", "1", "$popup"])
        .unwrap();
    let entry = app
        .driven
        .context
        .node(popup)
        .unwrap()
        .children
        .iter()
        .copied()
        .find(|entry| app.driven.context.node(*entry).unwrap().semantics.label == "Freeze up to C1")
        .unwrap();
    app.driven.context.action(entry, Action::Activate);
    app.frame();
    app.frame();
    assert_eq!(app.driven.state().grid.columns.frozen, 2);
    assert!(
        app.column(1, true)[0].is_some(),
        "now among the frozen cells"
    );
}

#[test]
fn a_right_to_left_grid_scrolls_from_the_right() {
    let mut app = App::new(Direction::Rtl);
    app.driven.state_mut().grid.columns.frozen = 1;
    app.frame();
    let rows = app.driven.context.node(app.rows()).unwrap().bounds;
    let right = rows.x + rows.width;
    // The frozen column is at the right edge, the first scrolling one beside it.
    let frozen = app.column(0, true)[2].unwrap();
    assert_eq!(frozen.x + frozen.width, right);
    let first = app.column(1, false)[2].unwrap();
    assert_eq!(first.x + first.width, right - 100.0);
    // Scrolling physically left moves toward the later columns.
    let rows_id = app.rows();
    app.driven
        .context
        .action(rows_id, Action::ScrollByX(-150.0));
    app.frame();
    let second = app.column(2, false)[2].unwrap();
    assert_eq!(second.x + second.width, right - 100.0 + 50.0);
}

#[test]
fn the_grid_scrollbar_drags_and_pages_its_columns() {
    let mut app = App::new(Direction::Ltr);
    let rows = app.driven.context.node(app.rows()).unwrap().bounds;
    let thumb = app
        .driven
        .context
        .find(&["grid", "$hbar", "$track", "$thumb"])
        .unwrap();
    let thumb = app.driven.context.node(thumb).unwrap().bounds;
    // 450 px of 3000 show: the thumb is 67.5 px, and each pixel it moves is
    // 2550 / 382.5 of the content.
    let middle = Point {
        x: thumb.x + thumb.width / 2.0,
        y: thumb.y + thumb.height / 2.0,
    };
    app.driven.context.event(Event::PointerMoved(middle));
    app.driven.context.event(Event::PointerDown);
    app.driven.context.event(Event::PointerMoved(Point {
        x: middle.x + 30.0,
        ..middle
    }));
    app.driven.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.xs(2, false), [rows.x; 3]);

    // Past the thumb, the track goes on by a view.
    app.driven.context.event(Event::PointerMoved(Point {
        x: rows.x + 440.0,
        ..middle
    }));
    app.driven.context.event(Event::PointerDown);
    app.driven.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.xs(6, false), [rows.x - 50.0; 3]);
}

fn thumb_x(app: &App) -> f32 {
    let thumb = app
        .driven
        .context
        .find(&["grid", "$hbar", "$track", "$thumb"])
        .unwrap();
    app.driven.context.node(thumb).unwrap().bounds.x
}

fn at(app: &mut App, milliseconds: u64) {
    app.driven
        .context
        .set_now(Duration::from_millis(milliseconds));
    app.frame();
}

#[test]
fn a_held_press_on_the_grid_scrollbar_track_keeps_paging_until_the_thumb_reaches_it() {
    let mut app = App::new(Direction::Ltr);
    let rows = app.driven.context.node(app.rows()).unwrap().bounds;
    let thumb = app
        .driven
        .context
        .find(&["grid", "$hbar", "$track", "$thumb"])
        .unwrap();
    let thumb = app.driven.context.node(thumb).unwrap().bounds;
    let pointer = Point {
        x: rows.x + 300.0,
        y: thumb.y + thumb.height / 2.0,
    };
    at(&mut app, 1000);
    app.driven.context.event(Event::PointerMoved(pointer));
    app.driven.context.event(Event::PointerDown);
    app.frame();
    let paged = thumb_x(&app);
    assert!(paged > thumb.x, "the press pages once");
    assert_eq!(
        app.driven.context.next_wake(),
        Some(Duration::from_millis(1400))
    );

    at(&mut app, 1399);
    assert_eq!(thumb_x(&app), paged, "and waits out the delay");
    at(&mut app, 1400);
    let repeated = thumb_x(&app);
    assert!(repeated > paged, "then pages again");
    assert_eq!(
        app.driven.context.next_wake(),
        Some(Duration::from_millis(1450))
    );

    // The thumb catches up with the pointer and the repeat ends.
    for step in 1..=20 {
        at(&mut app, 1400 + step * 50);
    }
    let stopped = thumb_x(&app);
    assert!(stopped + 1.0 > pointer.x - 70.0 && stopped <= pointer.x);
    assert_eq!(app.driven.context.next_wake(), None);
    at(&mut app, 5000);
    assert_eq!(thumb_x(&app), stopped);
}

#[test]
fn a_grid_scrollbar_track_press_stops_repeating_on_release_or_when_the_pointer_leaves() {
    for release in [true, false] {
        let mut app = App::new(Direction::Ltr);
        let rows = app.driven.context.node(app.rows()).unwrap().bounds;
        let before = thumb_x(&app);
        let pointer = Point {
            x: rows.x + 440.0,
            y: rows.y + rows.height + 5.0,
        };
        at(&mut app, 1000);
        app.driven.context.event(Event::PointerMoved(pointer));
        app.driven.context.event(Event::PointerDown);
        app.frame();
        let paged = thumb_x(&app);
        assert!(paged > before, "the press pages once");
        if release {
            app.driven.context.event(Event::PointerUp);
        } else {
            app.driven.context.event(Event::PointerMoved(Point {
                y: pointer.y - 200.0,
                ..pointer
            }));
        }
        assert_eq!(app.driven.context.next_wake(), None);
        at(&mut app, 2000);
        assert_eq!(thumb_x(&app), paged);
    }
}
