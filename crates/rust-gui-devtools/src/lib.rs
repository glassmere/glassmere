//! Inspector model and views shared by hosts; no platform dependencies.
//!
//! A host records per-window frames and events in a [`Recorder`], captures an
//! [`InspectorModel`] from the inspected window's context before declaring the
//! inspector, then applies the [`InspectorResponse`] and [`Inspector::overlay`]
//! back to the inspected window.
use rust_gui_core::{Button, Label};
use rust_gui_core::{
    Context, FontFamily, FrameStats, GraphNode, GridMsg, ItemId, Length, ListModel, ListState,
    MemoryReport, NodeDetails, NodeId, NodeSnapshot, StateKind, Style, TextWrap, Ui,
    VirtualListOptions,
};
use rust_gui_scene::{Affine, Color, Primitive, Rect, Size};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

/// One presented frame: core phase timings plus the host's render and total time.
#[derive(Clone, Copy, Debug)]
pub struct FrameRecord {
    pub stats: FrameStats,
    pub render: Duration,
    pub total: Duration,
    /// Time the GPU spent on this frame. It arrives a few frames late, and
    /// never on adapters without timestamp queries.
    pub gpu: Option<Duration>,
    pub draw_calls: u32,
    /// Bytes the renderer had allocated on the GPU after this frame.
    pub gpu_memory: u64,
}

/// What the renderer reports about the GPU, as of the latest frame. Plain
/// data, so that the inspector does not depend on a renderer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuSnapshot {
    /// Adapter name and backend, such as `NVIDIA GeForce RTX 4070 (Dx12)`.
    pub adapter: String,
    /// The window is painted without a graphics adapter, so the rest of this
    /// says nothing and the tab reports the fallback instead.
    pub software: bool,
    /// Whether the adapter can time passes. Without it only the CPU's render
    /// time is known, which measures the driver's queue, not the GPU.
    pub timestamps: bool,
    /// The most recent frame whose GPU times have arrived, by pass.
    pub passes: Vec<(String, Duration)>,
    pub draw_calls: u32,
    pub quads: u32,
    pub text_batches: u32,
    pub render_passes: u32,
    /// Bytes by category, as the renderer accounts for its own allocations.
    pub memory: Vec<(String, u64)>,
    pub memory_peak: u64,
    /// The driver's allocator, for the whole device: allocated and reserved
    /// bytes. Only some backends report it.
    pub driver: Option<(u64, u64)>,
}

impl GpuSnapshot {
    pub fn memory_total(&self) -> u64 {
        self.memory.iter().map(|(_, bytes)| bytes).sum()
    }
}

impl FrameRecord {
    pub const CSV_HEADER: &'static str = "frame,declare_us,layout_us,paint_us,shaping_us,render_us,total_us,created,reused,removed,elements,element_bytes,text_measures,layout_passes,paint_nodes,paint_cache_hits,scene_rebuilt,dirty,gpu_us,draw_calls,gpu_memory_bytes";

    pub fn csv_row(&self) -> String {
        let s = &self.stats;
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:?},{},{},{}",
            s.frame,
            s.declare.as_micros(),
            s.layout.as_micros(),
            s.paint.as_micros(),
            s.shaping.as_micros(),
            self.render.as_micros(),
            self.total.as_micros(),
            s.created,
            s.reused,
            s.removed,
            s.elements,
            s.element_bytes,
            s.text_measures,
            s.layout_passes,
            s.paint_nodes,
            s.paint_cache_hits,
            s.scene_rebuilt,
            s.dirty,
            // Empty when the GPU time is not known, rather than a zero that
            // would read as a measurement.
            self.gpu
                .map_or_else(String::new, |gpu| gpu.as_micros().to_string()),
            self.draw_calls,
            self.gpu_memory
        )
    }

    /// Column titles for [`FrameRecord::summary`] rows.
    const SUMMARY_HEADER: &'static str = "frame   declare  layout   paint  shaping  render     gpu   total ms   +new ~reused -gone  painted   dirty";

    fn summary(&self) -> String {
        let s = &self.stats;
        format!(
            "{:<7} {:>7.2} {:>7.2} {:>7.2} {:>7.2} {:>7.2} {:>7} {:>7.2}    {:>4} {:>7} {:>5}  {:>3}/{:<4} {}",
            s.frame,
            millis(s.declare),
            millis(s.layout),
            millis(s.paint),
            millis(s.shaping),
            millis(self.render),
            self.gpu
                .map_or_else(|| "-".to_owned(), |gpu| format!("{:.2}", millis(gpu))),
            millis(self.total),
            s.created,
            s.reused,
            s.removed,
            s.paint_nodes,
            s.paint_nodes + s.paint_cache_hits,
            dirty_names(s.dirty)
        )
    }
}

fn dirty_names(dirty: rust_gui_core::Dirty) -> String {
    if dirty.is_empty() {
        return "-".into();
    }
    dirty
        .iter_names()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("|")
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[derive(Clone, Debug)]
pub struct EventRecord {
    pub sequence: u64,
    pub frame: u64,
    pub description: String,
    pub handled: bool,
}

/// Bounded per-window history. Recording costs one small allocation per event.
pub struct Recorder {
    frames: VecDeque<FrameRecord>,
    events: VecDeque<EventRecord>,
    capacity: usize,
    next_event: u64,
    gpu: Option<GpuSnapshot>,
}

impl Recorder {
    pub fn new(capacity: usize) -> Self {
        debug_assert!(capacity > 0, "recorder capacity must be positive");
        let capacity = capacity.max(1);
        Self {
            frames: VecDeque::with_capacity(capacity),
            events: VecDeque::with_capacity(capacity),
            capacity,
            next_event: 0,
            gpu: None,
        }
    }

    pub fn frame(&mut self, stats: FrameStats, render: Duration, total: Duration) {
        if self.frames.len() == self.capacity {
            self.frames.pop_front();
        }
        self.frames.push_back(FrameRecord {
            stats,
            render,
            total,
            gpu: None,
            draw_calls: 0,
            gpu_memory: 0,
        });
    }

    /// The renderer's report after the frame just recorded. `completed` is a
    /// GPU time that has arrived now for an earlier frame, by its number.
    pub fn gpu(&mut self, snapshot: GpuSnapshot, completed: Option<(u64, Duration)>) {
        if let Some(last) = self.frames.back_mut() {
            last.draw_calls = snapshot.draw_calls;
            last.gpu_memory = snapshot.memory_total();
        }
        if let Some((frame, time)) = completed
            && let Some(record) = self
                .frames
                .iter_mut()
                .rev()
                .find(|record| record.stats.frame == frame)
        {
            record.gpu = Some(time);
        }
        self.gpu = Some(snapshot);
    }

    pub fn gpu_snapshot(&self) -> Option<&GpuSnapshot> {
        self.gpu.as_ref()
    }

    pub fn event(&mut self, frame: u64, description: String, handled: bool) {
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(EventRecord {
            sequence: self.next_event,
            frame,
            description,
            handled,
        });
        self.next_event += 1;
    }

    pub fn frames(&self) -> &VecDeque<FrameRecord> {
        &self.frames
    }

    pub fn events(&self) -> &VecDeque<EventRecord> {
        &self.events
    }

    pub fn csv(&self) -> String {
        let mut out = String::from(FrameRecord::CSV_HEADER);
        out.push('\n');
        for frame in &self.frames {
            out.push_str(&frame.csv_row());
            out.push('\n');
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct InspectedNode {
    pub snapshot: NodeSnapshot,
    pub details: NodeDetails,
    pub depth: usize,
}

/// A copy of an inspected window's state, refreshed by the host before the
/// inspector declares. Capturing is linear in the node count.
#[derive(Clone, Debug)]
pub struct InspectorModel {
    pub title: String,
    /// Logical size of the inspected window.
    pub size: Size,
    /// Logical size of the inspector's own window, for list heights.
    pub viewport: Size,
    pub root: NodeId,
    /// Parent-first order, matching `Context::snapshots`.
    pub nodes: Vec<InspectedNode>,
    index: HashMap<NodeId, usize>,
    pub focused: Option<NodeId>,
    pub hovered: Option<NodeId>,
    pub damage: Vec<Rect>,
    pub frames: Vec<FrameRecord>,
    pub events: Vec<EventRecord>,
    /// Where the last CSV export was written, if any.
    pub export_path: Option<String>,
    /// Unknown style classes requested by the inspected window.
    pub warnings: Vec<String>,
    /// Framework diagnostics of the inspected window.
    pub diagnostics: Vec<rust_gui_core::FrameDiagnostic>,
    /// The renderer's latest report, when the host supplies one.
    pub gpu: Option<GpuSnapshot>,
    /// The reactive store the window observed, if it observed one.
    pub state: Vec<GraphNode>,
    /// What the window is holding on the CPU.
    pub memory: MemoryReport,
    /// The layers of the last scene.
    pub layers: Vec<LayerRow>,
}

impl Default for InspectorModel {
    fn default() -> Self {
        Self {
            title: String::new(),
            size: Size::default(),
            viewport: Size::new(760.0, 820.0),
            root: NodeId(0),
            nodes: Vec::new(),
            index: HashMap::new(),
            focused: None,
            hovered: None,
            damage: Vec::new(),
            frames: Vec::new(),
            events: Vec::new(),
            export_path: None,
            warnings: Vec::new(),
            diagnostics: Vec::new(),
            gpu: None,
            state: Vec::new(),
            memory: MemoryReport::default(),
            layers: Vec::new(),
        }
    }
}

impl InspectorModel {
    pub fn capture(context: &Context, title: &str, size: Size, recorder: &Recorder) -> Self {
        let snapshots = context.snapshots();
        let mut nodes: Vec<InspectedNode> = Vec::with_capacity(snapshots.len());
        let mut index = HashMap::with_capacity(snapshots.len());
        for snapshot in snapshots {
            let depth = snapshot
                .parent
                .and_then(|parent| index.get(&parent))
                .map_or(0, |position: &usize| nodes[*position].depth + 1);
            let Some(details) = context.details(snapshot.id) else {
                continue;
            };
            index.insert(snapshot.id, nodes.len());
            nodes.push(InspectedNode {
                snapshot,
                details,
                depth,
            });
        }
        Self {
            title: title.into(),
            size,
            root: context.root(),
            nodes,
            index,
            focused: context.focused(),
            hovered: context.hovered(),
            damage: context.damage_regions().to_vec(),
            frames: recorder.frames().iter().copied().collect(),
            events: recorder.events().iter().cloned().collect(),
            warnings: context.style_warnings().to_vec(),
            diagnostics: context.diagnostics().to_vec(),
            state: context.state_graph().to_vec(),
            memory: context.memory(),
            layers: layers(&context.scene().primitives),
            gpu: recorder.gpu_snapshot().cloned(),
            ..Self::default()
        }
    }

    pub fn node(&self, id: NodeId) -> Option<&InspectedNode> {
        self.index.get(&id).map(|position| &self.nodes[*position])
    }

    /// Explicit key path from the root, e.g. `page/actions/save`.
    pub fn path(&self, id: NodeId) -> String {
        let mut keys = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            let Some(node) = self.node(id) else { break };
            next = node.snapshot.parent;
            if id != self.root {
                keys.push(node.details.key.clone());
            }
        }
        keys.reverse();
        keys.join("/")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Tree,
    Accessibility,
    Events,
    Frames,
    Gpu,
    Scene,
    State,
    Diagnostics,
}

/// One group in the last scene: what a renderer draws to an offscreen image
/// and composites back. Layers cost a texture and a pass each, so a view
/// that makes many of them is worth seeing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerRow {
    /// How many layers it sits inside.
    pub depth: usize,
    pub bounds: Rect,
    pub opacity: f32,
    pub transform: Affine,
    /// Primitives inside it, including those of the layers inside it.
    pub primitives: usize,
}

/// The layers of a scene, outermost first, in the order they are drawn.
fn layers(primitives: &[Primitive]) -> Vec<LayerRow> {
    let mut rows: Vec<LayerRow> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for primitive in primitives {
        match primitive {
            Primitive::PushLayer {
                bounds,
                opacity,
                transform,
            } => {
                open.push(rows.len());
                rows.push(LayerRow {
                    depth: open.len() - 1,
                    bounds: *bounds,
                    opacity: *opacity,
                    transform: *transform,
                    primitives: 0,
                });
            }
            Primitive::PopLayer => {
                open.pop();
            }
            _ => {
                for index in &open {
                    rows[*index].primitives += 1;
                }
            }
        }
    }
    rows
}

/// Requests the inspector makes of the inspected window.
#[derive(Clone, Debug, Default)]
pub struct InspectorResponse {
    pub export_csv: bool,
    pub focus: Option<NodeId>,
}

/// What the inspector's own window sends. A host runs it as an application
/// whose message this is: `update` calls [`Inspector::update`], `view` calls
/// [`Inspector::view`], and the host calls [`Inspector::sync`] after it
/// replaces the model.
#[derive(Clone, Debug, PartialEq)]
pub enum InspectorMsg {
    Tab(Tab),
    Damage(bool),
    Repaints(bool),
    /// The node list of the Tree and Accessibility tabs.
    Tree(GridMsg),
    Events(GridMsg),
    Frames(GridMsg),
    ExportCsv,
    /// Focus a node in the inspected window.
    Focus(NodeId),
    /// A diagnostic's node: select it in the tree and show the tree.
    ShowNode(NodeId),
}

/// Retained inspector UI state: what the inspector's view shows, changed only
/// by [`Inspector::update`] and [`Inspector::sync`].
pub struct Inspector {
    pub tab: Tab,
    pub show_damage: bool,
    pub show_repaints: bool,
    collapsed: HashSet<NodeId>,
    tree: ListState,
    events: ListState,
    frames: ListState,
    seen_events: usize,
    seen_frames: u64,
}

impl Default for Inspector {
    fn default() -> Self {
        Self {
            tab: Tab::Tree,
            show_damage: false,
            show_repaints: false,
            collapsed: HashSet::new(),
            tree: ListState::default(),
            events: ListState::default(),
            frames: ListState::default(),
            seen_events: 0,
            seen_frames: 0,
        }
    }
}

const MUTED: Color = Color::rgb(162, 180, 198);
const ACCENT: Color = Color::rgb(103, 232, 206);

impl Inspector {
    /// An inspector that opens on `tab`.
    pub fn showing(tab: Tab) -> Self {
        Self {
            tab,
            ..Self::default()
        }
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.tree.selected().map(|id| NodeId(id.0))
    }

    /// Applies what the inspector's window sent. The returned requests are
    /// for the inspected window.
    pub fn update(&mut self, message: InspectorMsg, model: &InspectorModel) -> InspectorResponse {
        let mut response = InspectorResponse::default();
        let select = |list: &mut ListState, message: GridMsg| match message {
            GridMsg::Select(id) => list.select(Some(id)),
            GridMsg::Deselect => list.select(None),
            _ => {}
        };
        match message {
            InspectorMsg::Tab(tab) => self.tab = tab,
            InspectorMsg::Damage(on) => self.show_damage = on,
            InspectorMsg::Repaints(on) => self.show_repaints = on,
            InspectorMsg::Tree(GridMsg::Expand { row, open }) => {
                let id = NodeId(row.0);
                if model
                    .node(id)
                    .is_some_and(|node| !node.snapshot.children.is_empty())
                {
                    if open {
                        self.collapsed.remove(&id);
                    } else {
                        self.collapsed.insert(id);
                    }
                }
            }
            InspectorMsg::Tree(message) => select(&mut self.tree, message),
            InspectorMsg::Events(message) => select(&mut self.events, message),
            InspectorMsg::Frames(message) => select(&mut self.frames, message),
            InspectorMsg::ExportCsv => response.export_csv = true,
            InspectorMsg::Focus(id) => response.focus = Some(id),
            InspectorMsg::ShowNode(id) => {
                self.tree.select(Some(ItemId(id.0)));
                self.tab = Tab::Tree;
            }
        }
        response
    }

    /// Notes that the host has replaced the model: the Events and Frames
    /// lists follow the newest record.
    pub fn sync(&mut self, model: &InspectorModel) {
        if model.events.len() != self.seen_events {
            self.seen_events = model.events.len();
            if let Some(last) = model.events.last() {
                self.events.reveal(ItemId(last.sequence));
            }
        }
        if let Some(last) = model.frames.last()
            && last.stats.frame != self.seen_frames
        {
            self.seen_frames = last.stats.frame;
            self.frames.reveal(ItemId(last.stats.frame));
        }
    }

    /// Declares the inspector in a window whose message is [`InspectorMsg`].
    #[track_caller]
    pub fn view(&self, ui: &mut Ui<'_, InspectorMsg>, model: &InspectorModel) {
        let list_height = (model.viewport.height - 132.0).max(120.0);
        let width = (model.viewport.width - 36.0).max(300.0);
        let mono = Style {
            font_size: Some(12.0),
            font_family: Some(FontFamily::Monospace),
            padding: 3.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.keyed("inspector", |ui| ui.column(Style {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                padding: 12.0,
                gap: 8.0,
                ..Style::default()
            }, |ui| {
                ui.keyed("title", |ui| ui.label(Label::new(format!(
                        "{}  ·  {:.0} × {:.0}  ·  {} nodes  ·  frame {}",
                        model.title,
                        model.size.width,
                        model.size.height,
                        model.nodes.len(),
                        model.frames.last().map_or(0, |frame| frame.stats.frame)
                    )).style(Style {
                        font_size: Some(14.0),
                        font_weight: Some(700),
                        ..Style::default()
                    })));
                if !model.warnings.is_empty() {
                    ui.keyed("warnings", |ui| ui.label(Label::new(format!("Style warnings: {}", model.warnings.join("; "))).style(Style {
                            font_size: Some(13.0),
                            foreground: Some(Color::rgb(255, 196, 96)),
                            width: Length::Percent(1.0),
                            wrap: TextWrap::Word,
                            ..Style::default()
                        })));
                }
                ui.keyed("tabs", |ui| ui.row(Style {
                        gap: 6.0,
                        ..Style::default()
                    }, |ui| {
                        for (tab, label) in [
                            (Tab::Tree, "Tree".to_owned()),
                            (Tab::Accessibility, "Accessibility".to_owned()),
                            (Tab::Events, "Events".to_owned()),
                            (Tab::Frames, "Frames".to_owned()),
                            (Tab::Gpu, "GPU".to_owned()),
                            (Tab::Scene, format!("Scene ({})", model.layers.len())),
                            (Tab::State, format!("State ({})", model.state.len())),
                            (
                                Tab::Diagnostics,
                                format!("Diagnostics ({})", model.diagnostics.len()),
                            ),
                        ] {
                            let style = Style {
                                background: (self.tab == tab).then_some(ui.theme().selection),
                                ..Style::button()
                            };
                            ui.keyed(format!("{tab:?}"), |ui| ui.button(Button::new(label).enabled(true).style(style).on_click(InspectorMsg::Tab(tab))));
                        }
                    }));
                // Their own row: with seven tabs there is no width left for
                // them beside the strip.
                ui.keyed("overlays", |ui| ui.row(Style {
                        gap: 12.0,
                        ..Style::default()
                    }, |ui| {
                        ui.keyed("damage", |ui| ui.checkbox_msg("Damage", self.show_damage, InspectorMsg::Damage));
                        ui.keyed("repaints", |ui| ui.checkbox_msg("Repaints", self.show_repaints, InspectorMsg::Repaints));
                    }));
                match self.tab {
                    Tab::Tree | Tab::Accessibility => {
                        let accessibility = self.tab == Tab::Accessibility;
                        let selected = self.tree.selected().map(|id| NodeId(id.0));
                        let tree_width = (width * 0.56).floor();
                        ui.keyed("body", |ui| ui.row(Style {
                                gap: 12.0,
                                width: Length::Percent(1.0),
                                ..Style::default()
                            }, |ui| {
                                let rows = Rows::new(model, &self.collapsed, accessibility);
                                ui.keyed("tree", |ui| ui.tree_view_msg_with(&rows, &self.tree, VirtualListOptions {
                                        label: "Nodes".into(),
                                        height: list_height,
                                        row_height: 24.0,
                                        style: Style {
                                            width: Length::Px(tree_width),
                                            ..Style::default()
                                        },
                                        ..VirtualListOptions::default()
                                    }, InspectorMsg::Tree, |ui, rows, index, _| {
                                        let node = rows.node(index);
                                        ui.keyed("label", |ui| ui.label(Label::new(rows.text(index)).style(Style {
                                                width: Length::Percent(1.0),
                                                padding: 4.0,
                                                font_size: Some(13.0),
                                                foreground: (!node.details.changed.is_empty())
                                                    .then_some(Color::rgb(255, 196, 96)),
                                                ..Style::default()
                                            })));
                                    }));
                                ui.keyed("details", |ui| ui.column(Style {
                                        gap: 4.0,
                                        width: Length::Px(width - tree_width - 12.0),
                                        ..Style::default()
                                    }, |ui| match selected.and_then(|id| model.node(id)) {
                                        Some(node) => details(ui, model, node),
                                        None => ui.keyed("hint", |ui| ui.label(Label::new("Select a node. Arrows move, Right/Left expand and collapse. Orange rows changed in the last declaration; ▮ marks a repainted node.").style(Style {
                                                font_size: Some(13.0),
                                                foreground: Some(MUTED),
                                                width: Length::Percent(1.0),
                                                wrap: TextWrap::Word,
                                                ..Style::default()
                                            }))),
                                    }));
                            }));
                    }
                    Tab::Events => {
                        let rows = EventRows(&model.events);
                        ui.keyed("events", |ui| ui.virtual_list_msg_with(&rows, &self.events, VirtualListOptions {
                                label: "Events".into(),
                                height: list_height,
                                row_height: 22.0,
                                ..VirtualListOptions::default()
                            }, InspectorMsg::Events, |ui, rows, index, _| {
                                let event = &rows.0[index];
                                ui.keyed("label", |ui| ui.label(Label::new(format!(
                                        "f{:<6} {} {}",
                                        event.frame,
                                        if event.handled { "●" } else { "○" },
                                        event.description
                                    )).style(Style {
                                        foreground: Some(if event.handled { Color::WHITE } else { MUTED }),
                                        ..mono.clone()
                                    })));
                            }));
                    }
                    Tab::Diagnostics => {
                        if model.diagnostics.is_empty() {
                            ui.keyed("none", |ui| ui.label(Label::new("No framework diagnostics. Checks run in debug builds.").style(Style {
                                    foreground: Some(MUTED),
                                    ..mono.clone()
                                })));
                        }
                        ui.keyed("diagnostics", |ui| ui.scroll(Style {
                                width: Length::Percent(1.0),
                                height: Length::Px(list_height),
                                gap: 6.0,
                                ..Style::default()
                            }, |ui| {
                                for (index, entry) in model.diagnostics.iter().enumerate() {
                                    // Selecting an entry shows its node in the tree.
                                    let title = format!(
                                        "{:?}  ·  {}  ·  ×{}",
                                        entry.kind,
                                        if entry.path.is_empty() {
                                            "(window)"
                                        } else {
                                            entry.path.as_str()
                                        },
                                        entry.count
                                    );
                                    let button = Button::new(title).enabled(entry.node.is_some()).style(Style {
                                        width: Length::Percent(1.0),
                                        padding: 6.0,
                                        ..Style::button()
                                    });
                                    let button = match entry.node {
                                        Some(node) => button.on_click(InspectorMsg::ShowNode(node)),
                                        None => button,
                                    };
                                    ui.keyed(format!("entry-{index}"), |ui| ui.button(button));
                                    ui.keyed(format!("message-{index}"), |ui| ui.label(Label::new(entry.message.clone()).style(Style {
                                            font_size: Some(12.0),
                                            foreground: Some(MUTED),
                                            width: Length::Percent(1.0),
                                            wrap: TextWrap::Word,
                                            ..Style::default()
                                        })));
                                }
                            }));
                    }
                    Tab::Gpu => gpu_tab(ui, model, &mono),
                    Tab::Scene => scene_tab(ui, model, &mono),
                    Tab::State => state_tab(ui, model, &mono),
                    Tab::Frames => {
                        let count = model.frames.len().max(1) as f64;
                        let average = |pick: fn(&FrameRecord) -> Duration| {
                            model.frames.iter().map(pick).sum::<Duration>().as_secs_f64() * 1000.0
                                / count
                        };
                        ui.keyed("summary", |ui| ui.label(format!(
                                "{} frames recorded  ·  average declare {:.2} ms  ·  layout {:.2}  ·  paint {:.2}  ·  shaping {:.2}  ·  render {:.2}  ·  total {:.2}",
                                model.frames.len(),
                                average(|frame| frame.stats.declare),
                                average(|frame| frame.stats.layout),
                                average(|frame| frame.stats.paint),
                                average(|frame| frame.stats.shaping),
                                average(|frame| frame.render),
                                average(|frame| frame.total)
                            )));
                        ui.keyed("actions", |ui| ui.row(Style {
                                gap: 8.0,
                                ..Style::default()
                            }, |ui| {
                                ui.keyed("export", |ui| ui.button(Button::new("Export CSV").on_click(InspectorMsg::ExportCsv)));
                                if let Some(path) = &model.export_path {
                                    ui.keyed("path", |ui| ui.label(Label::new(format!("Written to {path}")).style(Style {
                                            font_size: Some(13.0),
                                            foreground: Some(MUTED),
                                            ..Style::default()
                                        })));
                                }
                            }));
                        ui.keyed("columns", |ui| ui.label(Label::new(FrameRecord::SUMMARY_HEADER).style(Style {
                            foreground: Some(MUTED),
                            ..mono.clone()
                        })));
                        let rows = FrameRows(&model.frames);
                        ui.keyed("frames", |ui| ui.virtual_list_msg_with(&rows, &self.frames, VirtualListOptions {
                                label: "Frames".into(),
                                height: (list_height - 96.0).max(120.0),
                                row_height: 22.0,
                                ..VirtualListOptions::default()
                            }, InspectorMsg::Frames, |ui, rows, index, _| {
                                ui.keyed("label", |ui| ui.label(Label::new(rows.0[index].summary()).style(mono.clone())));
                            }));
                    }
                }
            }));
    }

    /// Logical-coordinate primitives for the inspected window to paint after its scene.
    pub fn overlay(&self, model: &InspectorModel) -> Vec<Primitive> {
        let mut out = Vec::new();
        if self.show_damage {
            for rect in &model.damage {
                out.push(Primitive::Quad {
                    rect: *rect,
                    color: Color(255, 80, 80, 60),
                    radius: 0.0,
                });
                outline(&mut out, *rect, 1.0, Color(255, 80, 80, 220));
            }
        }
        if self.show_repaints {
            for node in &model.nodes {
                if node.details.painted && node.snapshot.id != model.root {
                    outline(
                        &mut out,
                        node.snapshot.bounds,
                        1.0,
                        Color(90, 220, 130, 200),
                    );
                }
            }
        }
        if let Some(node) = self.selected().and_then(|id| model.node(id)) {
            outline(&mut out, node.snapshot.bounds, 2.0, ACCENT);
        }
        out
    }
}

fn outline(out: &mut Vec<Primitive>, rect: Rect, thickness: f32, color: Color) {
    let quad = |x, y, width, height| Primitive::Quad {
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        color,
        radius: 0.0,
    };
    out.push(quad(rect.x, rect.y, rect.width, thickness));
    out.push(quad(
        rect.x,
        rect.y + rect.height - thickness,
        rect.width,
        thickness,
    ));
    out.push(quad(rect.x, rect.y, thickness, rect.height));
    out.push(quad(
        rect.x + rect.width - thickness,
        rect.y,
        thickness,
        rect.height,
    ));
}

fn hex(color: Color) -> String {
    if color.3 == 255 {
        format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.0, color.1, color.2, color.3
        )
    }
}

fn details(ui: &mut Ui<'_, InspectorMsg>, model: &InspectorModel, node: &InspectedNode) {
    let snapshot = &node.snapshot;
    let details = &node.details;
    let style = &details.style;
    let line = |ui: &mut Ui<'_, InspectorMsg>, key: &str, text: String| {
        ui.keyed(key, |ui| {
            ui.label(Label::new(text).style(Style {
                font_size: Some(13.0),
                width: Length::Percent(1.0),
                wrap: TextWrap::Word,
                ..Style::default()
            }))
        });
    };
    line(ui, "path", format!("Path: {}", model.path(snapshot.id)));
    line(
        ui,
        "kind",
        format!(
            "{} · {:?} · id {}",
            details.kind, snapshot.semantics.role, snapshot.id.0
        ),
    );
    line(
        ui,
        "label",
        format!("Label: \"{}\"", snapshot.semantics.label),
    );
    if !details.text.is_empty() && details.text != snapshot.semantics.label {
        line(ui, "text", format!("Text: \"{}\"", details.text));
    }
    let bounds = snapshot.bounds;
    line(
        ui,
        "bounds",
        format!(
            "Bounds {:.1}, {:.1}  {:.1} × {:.1}",
            bounds.x, bounds.y, bounds.width, bounds.height
        ),
    );
    if snapshot.clip != bounds {
        let clip = snapshot.clip;
        line(
            ui,
            "clip",
            format!(
                "Clip {:.1}, {:.1}  {:.1} × {:.1}",
                clip.x, clip.y, clip.width, clip.height
            ),
        );
    }
    let mut state = Vec::new();
    state.push(if snapshot.semantics.disabled {
        "disabled"
    } else {
        "enabled"
    });
    if snapshot.semantics.focusable {
        state.push("focusable");
    }
    if model.focused == Some(snapshot.id) {
        state.push("focused");
    }
    if model.hovered == Some(snapshot.id) {
        state.push("hovered");
    }
    line(ui, "state", format!("State: {}", state.join(", ")));
    line(
        ui,
        "changed",
        format!(
            "Last declaration: {}{}",
            if details.changed.is_empty() {
                "unchanged".to_owned()
            } else {
                format!("{:?}", details.changed)
            },
            if details.painted { " · repainted" } else { "" }
        ),
    );
    line(
        ui,
        "layout",
        format!(
            "Style: width {:?} · height {:?} · padding {} · gap {} · grow {} · shrink {} · radius {}",
            style.width,
            style.height,
            style.padding,
            style.gap,
            style.grow,
            style.shrink,
            style.radius
        ),
    );
    line(
        ui,
        "align",
        format!(
            "Align items {:?} · justify {:?} · self {:?}/{:?} · min {:?}×{:?} · max {:?}×{:?} · aspect {:?}",
            style.align_items,
            style.justify_content,
            style.align_self,
            style.justify_self,
            style.min_width,
            style.min_height,
            style.max_width,
            style.max_height,
            style.aspect_ratio
        ),
    );
    if !style.columns.is_empty() || !style.rows.is_empty() || style.area.is_some() {
        line(
            ui,
            "grid",
            format!(
                "Grid columns {:?} · rows {:?} · auto {:?}/{:?} · area {:?}",
                style.columns, style.rows, style.auto_columns, style.auto_rows, style.area
            ),
        );
    }
    let resolved = &details.resolved;
    line(
        ui,
        "font",
        format!(
            "Font: {:?} {}px · weight {}{} · {:?}{}",
            resolved.font_family,
            resolved.font_size,
            resolved.font_weight,
            if resolved.italic { " · italic" } else { "" },
            style.wrap,
            if style.font_family.is_none()
                && style.font_size.is_none()
                && style.font_weight.is_none()
                && style.italic.is_none()
            {
                " · inherited"
            } else {
                ""
            }
        ),
    );
    line(
        ui,
        "colors",
        format!(
            "Background {} · Foreground {}{}",
            style.background.map_or("theme default".to_owned(), hex),
            hex(resolved.foreground),
            if style.foreground.is_none() {
                " (inherited)"
            } else {
                ""
            }
        ),
    );
    if let Some(states) = &style.states {
        line(ui, "states", format!("State overrides: {states:?}"));
    }
    if let Some(value) = &snapshot.value {
        line(ui, "value", format!("Value: \"{value}\""));
    }
    if let Some(checked) = snapshot.checked {
        line(ui, "checked", format!("Checked: {checked}"));
    }
    if let Some(selected) = snapshot.selected {
        line(ui, "selected", format!("Selected: {selected}"));
    }
    if let Some(expanded) = snapshot.expanded {
        line(ui, "expanded", format!("Expanded: {expanded}"));
    }
    if let Some(numeric) = snapshot.numeric {
        line(
            ui,
            "numeric",
            format!(
                "Value {} in {}..={} step {}",
                numeric.value, numeric.min, numeric.max, numeric.step
            ),
        );
    }
    if let Some(scroll) = snapshot.scroll {
        line(
            ui,
            "scroll",
            format!("Scroll {:.1} of {:.1}", scroll.offset, scroll.maximum),
        );
    }
    if let Some(info) = snapshot.collection {
        line(ui, "collection", format!("Collection: {info:?}"));
    }
    if let Some(drag) = snapshot.drag {
        line(ui, "drag", format!("Drag: {drag:?}"));
    }
    if let Some(selection) = snapshot.text_selection {
        line(ui, "selection", format!("Text selection: {selection:?}"));
    }
    if snapshot.semantics.focusable {
        ui.keyed("focus", |ui| {
            ui.button(
                Button::new("Focus in inspected window").on_click(InspectorMsg::Focus(snapshot.id)),
            )
        });
    }
}

/// Visible tree rows over the captured nodes, honoring collapsed subtrees.
struct Rows<'a> {
    model: &'a InspectorModel,
    collapsed: &'a HashSet<NodeId>,
    accessibility: bool,
    visible: Vec<usize>,
    position: HashMap<NodeId, usize>,
}

impl<'a> Rows<'a> {
    fn new(model: &'a InspectorModel, collapsed: &'a HashSet<NodeId>, accessibility: bool) -> Self {
        let mut rows = Self {
            model,
            collapsed,
            accessibility,
            visible: Vec::new(),
            position: HashMap::new(),
        };
        rows.rebuild();
        rows
    }

    fn rebuild(&mut self) {
        self.visible.clear();
        self.position.clear();
        let mut shown = HashSet::new();
        for (index, node) in self.model.nodes.iter().enumerate() {
            let snapshot = &node.snapshot;
            let visible = match snapshot.parent {
                None => true,
                Some(parent) => shown.contains(&parent) && !self.collapsed.contains(&parent),
            };
            if visible {
                shown.insert(snapshot.id);
                self.position.insert(snapshot.id, self.visible.len());
                self.visible.push(index);
            }
        }
    }

    fn node(&self, index: usize) -> &InspectedNode {
        &self.model.nodes[self.visible[index]]
    }

    fn text(&self, index: usize) -> String {
        let node = self.node(index);
        let snapshot = &node.snapshot;
        let details = &node.details;
        let mut text = match self.expanded(index) {
            Some(true) => "▾ ",
            Some(false) => "▸ ",
            None => "  ",
        }
        .to_owned();
        if self.accessibility {
            text.push_str(&format!(
                "{:?} \"{}\"",
                snapshot.semantics.role,
                truncate(&snapshot.semantics.label)
            ));
            if let Some(value) = &snapshot.value {
                text.push_str(&format!(" = \"{}\"", truncate(value)));
            }
            if let Some(checked) = snapshot.checked {
                text.push_str(if checked { " ☑" } else { " ☐" });
            }
            if snapshot.selected == Some(true) {
                text.push_str(" selected");
            }
            if let Some(expanded) = snapshot.expanded {
                text.push_str(if expanded { " expanded" } else { " collapsed" });
            }
            if snapshot.focused {
                text.push_str(" ● focus");
            }
            if snapshot.semantics.disabled {
                text.push_str(" disabled");
            }
        } else {
            text.push_str(&format!("{} · {}", details.key, details.kind));
            if !snapshot.semantics.label.is_empty() {
                text.push_str(&format!(" \"{}\"", truncate(&snapshot.semantics.label)));
            }
            if !details.changed.is_empty() {
                text.push_str(" ✎");
            }
            if details.painted {
                text.push_str(" ▮");
            }
        }
        text
    }
}

fn truncate(text: &str) -> String {
    let mut chars = text.chars();
    let short: String = chars.by_ref().take(40).collect();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

/// What the window holds on the CPU, and the layers of its last scene.
fn scene_tab(ui: &mut Ui<'_, InspectorMsg>, model: &InspectorModel, mono: &Style) {
    let memory = &model.memory;
    let kb = |bytes: usize| format!("{:.1} kB", bytes as f64 / 1024.0);
    for (index, line) in [
        format!(
            "{} nodes  ·  {} flat  ·  {} of text and keys",
            memory.nodes,
            kb(memory.node_bytes),
            kb(memory.text_bytes)
        ),
        format!(
            "{} primitives in the scene  ·  {}",
            memory.scene_primitives,
            kb(memory.scene_bytes)
        ),
        format!(
            "{} paint caches holding {} primitives",
            memory.cached_nodes, memory.cached_primitives
        ),
        format!(
            "{} images decoded  ·  {}",
            memory.images,
            kb(memory.image_bytes)
        ),
        "The layout engine's own arena, the allocator's overhead and the glyph atlas are not counted here; the atlas lives on the GPU."
            .to_owned(),
    ]
    .into_iter()
    .enumerate()
    {
        ui.keyed(format!("memory-{index}"), |ui| ui.label(Label::new(line).style(Style {
                foreground: Some(if index == 4 { MUTED } else { ACCENT }),
                width: Length::Percent(1.0),
                wrap: TextWrap::Word,
                ..mono.clone()
            })));
    }
    if model.layers.is_empty() {
        ui.keyed("no-layers", |ui| {
            ui.label(
                Label::new(
                    "No layers in the last scene. One is made by an opacity or a transform.",
                )
                .style(Style {
                    foreground: Some(MUTED),
                    ..mono.clone()
                }),
            )
        });
        return;
    }
    ui.keyed("layers", |ui| {
        ui.scroll(
            Style {
                width: Length::Percent(1.0),
                height: Length::Px((model.viewport.height - 260.0).max(120.0)),
                gap: 2.0,
                ..Style::default()
            },
            |ui| {
                for (index, layer) in model.layers.iter().enumerate() {
                    let moved = layer.transform != Affine::IDENTITY;
                    ui.keyed(format!("layer-{index}"), |ui| {
                        ui.label(Label::new(format!(
                        "{}{:.0}x{:.0} at {:.0},{:.0}  ·  {} primitives  ·  opacity {:.2}{}",
                        "  ".repeat(layer.depth),
                        layer.bounds.width,
                        layer.bounds.height,
                        layer.bounds.x,
                        layer.bounds.y,
                        layer.primitives,
                        layer.opacity,
                        if moved { "  ·  transformed" } else { "" }
                    )).style(mono.clone()))
                    });
                }
            },
        )
    });
}

/// The reactive store: what it holds, what each derived value read, and what
/// moved last. Empty until the window calls `Ui::observe`.
fn state_tab(ui: &mut Ui<'_, InspectorMsg>, model: &InspectorModel, mono: &Style) {
    if model.state.is_empty() {
        ui.keyed("none", |ui| {
            ui.label(
                Label::new(
                    "No reactive store observed. Call Ui::observe(&store) in the window's view.",
                )
                .style(Style {
                    foreground: Some(MUTED),
                    ..mono.clone()
                }),
            )
        });
        return;
    }
    let newest = model
        .state
        .iter()
        .map(|node| node.changed_at)
        .max()
        .unwrap_or_default();
    let width = model
        .state
        .iter()
        .map(|node| node.name.len())
        .max()
        .unwrap_or(1);
    ui.keyed("state", |ui| {
        ui.scroll(
            Style {
                width: Length::Percent(1.0),
                height: Length::Px((model.viewport.height - 132.0).max(120.0)),
                gap: 2.0,
                ..Style::default()
            },
            |ui| {
                for (index, node) in model.state.iter().enumerate() {
                    let inputs: Vec<&str> = node
                        .inputs
                        .iter()
                        .filter_map(|input| model.state.get(*input))
                        .map(|input| input.name.as_str())
                        .collect();
                    let kind = match node.kind {
                        StateKind::Signal => "signal",
                        StateKind::Memo => "memo  ",
                    };
                    let mut row = format!(
                        "{kind}  {:width$}  v{}",
                        node.name,
                        node.version,
                        width = width
                    );
                    if node.dirty {
                        row.push_str("  out of date");
                    }
                    if !inputs.is_empty() {
                        row.push_str(&format!("  <- {}", inputs.join(", ")));
                    }
                    // What moved on the last change stands out, which is the
                    // question the graph is usually opened to answer.
                    let changed = node.changed_at == newest && newest > 0;
                    ui.keyed(format!("state-{index}"), |ui| {
                        ui.label(Label::new(row).style(Style {
                            foreground: Some(if changed { ACCENT } else { MUTED }),
                            ..mono.clone()
                        }))
                    });
                }
            },
        )
    });
}

impl ListModel for Rows<'_> {
    fn len(&self) -> usize {
        self.visible.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(self.node(index).snapshot.id.0)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        self.position.get(&NodeId(id.0)).copied()
    }
    fn label(&self, index: usize) -> String {
        self.text(index)
    }
    fn depth(&self, index: usize) -> usize {
        self.node(index).depth
    }
    fn expanded(&self, index: usize) -> Option<bool> {
        let snapshot = &self.node(index).snapshot;
        (!snapshot.children.is_empty()).then(|| !self.collapsed.contains(&snapshot.id))
    }
    fn parent(&self, id: ItemId) -> Option<ItemId> {
        self.model
            .node(NodeId(id.0))
            .and_then(|node| node.snapshot.parent)
            .map(|parent| ItemId(parent.0))
    }
}

struct EventRows<'a>(&'a [EventRecord]);

impl ListModel for EventRows<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(self.0[index].sequence)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        let first = self.0.first()?.sequence;
        id.0.checked_sub(first)
            .map(|offset| offset as usize)
            .filter(|offset| *offset < self.0.len())
    }
    fn label(&self, index: usize) -> String {
        self.0[index].description.clone()
    }
}

struct FrameRows<'a>(&'a [FrameRecord]);

impl ListModel for FrameRows<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(self.0[index].stats.frame)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        // Frames are recorded in order, but a window may skip numbers between
        // declarations, so search instead of assuming contiguity.
        self.0
            .binary_search_by_key(&id.0, |frame| frame.stats.frame)
            .ok()
    }
    fn label(&self, index: usize) -> String {
        self.0[index].summary()
    }
}

fn mebibytes(bytes: u64) -> String {
    if bytes < 1 << 20 {
        // A few vertex buffers would otherwise read as nothing at all.
        return format!("{:.1} KiB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

/// A label, a value and a bar whose length is `share` of the row.
fn bar_row(
    ui: &mut Ui<'_, InspectorMsg>,
    key: &str,
    label: &str,
    value: String,
    share: f32,
    mono: &Style,
) {
    ui.keyed(key, |ui| {
        ui.row(
            Style {
                gap: 10.0,
                width: Length::Percent(1.0),
                align_items: rust_gui_core::Align::Center,
                ..Style::default()
            },
            |ui| {
                ui.keyed("label", |ui| {
                    ui.label(Label::new(label).style(mono.clone().width(Length::Px(190.0))))
                });
                ui.keyed("value", |ui| {
                    ui.label(Label::new(value).style(mono.clone().width(Length::Px(110.0))))
                });
                ui.keyed("track", |ui| {
                    ui.container(Style::default().grow(1.0).height(Length::Px(8.0)), |ui| {
                        let accent = ui.theme().accent;
                        ui.keyed("bar", |ui| {
                            ui.container(
                                Style::default()
                                    .width(Length::Percent(share.clamp(0.0, 1.0)))
                                    .height(Length::Px(8.0))
                                    .radius(3.0)
                                    .background(accent),
                                |_| {},
                            )
                        });
                    })
                });
            },
        )
    });
}

fn gpu_tab(ui: &mut Ui<'_, InspectorMsg>, model: &InspectorModel, mono: &Style) {
    let muted = Style {
        font_size: Some(13.0),
        foreground: Some(MUTED),
        ..Style::default()
    };
    let Some(gpu) = &model.gpu else {
        ui.keyed("none", |ui| {
            ui.label(Label::new("This host does not report GPU statistics.").style(muted))
        });
        return;
    };
    ui.keyed("adapter", |ui| ui.label(gpu.adapter.clone()));
    if gpu.software {
        ui.keyed("software", |ui| {
            ui.label(
                Label::new(
                    "This window is painted by the CPU renderer, so there is nothing on a GPU to \
             measure. The render column of the Frames tab is the time that took.",
                )
                .style(Style {
                    wrap: rust_gui_core::TextWrap::Word,
                    width: Length::Percent(1.0),
                    ..muted
                }),
            )
        });
        return;
    }
    let timed: Vec<Duration> = model.frames.iter().filter_map(|frame| frame.gpu).collect();
    if gpu.timestamps {
        let average =
            timed.iter().sum::<Duration>().as_secs_f64() * 1000.0 / timed.len().max(1) as f64;
        ui.keyed("time", |ui| {
            ui.label(format!(
                "GPU frame time  ·  last {}  ·  average {average:.2} ms over {} frames",
                timed
                    .last()
                    .map_or_else(|| "-".to_owned(), |last| format!("{:.2} ms", millis(*last))),
                timed.len()
            ))
        });
    } else {
        ui.keyed("time", |ui| ui.label(Label::new("This adapter has no timestamp queries, so GPU time is unknown. The render column \
             of the Frames tab is CPU time: it measures the driver's queue, not the GPU.").style(Style {
                wrap: rust_gui_core::TextWrap::Word,
                width: Length::Percent(1.0),
                ..muted.clone()
            })));
    }
    ui.keyed("counters", |ui| {
        ui.label(format!(
            "{} draw calls  ·  {} quads  ·  {} text batches  ·  {} render passes",
            gpu.draw_calls, gpu.quads, gpu.text_batches, gpu.render_passes
        ))
    });
    if !gpu.passes.is_empty() {
        ui.keyed("passes-title", |ui| {
            ui.label(Label::new("Passes of the last timed frame").style(muted.clone()))
        });
        let longest = gpu
            .passes
            .iter()
            .map(|(_, time)| time.as_secs_f32())
            .fold(f32::MIN_POSITIVE, f32::max);
        ui.keyed("passes", |ui| {
            ui.column(
                Style::default().gap(4.0).width(Length::Percent(1.0)),
                |ui| {
                    for (index, (label, time)) in gpu.passes.iter().enumerate() {
                        bar_row(
                            ui,
                            &format!("pass-{index}"),
                            label,
                            format!("{:.3} ms", millis(*time)),
                            time.as_secs_f32() / longest,
                            mono,
                        );
                    }
                },
            )
        });
    }
    let total = gpu.memory_total();
    ui.keyed("memory-title", |ui| {
        ui.label(
            Label::new(format!(
                "GPU memory allocated by the renderer  ·  {}  ·  peak {}",
                mebibytes(total),
                mebibytes(gpu.memory_peak)
            ))
            .style(muted.clone()),
        )
    });
    ui.keyed("memory", |ui| {
        ui.column(
            Style::default().gap(4.0).width(Length::Percent(1.0)),
            |ui| {
                for (index, (category, bytes)) in gpu.memory.iter().enumerate() {
                    bar_row(
                        ui,
                        &format!("memory-{index}"),
                        category,
                        mebibytes(*bytes),
                        *bytes as f32 / total.max(1) as f32,
                        mono,
                    );
                }
            },
        )
    });
    ui.keyed("driver", |ui| {
        ui.label(Label::new(match gpu.driver {
            Some((allocated, reserved)) => format!(
                "The driver's allocator reports {} allocated and {} reserved for the whole device.",
                mebibytes(allocated),
                mebibytes(reserved)
            ),
            None => "This backend does not report the driver's allocator.".to_owned(),
        }).style(muted))
    });
}
