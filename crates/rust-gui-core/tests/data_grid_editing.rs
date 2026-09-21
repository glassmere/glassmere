use rust_gui_core::{
    Action, CellEditor, CellValue, Context, Event, GridColumn, GridModel, GridState, ItemId, Key,
    ListModel, MonospaceMeasurer, NodeId, NumberOptions, SortDirection, TextEdit,
    VirtualListOptions,
};
use rust_gui_scene::{Point, Size};

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

struct App {
    context: Context,
    model: People,
    state: GridState,
    edited: Vec<(ItemId, usize)>,
    activated: Vec<ItemId>,
}

impl App {
    fn new() -> Self {
        let age = NumberOptions::default().range(0.0, 150.0).suffix(" y");
        let mut app = Self {
            context: Context::new(),
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
            state: GridState::default(),
            edited: Vec::new(),
            activated: Vec::new(),
        };
        app.frame();
        let rows = app.rows();
        app.context.action(rows, Action::Focus);
        app.state.rows.select(Some(ItemId(0)));
        app.frame();
        app
    }

    fn frame(&mut self) {
        let (model, state) = (&mut self.model, &mut self.state);
        let mut response = Default::default();
        self.context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            response = ui.data_grid("grid", model, state, VirtualListOptions::default());
            ui.button("after");
        });
        self.edited.extend(response.edited);
        self.activated.extend(response.activated);
    }

    /// Two declarations: one that reacts to input, one that shows the result.
    fn settle(&mut self) {
        self.frame();
        self.frame();
    }

    fn rows(&self) -> NodeId {
        self.context.find(&["grid", "$rows"]).unwrap()
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, false);
    }

    fn key_with(&mut self, key: Key, shift: bool) {
        self.context.event(Event::KeyDown {
            key,
            shift,
            repeat: false,
        });
        self.settle();
    }

    fn editor(&self, row: usize) -> Option<NodeId> {
        let item = format!("item-{row}");
        self.context
            .find(&["grid", "$rows", &item, "$cells", "$edit"])
    }

    fn retype(&mut self, text: &str) {
        self.context.event(Event::Text(TextEdit::SelectAll));
        self.context
            .event(Event::Text(TextEdit::Insert(text.into())));
        self.frame();
    }

    fn cell_centre(&self, row: usize, column: usize) -> Point {
        let (item, column) = (format!("item-{row}"), column.to_string());
        let id = self
            .context
            .find(&["grid", "$rows", &item, "$cells", &column])
            .unwrap();
        let bounds = self.context.node(id).unwrap().bounds;
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    }

    fn click(&mut self, at: Point) {
        self.context.event(Event::PointerMoved(at));
        self.context.event(Event::PointerDown);
        self.context.event(Event::PointerUp);
        self.settle();
    }
}

#[test]
fn f2_and_enter_edit_a_text_cell_and_enter_stores_it_and_moves_down() {
    let mut app = App::new();
    app.key(Key::F2);
    assert_eq!(app.state.editing(), Some((ItemId(0), 0)));
    let editor = app.editor(0).expect("the editor replaces the cell");
    assert_eq!(
        app.context.focused(),
        Some(editor),
        "and takes the keyboard"
    );
    let node = app.context.node(editor).unwrap();
    assert_eq!(node.semantics.label, "Name");
    assert_eq!(node.value.as_deref(), Some("Person 0"));

    app.retype("Ada");
    assert_eq!(
        app.model.rows[0].name, "Person 0",
        "a draft until committed"
    );
    app.key(Key::Enter);
    assert_eq!(app.model.rows[0].name, "Ada");
    assert_eq!(app.edited, [(ItemId(0), 0)]);
    assert_eq!(app.state.editing(), None);
    assert_eq!(
        app.state.rows.selected(),
        Some(ItemId(1)),
        "Enter moves down"
    );
    assert_eq!(
        app.context.focused(),
        Some(app.rows()),
        "the grid has the keys back"
    );

    // Enter on an editable cell edits; it does not activate the row.
    app.key(Key::Enter);
    assert_eq!(app.state.editing(), Some((ItemId(1), 0)));
    assert!(app.activated.is_empty());
    // Escape drops the draft.
    app.retype("Nobody");
    app.key(Key::Escape);
    assert_eq!(app.state.editing(), None);
    assert_eq!(app.model.rows[1].name, "Person 1");
    assert_eq!(app.context.focused(), Some(app.rows()));
}

#[test]
fn arrows_move_the_cell_cursor_typing_replaces_and_tab_moves_sideways() {
    let mut app = App::new();
    app.key(Key::ArrowRight);
    app.key(Key::ArrowRight);
    assert_eq!(app.state.cursor, 2);
    app.key(Key::ArrowLeft);
    app.key(Key::ArrowLeft);
    app.key(Key::ArrowLeft);
    assert_eq!(app.state.cursor, 0, "stops at the first column");

    // Typing on the grid opens the cell with what was typed.
    app.context.event(Event::Text(TextEdit::Insert("Z".into())));
    app.settle();
    let editor = app.editor(0).unwrap();
    assert_eq!(
        app.context.node(editor).unwrap().value.as_deref(),
        Some("Z")
    );
    // Tab stores it and moves one column on, staying in the grid.
    app.key(Key::Tab);
    assert_eq!(app.model.rows[0].name, "Z");
    assert_eq!((app.state.cursor, app.state.editing()), (1, None));
    assert_eq!(app.context.focused(), Some(app.rows()));
    assert_eq!(app.state.rows.selected(), Some(ItemId(0)));
    app.key_with(Key::ArrowRight, false);
    app.key(Key::F2);
    app.key_with(Key::Tab, true);
    assert_eq!(app.state.cursor, 1, "Shift+Tab moves back");

    // A read-only column: Enter activates the row as before, F2 does nothing.
    app.state.cursor = 4;
    app.frame();
    app.key(Key::F2);
    assert_eq!(app.state.editing(), None);
    app.key(Key::Enter);
    assert_eq!(app.activated, [ItemId(0)]);
}

#[test]
fn numbers_parse_clamp_and_refuse_what_is_not_a_number() {
    let mut app = App::new();
    app.state.cursor = 2;
    app.frame();
    app.key(Key::F2);
    let editor = app.editor(0).unwrap();
    // The draft is the bare number; the unit is shown, not typed.
    assert_eq!(
        app.context.node(editor).unwrap().value.as_deref(),
        Some("30")
    );
    app.retype("abc");
    app.key(Key::Enter);
    assert_eq!(
        app.state.editing(),
        Some((ItemId(0), 2)),
        "the editor stays open"
    );
    assert_eq!(app.state.edit_error(), Some("Enter a number."));
    assert_eq!(
        app.state.rows.selected(),
        Some(ItemId(0)),
        "and the cursor stays"
    );
    let editor = app.editor(0).unwrap();
    assert_eq!(
        app.context.node(editor).unwrap().description.as_deref(),
        Some("Enter a number.")
    );
    app.retype("41,5");
    app.key(Key::Enter);
    assert_eq!(
        app.model.rows[0].age, 42.0,
        "a decimal comma, snapped to whole years"
    );
    app.key(Key::ArrowUp);
    app.key(Key::F2);
    app.retype("999");
    app.key(Key::Enter);
    assert_eq!(app.model.rows[0].age, 150.0);
}

#[test]
fn the_model_can_refuse_a_value_with_its_own_message() {
    let mut app = App::new();
    app.key(Key::F2);
    app.retype("   ");
    app.key(Key::Enter);
    assert_eq!(app.state.edit_error(), Some("A name is required."));
    assert_eq!(app.model.rows[0].name, "Person 0");
    assert!(app.edited.is_empty());
    app.retype("Grace");
    app.key(Key::Enter);
    assert_eq!(app.model.rows[0].name, "Grace");
    assert_eq!(app.state.edit_error(), None);
}

#[test]
fn bools_toggle_in_place_and_choices_store_the_moment_one_is_picked() {
    let mut app = App::new();
    app.state.cursor = 3;
    app.frame();
    app.key(Key::Space);
    assert!(app.model.rows[0].active);
    assert_eq!(app.state.editing(), None, "no editor opens for a bool");
    app.key(Key::Enter);
    assert!(!app.model.rows[0].active);
    assert_eq!(app.edited, [(ItemId(0), 3), (ItemId(0), 3)]);

    app.state.cursor = 1;
    app.frame();
    app.key(Key::F2);
    let editor = app.editor(0).unwrap();
    assert_eq!(app.context.focused(), Some(editor));
    // Open the list and take "Vienna".
    app.key(Key::ArrowDown);
    app.key(Key::ArrowDown);
    app.key(Key::ArrowDown);
    app.key(Key::Enter);
    app.frame();
    assert_eq!(CITIES[app.model.rows[0].city], "Vienna");
    assert_eq!(app.state.editing(), None);
    assert_eq!(app.context.focused(), Some(app.rows()));
}

#[test]
fn a_second_click_on_the_current_cell_opens_it_and_clicking_away_stores_it() {
    let mut app = App::new();
    let name = app.cell_centre(2, 0);
    app.click(name);
    assert_eq!(app.state.rows.selected(), Some(ItemId(2)));
    assert_eq!((app.state.cursor, app.state.editing()), (0, None));
    app.click(name);
    assert_eq!(app.state.editing(), Some((ItemId(2), 0)));
    app.retype("Linus");
    // Clicking another row leaves the editor, which stores the draft.
    let other = app.cell_centre(5, 2);
    app.click(other);
    assert_eq!(app.model.rows[2].name, "Linus");
    assert_eq!(app.state.editing(), None);
    assert_eq!(
        (app.state.rows.selected(), app.state.cursor),
        (Some(ItemId(5)), 2)
    );
}

mod filtering {
    use rust_gui_core::{
        Action, Context, Event, FILTER_DELAY, GridColumn, GridModel, GridState, ItemId, ListModel,
        MonospaceMeasurer, SortDirection, TextEdit, VirtualListOptions, text_matches,
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

    fn frame(context: &mut Context, model: &mut Fruit, state: &mut GridState, at: u64) {
        context.set_now(Duration::from_millis(at));
        context.run(Size::new(700.0, 400.0), &mut MonospaceMeasurer, |ui| {
            ui.data_grid("grid", model, state, VirtualListOptions::default());
        });
    }

    #[test]
    fn the_model_filters_once_typing_pauses_and_at_once_when_the_app_sets_the_text() {
        let mut model = Fruit {
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
        let mut context = Context::new();
        let mut state = GridState::default();
        state.rows.select(Some(ItemId(2)));
        frame(&mut context, &mut model, &mut state, 0);

        // One field per filterable column, named for assistive technology; none for Notes.
        let name = context.find(&["grid", "$filters", "0"]).unwrap();
        assert_eq!(context.node(name).unwrap().semantics.label, "Filter Name");
        let notes = context.find(&["grid", "$filters", "2"]).unwrap();
        assert!(!context.node(notes).unwrap().semantics.focusable);

        // Typing a word filters once, after the pause.
        context.action(name, Action::Focus);
        for (at, letter) in [(10, "a"), (60, "p")] {
            context.event(Event::Text(TextEdit::Insert(letter.into())));
            frame(&mut context, &mut model, &mut state, at);
        }
        assert!(model.calls.is_empty(), "still typing");
        assert!(
            context.animating(),
            "and frames keep coming until it is due"
        );
        frame(
            &mut context,
            &mut model,
            &mut state,
            60 + FILTER_DELAY.as_millis() as u64,
        );
        assert_eq!(model.calls, [vec![(0, "ap".to_owned())]]);
        frame(&mut context, &mut model, &mut state, 400);
        assert_eq!(model.len(), 2);
        let info = context.node(context.find(&["grid"]).unwrap()).unwrap();
        assert_eq!(info.collection.unwrap().total, 2);
        // The selected row was filtered away, so nothing is selected.
        assert_eq!(state.rows.selected(), None);
        frame(&mut context, &mut model, &mut state, 500);
        assert_eq!(model.calls.len(), 1, "unchanged text does not filter again");
        assert!(!context.animating());

        // Text the app sets applies at the next declaration; filters combine; spaces are trimmed.
        state.filters.insert(1, "  RED ".into());
        frame(&mut context, &mut model, &mut state, 600);
        assert_eq!(
            model.calls.last().unwrap(),
            &vec![(0, "ap".to_owned()), (1, "RED".to_owned())]
        );
        assert_eq!(model.shown, [0]);
        // Clearing everything asks for all rows with an empty slice.
        state.filters.clear();
        frame(&mut context, &mut model, &mut state, 700);
        assert_eq!(model.calls.last().unwrap(), &Vec::new());
        assert_eq!(model.len(), 4);
    }
}
