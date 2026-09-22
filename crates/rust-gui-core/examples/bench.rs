//! The four benchmarks the feature list names: a million-row table, a
//! hundred-thousand-node tree, a multi-megabyte text file and hundreds of
//! animated elements. Headless, so the numbers are the framework's own work
//! without a GPU, a font stack or a compositor in them.
//!
//! ```sh
//! cargo run -p rust-gui-core --example bench --release
//! ```
use rust_gui_core::{
    CodeEditorOptions, Context, Easing, FrameStats, GridColumn, GridModel, GridState, ItemId,
    Length, ListModel, ListState, MonospaceMeasurer, SortDirection, Style, TextDocument,
    Transition, TreeItem, TreeModel, VirtualListOptions,
};
use rust_gui_scene::{Color, Size};
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

fn table() -> Vec<Timing> {
    let mut model = Rows {
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
    let mut state = GridState::default();
    let mut context = Context::new();
    let mut frame = |row: usize, model: &mut Rows, state: &mut GridState| {
        state.rows.reveal(ItemId(row as u64));
        context.run(WINDOW, &mut MonospaceMeasurer, |ui| {
            ui.data_grid("grid", model, state, viewport());
        })
    };
    vec![
        measure("table", "still", RUNS, |_| frame(0, &mut model, &mut state)),
        measure("table", "scrolling", RUNS, |run| {
            frame(run * 10, &mut model, &mut state)
        }),
        // Jumping about the million rows, where a table that realized them
        // all would already have given up.
        measure("table", "far", RUNS, |run| {
            frame(run * 97_003 % ROWS, &mut model, &mut state)
        }),
    ]
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
    let mut state = ListState::default();
    let mut context = Context::new();
    let mut frame = |row: usize, model: &mut TreeModel, state: &mut ListState| {
        state.reveal(ItemId(row as u64));
        context.run(WINDOW, &mut MonospaceMeasurer, |ui| {
            ui.tree_view("tree", model, state, viewport());
        })
    };
    vec![
        measure("tree", "expanded", RUNS, |_| {
            frame(0, &mut model, &mut state)
        }),
        measure("tree", "scrolling", RUNS, |run| {
            frame(run * 10, &mut model, &mut state)
        }),
        measure("tree", "collapse", RUNS, |run| {
            model.set_expanded(ItemId(0), run % 2 == 0);
            frame(0, &mut model, &mut state)
        }),
    ]
}

fn text() -> Vec<Timing> {
    let line = "let value = compute(index, offset) + adjust(previous, 3.14159); // a line";
    let source: String = (0..LINES)
        .map(|index| format!("{line} {index}\n"))
        .collect();
    println!("# text document: {} bytes", source.len());
    let mut document = TextDocument::from_text(&source);
    let mut context = Context::new();
    let mut frame = |document: &mut TextDocument| {
        context.run(WINDOW, &mut MonospaceMeasurer, |ui| {
            ui.code_editor(
                "editor",
                document,
                CodeEditorOptions {
                    line_numbers: true,
                    ..CodeEditorOptions::default()
                },
            );
        })
    };
    let open = measure("text", "open", 1, |_| frame(&mut document));
    let still = measure("text", "still", RUNS, |_| frame(&mut document));
    let typing = measure("text", "typing", RUNS, |run| {
        document.insert(&format!("{run}"));
        frame(&mut document)
    });
    vec![open, still, typing]
}

fn animation() -> Vec<Timing> {
    let mut context = Context::new();
    let mut clock = Duration::ZERO;
    let frame = |run: usize, context: &mut Context, clock: &mut Duration| {
        // Sixteen milliseconds a frame, with the colour retargeted every
        // twentieth, so most frames are the interpolation itself.
        *clock += Duration::from_millis(16);
        context.set_now(*clock);
        let lit = (run / 20).is_multiple_of(2);
        context.run(WINDOW, &mut MonospaceMeasurer, |ui| {
            ui.row("wall", Style::default(), |ui| {
                for index in 0..ANIMATED {
                    let shade = (index % 8) as u8 * 24;
                    ui.label_styled(
                        format!("cell{index}"),
                        "*",
                        Style::default()
                            .width(Length::Px(20.0))
                            .background(if lit {
                                Color::rgb(220, 40 + shade, 60)
                            } else {
                                Color::rgb(40, 60 + shade, 220)
                            })
                            .transition(Transition::new(300).easing(Easing::EaseInOut)),
                    );
                }
            });
        })
    };
    vec![measure("animation", "transitions", RUNS * 4, |run| {
        frame(run, &mut context, &mut clock)
    })]
}

fn main() {
    println!(
        "scenario,phase,mean_us,p95_us,shaping_us,elements,element_bytes,measures,paint_nodes,cache_hits"
    );
    for timing in [table(), tree(), text(), animation()].into_iter().flatten() {
        println!("{}", timing.row());
    }
}
