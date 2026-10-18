//! The four benchmarks the feature list names: a million-row table, a
//! hundred-thousand-node tree, a multi-megabyte text file and hundreds of
//! animated elements; and an `App`'s batches that declare nothing. Headless,
//! so the numbers are the framework's own work without a GPU, a font stack or
//! a compositor in them.
//!
//! Each scenario is an `App`: a frame sends the messages its input produces,
//! and a frame with none declares only when something it reads changed.
//!
//! ```sh
//! cargo run -p glassmere-core --example bench --release
//! ```
use glassmere_core::{Action, Label};
use glassmere_core::{
    App, AppDriver, Button, CodeEditorOptions, Context, Easing, Effects, Event, FrameStats,
    GridColumn, GridModel, GridMsg, GridState, ItemId, Length, ListModel, ListState,
    MonospaceMeasurer, SortDirection, Style, TextDocument, TextOp, Transition, TreeItem, TreeModel,
    Ui, VirtualListOptions,
};
use glassmere_scene::{Color, Point, Size};
use std::time::{Duration, Instant};

const WINDOW: Size = Size::new(1200.0, 800.0);
const ROWS: usize = 1_000_000;
const NODES: usize = 100_000;
const LINES: usize = 40_000;
const ANIMATED: usize = 400;
/// Frames per phase, after one that is thrown away as the cold one.
const RUNS: usize = 30;

/// A million rows, numbered. Nothing is stored: the model answers from the
/// index, which is what a virtualized collection is for.
struct Rows {
    count: usize,
    columns: Vec<GridColumn>,
}

impl ListModel for Rows {
    fn len(&self) -> usize {
        self.count
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        ((id.0 as usize) < self.count).then_some(id.0 as usize)
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

fn column(label: &str, width: f32) -> GridColumn {
    GridColumn {
        label: label.into(),
        width,
        sortable: false,
        editor: None,
        filterable: false,
    }
}

fn viewport() -> VirtualListOptions {
    VirtualListOptions {
        height: 700.0,
        row_height: 24.0,
        ..VirtualListOptions::default()
    }
}

/// One phase: how long a frame takes, and what it cost to declare.
struct Timing {
    scenario: &'static str,
    phase: &'static str,
    mean: f64,
    p95: f64,
    stats: FrameStats,
}

impl Timing {
    fn row(&self) -> String {
        format!(
            "{},{},{:.1},{:.1},{:.1},{},{},{},{},{}",
            self.scenario,
            self.phase,
            self.mean,
            self.p95,
            self.stats.shaping.as_secs_f64() * 1_000_000.0,
            self.stats.elements,
            self.stats.element_bytes,
            self.stats.text_measures,
            self.stats.paint_nodes,
            self.stats.paint_cache_hits,
        )
    }
}

/// Runs `frame` until the times settle, throwing the first away because it
/// builds the tree that all the others reuse.
fn measure(
    scenario: &'static str,
    phase: &'static str,
    runs: usize,
    mut frame: impl FnMut(usize) -> FrameStats,
) -> Timing {
    frame(0);
    let mut times = Vec::with_capacity(runs);
    let mut stats = FrameStats::default();
    for run in 0..runs {
        let started = Instant::now();
        stats = frame(run + 1);
        times.push(started.elapsed().as_secs_f64() * 1_000_000.0);
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    times.sort_by(f64::total_cmp);
    Timing {
        scenario,
        phase,
        mean,
        p95: times[(times.len() * 95 / 100).min(times.len() - 1)],
        stats,
    }
}

/// One scenario's application with its context, driven one batch at a time.
struct Run<A: App> {
    app: A,
    driver: AppDriver<A::Message>,
    context: Context,
}

impl<A: App> Run<A> {
    fn new(app: A) -> Self {
        Self {
            app,
            driver: AppDriver::new(|| true),
            context: Context::new(),
        }
    }

    /// One frame: `messages` are what the frame's input produced, and the
    /// batch is one `update` each, then at most one declaration.
    fn frame(&mut self, messages: Vec<A::Message>) -> FrameStats {
        for message in messages {
            self.driver.send(message);
        }
        self.driver.frame(
            &mut self.app,
            &mut self.context,
            WINDOW,
            &mut MonospaceMeasurer,
        )
    }
}

/// The table: a scroll is a message, and a still frame has none.
struct Table {
    model: Rows,
    state: GridState,
}

#[derive(Clone)]
enum TableMsg {
    Reveal(usize),
    Grid(GridMsg),
}

impl App for Table {
    type Message = TableMsg;

    fn update(&mut self, message: TableMsg, _effects: &mut Effects<'_, TableMsg>) {
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

fn table() -> Vec<Timing> {
    let model = Rows {
        count: ROWS,
        columns: vec![
            column("ID", 120.0),
            column("Status", 140.0),
            column("Value", 110.0),
            column("Margin", 110.0),
            column("Owner", 160.0),
            column("Updated", 160.0),
        ],
    };
    let mut run = Run::new(Table {
        model,
        state: GridState::default(),
    });
    vec![
        measure("table", "still", RUNS, |_| run.frame(Vec::new())),
        measure("table", "scrolling", RUNS, |index| {
            run.frame(vec![TableMsg::Reveal(index * 10)])
        }),
        // Jumping about the million rows, where a table that realized them
        // all would already have given up.
        measure("table", "far", RUNS, |index| {
            run.frame(vec![TableMsg::Reveal(index * 97_003 % ROWS)])
        }),
    ]
}

/// The tree: a scroll is a message, collapsing the root is one too.
struct Outline {
    model: TreeModel,
    state: ListState,
}

#[derive(Clone)]
enum OutlineMsg {
    Reveal(usize),
    Collapse(bool),
    Tree(GridMsg),
}

impl App for Outline {
    type Message = OutlineMsg;

    fn update(&mut self, message: OutlineMsg, _effects: &mut Effects<'_, OutlineMsg>) {
        match message {
            OutlineMsg::Reveal(row) => self.state.reveal(ItemId(row as u64)),
            OutlineMsg::Collapse(open) => {
                self.model.set_expanded(ItemId(0), open);
            }
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

fn tree() -> Vec<Timing> {
    let branch = 100;
    let mut items = Vec::with_capacity(NODES);
    for index in 0..NODES {
        items.push(TreeItem {
            id: ItemId(index as u64),
            parent: (index % branch != 0).then(|| ItemId((index / branch * branch) as u64)),
            label: format!("Node {index}"),
        });
    }
    let mut model = TreeModel::new(items).expect("unique identities");
    for root in (0..NODES).step_by(branch) {
        model.set_expanded(ItemId(root as u64), true);
    }
    let mut run = Run::new(Outline {
        model,
        state: ListState::default(),
    });
    vec![
        measure("tree", "expanded", RUNS, |_| run.frame(Vec::new())),
        measure("tree", "scrolling", RUNS, |index| {
            run.frame(vec![OutlineMsg::Reveal(index * 10)])
        }),
        measure("tree", "collapse", RUNS, |index| {
            run.frame(vec![
                OutlineMsg::Collapse(index % 2 == 0),
                OutlineMsg::Reveal(0),
            ])
        }),
    ]
}

/// The document: typing is a message per frame, and an edit from the editor
/// is applied to the document in `update`.
struct Source {
    document: TextDocument,
}

#[derive(Clone)]
enum SourceMsg {
    Type(String),
    Edit(TextOp),
}

impl App for Source {
    type Message = SourceMsg;

    fn update(&mut self, message: SourceMsg, _effects: &mut Effects<'_, SourceMsg>) {
        match message {
            SourceMsg::Type(text) => {
                self.document.insert(&text);
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

fn text() -> Vec<Timing> {
    let line = "let value = compute(index, offset) + adjust(previous, 3.14159); // a line";
    let source: String = (0..LINES)
        .map(|index| format!("{line} {index}\n"))
        .collect();
    println!("# text document: {} bytes", source.len());
    let mut run = Run::new(Source {
        document: TextDocument::from_text(&source),
    });
    let open = measure("text", "open", 1, |_| run.frame(Vec::new()));
    let still = measure("text", "still", RUNS, |_| run.frame(Vec::new()));
    let typing = measure("text", "typing", RUNS, |index| {
        run.frame(vec![SourceMsg::Type(format!("{index}"))])
    });
    vec![open, still, typing]
}

/// Hundreds of cells whose colour is retargeted every twentieth frame. The
/// frames between are interpolation over the retained tree: no declaration.
struct Wall {
    lit: bool,
}

#[derive(Clone)]
enum WallMsg {
    Retarget(bool),
}

impl App for Wall {
    type Message = WallMsg;

    fn update(&mut self, message: WallMsg, _effects: &mut Effects<'_, WallMsg>) {
        let WallMsg::Retarget(lit) = message;
        self.lit = lit;
    }

    fn view(&self, ui: &mut Ui<'_, WallMsg>) {
        ui.keyed("wall", |ui| {
            ui.row(Style::default(), |ui| {
                for index in 0..ANIMATED {
                    let shade = (index % 8) as u8 * 24;
                    ui.keyed(format!("cell{index}"), |ui| {
                        ui.label(
                            Label::new("*").style(
                                Style::default()
                                    .width(Length::Px(20.0))
                                    .background(if self.lit {
                                        Color::rgb(220, 40 + shade, 60)
                                    } else {
                                        Color::rgb(40, 60 + shade, 220)
                                    })
                                    .transition(Transition::new(300).easing(Easing::EaseInOut)),
                            ),
                        )
                    });
                }
            })
        });
    }
}

fn animation() -> Vec<Timing> {
    let mut run = Run::new(Wall { lit: true });
    let mut clock = Duration::ZERO;
    vec![measure("animation", "transitions", RUNS * 4, |index| {
        // Sixteen milliseconds a frame.
        clock += Duration::from_millis(16);
        run.context.set_now(clock);
        let messages = if index % 20 == 0 {
            vec![WallMsg::Retarget((index / 20).is_multiple_of(2))]
        } else {
            Vec::new()
        };
        run.frame(messages)
    })]
}

/// A screenful of buttons whose hover and focus the view does not read. The
/// count of clicks is the one thing the view reads.
struct Buttons {
    clicks: usize,
}

impl App for Buttons {
    type Message = usize;

    fn update(&mut self, _: usize, _: &mut Effects<'_, usize>) {
        self.clicks += 1;
    }

    fn view(&self, ui: &mut Ui<'_, usize>) {
        ui.keyed("buttons", |ui| {
            ui.column(Style::default(), |ui| {
                ui.label(format!("Clicks {}", self.clicks));
                for index in 0..ANIMATED {
                    ui.keyed(format!("button{index}"), |ui| {
                        ui.button(Button::new(format!("Button {index}")).on_click(index));
                    });
                }
            })
        });
    }
}

/// Batches with no input, and with the pointer moving over buttons nobody
/// observes: neither declares.
fn app() -> Vec<Timing> {
    let mut app = Buttons { clicks: 0 };
    let mut driver = AppDriver::new(|| true);
    let mut context = Context::new();
    let mut first = true;
    let mut frame = |app: &mut Buttons, context: &mut Context| {
        let stats = driver.frame(app, context, WINDOW, &mut MonospaceMeasurer);
        assert_eq!(stats.declared, first, "only the first batch declares");
        first = false;
        stats
    };
    vec![
        measure("app", "idle", RUNS, |_| frame(&mut app, &mut context)),
        measure("app", "hover", RUNS, |run| {
            context.event(Event::PointerMoved(Point {
                x: 40.0,
                y: (run % 20) as f32 * 30.0 + 10.0,
            }));
            frame(&mut app, &mut context)
        }),
    ]
}

/// One click: an activation that sends one message, the update it runs, and
/// the one declaration that follows.
fn click() -> Vec<Timing> {
    let mut run = Run::new(Buttons { clicks: 0 });
    run.frame(Vec::new());
    let button = run
        .context
        .find(&["buttons", "button0"])
        .expect("the first button");
    vec![measure("app", "click", RUNS, |_| {
        run.context.action(button, Action::Activate);
        run.frame(Vec::new())
    })]
}

fn main() {
    println!(
        "scenario,phase,mean_us,p95_us,shaping_us,elements,element_bytes,measures,paint_nodes,cache_hits"
    );
    for timing in [table(), tree(), text(), animation(), app(), click()]
        .into_iter()
        .flatten()
    {
        println!("{}", timing.row());
    }
}
