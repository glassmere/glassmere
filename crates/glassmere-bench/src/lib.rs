//! The scenarios the benchmarks time and the profiler samples.
//!
//! Each is a small `App` and one step of it: a frame with the input that
//! frame would have, a paint of a scene, or a batch of text to shape. They
//! are headless, so what they measure is the framework's own work, and the
//! same step runs under `cargo bench`, where it is timed, and under the
//! `profile` binary, where it is sampled, counted or fingerprinted.
//!
//! Text is measured two ways. Most scenarios use the monospace measurer,
//! which costs nothing, so that the number is the tree's. The ones named
//! `shaped` use the real text system over the one font the workspace
//! carries, so that they run the same on every machine.
//!
//! A scenario also describes what its last step left: every node, the
//! scene and the damage, or the pixels a renderer painted. A change that
//! is meant to make a step cheaper and nothing else leaves that
//! description as it was, which is what `profile --fingerprint` checks.

use glassmere_core::{
    Action, App, AppDriver, Button, CodeEditorOptions, Context, Easing, Effects, Event, FrameStats,
    GridColumn, GridModel, GridMsg, GridState, ItemId, Key, Label, Length, ListModel, ListState,
    NodeId, SortDirection, Style, TextDocument, TextEdit, TextOp, Transition, TreeItem, TreeModel,
    Ui, VirtualListOptions,
};
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{
    Color, Image, MonospaceMeasurer, Point, Primitive, Rect, Scene, Size, TextMeasurer, TextStyle,
    TextWrap,
};
use glassmere_text::TextSystem;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::time::Duration;

/// The window every scenario runs in.
pub const WINDOW: Size = Size::new(1200.0, 800.0);

/// A text system over the workspace's own font and no system fonts.
pub fn text_system() -> TextSystem {
    TextSystem::with_fonts([include_bytes!(
        "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()])
}

/// A hash of text that is the same on every run and every machine.
#[derive(Clone, Copy)]
pub struct Fingerprint(u64);

impl Default for Fingerprint {
    fn default() -> Self {
        // FNV-1a, 64 bit.
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fingerprint {
    pub fn add(&mut self, text: &str) {
        for byte in text.as_bytes() {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
        // A boundary, so that two texts do not hash as their concatenation.
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0100_0000_01b3);
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

/// Everything a frame left that an application, a renderer or assistive
/// technology could see: every node in tree order, where the focus and the
/// pointer are, the damage, and the scene.
///
/// Identities are written as places in that order. A node's number says
/// only how many were made before it, which is not something a frame
/// promises, so two trees that differ in nothing else read the same.
pub fn describe(context: &Context) -> String {
    let mut order = Vec::new();
    let mut stack = vec![context.root()];
    while let Some(id) = stack.pop() {
        let Some(node) = context.node(id) else {
            continue;
        };
        stack.extend(node.children.iter().rev().copied());
        order.push((id, node));
    }
    let places: HashMap<NodeId, usize> = order
        .iter()
        .enumerate()
        .map(|(place, (id, _))| (*id, place))
        .collect();
    let mut out = String::new();
    for (_, node) in &order {
        renumber(&format!("{node:?}"), &places, &mut out);
        out.push('\n');
    }
    renumber(
        &format!(
            "focused {:?} hovered {:?}",
            context.focused(),
            context.hovered()
        ),
        &places,
        &mut out,
    );
    let _ = write!(
        out,
        "\ndamage {:?}\n{:?}\n",
        context.damage_regions(),
        context.scene()
    );
    out
}

/// Copies `text` into `out` with each `NodeId(n)` written as the node's place.
fn renumber(text: &str, places: &HashMap<NodeId, usize>, out: &mut String) {
    const OPEN: &str = "NodeId(";
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        let digits = at + OPEN.len();
        out.push_str(&rest[..digits]);
        let tail = &rest[digits..];
        let end = tail.find(')').unwrap_or(tail.len());
        let place = tail[..end]
            .parse::<u64>()
            .ok()
            .and_then(|id| places.get(&NodeId(id)));
        match place {
            Some(place) => {
                let _ = write!(out, "#{place}");
            }
            // An identity that is not in the tree.
            None => out.push('?'),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
}

/// One scenario under way.
pub trait Step {
    /// What a benchmark times and a profiler samples.
    fn step(&mut self);

    /// What the last step left, as text that is the same whenever the
    /// output is. It is not part of what `step` costs.
    fn describe(&mut self) -> String;
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

    /// A press and a release in the middle of `rect`.
    pub fn click(&mut self, rect: Rect) {
        self.context.event(Event::PointerMoved(Point {
            x: rect.x + rect.width / 2.0,
            y: rect.y + rect.height / 2.0,
        }));
        self.context.event(Event::PointerDown);
        self.context.event(Event::PointerUp);
    }

    pub fn key(&mut self, key: Key) {
        self.context.event(Event::KeyDown {
            key,
            shift: false,
            repeat: false,
        });
    }
}

/// A scenario that is frames of an application.
struct Frames<A: App, T: TextMeasurer, F: FnMut(&mut Run<A, T>)> {
    run: Run<A, T>,
    each: F,
}

impl<A: App, T: TextMeasurer, F: FnMut(&mut Run<A, T>)> Step for Frames<A, T, F> {
    fn step(&mut self) {
        (self.each)(&mut self.run);
    }

    fn describe(&mut self) -> String {
        describe(&self.run.context)
    }
}

fn frames<A: App + 'static, T: TextMeasurer + 'static>(
    app: A,
    measurer: T,
    each: impl FnMut(&mut Run<A, T>) + 'static,
) -> Box<dyn Step> {
    Box::new(Frames {
        run: Run::new(app, measurer),
        each,
    })
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
    /// The text of one row, a different one each frame.
    OneRow,
}

/// How many rows of the form fit the window, for the scenarios that act on
/// rows that can be seen.
const SHOWN: usize = 10;

/// Rows of a label, a field, a checkbox and a button, as a settings page or
/// a long form has them. Unless `memo` is set, `Ui::memo` is deliberately
/// not used: this is what the tree costs when the view declares all of it.
pub struct Form {
    pub rows: usize,
    pub change: Change,
    /// Each row is declared through `Ui::memo`, keyed on what it shows.
    pub memo: bool,
    tick: usize,
    names: Vec<String>,
    checks: Vec<bool>,
    presses: usize,
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
            memo: false,
            tick: 0,
            names: (0..rows).map(|row| format!("name {row}")).collect(),
            checks: (0..rows).map(|row| row % 2 == 0).collect(),
            presses: 0,
        }
    }

    fn row(&self, ui: &mut Ui<'_, FormMsg>, row: usize) {
        let tick = self.tick;
        let text = match self.change {
            Change::Texts => format!("Row {row} at {}", tick + row),
            Change::OneRow if row == tick % SHOWN => format!("Row {row} at {tick}"),
            _ => format!("Row {row}"),
        };
        let mut style = Style::default().gap(8.0);
        if self.change == Change::Colours {
            let shade = ((tick + row) % 2 * 20) as u8;
            style = style.background(Color::rgb(40 + shade, 44, 52));
        }
        ui.row(style, |ui| {
            ui.label(Label::new(text).style(Style::default().width(Length::Px(220.0))));
            ui.text_input("Name", &self.names[row], move |name| {
                FormMsg::Name(row, name)
            });
            ui.checkbox("Enabled", self.checks[row], move |on| {
                FormMsg::Check(row, on)
            });
            ui.button(Button::new("Apply").on_click(FormMsg::Press(row)));
        });
    }
}

impl App for Form {
    type Message = FormMsg;

    fn update(&mut self, message: FormMsg, _: &mut Effects<'_, FormMsg>) {
        match message {
            FormMsg::Tick => self.tick += 1,
            FormMsg::Name(row, name) => self.names[row] = name,
            FormMsg::Check(row, on) => self.checks[row] = on,
            FormMsg::Press(_) => self.presses += 1,
        }
    }

    fn view(&self, ui: &mut Ui<'_, FormMsg>) {
        ui.keyed("form", |ui| {
            ui.scroll(
                Style::default()
                    .width(Length::Percent(1.0))
                    .height(Length::Percent(1.0)),
                |ui| {
                    ui.label(format!("Frame {}, {} presses", self.tick, self.presses));
                    for row in 0..self.rows {
                        ui.keyed(row.to_string(), |ui| {
                            if self.memo {
                                let shown = (row, &self.names[row], self.checks[row]);
                                ui.memo(&shown, Style::default(), |ui| self.row(ui, row));
                            } else {
                                self.row(ui, row);
                            }
                        });
                    }
                },
            );
        });
    }
}

/// The box of one part of a row of the form: 0 the label, 1 the field,
/// 2 the checkbox, 3 the button.
fn part(context: &Context, row: usize, part: usize) -> Rect {
    let row = context
        .find(&["form", &row.to_string()])
        .expect("the row is declared");
    let id = context.node(row).expect("the row").children[part];
    context.node(id).expect("the part").bounds
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

/// A combo box over thirty choices, on a page with something else on it.
pub struct Picker {
    selected: usize,
    items: Vec<String>,
}

#[derive(Clone)]
pub enum PickerMsg {
    Choose(usize),
}

impl Picker {
    pub fn new() -> Self {
        Self {
            selected: 0,
            items: (0..30).map(|item| format!("Choice {item}")).collect(),
        }
    }
}

impl Default for Picker {
    fn default() -> Self {
        Self::new()
    }
}

impl App for Picker {
    type Message = PickerMsg;

    fn update(&mut self, message: PickerMsg, _: &mut Effects<'_, PickerMsg>) {
        let PickerMsg::Choose(item) = message;
        self.selected = item;
    }

    fn view(&self, ui: &mut Ui<'_, PickerMsg>) {
        let items: Vec<&str> = self.items.iter().map(String::as_str).collect();
        ui.keyed("page", |ui| {
            ui.column(Style::default().padding(24.0), |ui| {
                ui.label(format!("Chosen: {}", self.items[self.selected]));
                ui.keyed("pick", |ui| {
                    ui.combo_box("Choice", self.selected, &items, PickerMsg::Choose)
                });
                for line in 0..40 {
                    ui.label(format!("Line {line} of the page under the popup"));
                }
            });
        });
    }
}

const CELLS: usize = 400;

/// Hundreds of cells whose colour is retargeted every twentieth frame. The
/// frames between are interpolation over the retained tree.
pub struct Wall {
    lit: bool,
}

#[derive(Clone)]
pub enum WallMsg {
    Retarget(bool),
}

impl App for Wall {
    type Message = WallMsg;

    fn update(&mut self, message: WallMsg, _: &mut Effects<'_, WallMsg>) {
        let WallMsg::Retarget(lit) = message;
        self.lit = lit;
    }

    fn view(&self, ui: &mut Ui<'_, WallMsg>) {
        ui.keyed("wall", |ui| {
            ui.row(Style::default(), |ui| {
                for index in 0..CELLS {
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

/// Paragraphs that wrap to the width of the window.
pub struct Article {
    paragraphs: Vec<String>,
}

impl Article {
    pub fn new() -> Self {
        let sentence = "The quick brown fox jumps over the lazy dog, and the dog, \
                        being lazy, lets it; ";
        Self {
            paragraphs: (0..40)
                .map(|paragraph| format!("Paragraph {paragraph}. {}", sentence.repeat(5)))
                .collect(),
        }
    }
}

impl Default for Article {
    fn default() -> Self {
        Self::new()
    }
}

impl App for Article {
    type Message = ();

    fn update(&mut self, (): (), _: &mut Effects<'_, ()>) {}

    fn view(&self, ui: &mut Ui<'_, ()>) {
        ui.keyed("article", |ui| {
            ui.scroll(
                Style::default()
                    .width(Length::Percent(1.0))
                    .height(Length::Percent(1.0))
                    .padding(16.0),
                |ui| {
                    for paragraph in &self.paragraphs {
                        ui.label(
                            Label::new(paragraph.as_str()).style(
                                Style::default()
                                    .width(Length::Percent(1.0))
                                    .wrap(TextWrap::Word),
                            ),
                        );
                    }
                },
            );
        });
    }
}

/// A scenario that is the CPU renderer painting what a form declared.
struct Paints {
    run: Run<Form, MonospaceMeasurer>,
    renderer: CpuRenderer,
    /// What to paint: the whole frame, one rectangle of a frame that does
    /// not change, or a new frame and the damage it reports.
    mode: Paint,
}

#[derive(Clone, Copy, PartialEq)]
enum Paint {
    Whole,
    Rectangle(Rect),
    Frame,
}

const CLEAR: Color = Color::rgb(24, 26, 31);

impl Paints {
    fn new(change: Change, mode: Paint) -> Self {
        let run = Run::new(Form::new(300, change), MonospaceMeasurer);
        let mut paints = Self {
            run,
            renderer: CpuRenderer::with_text_system(text_system()),
            mode,
        };
        // A frame painted whole, for the ones painted over it.
        paints.paint(&[]);
        paints
    }

    fn paint(&mut self, damage: &[Rect]) {
        let size = [WINDOW.width as u32, WINDOW.height as u32];
        let pixels =
            self.renderer
                .render_damaged(size, 1.0, self.run.context.scene(), CLEAR, damage);
        std::hint::black_box(pixels.len());
    }
}

impl Step for Paints {
    fn step(&mut self) {
        match self.mode {
            Paint::Whole => self.paint(&[]),
            Paint::Rectangle(rect) => self.paint(&[rect]),
            Paint::Frame => {
                self.run.frame([FormMsg::Tick]);
                let damage = self.run.context.damage_regions().to_vec();
                self.paint(&damage);
            }
        }
    }

    fn describe(&mut self) -> String {
        // Painted again with nothing damaged, which changes no pixel of a
        // frame that was painted right and gives them back to be read.
        let size = [WINDOW.width as u32, WINDOW.height as u32];
        let scene = self.run.context.scene().clone();
        let nothing = [Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }];
        let pixels = self
            .renderer
            .render_damaged(size, 1.0, &scene, CLEAR, &nothing);
        format!(
            "pixels {:016x}\n{}",
            hash_pixels(pixels, size[0] as usize),
            describe(&self.run.context)
        )
    }
}

/// A hash of RGBA8 pixels that is `width` wide, a row at a time.
fn hash_pixels(pixels: &[u8], width: usize) -> u64 {
    let mut hash = Fingerprint::default();
    for row in pixels.chunks(width * 4) {
        let mut line = String::with_capacity(row.len() * 2);
        for byte in row {
            let _ = write!(line, "{byte:02x}");
        }
        hash.add(&line);
    }
    hash.value()
}

/// A photo of 2000 by 1500 texels, a gradient with a few translucent ones
/// scattered over it. Drawn at 150 by 100 it is halved three times, and the
/// last of them from an odd number of rows.
fn photo() -> Image {
    const WIDTH: u32 = 2000;
    const HEIGHT: u32 = 1500;
    let mut rgba = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let alpha = if (x / 97 + y / 61) % 29 == 0 {
                140
            } else {
                255
            };
            rgba.extend([
                (x * 255 / WIDTH) as u8,
                (y * 255 / HEIGHT) as u8,
                ((x ^ y) & 255) as u8,
                alpha,
            ]);
        }
    }
    Image::from_rgba8(WIDTH, HEIGHT, rgba).expect("the buffer holds every texel")
}

/// Where the photo is drawn, and the quad that sits beside it.
const THUMBNAIL: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 150.0,
    height: 100.0,
};
const ELSEWHERE: Rect = Rect {
    x: 200.0,
    y: 20.0,
    width: 60.0,
    height: 60.0,
};

/// How a scenario of the photo is drawn.
#[derive(Clone, Copy)]
enum Visit {
    /// By a renderer that has not seen it, on every step.
    Cold,
    /// Again after frames that did not draw it, as a hover does.
    Revisit,
}

/// A scenario that is the CPU renderer drawing a large photo small.
struct Photo {
    scene: Scene,
    renderer: CpuRenderer,
    visit: Visit,
    size: [u32; 2],
}

impl Photo {
    fn new(visit: Visit) -> Self {
        let mut primitives = vec![Primitive::Image {
            rect: THUMBNAIL,
            image: photo(),
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: 2000.0,
                height: 1500.0,
            },
            tint: None,
            radius: 0.0,
        }];
        // Something to damage that is not the photo, which needs room beside it.
        let size = match visit {
            Visit::Cold => [150, 100],
            Visit::Revisit => {
                primitives.push(Primitive::Quad {
                    rect: ELSEWHERE,
                    color: Color::rgb(200, 80, 40),
                    radius: 0.0,
                });
                [320, 100]
            }
        };
        let scene = Scene {
            size: Size::new(size[0] as f32, size[1] as f32),
            primitives,
        };
        let mut paints = Self {
            scene,
            renderer: CpuRenderer::with_text_system(text_system()),
            visit,
            size,
        };
        // Drawn once, so that a revisit finds the photo as an application would.
        paints.paint(&[]);
        paints
    }

    fn paint(&mut self, damage: &[Rect]) {
        let pixels = self
            .renderer
            .render_damaged(self.size, 1.0, &self.scene, CLEAR, damage);
        std::hint::black_box(pixels.len());
    }
}

impl Step for Photo {
    fn step(&mut self) {
        match self.visit {
            Visit::Cold => {
                self.renderer = CpuRenderer::with_text_system(text_system());
                self.paint(&[]);
            }
            Visit::Revisit => {
                // Two frames that leave the photo alone, then the one that
                // draws it.
                self.paint(&[ELSEWHERE]);
                self.paint(&[ELSEWHERE]);
                self.paint(&[THUMBNAIL]);
            }
        }
    }

    fn describe(&mut self) -> String {
        // Painted again with nothing damaged, as `Paints` does, to read the
        // pixels the last step left.
        let nothing = [Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }];
        let pixels = self
            .renderer
            .render_damaged(self.size, 1.0, &self.scene, CLEAR, &nothing);
        format!("pixels {:016x}", hash_pixels(pixels, self.size[0] as usize))
    }
}

/// A scenario that is lines of text being shaped.
struct Shapes {
    text: TextSystem,
    lines: Vec<String>,
    /// Shaped for the first time on every step.
    cold: bool,
    round: usize,
}

impl Step for Shapes {
    fn step(&mut self) {
        if self.cold {
            self.round += 1;
            for (line, text) in self.lines.iter_mut().enumerate() {
                *text = format!("Line {line} of round {}, shaped", self.round);
            }
        }
        for line in &self.lines {
            std::hint::black_box(self.text.ensure(line, 14.0));
        }
        if self.cold {
            // Nothing is kept, so the next round shapes again.
            self.text.retain(&HashSet::new());
        }
    }

    fn describe(&mut self) -> String {
        let style = TextStyle {
            font_size: 14.0,
            ..TextStyle::default()
        };
        let mut out = String::new();
        for line in &self.lines {
            let size = self.text.measure_styled(line, &style, None);
            let _ = writeln!(out, "{line}: {size:?}");
        }
        out
    }
}

/// One scenario: its name, what it measures, and how it starts.
pub struct Scenario {
    pub name: &'static str,
    pub about: &'static str,
    /// Builds the scenario, which is not measured.
    pub build: fn() -> Box<dyn Step>,
}

fn form(change: Change) -> Box<dyn Step> {
    frames(Form::new(300, change), MonospaceMeasurer, |run| {
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
                frames(Form::new(300, Change::Heading), MonospaceMeasurer, |run| {
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
            name: "form/memo",
            about: "the same, each row behind Ui::memo: what skipping a subtree costs",
            build: || {
                let mut form = Form::new(300, Change::Heading);
                form.memo = true;
                frames(form, MonospaceMeasurer, |run| {
                    run.frame([FormMsg::Tick]);
                })
            },
        },
        Scenario {
            name: "form/repaint",
            about: "every row a new colour: declare, reconcile and paint",
            build: || form(Change::Colours),
        },
        Scenario {
            name: "form/relayout",
            about: "every row a new text: measure, layout and paint as well",
            build: || form(Change::Texts),
        },
        Scenario {
            name: "form/one-row",
            about: "the text of one row changes, as most frames of an application go",
            build: || form(Change::OneRow),
        },
        Scenario {
            name: "form/resize",
            about: "the same form in a window that changes width every frame",
            build: || {
                let mut wide = false;
                frames(
                    Form::new(300, Change::Heading),
                    MonospaceMeasurer,
                    move |run| {
                        wide = !wide;
                        run.size = Size::new(if wide { 1200.0 } else { 1000.0 }, 800.0);
                        run.frame([]);
                    },
                )
            },
        },
        Scenario {
            name: "form/hover",
            about: "the pointer moves to another widget: a hit test and two repaints",
            build: || {
                let mut at = 0usize;
                frames(
                    Form::new(300, Change::Heading),
                    MonospaceMeasurer,
                    move |run| {
                        // The field, the checkbox and the button of one row
                        // after another.
                        at += 1;
                        let over = part(&run.context, at / 3 % SHOWN, 1 + at % 3);
                        run.context.event(Event::PointerMoved(Point {
                            x: over.x + over.width / 2.0,
                            y: over.y + over.height / 2.0,
                        }));
                        run.frame([]);
                    },
                )
            },
        },
        Scenario {
            name: "form/scroll",
            about: "the wheel scrolls the form: every box moves, nothing is declared",
            build: || {
                let mut at = 0usize;
                frames(
                    Form::new(300, Change::Heading),
                    MonospaceMeasurer,
                    move |run| {
                        if at == 0 {
                            run.context
                                .event(Event::PointerMoved(Point { x: 600.0, y: 400.0 }));
                        }
                        // Down sixteen notches, then back up them.
                        let down = (at / 16).is_multiple_of(2);
                        at += 1;
                        run.context.event(Event::Scroll {
                            delta_x: 0.0,
                            delta_y: if down { 120.0 } else { -120.0 },
                        });
                        run.frame([]);
                    },
                )
            },
        },
        Scenario {
            name: "form/interact",
            about: "a checkbox clicked, a field focused, typed in and erased, Tab, a button",
            build: || {
                let mut at = 0usize;
                frames(
                    Form::new(300, Change::Heading),
                    MonospaceMeasurer,
                    move |run| {
                        let row = at / 6 % 8;
                        match at % 6 {
                            0 => run.click(part(&run.context, row, 2)),
                            1 => run.click(part(&run.context, row, 1)),
                            2 => {
                                run.context.event(Event::Text(TextEdit::Insert("a".into())));
                            }
                            3 => {
                                run.context
                                    .event(Event::Text(TextEdit::Backspace { word: false }));
                            }
                            4 => run.key(Key::Tab),
                            _ => run.click(part(&run.context, row, 3)),
                        }
                        at += 1;
                        run.frame([]);
                    },
                )
            },
        },
        Scenario {
            name: "form/relayout-shaped",
            about: "form/relayout with real text shaping in place of the monospace measurer",
            build: || {
                frames(Form::new(300, Change::Texts), text_system(), |run| {
                    run.frame([FormMsg::Tick]);
                })
            },
        },
        Scenario {
            name: "combo/navigate",
            about: "a combo box opened, three arrow keys, Enter, and a frame after",
            build: || {
                let mut at = 0usize;
                frames(Picker::new(), MonospaceMeasurer, move |run| {
                    match at % 6 {
                        0 => {
                            let combo = run.context.find(&["page", "pick"]).expect("the combo box");
                            run.context.action(combo, Action::Expand);
                        }
                        1..=3 => run.key(Key::ArrowDown),
                        4 => run.key(Key::Enter),
                        _ => {}
                    }
                    at += 1;
                    run.frame([]);
                })
            },
        },
        Scenario {
            name: "animation/transitions",
            about: "400 cells easing to a colour that changes every twentieth frame",
            build: || {
                let mut at = 0u64;
                frames(Wall { lit: true }, MonospaceMeasurer, move |run| {
                    at += 1;
                    // Sixteen milliseconds a frame.
                    run.context.set_now(Duration::from_millis(at * 16));
                    if at.is_multiple_of(20) {
                        run.frame([WallMsg::Retarget((at / 20).is_multiple_of(2))]);
                    } else {
                        run.frame([]);
                    }
                })
            },
        },
        Scenario {
            name: "table/scroll",
            about: "a grid of 1,000,000 rows scrolled ten rows a frame",
            build: || {
                let mut row = 0;
                frames(Table::new(), MonospaceMeasurer, move |run| {
                    row = (row + 10) % (ROWS - 100);
                    run.frame([TableMsg::Reveal(row)]);
                })
            },
        },
        Scenario {
            name: "tree/scroll",
            about: "a tree of 100,000 expanded nodes scrolled ten rows a frame",
            build: || {
                let mut row = 0;
                frames(Outline::new(), MonospaceMeasurer, move |run| {
                    row = (row + 10) % (NODES - 100);
                    run.frame([OutlineMsg::Reveal(row)]);
                })
            },
        },
        Scenario {
            name: "editor/type",
            about: "a character typed into a 40,000-line document, or erased from it",
            build: || {
                let mut typed = false;
                frames(Source::new(40_000), MonospaceMeasurer, move |run| {
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
            name: "text/wrap-resize",
            about: "40 wrapped paragraphs in a window dragged narrower and wider, with real shaping",
            build: || {
                let mut at = 0usize;
                frames(Article::new(), text_system(), move |run| {
                    // Thirty widths eight pixels apart, there and back, as
                    // a window being resized passes through them.
                    at += 1;
                    let along = at % 60;
                    let notch = if along < 30 { along } else { 60 - along };
                    run.size = Size::new(900.0 + notch as f32 * 8.0, 800.0);
                    run.frame([]);
                })
            },
        },
        Scenario {
            name: "render-cpu/full",
            about: "the CPU renderer painting the whole form at 1200 by 800",
            build: || Box::new(Paints::new(Change::Colours, Paint::Whole)),
        },
        Scenario {
            name: "render-cpu/damage",
            about: "the same scene with one row's rectangle damaged",
            build: || {
                Box::new(Paints::new(
                    Change::Colours,
                    Paint::Rectangle(Rect {
                        x: 0.0,
                        y: 200.0,
                        width: WINDOW.width,
                        height: 48.0,
                    }),
                ))
            },
        },
        Scenario {
            name: "render-cpu/photo-cold",
            about: "a 3 MP photo drawn at thumbnail size by a renderer that has not seen it: \
                    the filtering on the render thread",
            build: || Box::new(Photo::new(Visit::Cold)),
        },
        Scenario {
            name: "render-cpu/photo-revisit",
            about: "the same photo repainted by a hover after frames that did not draw it",
            build: || Box::new(Photo::new(Visit::Revisit)),
        },
        Scenario {
            name: "pipeline/one-row",
            about: "one row's text changes and the CPU renderer paints the damage: \
                    a message to pixels",
            build: || Box::new(Paints::new(Change::OneRow, Paint::Frame)),
        },
        Scenario {
            name: "text/shape-cold",
            about: "100 lines of text shaped for the first time",
            build: || {
                Box::new(Shapes {
                    text: text_system(),
                    lines: vec![String::new(); 100],
                    cold: true,
                    round: 0,
                })
            },
        },
        Scenario {
            name: "text/shape-warm",
            about: "the same 100 lines asked for again: the cache alone",
            build: || {
                Box::new(Shapes {
                    text: text_system(),
                    lines: (0..100)
                        .map(|line| format!("Line {line} of the same round, shaped"))
                        .collect(),
                    cold: false,
                    round: 0,
                })
            },
        },
    ]
}
