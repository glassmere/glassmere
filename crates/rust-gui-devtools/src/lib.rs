//! Inspector model and views shared by hosts; no platform dependencies.
//!
//! A host records per-window frames and events in a [`Recorder`], captures an
//! [`InspectorModel`] from the inspected window's context before declaring the
//! inspector, then applies the [`InspectorResponse`] and [`Inspector::overlay`]
//! back to the inspected window.
use rust_gui_core::{
    Context, FontFamily, FrameStats, ItemId, Length, ListModel, ListState, NodeDetails, NodeId,
    NodeSnapshot, Style, TextWrap, Ui, VirtualListOptions,
};
use rust_gui_scene::{Color, Primitive, Rect, Size};
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
}

impl FrameRecord {
    pub const CSV_HEADER: &'static str = "frame,declare_us,layout_us,paint_us,render_us,total_us,created,reused,removed,text_measures,layout_passes,paint_nodes,paint_cache_hits,scene_rebuilt,dirty";

    pub fn csv_row(&self) -> String {
        let s = &self.stats;
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:?}",
            s.frame,
            s.declare.as_micros(),
            s.layout.as_micros(),
            s.paint.as_micros(),
            self.render.as_micros(),
            self.total.as_micros(),
            s.created,
            s.reused,
            s.removed,
            s.text_measures,
            s.layout_passes,
            s.paint_nodes,
            s.paint_cache_hits,
            s.scene_rebuilt,
            s.dirty
        )
    }

    /// Column titles for [`FrameRecord::summary`] rows.
    const SUMMARY_HEADER: &'static str =
        "frame   declare  layout   paint  render   total ms   +new ~reused -gone  painted   dirty";

    fn summary(&self) -> String {
        let s = &self.stats;
        format!(
            "{:<7} {:>7.2} {:>7.2} {:>7.2} {:>7.2} {:>7.2}    {:>4} {:>7} {:>5}  {:>3}/{:<4} {}",
            s.frame,
            millis(s.declare),
            millis(s.layout),
            millis(s.paint),
            millis(self.render),
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
}

impl Recorder {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "recorder capacity must be positive");
        Self {
            frames: VecDeque::with_capacity(capacity),
            events: VecDeque::with_capacity(capacity),
            capacity,
            next_event: 0,
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
        });
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
            let details = context
                .details(snapshot.id)
                .expect("snapshots only list live nodes");
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
}

/// Requests the inspector makes of the inspected window.
#[derive(Clone, Debug, Default)]
pub struct InspectorResponse {
    pub export_csv: bool,
    pub focus: Option<NodeId>,
}

/// Retained inspector UI state. Declare with [`Inspector::ui`] inside any window.
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
    pub fn selected(&self) -> Option<NodeId> {
        self.tree.selected().map(|id| NodeId(id.0))
    }

    pub fn ui(&mut self, ui: &mut Ui<'_>, model: &InspectorModel) -> InspectorResponse {
        let mut response = InspectorResponse::default();
        let list_height = (model.viewport.height - 132.0).max(120.0);
        let width = (model.viewport.width - 36.0).max(300.0);
        let mono = Style {
            font_size: 12.0,
            font_family: FontFamily::Monospace,
            padding: 3.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(
            "inspector",
            Style {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                padding: 12.0,
                gap: 8.0,
                ..Style::default()
            },
            |ui| {
                ui.label_styled(
                    "title",
                    format!(
                        "{}  ·  {:.0} × {:.0}  ·  {} nodes  ·  frame {}",
                        model.title,
                        model.size.width,
                        model.size.height,
                        model.nodes.len(),
                        model.frames.last().map_or(0, |frame| frame.stats.frame)
                    ),
                    Style {
                        font_size: 14.0,
                        font_weight: 700,
                        ..Style::default()
                    },
                );
                ui.row(
                    "tabs",
                    Style {
                        gap: 6.0,
                        ..Style::default()
                    },
                    |ui| {
                        for (tab, label) in [
                            (Tab::Tree, "Tree"),
                            (Tab::Accessibility, "Accessibility"),
                            (Tab::Events, "Events"),
                            (Tab::Frames, "Frames"),
                        ] {
                            let style = Style {
                                background: if self.tab == tab {
                                    Color::rgb(22, 105, 99)
                                } else {
                                    Style::button().background
                                },
                                ..Style::button()
                            };
                            if ui.button_styled(label, label, true, style).clicked() {
                                self.tab = tab;
                            }
                        }
                        ui.checkbox("damage", "Damage", &mut self.show_damage);
                        ui.checkbox("repaints", "Repaints", &mut self.show_repaints);
                    },
                );
                match self.tab {
                    Tab::Tree | Tab::Accessibility => {
                        let accessibility = self.tab == Tab::Accessibility;
                        let selected = self.tree.selected().map(|id| NodeId(id.0));
                        let tree_width = (width * 0.56).floor();
                        ui.row(
                            "body",
                            Style {
                                gap: 12.0,
                                width: Length::Percent(1.0),
                                ..Style::default()
                            },
                            |ui| {
                                let mut rows = Rows::new(model, &mut self.collapsed, accessibility);
                                ui.tree_view_with(
                                    "tree",
                                    &mut rows,
                                    &mut self.tree,
                                    VirtualListOptions {
                                        label: "Nodes".into(),
                                        height: list_height,
                                        row_height: 24.0,
                                        style: Style {
                                            width: Length::Px(tree_width),
                                            ..Style::default()
                                        },
                                        ..VirtualListOptions::default()
                                    },
                                    |ui, rows, index, _| {
                                        let node = rows.node(index);
                                        ui.label_styled(
                                            "label",
                                            rows.text(index),
                                            Style {
                                                width: Length::Percent(1.0),
                                                padding: 4.0,
                                                font_size: 13.0,
                                                foreground: if node.details.changed.is_empty() {
                                                    Color::WHITE
                                                } else {
                                                    Color::rgb(255, 196, 96)
                                                },
                                                ..Style::default()
                                            },
                                        );
                                    },
                                );
                                drop(rows);
                                ui.column(
                                    "details",
                                    Style {
                                        gap: 4.0,
                                        width: Length::Px(width - tree_width - 12.0),
                                        ..Style::default()
                                    },
                                    |ui| match selected.and_then(|id| model.node(id)) {
                                        Some(node) => details(ui, model, node, &mut response),
                                        None => ui.label_styled(
                                            "hint",
                                            "Select a node. Arrows move, Right/Left expand and collapse. Orange rows changed in the last declaration; ▮ marks a repainted node.",
                                            Style {
                                                font_size: 13.0,
                                                foreground: MUTED,
                                                width: Length::Percent(1.0),
                                                wrap: TextWrap::Word,
                                                ..Style::default()
                                            },
                                        ),
                                    },
                                );
                            },
                        );
                    }
                    Tab::Events => {
                        if model.events.len() != self.seen_events {
                            self.seen_events = model.events.len();
                            if let Some(last) = model.events.last() {
                                self.events.reveal(ItemId(last.sequence));
                            }
                        }
                        let mut rows = EventRows(&model.events);
                        ui.virtual_list_with(
                            "events",
                            &mut rows,
                            &mut self.events,
                            VirtualListOptions {
                                label: "Events".into(),
                                height: list_height,
                                row_height: 22.0,
                                ..VirtualListOptions::default()
                            },
                            |ui, rows, index, _| {
                                let event = &rows.0[index];
                                ui.label_styled(
                                    "label",
                                    format!(
                                        "f{:<6} {} {}",
                                        event.frame,
                                        if event.handled { "●" } else { "○" },
                                        event.description
                                    ),
                                    Style {
                                        foreground: if event.handled { Color::WHITE } else { MUTED },
                                        ..mono.clone()
                                    },
                                );
                            },
                        );
                    }
                    Tab::Frames => {
                        let count = model.frames.len().max(1) as f64;
                        let average = |pick: fn(&FrameRecord) -> Duration| {
                            model.frames.iter().map(pick).sum::<Duration>().as_secs_f64() * 1000.0
                                / count
                        };
                        ui.label_keyed(
                            "summary",
                            format!(
                                "{} frames recorded  ·  average declare {:.2} ms  ·  layout {:.2}  ·  paint {:.2}  ·  render {:.2}  ·  total {:.2}",
                                model.frames.len(),
                                average(|frame| frame.stats.declare),
                                average(|frame| frame.stats.layout),
                                average(|frame| frame.stats.paint),
                                average(|frame| frame.render),
                                average(|frame| frame.total)
                            ),
                        );
                        ui.row(
                            "actions",
                            Style {
                                gap: 8.0,
                                ..Style::default()
                            },
                            |ui| {
                                if ui.button_keyed("export", "Export CSV").clicked() {
                                    response.export_csv = true;
                                }
                                if let Some(path) = &model.export_path {
                                    ui.label_styled(
                                        "path",
                                        format!("Written to {path}"),
                                        Style {
                                            font_size: 13.0,
                                            foreground: MUTED,
                                            ..Style::default()
                                        },
                                    );
                                }
                            },
                        );
                        if let Some(last) = model.frames.last()
                            && last.stats.frame != self.seen_frames
                        {
                            self.seen_frames = last.stats.frame;
                            self.frames.reveal(ItemId(last.stats.frame));
                        }
                        ui.label_styled("columns", FrameRecord::SUMMARY_HEADER, Style {
                            foreground: MUTED,
                            ..mono.clone()
                        });
                        let mut rows = FrameRows(&model.frames);
                        ui.virtual_list_with(
                            "frames",
                            &mut rows,
                            &mut self.frames,
                            VirtualListOptions {
                                label: "Frames".into(),
                                height: (list_height - 96.0).max(120.0),
                                row_height: 22.0,
                                ..VirtualListOptions::default()
                            },
                            |ui, rows, index, _| {
                                ui.label_styled("label", rows.0[index].summary(), mono.clone());
                            },
                        );
                    }
                }
            },
        );
        response
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

fn details(
    ui: &mut Ui<'_>,
    model: &InspectorModel,
    node: &InspectedNode,
    response: &mut InspectorResponse,
) {
    let snapshot = &node.snapshot;
    let details = &node.details;
    let style = &details.style;
    let line = |ui: &mut Ui<'_>, key: &str, text: String| {
        ui.label_styled(
            key,
            text,
            Style {
                font_size: 13.0,
                width: Length::Percent(1.0),
                wrap: TextWrap::Word,
                ..Style::default()
            },
        );
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
            "Style: width {:?} · height {:?} · padding {} · gap {} · grow {} · radius {}",
            style.width, style.height, style.padding, style.gap, style.grow, style.radius
        ),
    );
    line(
        ui,
        "font",
        format!(
            "Font: {:?} {}px · weight {}{} · {:?}",
            style.font_family,
            style.font_size,
            style.font_weight,
            if style.italic { " · italic" } else { "" },
            style.wrap
        ),
    );
    line(
        ui,
        "colors",
        format!(
            "Background {} · Foreground {}",
            hex(style.background),
            hex(style.foreground)
        ),
    );
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
    if snapshot.semantics.focusable
        && ui
            .button_keyed("focus", "Focus in inspected window")
            .clicked()
    {
        response.focus = Some(snapshot.id);
    }
}

/// Visible tree rows over the captured nodes, honoring collapsed subtrees.
struct Rows<'a> {
    model: &'a InspectorModel,
    collapsed: &'a mut HashSet<NodeId>,
    accessibility: bool,
    visible: Vec<usize>,
    position: HashMap<NodeId, usize>,
}

impl<'a> Rows<'a> {
    fn new(
        model: &'a InspectorModel,
        collapsed: &'a mut HashSet<NodeId>,
        accessibility: bool,
    ) -> Self {
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
    fn set_expanded(&mut self, id: ItemId, expanded: bool) -> bool {
        let id = NodeId(id.0);
        if self
            .model
            .node(id)
            .is_none_or(|node| node.snapshot.children.is_empty())
        {
            return false;
        }
        let changed = if expanded {
            self.collapsed.remove(&id)
        } else {
            self.collapsed.insert(id)
        };
        if changed {
            self.rebuild();
        }
        changed
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
