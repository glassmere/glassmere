//! How long glassmere takes to show a long list: first frame, one label
//! changing, and a scroll step. The row is a label, a button and a check box.
//!
//! Each measurement is a whole frame as a host runs it (messages, the view,
//! reconcile, layout, paint) plus painting the scene to pixels on the CPU.
//! The same list is built three ways, because how it is built is the cost:
//!
//! - plain: the view declares every row on every frame,
//! - memo: the rows sit in a `Ui::memo` that is skipped while they are
//!   unchanged,
//! - virtual: only the rows in view are declared.
use glassmere_core::{
    Align, App, AppDriver, Button, Context, Effects, Event, GridMsg, ItemId, Length, ListModel,
    ListState, Style, Ui, VirtualListOptions,
};
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{Color, Point, Size};
use glassmere_text::TextSystem;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::time::Instant;

const ROUNDS: usize = 20;
const SIZE: Size = Size::new(420.0, 460.0);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Plain,
    Memo,
    Virtual,
}

#[derive(Clone)]
#[allow(dead_code)]
enum Msg {
    Bump,
    Grid(GridMsg),
    Noop,
}

struct Rows(usize);

impl ListModel for Rows {
    fn len(&self) -> usize {
        self.0
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        let index = id.0 as usize;
        (index < self.0).then_some(index)
    }
    fn label(&self, index: usize) -> String {
        format!("Item {index}: some text")
    }
}

struct Perf {
    mode: Mode,
    rows: Rows,
    state: ListState,
    bumps: u32,
}

fn row(ui: &mut Ui<'_, Msg>, index: usize) {
    ui.keyed(format!("r{index}"), |ui| {
        let style = Style::default()
            .gap(8.0)
            .align_items(Align::Center)
            .height(Length::Px(36.0));
        ui.row(style, |ui| {
            ui.label(format!("Item {index}: some text"));
            ui.button(Button::new("Open"));
            ui.checkbox("Done", false, |_| Msg::Noop);
        });
    });
}

impl App for Perf {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        if let Msg::Bump = message {
            self.bumps += 1;
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let list = Style::default()
            .width(Length::Px(420.0))
            .height(Length::Px(400.0))
            .gap(0.0);
        ui.column(Style::default().gap(0.0), |ui| {
            // The one label that changes, outside the list.
            ui.label(format!("Header {}", self.bumps));
            match self.mode {
                Mode::Plain => ui.scroll(list, |ui| {
                    for index in 0..self.rows.0 {
                        row(ui, index);
                    }
                }),
                Mode::Memo => ui.scroll(list, |ui| {
                    ui.memo(&self.rows.0, Style::default().gap(0.0), |ui| {
                        for index in 0..self.rows.0 {
                            row(ui, index);
                        }
                    });
                }),
                Mode::Virtual => {
                    let options = VirtualListOptions {
                        height: 400.0,
                        row_height: 36.0,
                        style: Style::default().width(Length::Px(420.0)),
                        ..VirtualListOptions::default()
                    };
                    ui.virtual_list_with(
                        &self.rows,
                        &self.state,
                        options,
                        Msg::Grid,
                        |ui, _, index, _| row(ui, index),
                    );
                }
            }
        });
    }
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn measure(mode: Mode, rows: usize) -> Value {
    let mut app = Perf {
        mode,
        rows: Rows(rows),
        state: ListState::default(),
        bumps: 0,
    };
    let mut driver = AppDriver::new(|| true);
    let mut context = Context::new();
    let mut text = TextSystem::default();
    let mut renderer = CpuRenderer::with_text_system(TextSystem::default());
    let clear = Color::rgb(0x1b, 0x22, 0x30);
    let size = [SIZE.width as u32, SIZE.height as u32];

    let start = Instant::now();
    driver.frame(&mut app, &mut context, SIZE, &mut text);
    renderer.render(size, 1.0, context.scene(), clear);
    let first = ms(start);

    let start = Instant::now();
    for _ in 0..ROUNDS {
        driver.send(Msg::Bump);
        driver.frame(&mut app, &mut context, SIZE, &mut text);
        renderer.render(size, 1.0, context.scene(), clear);
    }
    let update = ms(start) / ROUNDS as f64;

    context.event(Event::PointerMoved(Point { x: 200.0, y: 200.0 }));
    driver.frame(&mut app, &mut context, SIZE, &mut text);
    let start = Instant::now();
    for round in 0..ROUNDS {
        context.event(Event::Scroll {
            delta_x: 0.0,
            delta_y: 120.0,
        });
        let stats = driver.frame(&mut app, &mut context, SIZE, &mut text);
        if std::env::var("PARITY_DIAG").is_ok() && round == ROUNDS - 1 {
            eprintln!(
                "  scroll frame, {rows} rows: declared={} declare={:.1?} layout={:.1?} paint={:.1?} elements={} paint_nodes={} cache_hits={} layout_passes={}",
                stats.declared,
                stats.declare,
                stats.layout,
                stats.paint,
                stats.elements,
                stats.paint_nodes,
                stats.paint_cache_hits,
                stats.layout_passes
            );
        }
        renderer.render(size, 1.0, context.scene(), clear);
    }
    let scroll = ms(start) / ROUNDS as f64;
    json!({ "first": first, "update": update, "scroll": scroll })
}

pub fn run(out: &Path) {
    let rows: Vec<usize> = std::env::var("PARITY_ROWS")
        .unwrap_or_else(|_| "100,1000,5000".into())
        .split(',')
        .filter_map(|n| n.parse().ok())
        .collect();
    let mut variants = Map::new();
    for (name, mode) in [
        ("plain", Mode::Plain),
        ("plain + Ui::memo", Mode::Memo),
        ("virtual list", Mode::Virtual),
    ] {
        let mut by_rows = Map::new();
        for &n in &rows {
            let m = measure(mode, n);
            println!(
                "glassmere ({name}): {n} rows  first {:.1} ms  update {:.2} ms  scroll {:.2} ms",
                m["first"].as_f64().unwrap(),
                m["update"].as_f64().unwrap(),
                m["scroll"].as_f64().unwrap()
            );
            by_rows.insert(n.to_string(), m);
        }
        variants.insert(name.to_string(), Value::Object(by_rows));
    }
    let result = json!({ "framework": "glassmere", "variants": variants });
    std::fs::create_dir_all(out).unwrap();
    std::fs::write(
        out.join("perf.json"),
        serde_json::to_string_pretty(&result).unwrap(),
    )
    .unwrap();
}
