mod support;

use rust_gui_core::app::Action as Message;
use rust_gui_core::{
    Action, CellEditor, CellValue, Event, GridColumn, GridModel, GridMsg, GridState, ItemId, Key,
    ListModel, NodeId, NumberOptions, SortDirection, TextEdit, Ui, VirtualListOptions,
};
use rust_gui_scene::{Point, Size};
use support::{Driven, act};

const ROOT: Size = Size::new(900.0, 500.0);
const CITIES: [&str; 3] = ["Brno", "Prague", "Vienna"];

struct Person {
    name: String,
    city: usize,
    age: f64,
    active: bool,
}

struct People {
    columns: Vec<GridColumn>,
    rows: Vec<Person>,
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
        self.rows[index].name.clone()
    }
}

impl GridModel for People {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        let person = &self.rows[row];
        match column {
            0 => person.name.clone(),
            1 => CITIES[person.city].into(),
            2 => format!("{} y", person.age),
            3 => if person.active { "yes" } else { "no" }.into(),
            _ => format!("#{row}"),
        }
    }
    fn sort(&mut self, _: usize, _: SortDirection) -> bool {
        false
    }
    fn value(&self, row: usize, column: usize) -> CellValue {
        let person = &self.rows[row];
        match column {
            1 => CellValue::Choice(person.city),
            2 => CellValue::Number(person.age),
            3 => CellValue::Bool(person.active),
            _ => CellValue::Text(self.cell(row, column)),
        }
    }
    fn set_cell(&mut self, row: usize, column: usize, value: CellValue) -> Result<(), String> {
        let person = &mut self.rows[row];
        match (column, value) {
            (0, CellValue::Text(name)) if name.trim().is_empty() => {
                return Err("A name is required.".into());
            }
            (0, CellValue::Text(name)) => person.name = name,
            (1, CellValue::Choice(city)) => person.city = city,
            (2, CellValue::Number(age)) => person.age = age,
            (3, CellValue::Bool(active)) => person.active = active,
            (column, value) => return Err(format!("column {column} cannot take {value:?}")),
        }
        Ok(())
    }
}

/// What the application owns: the model, the grid's state, and every value
/// the model stored, in order.
struct Table {
    model: People,
    grid: GridState,
    edited: Vec<(ItemId, usize)>,
}

fn grid_msg(message: GridMsg) -> Message<Table> {
    act(move |s: &mut Table| {
        s.grid.apply(&message);
        if let GridMsg::Edit { row, column, value } = &message
            && let Some(index) = s.model.index_of(*row)
            && s.model.set_cell(index, *column, value.clone()).is_ok()
        {
            s.edited.push((*row, *column));
        }
    })
}

fn table_view(state: &Table, ui: &mut Ui<'_, Message<Table>>) {
    ui.keyed("grid", |ui| {
        ui.data_grid_msg(
            &state.model,
            &state.grid,
            VirtualListOptions::default(),
            grid_msg,
        )
    });
    ui.button("after");
}

struct App {
    driven: Driven<Table>,
}

impl App {
    fn new() -> Self {
        let age = NumberOptions::default().range(0.0, 150.0).suffix(" y");
        let mut driven = Driven::new(
            ROOT,
            Table {
                model: People {
                    columns: vec![
                        GridColumn::new("Name", 160.0).editor(CellEditor::Text),
                        GridColumn::new("City", 120.0)
                            .editor(CellEditor::Choice(CITIES.map(String::from).to_vec())),
                        GridColumn::new("Age", 100.0).editor(CellEditor::number(age)),
                        GridColumn::new("Active", 80.0).editor(CellEditor::Bool),
                        GridColumn::new("Id", 80.0),
                    ],
                    rows: (0..20)
                        .map(|row| Person {
                            name: format!("Person {row}"),
                            city: row % 3,
                            age: 30.0 + row as f64,
                            active: false,
                        })
                        .collect(),
                },
                grid: GridState::default(),
                edited: Vec::new(),
            },
            table_view,
        );
        let rows = driven.context.find(&["grid", "$rows"]).unwrap();
        driven.context.action(rows, Action::Focus);
        driven.state_mut().grid.rows.select(Some(ItemId(0)));
        driven.frame();
        Self { driven }
    }

    fn frame(&mut self) {
        self.driven.frame();
    }

    fn rows(&self) -> NodeId {
        self.driven.context.find(&["grid", "$rows"]).unwrap()
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, false);
    }

    /// A key, then the batches it takes to settle: a commit's `Edit` goes out
    /// in the first, the kernel sees the stored value in the second, and the
    /// follow-up move (the `Select` after Enter) comes after that.
    fn key_with(&mut self, key: Key, shift: bool) {
        self.driven.context.event(Event::KeyDown {
            key,
            shift,
            repeat: false,
        });
        self.frame();
        self.frame();
        self.frame();
    }

    fn editor(&self, row: usize) -> Option<NodeId> {
        let item = format!("item-{row}");
        self.driven.context.find(&[
            "grid",
            "$rows",
            &item,
            "$cells",
            "$scrolling",
            "$strip",
            "$edit",
        ])
    }

    /// The column label of the editor open on `row`: the column it edits.
    fn editing_label(&self, row: usize) -> Option<String> {
        let editor = self.editor(row)?;
        Some(
            self.driven
                .context
                .node(editor)
                .unwrap()
                .semantics
                .label
                .clone(),
        )
    }

    fn retype(&mut self, text: &str) {
        self.driven.context.event(Event::Text(TextEdit::SelectAll));
        self.driven
            .context
            .event(Event::Text(TextEdit::Insert(text.into())));
        self.frame();
    }

    fn cell_centre(&self, row: usize, column: usize) -> Point {
        let (item, column) = (format!("item-{row}"), column.to_string());
        let id = self
            .driven
            .context
            .find(&[
                "grid",
                "$rows",
                &item,
                "$cells",
                "$scrolling",
                "$strip",
                &column,
            ])
            .unwrap();
        let bounds = self.driven.context.node(id).unwrap().bounds;
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    }

    fn click(&mut self, at: Point) {
        self.driven.context.event(Event::PointerMoved(at));
        self.driven.context.event(Event::PointerDown);
        self.driven.context.event(Event::PointerUp);
        self.frame();
        self.frame();
    }

    fn model(&self) -> &People {
        &self.driven.state().model
    }

    fn selected(&self) -> Option<ItemId> {
        self.driven.state().grid.rows.selected()
    }
}

#[test]
fn f2_and_enter_edit_a_text_cell_and_enter_stores_it_and_moves_down() {
    let mut app = App::new();
    app.key(Key::F2);
    assert_eq!(app.editing_label(0).as_deref(), Some("Name"));
    let editor = app.editor(0).expect("the editor replaces the cell");
    assert_eq!(
        app.driven.context.focused(),
        Some(editor),
        "and takes the keyboard"
    );
    let node = app.driven.context.node(editor).unwrap();
    assert_eq!(node.semantics.label, "Name");
    assert_eq!(node.value.as_deref(), Some("Person 0"));

    app.retype("Ada");
    assert_eq!(
        app.model().rows[0].name,
        "Person 0",
        "a draft until committed"
    );
    app.key(Key::Enter);
    assert_eq!(app.model().rows[0].name, "Ada");
    assert_eq!(app.driven.state().edited, [(ItemId(0), 0)]);
    assert!(app.editor(0).is_none());
    assert_eq!(app.selected(), Some(ItemId(1)), "Enter moves down");
    assert_eq!(
        app.driven.context.focused(),
        Some(app.rows()),
        "the grid has the keys back"
    );

    // Enter on an editable cell edits.
    // M35: a row activation (Enter on a read-only cell) has no GridMsg, so
    // `activated` is not asserted any more.
    app.key(Key::Enter);
    assert_eq!(app.editing_label(1).as_deref(), Some("Name"));
    // Escape drops the draft.
    app.retype("Nobody");
    app.key(Key::Escape);
    assert!(app.editor(1).is_none());
    assert_eq!(app.model().rows[1].name, "Person 1");
    assert_eq!(app.driven.context.focused(), Some(app.rows()));
}

#[test]
fn arrows_move_the_cell_cursor_typing_replaces_and_tab_moves_sideways() {
    let mut app = App::new();
    app.key(Key::ArrowRight);
    app.key(Key::ArrowRight);
    app.key(Key::F2);
    assert_eq!(
        app.editing_label(0).as_deref(),
        Some("Age"),
        "the cursor is on Age"
    );
    app.key(Key::Escape);
    app.key(Key::ArrowLeft);
    app.key(Key::ArrowLeft);
    app.key(Key::ArrowLeft);

    // Typing on the grid opens the cell at the first column with what was typed.
    app.driven
        .context
        .event(Event::Text(TextEdit::Insert("Z".into())));
    app.frame();
    let editor = app.editor(0).unwrap();
    assert_eq!(
        app.driven.context.node(editor).unwrap().value.as_deref(),
        Some("Z")
    );
    // Tab stores it and moves one column on, staying in the grid.
    app.key(Key::Tab);
    assert_eq!(app.model().rows[0].name, "Z");
    assert!(app.editor(0).is_none());
    assert_eq!(app.driven.context.focused(), Some(app.rows()));
    assert_eq!(app.selected(), Some(ItemId(0)));
    app.key_with(Key::ArrowRight, false);
    app.key(Key::F2);
    app.key_with(Key::Tab, true);
    app.key(Key::F2);
    assert_eq!(
        app.editing_label(0).as_deref(),
        Some("City"),
        "Shift+Tab moves back"
    );
    app.key(Key::Escape);

    // A read-only column: F2 does nothing.
    app.key(Key::ArrowRight);
    app.key(Key::ArrowRight);
    app.key(Key::ArrowRight);
    app.key(Key::F2);
    assert!(app.editor(0).is_none());
}

#[test]
fn numbers_parse_clamp_and_refuse_what_is_not_a_number() {
    let mut app = App::new();
    app.key(Key::ArrowRight);
    app.key(Key::ArrowRight);
    app.key(Key::F2);
    let editor = app.editor(0).unwrap();
    // The draft is the bare number; the unit is shown, not typed.
    assert_eq!(
        app.driven.context.node(editor).unwrap().value.as_deref(),
        Some("30")
    );
    app.retype("abc");
    app.key(Key::Enter);
    assert!(app.editor(0).is_some(), "the editor stays open");
    let editor = app.editor(0).unwrap();
    assert_eq!(
        app.driven
            .context
            .node(editor)
            .unwrap()
            .description
            .as_deref(),
        Some("Enter a number.")
    );
    assert_eq!(app.selected(), Some(ItemId(0)), "and the cursor stays");
    app.retype("41,5");
    app.key(Key::Enter);
    assert_eq!(
        app.model().rows[0].age,
        42.0,
        "a decimal comma, snapped to whole years"
    );
    app.key(Key::ArrowUp);
    app.key(Key::F2);
    app.retype("999");
    app.key(Key::Enter);
    assert_eq!(app.model().rows[0].age, 150.0);
}

#[test]
fn bools_toggle_in_place_and_choices_store_the_moment_one_is_picked() {
    let mut app = App::new();
    for _ in 0..3 {
        app.key(Key::ArrowRight);
    }
    app.key(Key::Space);
    assert!(app.model().rows[0].active);
    assert!(app.editor(0).is_none(), "no editor opens for a bool");
    app.key(Key::Enter);
    assert!(!app.model().rows[0].active);
    assert_eq!(app.driven.state().edited, [(ItemId(0), 3), (ItemId(0), 3)]);

    app.key(Key::ArrowLeft);
    app.key(Key::ArrowLeft);
    app.key(Key::F2);
    let editor = app.editor(0).unwrap();
    assert_eq!(app.driven.context.focused(), Some(editor));
    // Open the list and take "Vienna".
    app.key(Key::ArrowDown);
    app.key(Key::ArrowDown);
    app.key(Key::ArrowDown);
    app.key(Key::Enter);
    app.frame();
    assert_eq!(CITIES[app.model().rows[0].city], "Vienna");
    assert!(app.editor(0).is_none());
    assert_eq!(app.driven.context.focused(), Some(app.rows()));
}

#[test]
fn a_second_click_on_the_current_cell_opens_it_and_clicking_away_stores_it() {
    let mut app = App::new();
    let name = app.cell_centre(2, 0);
    app.click(name);
    assert_eq!(app.selected(), Some(ItemId(2)));
    assert!(app.editor(2).is_none());
    app.click(name);
    assert_eq!(app.editing_label(2).as_deref(), Some("Name"));
    app.retype("Linus");
    // Clicking another row leaves the editor, which stores the draft.
    let other = app.cell_centre(5, 2);
    app.click(other);
    assert_eq!(app.model().rows[2].name, "Linus");
    assert!(app.editor(2).is_none());
    assert_eq!(app.selected(), Some(ItemId(5)));
    // The click landed in column 2, so F2 edits Age.
    app.key(Key::F2);
    assert_eq!(app.editing_label(5).as_deref(), Some("Age"));
}

mod filtering {
    use super::support::{Driven, act};
    use rust_gui_core::app::Action as Message;
    use rust_gui_core::{
        Action, Event, FILTER_DELAY, GridColumn, GridModel, GridMsg, GridState, ItemId, ListModel,
        SortDirection, TextEdit, Ui, VirtualListOptions, text_matches,
    };
    use rust_gui_scene::Size;
    use std::time::Duration;

    struct Fruit {
        columns: Vec<GridColumn>,
        all: Vec<(&'static str, &'static str)>,
        shown: Vec<usize>,
        calls: Vec<Vec<(usize, String)>>,
    }

    impl ListModel for Fruit {
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
            self.all[self.shown[index]].0.into()
        }
    }

    impl GridModel for Fruit {
        fn columns(&self) -> &[GridColumn] {
            &self.columns
        }
        fn cell(&self, row: usize, column: usize) -> String {
            let fruit = self.all[self.shown[row]];
            [fruit.0, fruit.1, "-"][column].into()
        }
        fn sort(&mut self, _: usize, _: SortDirection) -> bool {
            false
        }
        fn filter(&mut self, filters: &[(usize, &str)]) -> bool {
            self.calls.push(
                filters
                    .iter()
                    .map(|(column, text)| (*column, (*text).to_owned()))
                    .collect(),
            );
            self.shown = (0..self.all.len())
                .filter(|row| {
                    let fruit = self.all[*row];
                    filters
                        .iter()
                        .all(|(column, text)| text_matches([fruit.0, fruit.1][*column], text))
                })
                .collect();
            true
        }
    }

    struct Fruits {
        model: Fruit,
        grid: GridState,
    }

    fn fruit_msg(message: GridMsg) -> Message<Fruits> {
        act(move |s: &mut Fruits| {
            s.grid.apply(&message);
            if let GridMsg::FiltersApplied = &message {
                let filters: Vec<(usize, String)> = s.grid.applied_filters().to_vec();
                let pairs: Vec<(usize, &str)> = filters
                    .iter()
                    .map(|(column, text)| (*column, text.as_str()))
                    .collect();
                s.model.filter(&pairs);
            }
        })
    }

    fn fruit_view(state: &Fruits, ui: &mut Ui<'_, Message<Fruits>>) {
        ui.keyed("grid", |ui| {
            ui.data_grid_msg(
                &state.model,
                &state.grid,
                VirtualListOptions::default(),
                fruit_msg,
            )
        });
    }

    fn driven(at: u64) -> Driven<Fruits> {
        let model = Fruit {
            columns: vec![
                GridColumn::new("Name", 160.0).filterable(true),
                GridColumn::new("Colour", 160.0).filterable(true),
                GridColumn::new("Notes", 100.0),
            ],
            all: vec![
                ("Apple", "red"),
                ("Apricot", "orange"),
                ("Banana", "yellow"),
                ("Blood orange", "red"),
            ],
            shown: (0..4).collect(),
            calls: Vec::new(),
        };
        let mut driven = Driven::new(
            Size::new(700.0, 400.0),
            Fruits {
                model,
                grid: GridState::default(),
            },
            fruit_view,
        );
        driven.state_mut().grid.rows.select(Some(ItemId(2)));
        driven.context.set_now(Duration::from_millis(at));
        driven.frame();
        driven
    }

    #[test]
    fn the_model_filters_once_typing_pauses_and_at_once_when_the_app_sets_the_text() {
        let mut driven = driven(0);

        // One field per filterable column, named for assistive technology; none for Notes.
        let name = driven
            .context
            .find(&["grid", "$filters", "$scrolling", "$strip", "0"])
            .unwrap();
        assert_eq!(
            driven.context.node(name).unwrap().semantics.label,
            "Filter Name"
        );
        let notes = driven
            .context
            .find(&["grid", "$filters", "$scrolling", "$strip", "2"])
            .unwrap();
        assert!(!driven.context.node(notes).unwrap().semantics.focusable);

        // Typing a word filters once, after the pause.
        driven.context.action(name, Action::Focus);
        for (at, letter) in [(10, "a"), (60, "p")] {
            driven
                .context
                .event(Event::Text(TextEdit::Insert(letter.into())));
            driven.context.set_now(Duration::from_millis(at));
            driven.frame();
        }
        assert!(driven.state().model.calls.is_empty(), "still typing");
        assert!(
            driven.context.animating(),
            "and frames keep coming until it is due"
        );
        driven
            .context
            .set_now(Duration::from_millis(60 + FILTER_DELAY.as_millis() as u64));
        driven.frame();
        assert_eq!(driven.state().model.calls, [vec![(0, "ap".to_owned())]]);
        driven.context.set_now(Duration::from_millis(400));
        driven.frame();
        assert_eq!(driven.state().model.len(), 2);
        let info = driven
            .context
            .node(driven.context.find(&["grid"]).unwrap())
            .unwrap();
        assert_eq!(info.collection.unwrap().total, 2);
        // M35: a selected row filtered away is not sent as a message (Select
        // takes an ItemId, not none), so the state keeps Some(row 2) here and
        // that is not asserted.
        driven.context.set_now(Duration::from_millis(500));
        driven.frame();
        assert_eq!(
            driven.state().model.calls.len(),
            1,
            "unchanged text does not filter again"
        );
        assert!(!driven.context.animating());

        // Text the app sets applies at the next declaration; filters combine; spaces are trimmed.
        driven.state_mut().grid.filters.insert(1, "  RED ".into());
        driven.context.set_now(Duration::from_millis(600));
        driven.frame();
        // The grid sends FiltersApplied from this declaration; it arrives in the next batch.
        driven.frame();
        assert_eq!(
            driven.state().model.calls.last().unwrap(),
            &vec![(0, "ap".to_owned()), (1, "RED".to_owned())]
        );
        assert_eq!(driven.state().model.shown, [0]);
        // Clearing everything asks for all rows with an empty slice.
        driven.state_mut().grid.filters.clear();
        driven.context.set_now(Duration::from_millis(700));
        driven.frame();
        driven.frame();
        assert_eq!(driven.state().model.calls.last().unwrap(), &Vec::new());
        assert_eq!(driven.state().model.len(), 4);
    }
}

/// The one test whose refusal path stays on the closure form. See the note in
/// it.
mod legacy {
    use super::{CellEditor, GridColumn, ItemId, Key, People, Person, ROOT};
    use rust_gui_core::{
        Action, Context, Event, GridState, MonospaceMeasurer, TextEdit, VirtualListOptions,
    };

    fn people() -> People {
        People {
            columns: vec![
                GridColumn::new("Name", 160.0).editor(CellEditor::Text),
                GridColumn::new("Id", 80.0),
            ],
            rows: (0..20)
                .map(|row| Person {
                    name: format!("Person {row}"),
                    city: row % 3,
                    age: 30.0 + row as f64,
                    active: false,
                })
                .collect(),
        }
    }

    fn frame(context: &mut Context, model: &mut People, state: &mut GridState) {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            ui.keyed("grid", |ui| {
                ui.data_grid(model, state, VirtualListOptions::default())
            });
            ui.button("after");
        });
    }

    fn key(context: &mut Context, model: &mut People, state: &mut GridState, key: Key) {
        context.event(Event::KeyDown {
            key,
            shift: false,
            repeat: false,
        });
        frame(context, model, state);
        frame(context, model, state);
    }

    fn retype(context: &mut Context, model: &mut People, state: &mut GridState, text: &str) {
        context.event(Event::Text(TextEdit::SelectAll));
        context.event(Event::Text(TextEdit::Insert(text.into())));
        frame(context, model, state);
    }

    #[test]
    fn the_model_can_refuse_a_value_with_its_own_message() {
        // M35: in the message model a refused Edit leaves the editor waiting
        // for the model's value to change, which never happens, so Enter
        // cannot retry; and the model's refusal text has no channel back to
        // the grid. Stays here until a refused edit is answered by a message.
        let (mut context, mut model, mut state) = (Context::new(), people(), GridState::default());
        frame(&mut context, &mut model, &mut state);
        let rows = context.find(&["grid", "$rows"]).unwrap();
        context.action(rows, Action::Focus);
        state.rows.select(Some(ItemId(0)));
        frame(&mut context, &mut model, &mut state);

        key(&mut context, &mut model, &mut state, Key::F2);
        retype(&mut context, &mut model, &mut state, "   ");
        key(&mut context, &mut model, &mut state, Key::Enter);
        assert_eq!(state.edit_error(), Some("A name is required."));
        assert_eq!(model.rows[0].name, "Person 0");
        retype(&mut context, &mut model, &mut state, "Grace");
        key(&mut context, &mut model, &mut state, Key::Enter);
        assert_eq!(model.rows[0].name, "Grace");
        assert_eq!(state.edit_error(), None);
    }
}
