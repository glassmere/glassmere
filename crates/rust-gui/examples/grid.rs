//! The data grid: typed in-place editing with a cell cursor, and columns the
//! user resizes, reorders and hides. F2, Enter, a second click or typing opens
//! a cell; Enter stores and moves down, Tab stores and moves sideways, Escape
//! cancels. Right-click a header, or press Shift+F10 on it, for its menu,
//! which also freezes columns. Name and City can be filtered. The grid is wider
//! than its window: Id and Name stay while the rest scrolls sideways.
use rust_gui::{
    AppOptions, CellEditor, CellValue, GridColumn, GridModel, GridState, ItemId, Label, Length,
    ListModel, NumberOptions, Size, SortDirection, Style, VirtualListOptions, run, text_matches,
};

const CITIES: [&str; 6] = ["Berlin", "Brno", "Lisbon", "Prague", "Vienna", "Zagreb"];
const FIRST: [&str; 8] = [
    "Ada", "Alan", "Edsger", "Grace", "Ken", "Linus", "Margaret", "Niklaus",
];
const LAST: [&str; 7] = [
    "Hopper", "Kay", "Lamport", "Liskov", "Ritchie", "Turing", "Wirth",
];

struct Person {
    id: u64,
    name: String,
    city: usize,
    age: f64,
    salary: f64,
    active: bool,
}

struct People {
    columns: Vec<GridColumn>,
    /// Everyone; `rows` is the part the filters let through.
    everyone: Vec<Person>,
    rows: Vec<usize>,
}

impl People {
    fn new(count: u64) -> Self {
        let money = NumberOptions::default()
            .range(0.0, 500_000.0)
            .step(100.0)
            .suffix(" €");
        let age = NumberOptions::default().range(18.0, 99.0);
        Self {
            columns: vec![
                GridColumn::new("Id", 70.0).sortable(true),
                GridColumn::new("Name", 190.0)
                    .sortable(true)
                    .filterable(true)
                    .editor(CellEditor::Text),
                GridColumn::new("City", 130.0)
                    .sortable(true)
                    .filterable(true)
                    .editor(CellEditor::Choice(CITIES.map(String::from).to_vec())),
                GridColumn::new("Age", 80.0)
                    .sortable(true)
                    .editor(CellEditor::number(age)),
                GridColumn::new("Salary", 130.0)
                    .sortable(true)
                    .editor(CellEditor::number(money)),
                GridColumn::new("Active", 80.0).editor(CellEditor::Bool),
                GridColumn::new("Email", 260.0),
                GridColumn::new("Department", 170.0),
                GridColumn::new("Badge", 150.0),
            ],
            rows: (0..count as usize).collect(),
            everyone: (0..count)
                .map(|id| Person {
                    id,
                    name: format!(
                        "{} {}",
                        FIRST[(id * 5 % 8) as usize],
                        LAST[(id * 3 % 7) as usize]
                    ),
                    city: (id * 7 % 6) as usize,
                    age: 22.0 + (id * 13 % 40) as f64,
                    salary: 32_000.0 + (id * 7919 % 900) as f64 * 100.0,
                    active: id % 4 != 0,
                })
                .collect(),
        }
    }
}

impl People {
    fn person(&self, row: usize) -> &Person {
        &self.everyone[self.rows[row]]
    }
}

impl ListModel for People {
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(self.person(index).id)
    }
    // ponytail: a linear search; keep an id-to-row map once there are more than a few thousand rows.
    fn index_of(&self, id: ItemId) -> Option<usize> {
        self.rows
            .iter()
            .position(|person| self.everyone[*person].id == id.0)
    }
    fn label(&self, index: usize) -> String {
        self.person(index).name.clone()
    }
}

impl GridModel for People {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        let person = self.person(row);
        match column {
            0 => format!("#{}", person.id),
            1 => person.name.clone(),
            2 => CITIES[person.city].into(),
            3 => format!("{}", person.age),
            4 => format!("{} €", person.salary),
            5 => if person.active { "☑" } else { "☐" }.into(),
            6 => format!(
                "{}@example.test",
                person.name.to_lowercase().replace(' ', ".")
            ),
            7 => ["Research", "Design", "Support", "Sales"][(person.id % 4) as usize].into(),
            _ => format!("B-{:05}", person.id * 37 % 100_000),
        }
    }
    fn value(&self, row: usize, column: usize) -> CellValue {
        let person = self.person(row);
        match column {
            2 => CellValue::Choice(person.city),
            3 => CellValue::Number(person.age),
            4 => CellValue::Number(person.salary),
            5 => CellValue::Bool(person.active),
            _ => CellValue::Text(self.cell(row, column)),
        }
    }
    fn set_cell(&mut self, row: usize, column: usize, value: CellValue) -> Result<(), String> {
        let person = &mut self.everyone[self.rows[row]];
        match (column, value) {
            (1, CellValue::Text(name)) if name.trim().is_empty() => {
                return Err("A name is required.".into());
            }
            (1, CellValue::Text(name)) => person.name = name.trim().into(),
            (2, CellValue::Choice(city)) => person.city = city,
            (3, CellValue::Number(age)) => person.age = age,
            (4, CellValue::Number(salary)) => person.salary = salary,
            (5, CellValue::Bool(active)) => person.active = active,
            _ => return Err("This cell cannot be edited.".into()),
        }
        Ok(())
    }
    fn sort(&mut self, column: usize, direction: SortDirection) -> bool {
        let everyone = &self.everyone;
        self.rows.sort_by(|a, b| {
            let (a, b) = (&everyone[*a], &everyone[*b]);
            match column {
                1 => a.name.cmp(&b.name),
                2 => CITIES[a.city].cmp(CITIES[b.city]),
                3 => a.age.total_cmp(&b.age),
                4 => a.salary.total_cmp(&b.salary),
                _ => a.id.cmp(&b.id),
            }
        });
        if direction == SortDirection::Descending {
            self.rows.reverse();
        }
        true
    }
    fn filter(&mut self, filters: &[(usize, &str)]) -> bool {
        // Sorting is kept: the rows that pass stay in their current order,
        // and the ones that come back are appended in id order.
        let passes = |person: &Person| {
            filters.iter().all(|(column, text)| match column {
                1 => text_matches(&person.name, text),
                _ => text_matches(CITIES[person.city], text),
            })
        };
        let mut rows: Vec<usize> = self
            .rows
            .iter()
            .copied()
            .filter(|row| passes(&self.everyone[*row]))
            .collect();
        for row in 0..self.everyone.len() {
            if passes(&self.everyone[row]) && !rows.contains(&row) {
                rows.push(row);
            }
        }
        self.rows = rows;
        true
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut people = People::new(2000);
    let mut state = GridState::default();
    // Id and Name stay in place while the other columns scroll sideways.
    state.columns.frozen = 2;
    let mut last = String::from("nothing yet");
    run(
        AppOptions {
            title: "Rust GUI / Data grid".into(),
            size: Size::new(860.0, 640.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
            let page = Style {
                padding: 24.0,
                gap: 12.0,
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Style::default()
            };
            ui.column(page, |ui| {
                ui.label(Label::new(format!("People · {} of 2,000 rows", people.rows.len())).style(Style {
                        font_size: Some(24.0),
                        ..Style::default()
                    }));
                ui.label("F2, Enter, a second click or typing edits · Enter stores, Tab moves on, Esc cancels · Shift+wheel scrolls sideways");
                let options = VirtualListOptions {
                    label: "People".into(),
                    height: 460.0,
                    ..VirtualListOptions::default()
                };
                let response = ui.data_grid(&mut people, &mut state, options);
                if let Some((item, column)) = response.edited {
                    last = format!("stored {} of #{}", people.columns[column].label, item.0);
                }
                let status = match state.edit_error() {
                    Some(error) => format!("Not stored: {error}"),
                    None => format!("Last change: {last}"),
                };
                ui.label(status);
            });
        },
    )
}
