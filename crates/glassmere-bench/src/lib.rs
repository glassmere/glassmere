//! The scenarios the benchmarks time and the profiler samples.
//!
//! Each is a small `App` and one step of it: a frame with the input that
//! frame would have, a paint of a scene, or a batch of text to shape. They
//! are headless, so what they measure is the framework's own work, and the
//! same step runs under `cargo bench`, where it is timed, and under the
//! `profile` binary, where it is sampled.
//!
//! Text is measured two ways. Most scenarios use the monospace measurer,
//! which costs nothing, so that the number is the tree's. The ones named
//! `shaped` use the real text system over the one font the workspace
//! carries, so that they run the same on every machine.

use glassmere_core::{
    App, AppDriver, Button, CodeEditorOptions, Context, Effects, FrameStats, GridColumn, GridModel,
    GridMsg, GridState, ItemId, Label, Length, ListModel, ListState, SortDirection, Style,
    TextDocument, TextOp, TreeItem, TreeModel, Ui, VirtualListOptions,
};
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{Color, MonospaceMeasurer, Rect, Size, TextMeasurer};
use glassmere_text::TextSystem;
use std::collections::HashSet;

/// The window every scenario runs in.
pub const WINDOW: Size = Size::new(1200.0, 800.0);

/// A text system over the workspace's own font and no system fonts.
pub fn text_system() -> TextSystem {
    TextSystem::with_fonts([include_bytes!(
        "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()])
}

/// An application, the tree it declares into and what measures its text.
pub struct Run<A: App, T: TextMeasurer> {
    pub app: A,
    pub context: Context,
    driver: AppDriver<A::Message>,
    measurer: T,
    pub size: Size,
}

impl<A: App, T: TextMeasurer> Run<A, T> {
    pub fn new(app: A, measurer: T) -> Self {
        let mut run = Self {
            app,
            context: Context::new(),
            driver: AppDriver::new(|| true),
            measurer,
            size: WINDOW,
        };
        // The first frame builds everything; no scenario is about that one.
        run.frame([]);
        run
    }

    /// One frame: an `update` for each message, then at most one declaration.
    pub fn frame(&mut self, messages: impl IntoIterator<Item = A::Message>) -> FrameStats {
        for message in messages {
            self.driver.send(message);
        }
        self.driver.frame(
            &mut self.app,
            &mut self.context,
            self.size,
            &mut self.measurer,
        )
    }
}

/// What a frame of the form changes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// One label at the top: everything is declared again and found the same.
    Heading,
    /// Every row's colour: nothing is laid out, everything is painted.
    Colours,
    /// Every row's text: everything is measured, laid out and painted.
    Texts,
}

/// Rows of a label, a field, a checkbox and a button, as a settings page or
/// a long form has them. `Ui::memo` is deliberately not used: this is what
/// the tree costs when the view declares all of it.
pub struct Form {
    pub rows: usize,
    pub change: Change,
    tick: usize,
    names: Vec<String>,
}

#[derive(Clone)]
pub enum FormMsg {
    Tick,
    Name(usize, String),
    Check(usize, bool),
    Press(usize),
}

impl Form {
    pub fn new(rows: usize, change: Change) -> Self {
        Self {
            rows,
            change,
            tick: 0,
            names: (0..rows).map(|row| format!("name {row}")).collect(),
        }
    }
}

impl App for Form {
    type Message = FormMsg;

    fn update(&mut self, message: FormMsg, _: &mut Effects<'_, FormMsg>) {
        match message {
            FormMsg::Tick => self.tick += 1,
            FormMsg::Name(row, name) => self.names[row] = name,
            FormMsg::Check(..) | FormMsg::Press(_) => {}
        }
    }

    fn view(&self, ui: &mut Ui<'_, FormMsg>) {
        let tick = self.tick;
        ui.scroll(
            Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0)),
            |ui| {
                ui.label(format!("Frame {tick}"));
                for row in 0..self.rows {
                    let text = match self.change {
                        Change::Texts => format!("Row {row} at {}", tick + row),
                        _ => format!("Row {row}"),
                    };
                    let mut style = Style::default().gap(8.0);
                    if self.change == Change::Colours {
                        let shade = ((tick + row) % 2 * 20) as u8;
                        style = style.background(Color::rgb(40 + shade, 44, 52));
                    }
                    ui.keyed(row.to_string(), |ui| {
                        ui.row(style, |ui| {
                            ui.label(
                                Label::new(text).style(Style::default().width(Length::Px(220.0))),
                            );
                            ui.text_input("Name", &self.names[row], move |name| {
                                FormMsg::Name(row, name)
                            });
                            ui.checkbox("Enabled", row % 2 == 0, move |on| FormMsg::Check(row, on));
                            ui.button(Button::new("Apply").on_click(FormMsg::Press(row)));
                        });
                    });
                }
            },
        );
    }
}

/// A million rows that are computed, not stored.
struct Rows {
    columns: Vec<GridColumn>,
}

const ROWS: usize = 1_000_000;

impl ListModel for Rows {
    fn len(&self) -> usize {
        ROWS
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        ((id.0 as usize) < ROWS).then_some(id.0 as usize)
    }
    fn label(&self, index: usize) -> String {
        format!("Row {index}")
    }
}

impl GridModel for Rows {
    fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    fn cell(&self, row: usize, column: usize) -> String {
        match column {
            0 => format!("{row:07}"),
            1 => ["open", "triaged", "closed"][row % 3].to_owned(),
            _ => format!("{}.{:02}", row % 97, column * 7 % 100),
        }
    }
    fn sort(&mut self, _: usize, _: SortDirection) -> bool {
        false
    }
}

fn viewport() -> VirtualListOptions {
    VirtualListOptions {
        height: 700.0,
        row_height: 24.0,
        ..VirtualListOptions::default()
    }
}

pub struct Table {
    model: Rows,
    state: GridState,
}

#[derive(Clone)]
pub enum TableMsg {
    Reveal(usize),
    Grid(GridMsg),
}

impl Table {
    pub fn new() -> Self {
        let column = GridColumn::new;
        Self {
            model: Rows {
                columns: vec![
                    column("ID", 120.0),
                    column("Status", 140.0),
                    column("Value", 110.0),
                    column("Margin", 110.0),
                    column("Owner", 160.0),
                    column("Updated", 160.0),
                ],
            },
            state: GridState::default(),
        }
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl App for Table {
    type Message = TableMsg;

    fn update(&mut self, message: TableMsg, _: &mut Effects<'_, TableMsg>) {
        match message {
            TableMsg::Reveal(row) => self.state.rows.reveal(ItemId(row as u64)),
            TableMsg::Grid(message) => self.state.apply(&message),
        }
    }

    fn view(&self, ui: &mut Ui<'_, TableMsg>) {
        ui.keyed("grid", |ui| {
            ui.data_grid(&self.model, &self.state, viewport(), TableMsg::Grid)
        });
    }
}

pub struct Outline {
    model: TreeModel,
    state: ListState,
}

#[derive(Clone)]
pub enum OutlineMsg {
    Reveal(usize),
    Tree(GridMsg),
}

const NODES: usize = 100_000;

impl Outline {
    pub fn new() -> Self {
        let branch = 100;
        let items = (0..NODES)
            .map(|index| TreeItem {
                id: ItemId(index as u64),
                parent: (index % branch != 0).then(|| ItemId((index / branch * branch) as u64)),
                label: format!("Node {index}"),
            })
            .collect();
        let mut model = TreeModel::new(items).expect("unique identities");
        for root in (0..NODES).step_by(branch) {
            model.set_expanded(ItemId(root as u64), true);
        }
        Self {
            model,
            state: ListState::default(),
        }
    }
}

impl Default for Outline {
    fn default() -> Self {
        Self::new()
    }
}

impl App for Outline {
    type Message = OutlineMsg;

    fn update(&mut self, message: OutlineMsg, _: &mut Effects<'_, OutlineMsg>) {
        match message {
            OutlineMsg::Reveal(row) => self.state.reveal(ItemId(row as u64)),
            OutlineMsg::Tree(GridMsg::Select(id)) => self.state.select(Some(id)),
            OutlineMsg::Tree(GridMsg::Expand { row, open }) => {
                self.model.set_expanded(row, open);
            }
            OutlineMsg::Tree(_) => {}
        }
    }

    fn view(&self, ui: &mut Ui<'_, OutlineMsg>) {
        ui.keyed("tree", |ui| {
            ui.tree_view(&self.model, &self.state, viewport(), OutlineMsg::Tree)
        });
    }
}

pub struct Source {
    document: TextDocument,
}

#[derive(Clone)]
pub enum SourceMsg {
    Type(String),
    Erase,
    Edit(TextOp),
}

impl Source {
    pub fn new(lines: usize) -> Self {
        let line = "let value = compute(index, offset) + adjust(previous, 3.14159); // a line";
        let text: String = (0..lines)
            .map(|index| format!("{line} {index}\n"))
            .collect();
        Self {
            document: TextDocument::from_text(&text),
        }
    }
}

impl App for Source {
    type Message = SourceMsg;

    fn update(&mut self, message: SourceMsg, _: &mut Effects<'_, SourceMsg>) {
        match message {
            SourceMsg::Type(text) => {
                self.document.insert(&text);
            }
            SourceMsg::Erase => {
                self.document.backspace(false);
            }
            SourceMsg::Edit(op) => self.document.apply(op),
        }
    }

    fn view(&self, ui: &mut Ui<'_, SourceMsg>) {
        ui.keyed("editor", |ui| {
            ui.code_editor(
                &self.document,
                CodeEditorOptions {
                    line_numbers: true,
                    ..CodeEditorOptions::default()
                },
                SourceMsg::Edit,
            );
        });
    }
}

/// One scenario: its name, what it measures, and its step.
pub struct Scenario {
    pub name: &'static str,
    pub about: &'static str,
    /// Builds the scenario, which is not measured, and returns its step.
    pub build: fn() -> Box<dyn FnMut()>,
}

fn form(change: Change) -> Box<dyn FnMut()> {
    let mut run = Run::new(Form::new(300, change), MonospaceMeasurer);
    Box::new(move || {
        run.frame([FormMsg::Tick]);
    })
}

/// Every scenario, in the order they are listed and run.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "form/idle",
            about: "1,200 widgets and a frame with no input: nothing should run",
            build: || {
                let mut run = Run::new(Form::new(300, Change::Heading), MonospaceMeasurer);
                Box::new(move || {
                    run.frame([]);
                })
            },
        },
        Scenario {
            name: "form/redeclare",
            about: "1,200 widgets declared again and found unchanged: declare and reconcile",
            build: || form(Change::Heading),
        },
        Scenario {
            name: "form/repaint",
            about: "the same, every row a new colour: declare, reconcile and paint",
            build: || form(Change::Colours),
        },
        Scenario {
            name: "form/relayout",
            about: "the same, every row a new text: measure, layout and paint as well",
            build: || form(Change::Texts),
        },
        Scenario {
            name: "form/resize",
            about: "the same form in a window that changes width every frame",
            build: || {
                let mut run = Run::new(Form::new(300, Change::Heading), MonospaceMeasurer);
                let mut wide = false;
                Box::new(move || {
                    wide = !wide;
                    run.size = Size::new(if wide { 1200.0 } else { 1000.0 }, 800.0);
                    run.frame([]);
                })
            },
        },
        Scenario {
            name: "form/relayout-shaped",
            about: "form/relayout with real text shaping in place of the monospace measurer",
            build: || {
                let mut run = Run::new(Form::new(300, Change::Texts), text_system());
                Box::new(move || {
                    run.frame([FormMsg::Tick]);
                })
            },
        },
        Scenario {
            name: "table/scroll",
            about: "a grid of 1,000,000 rows scrolled ten rows a frame",
            build: || {
                let mut run = Run::new(Table::new(), MonospaceMeasurer);
                let mut row = 0;
                Box::new(move || {
                    row = (row + 10) % (ROWS - 100);
                    run.frame([TableMsg::Reveal(row)]);
                })
            },
        },
        Scenario {
            name: "tree/scroll",
            about: "a tree of 100,000 expanded nodes scrolled ten rows a frame",
            build: || {
                let mut run = Run::new(Outline::new(), MonospaceMeasurer);
                let mut row = 0;
                Box::new(move || {
                    row = (row + 10) % (NODES - 100);
                    run.frame([OutlineMsg::Reveal(row)]);
                })
            },
        },
        Scenario {
            name: "editor/type",
            about: "a character typed into a 40,000-line document, or erased from it",
            build: || {
                let mut run = Run::new(Source::new(40_000), MonospaceMeasurer);
                let mut typed = false;
                Box::new(move || {
                    // Typed and erased in turn, so that the line stays the
                    // length it was however long the scenario runs.
                    typed = !typed;
                    run.frame([if typed {
                        SourceMsg::Type("x".into())
                    } else {
                        SourceMsg::Erase
                    }]);
                })
            },
        },
        Scenario {
            name: "render-cpu/full",
            about: "the CPU renderer painting the whole form at 1200 by 800",
            build: || {
                let run = Run::new(Form::new(300, Change::Colours), MonospaceMeasurer);
                let mut renderer = CpuRenderer::with_text_system(text_system());
                Box::new(move || {
                    let pixels = renderer.render(
                        [WINDOW.width as u32, WINDOW.height as u32],
                        1.0,
                        run.context.scene(),
                        Color::rgb(24, 26, 31),
                    );
                    std::hint::black_box(pixels.len());
                })
            },
        },
        Scenario {
            name: "render-cpu/damage",
            about: "the same scene with one row's rectangle damaged",
            build: || {
                let run = Run::new(Form::new(300, Change::Colours), MonospaceMeasurer);
                let mut renderer = CpuRenderer::with_text_system(text_system());
                let size = [WINDOW.width as u32, WINDOW.height as u32];
                let clear = Color::rgb(24, 26, 31);
                renderer.render(size, 1.0, run.context.scene(), clear);
                let damage = [Rect {
                    x: 0.0,
                    y: 200.0,
                    width: WINDOW.width,
                    height: 48.0,
                }];
                Box::new(move || {
                    let pixels =
                        renderer.render_damaged(size, 1.0, run.context.scene(), clear, &damage);
                    std::hint::black_box(pixels.len());
                })
            },
        },
        Scenario {
            name: "text/shape-cold",
            about: "100 lines of text shaped for the first time",
            build: || {
                let mut text = text_system();
                let mut round = 0usize;
                Box::new(move || {
                    round += 1;
                    for line in 0..100 {
                        let key =
                            text.ensure(&format!("Line {line} of round {round}, shaped"), 14.0);
                        std::hint::black_box(key);
                    }
                    // Nothing is kept, so the next round shapes again.
                    text.retain(&HashSet::new());
                })
            },
        },
        Scenario {
            name: "text/shape-warm",
            about: "the same 100 lines asked for again: the cache alone",
            build: || {
                let mut text = text_system();
                let lines: Vec<String> = (0..100)
                    .map(|line| format!("Line {line} of the same round, shaped"))
                    .collect();
                Box::new(move || {
                    for line in &lines {
                        std::hint::black_box(text.ensure(line, 14.0));
                    }
                })
            },
        },
    ]
}
