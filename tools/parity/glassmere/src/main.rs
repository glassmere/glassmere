//! The glassmere side of the parity bench: the shared scenarios built with
//! glassmere's own widgets, driven through the same event path a host uses
//! (`Context::event`, `AppDriver`), and painted by the CPU renderer. No window
//! or display is needed.
//!
//! An omitted `gap` or `padding` is glassmere's own default, which is the
//! point of the layout scenario.
mod perf;

use glassmere_core::{
    Align, App, AppDriver, Button, Context, Cursor, Effects, Event, Justify, Key, Length, Role,
    Style, TextEdit, TextInput, Ui,
};
use glassmere_render_cpu::CpuRenderer;
use glassmere_scene::{Color, Point, Primitive, Size};
use glassmere_text::TextSystem;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone)]
enum Msg {
    Click(String),
    Toggle(String, bool),
    Select(String, usize),
    Text(String, String),
    Slide(String, f64),
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Bool(bool),
    Text(String),
    Num(f64),
    Index(usize),
}

struct Bench {
    ui: Value,
    size: [f32; 2],
    values: HashMap<String, Val>,
    log: Vec<String>,
}

fn number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

impl App for Bench {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message {
            Msg::Click(id) => self.log.push(format!("click:{id}")),
            Msg::Toggle(id, on) => {
                self.log.push(format!("toggle:{id}:{on}"));
                self.values.insert(id, Val::Bool(on));
            }
            Msg::Select(group, index) => {
                self.log.push(format!("select:{group}:{index}"));
                self.values.insert(group, Val::Index(index));
            }
            Msg::Text(id, text) => {
                self.log.push(format!("input:{id}:{text}"));
                self.values.insert(id, Val::Text(text));
            }
            Msg::Slide(id, value) => {
                self.log.push(format!("slide:{id}:{}", number(value)));
                self.values.insert(id, Val::Num(value));
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let mut root = self.ui.clone();
        root["width"] = json!(self.size[0]);
        root["height"] = json!(self.size[1]);
        self.build(ui, &root);
    }
}

fn px(node: &Value, key: &str) -> Option<f32> {
    node.get(key).and_then(Value::as_f64).map(|v| v as f32)
}

fn colour(hex: &str) -> Color {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn align(name: &str) -> Align {
    match name {
        "center" => Align::Center,
        "end" => Align::End,
        "stretch" => Align::Stretch,
        _ => Align::Start,
    }
}

fn justify(name: &str) -> Justify {
    match name {
        "center" => Justify::Center,
        "end" => Justify::End,
        "stretch" => Justify::Stretch,
        "space-between" => Justify::SpaceBetween,
        "space-around" => Justify::SpaceAround,
        "space-evenly" => Justify::SpaceEvenly,
        _ => Justify::Start,
    }
}

/// Sizes, margin, border, flex factors, placement and background from the
/// spec; everything else is the widget's.
fn sized(node: &Value, mut style: Style) -> Style {
    if let Some(w) = px(node, "width") {
        style.width = Length::Px(w);
    }
    if let Some(h) = px(node, "height") {
        style.height = Length::Px(h);
    }
    if let Some(f) = px(node, "widthPercent") {
        style.width = Length::Percent(f);
    }
    if let Some(f) = px(node, "heightPercent") {
        style.height = Length::Percent(f);
    }
    if let Some(w) = px(node, "minWidth") {
        style.min_width = Length::Px(w);
    }
    if let Some(w) = px(node, "maxWidth") {
        style.max_width = Length::Px(w);
    }
    if let Some(m) = px(node, "margin") {
        style = style.margin(m);
    }
    // The web side draws a 1-pixel-radius-less border in this colour.
    if let Some(b) = px(node, "border") {
        style = style.border(b).border_color(colour("#8aa4c8"));
    }
    // `[top, right, bottom, left]`, as CSS orders them.
    if let Some(sides) = node.get("border").and_then(Value::as_array) {
        let side = |i: usize| sides.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        style = style
            .border_sides(glassmere_core::Edges::new(side(0), side(1), side(2), side(3)))
            .border_color(colour("#8aa4c8"));
    }
    if let Some(g) = px(node, "grow") {
        style.grow = g;
    }
    // An omitted `shrink` is glassmere's own default, which is not to shrink.
    if let Some(g) = px(node, "shrink") {
        style.shrink = g;
    }
    if let Some(bg) = node.get("background").and_then(Value::as_str) {
        style.background = Some(colour(bg));
    }
    if let Some(z) = node.get("z").and_then(Value::as_i64) {
        style.z_index = z as i32;
    }
    // Set by a `stack` on the children it places.
    if let Some(at) = node.get("__at").and_then(Value::as_array) {
        style = style.offset(
            at[0].as_f64().unwrap_or(0.0) as f32,
            at[1].as_f64().unwrap_or(0.0) as f32,
        );
    }
    style
}

impl Bench {
    fn text(&self, id: &str) -> String {
        match self.values.get(id) {
            Some(Val::Text(t)) => t.clone(),
            _ => String::new(),
        }
    }

    fn build(&self, ui: &mut Ui<'_, Msg>, node: &Value) {
        // There is no `display: none`: a widget that is not shown is not declared.
        if node.get("visible").and_then(Value::as_bool) == Some(false) {
            return;
        }
        let kind = node["type"].as_str().unwrap_or("");
        let id = node
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let text = node
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let off = node
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let children = |ui: &mut Ui<'_, Msg>| {
            for child in node
                .get("children")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                self.build(ui, child);
            }
        };
        match kind {
            "column" | "row" => {
                let mut style = sized(node, Style::default());
                style.align_items =
                    align(node.get("align").and_then(Value::as_str).unwrap_or("start"));
                style.justify_content = justify(
                    node.get("justify")
                        .and_then(Value::as_str)
                        .unwrap_or("start"),
                );
                style.wrap_items = node.get("wrap").and_then(Value::as_bool).unwrap_or(false);
                if let Some(gap) = px(node, "gap") {
                    style.gap = gap;
                }
                if let Some(padding) = px(node, "padding") {
                    style.padding = padding;
                }
                // `[top, right, bottom, left]`, as CSS orders them.
                if let Some(sides) = node.get("padding").and_then(Value::as_array) {
                    let side = |i: usize| sides.get(i).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    style.padding_sides =
                        Some(glassmere_core::Edges::new(side(0), side(1), side(2), side(3)));
                }
                if let Some(name) = node.get("alignContent").and_then(Value::as_str) {
                    style.align_content = Some(justify(name));
                }
                let content = |ui: &mut Ui<'_, Msg>| {
                    // `Ui::enabled` is how a parent disables what is inside it.
                    if off {
                        ui.enabled(false, children);
                    } else {
                        children(ui);
                    }
                };
                ui.keyed(id, |ui| {
                    if kind == "column" {
                        ui.column(style, content);
                    } else {
                        ui.row(style, content);
                    }
                });
            }
            "stack" => {
                let style = sized(node, Style::default());
                let kids: Vec<Value> = node
                    .get("children")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|child| {
                        let at = child
                            .get("position")
                            .cloned()
                            .unwrap_or(json!({"x": 0, "y": 0}));
                        let mut child = child.clone();
                        child["__at"] = json!([at["x"], at["y"]]);
                        child
                    })
                    .collect();
                ui.keyed(id, |ui| {
                    ui.container(style, |ui| {
                        for child in &kids {
                            self.build(ui, child);
                        }
                    });
                });
            }
            "scroll" => {
                let style = sized(node, Style::default().align_items(Align::Start));
                ui.keyed(id, |ui| ui.scroll(style, children));
            }
            "panel" | "spacer" => {
                let style = sized(node, Style::default());
                ui.keyed(id, |ui| ui.container(style, |_| {}));
            }
            "label" => {
                ui.keyed(id, |ui| ui.label(text));
            }
            "button" => {
                let style = sized(node, Style::button());
                let message = Msg::Click(id.clone());
                ui.keyed(id, |ui| {
                    ui.button(
                        Button::new(text)
                            .style(style)
                            .enabled(!off)
                            .on_click(message),
                    );
                });
            }
            "checkbox" => {
                let checked = matches!(self.values.get(&id), Some(Val::Bool(true)));
                let key = id.clone();
                ui.keyed(id, |ui| {
                    ui.checkbox(text, checked, move |on| Msg::Toggle(key.clone(), on));
                });
            }
            "switch" => {
                let on = matches!(self.values.get(&id), Some(Val::Bool(true)));
                let key = id.clone();
                ui.keyed(id, |ui| {
                    ui.switch(text, on, move |on| Msg::Toggle(key.clone(), on));
                });
            }
            "radiogroup" => {
                let selected = match self.values.get(&id) {
                    Some(Val::Index(i)) => *i,
                    _ => usize::MAX,
                };
                let options = node["options"].as_array().cloned().unwrap_or_default();
                let group = id.clone();
                ui.keyed(id, |ui| {
                    ui.column(Style::default().gap(0.0).align_items(Align::Start), |ui| {
                        for (index, option) in options.iter().enumerate() {
                            let oid = option["id"].as_str().unwrap_or("").to_string();
                            let label = option["text"].as_str().unwrap_or("").to_string();
                            let group = group.clone();
                            ui.keyed(oid, |ui| {
                                ui.radio(label, selected, index, move |i| {
                                    Msg::Select(group.clone(), i)
                                });
                            });
                        }
                    });
                });
            }
            "input" => {
                let value = self.text(&id);
                let mut style = Style {
                    width: Length::Px(280.0),
                    height: Length::Px(44.0),
                    padding: 10.0,
                    ..Style::button()
                };
                if let Some(w) = px(node, "width") {
                    style.width = Length::Px(w);
                }
                let key = id.clone();
                ui.keyed(id.clone(), |ui| {
                    ui.text_input(
                        TextInput::new(node.get("label").and_then(Value::as_str).unwrap_or(""))
                            .style(style),
                        &value,
                        move |t| Msg::Text(key.clone(), t),
                    );
                });
            }
            "slider" => {
                let value = match self.values.get(&id) {
                    Some(Val::Num(n)) => *n,
                    _ => 0.0,
                };
                let (min, max) = (
                    node["min"].as_f64().unwrap_or(0.0),
                    node["max"].as_f64().unwrap_or(100.0),
                );
                let step = node["step"].as_f64().unwrap_or(1.0);
                let key = id.clone();
                // The slider's size is fixed by the widget: `width` is not honoured.
                ui.keyed(id.clone(), |ui| {
                    ui.slider(
                        node.get("label").and_then(Value::as_str).unwrap_or(""),
                        value,
                        min..=max,
                        step,
                        move |v| Msg::Slide(key.clone(), v),
                    );
                });
            }
            other => panic!("unknown widget type {other}"),
        }
    }
}

/// Where each id sits in the tree, as the path `Context::find` takes.
fn paths(
    node: &Value,
    prefix: &mut Vec<String>,
    out: &mut HashMap<String, Vec<String>>,
    kinds: &mut HashMap<String, String>,
) {
    let id = node
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    prefix.push(id.clone());
    out.insert(id.clone(), prefix.clone());
    kinds.insert(id.clone(), node["type"].as_str().unwrap_or("").to_string());
    for child in node
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        paths(child, prefix, out, kinds);
    }
    for option in node
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let oid = option["id"].as_str().unwrap_or("").to_string();
        let mut path = prefix.clone();
        path.push(oid.clone());
        out.insert(oid.clone(), path);
        kinds.insert(oid, "radio".to_string());
    }
    prefix.pop();
}

/// Ids of widgets the spec does not show, and of what is inside them.
fn hidden_ids(node: &Value) -> std::collections::HashSet<String> {
    fn walk(node: &Value, hidden: bool, out: &mut std::collections::HashSet<String>) {
        let hidden = hidden || node.get("visible").and_then(Value::as_bool) == Some(false);
        if hidden {
            if let Some(id) = node.get("id").and_then(Value::as_str) {
                out.insert(id.to_string());
            }
            for option in node
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                out.insert(option["id"].as_str().unwrap_or("").to_string());
            }
        }
        for child in node
            .get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            walk(child, hidden, out);
        }
    }
    let mut out = std::collections::HashSet::new();
    walk(node, false, &mut out);
    out
}

fn initial(node: &Value, out: &mut HashMap<String, Val>) {
    let id = node
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match node["type"].as_str().unwrap_or("") {
        "checkbox" | "switch" => {
            out.insert(
                id,
                Val::Bool(
                    node.get("checked")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                ),
            );
        }
        "radiogroup" => {
            out.insert(id, Val::Index(node["value"].as_u64().unwrap_or(0) as usize));
        }
        "input" => {
            out.insert(
                id,
                Val::Text(node["value"].as_str().unwrap_or("").to_string()),
            );
        }
        "slider" => {
            out.insert(id, Val::Num(node["value"].as_f64().unwrap_or(0.0)));
        }
        _ => {}
    }
    for child in node
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        initial(child, out);
    }
}

fn key_of(name: &str) -> Option<Key> {
    Some(match name {
        "Tab" => Key::Tab,
        "Enter" => Key::Enter,
        "Space" => Key::Space,
        "Escape" => Key::Escape,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        _ => return None,
    })
}

fn role(role: Role) -> &'static str {
    match role {
        Role::Button => "button",
        Role::Checkbox => "checkbox",
        Role::Radio => "radio",
        Role::TextInput | Role::PasswordInput => "textbox",
        Role::Slider => "slider",
        Role::Label => "text",
        Role::Group => "generic",
        Role::ScrollView => "scrollview",
        _ => "other",
    }
}

fn cursor(cursor: Cursor) -> &'static str {
    match cursor {
        Cursor::Default => "default",
        Cursor::Pointer => "pointer",
        Cursor::Text => "text",
        Cursor::Crosshair => "crosshair",
        Cursor::Move => "move",
        Cursor::Grab => "grab",
        Cursor::Grabbing => "grabbing",
        Cursor::NotAllowed => "not-allowed",
        Cursor::Help => "help",
        Cursor::Wait => "wait",
        Cursor::ColResize => "col-resize",
        Cursor::RowResize => "row-resize",
    }
}

struct Rig {
    app: Bench,
    driver: AppDriver<Msg>,
    context: Context,
    text: TextSystem,
    renderer: CpuRenderer,
    size: Size,
    now: Duration,
    paths: HashMap<String, Vec<String>>,
    kinds: HashMap<String, String>,
    hidden: std::collections::HashSet<String>,
}

impl Rig {
    fn frame(&mut self) {
        self.driver
            .frame(&mut self.app, &mut self.context, self.size, &mut self.text);
    }

    /// Frames until animations have settled, so a screenshot is the resting look.
    fn settle(&mut self) {
        for _ in 0..6 {
            self.now += Duration::from_millis(120);
            self.context.set_now(self.now);
            self.frame();
        }
    }

    fn node(&self, id: &str) -> glassmere_core::NodeId {
        let path: Vec<&str> = self.paths[id].iter().map(String::as_str).collect();
        self.context
            .find(&path)
            .unwrap_or_else(|| panic!("no node for {id}: {path:?}"))
    }

    fn rect(&self, id: &str) -> [f32; 4] {
        let b = self.context.node(self.node(id)).unwrap().bounds;
        [b.x, b.y, b.width, b.height]
    }

    fn centre(&self, id: &str, fx: f32, fy: f32) -> Point {
        let [x, y, w, h] = self.rect(id);
        Point {
            x: x + w * fx,
            y: y + h * fy,
        }
    }

    fn event(&mut self, event: Event) {
        self.context.event(event);
        self.frame();
    }

    fn focus(&self) -> Value {
        self.context
            .focused()
            .and_then(|id| self.context.details(id))
            .map_or(Value::Null, |d| Value::String(d.key))
    }

    /// Whether a focus ring is on screen: a quad or stroke in the theme's focus colour.
    fn ring(&self) -> Value {
        if self.context.focused().is_none() {
            return Value::Null;
        }
        let focus = self.context.theme().focus;
        let shown = self.context.scene().primitives.iter().any(|p| match p {
            Primitive::Quad { color, .. } => *color == focus,
            Primitive::Path {
                stroke: Some(s), ..
            } => s.color == focus,
            _ => false,
        });
        Value::Bool(shown)
    }

    fn value_json(v: &Val) -> Value {
        match v {
            Val::Bool(b) => json!(b),
            Val::Text(t) => json!(t),
            Val::Num(n) => json!(n),
            Val::Index(i) => json!(i),
        }
    }

    fn observe(&mut self, label: &str, ids: &[String]) -> Value {
        self.settle();
        let snapshots: HashMap<_, _> = self
            .context
            .snapshots()
            .into_iter()
            .map(|s| (s.id, s))
            .collect();
        let mut a11y = Map::new();
        let mut rects = Map::new();
        let mut scroll = Map::new();
        for id in ids {
            if self.hidden.contains(id) {
                a11y.insert(id.clone(), json!({ "role": "none", "name": "" }));
                rects.insert(id.clone(), json!([0, 0, 0, 0]));
                continue;
            }
            let node = self.node(id);
            let snap = &snapshots[&node];
            let mut entry = Map::new();
            entry.insert("role".into(), json!(role(snap.semantics.role)));
            entry.insert("name".into(), json!(snap.semantics.label));
            if snap.semantics.disabled {
                entry.insert("disabled".into(), json!(true));
            }
            // Other toolkits call a chosen radio button checked; glassmere says selected.
            if let Some(checked) = snap.checked.or(if snap.semantics.role == Role::Radio {
                snap.selected
            } else {
                None
            }) {
                entry.insert("checked".into(), json!(checked));
            }
            if let Some(n) = snap.numeric.as_ref().or(snap.semantics.numeric.as_ref()) {
                entry.insert("value".into(), json!(number(n.value)));
            } else if let Some(value) = &snap.value {
                entry.insert("value".into(), json!(value));
            }
            a11y.insert(id.clone(), Value::Object(entry));
            let r = self.rect(id);
            rects.insert(id.clone(), json!(r.map(|v| (v * 10.0).round() / 10.0)));
            if self.kinds[id] == "scroll" {
                let offset = snap.scroll.map_or(0.0, |s| s.offset);
                scroll.insert(id.clone(), json!(offset.round() as i64));
            }
        }
        let values: Map<String, Value> = self
            .app
            .values
            .iter()
            .map(|(k, v)| (k.clone(), Self::value_json(v)))
            .collect();
        json!({
            "type": "observe", "label": label, "focus": self.focus(), "focusVisible": self.ring(),
            "values": values, "scroll": scroll, "log": self.app.log, "a11y": a11y, "rects": rects,
        })
    }

    fn shot(&mut self, dir: &Path, name: &str) {
        let (w, h) = (self.size.width as u32, self.size.height as u32);
        let pixels = self
            .renderer
            .render(
                [w, h],
                1.0,
                self.context.scene(),
                Color::rgb(0x1b, 0x22, 0x30),
            )
            .to_vec();
        let file = fs::File::create(dir.join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
}

fn run(spec: &Value, out: &Path) -> Value {
    let size = [
        spec["size"][0].as_f64().unwrap() as f32,
        spec["size"][1].as_f64().unwrap() as f32,
    ];
    let mut values = HashMap::new();
    initial(&spec["ui"], &mut values);
    let (mut paths_map, mut kinds) = (HashMap::new(), HashMap::new());
    paths(&spec["ui"], &mut Vec::new(), &mut paths_map, &mut kinds);
    let ids: Vec<String> = {
        let mut v: Vec<_> = paths_map
            .keys()
            .filter(|k| !k.is_empty())
            .cloned()
            .collect();
        v.sort();
        v
    };
    let mut rig = Rig {
        app: Bench {
            ui: spec["ui"].clone(),
            size,
            values,
            log: Vec::new(),
        },
        driver: AppDriver::new(|| true),
        context: Context::new(),
        text: TextSystem::default(),
        renderer: CpuRenderer::with_text_system(TextSystem::default()),
        size: Size::new(size[0], size[1]),
        now: Duration::ZERO,
        paths: paths_map,
        kinds,
        hidden: hidden_ids(&spec["ui"]),
    };
    // The ring is found in the scene by its colour. The dark theme's accent
    // is the same colour as its focus, so the focus colour is nudged by one
    // step of blue: it looks the same and cannot be mistaken for the accent.
    let mut theme = glassmere_core::Theme::dark();
    theme.focus = Color::rgb(103, 232, 207);
    rig.context.set_theme(theme);
    rig.frame();
    let mut records: Vec<Value> = Vec::new();
    let mut shots = Vec::new();
    for step in spec["steps"].as_array().unwrap() {
        let target = step["target"].as_str().unwrap_or("");
        match step["do"].as_str().unwrap() {
            "observe" => {
                let record = rig.observe(step["label"].as_str().unwrap(), &ids);
                let name = format!("{}--{}.png", spec["id"].as_str().unwrap(), records.len());
                rig.shot(out, &name);
                shots.push(name);
                records.push(record);
            }
            "click" | "clickAt" => {
                let at = rig.centre(
                    target,
                    step["fx"].as_f64().unwrap_or(0.5) as f32,
                    step["fy"].as_f64().unwrap_or(0.5) as f32,
                );
                rig.event(Event::PointerMoved(at));
                rig.event(Event::PointerDown);
                rig.event(Event::PointerUp);
            }
            "hover" => {
                let at = rig.centre(target, 0.5, 0.5);
                rig.event(Event::PointerMoved(at));
            }
            "dragOff" => {
                let at = rig.centre(target, 0.5, 0.5);
                rig.event(Event::PointerMoved(at));
                rig.event(Event::PointerDown);
                rig.event(Event::PointerMoved(Point {
                    x: at.x + 150.0,
                    y: at.y,
                }));
                rig.event(Event::PointerUp);
            }
            "wheel" => {
                let at = rig.centre(target, 0.5, 0.5);
                rig.event(Event::PointerMoved(at));
                rig.event(Event::Scroll {
                    delta_x: 0.0,
                    delta_y: step["dy"].as_f64().unwrap() as f32,
                });
            }
            "press" => {
                let name = step["key"].as_str().unwrap();
                let (shift, ctrl, key) = {
                    let parts: Vec<&str> = name.split('+').collect();
                    (
                        parts.contains(&"Shift"),
                        parts.contains(&"Control"),
                        *parts.last().unwrap(),
                    )
                };
                if ctrl && key == "a" {
                    // What the winit host sends for Ctrl+A in an editor.
                    rig.event(Event::Text(TextEdit::SelectAll));
                } else if let Some(key) = key_of(key) {
                    // A host sends the release too, and a button activates on it.
                    rig.event(Event::KeyDown {
                        key,
                        shift,
                        repeat: false,
                    });
                    rig.event(Event::KeyUp(key));
                }
            }
            "type" => {
                // A host sends each character as it is typed.
                for ch in step["text"].as_str().unwrap().chars() {
                    rig.event(Event::Text(TextEdit::Insert(ch.to_string())));
                }
            }
            "cursor" => {
                let mut map = Map::new();
                for id in step["targets"].as_array().unwrap() {
                    let id = id.as_str().unwrap();
                    map.insert(
                        id.to_string(),
                        json!(cursor(rig.context.cursor(rig.node(id)))),
                    );
                }
                records.push(json!({ "type": "cursor", "label": step["label"], "cursor": map }));
            }
            "tabSequence" => {
                let shift = step["shift"].as_bool().unwrap_or(false);
                let mut seen = Vec::new();
                for _ in 0..step["count"].as_u64().unwrap() {
                    rig.event(Event::KeyDown {
                        key: Key::Tab,
                        shift,
                        repeat: false,
                    });
                    rig.event(Event::KeyUp(Key::Tab));
                    seen.push(rig.focus());
                }
                records.push(json!({ "type": "tabs", "label": step["label"], "focus": seen }));
            }
            other => panic!("unknown step {other}"),
        }
    }
    json!({
        "scenario": spec["id"], "framework": "glassmere", "title": spec["title"], "size": spec["size"],
        "records": records, "screenshots": shots,
        "unsupported": Vec::<Value>::new(),
        "notes": ["A slider's width is fixed by the widget; `width` in the spec is not honoured.",
                  "Fonts differ between frameworks, so widths of text are not comparable."],
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let at = args.iter().position(|a| a == "--out");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = at.map_or(manifest.join("../out/glassmere"), |i| {
        PathBuf::from(&args[i + 1])
    });
    let only: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && Some(i - 1) != at)
        .map(|(_, a)| a)
        .collect();
    fs::create_dir_all(&out).unwrap();
    if args.iter().any(|a| a == "--perf") {
        perf::run(&out);
        return;
    }
    let mut files: Vec<_> = fs::read_dir(manifest.join("../scenarios"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    for path in files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let spec: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        if !only.is_empty() && !only.iter().any(|o| **o == spec["id"].as_str().unwrap()) {
            continue;
        }
        let result = run(&spec, &out);
        fs::write(
            out.join(format!("{}.json", spec["id"].as_str().unwrap())),
            serde_json::to_string_pretty(&result).unwrap(),
        )
        .unwrap();
        println!(
            "glassmere: {} ({} records)",
            spec["id"].as_str().unwrap(),
            result["records"].as_array().unwrap().len()
        );
    }
}
