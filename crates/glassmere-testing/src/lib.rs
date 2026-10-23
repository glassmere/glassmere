//! Headless harness, semantic finders and text snapshots for UI tests.
use glassmere_core::{
    Action, App, AppDriver, Context, Event, FrameStats, Key, MonospaceMeasurer, NodeId,
    NodeSnapshot, TextEdit,
};
use glassmere_scene::{Color, Point, Primitive, Rect, Size};
use std::{any::Any, path::Path};

/// Erases the app's message type, so `Harness<S>` needs no bound on `S`.
struct Runner<S> {
    /// An `AppDriver` for the app's message type.
    driver: Box<dyn Any>,
    frame: fn(&mut S, &mut dyn Any, &mut Context, Size) -> FrameStats,
}

/// Owns an application, declares through a real `Context` with
/// synthetic text metrics, and drives it through the same input paths as a host.
pub struct Harness<S> {
    state: S,
    runner: Runner<S>,
    context: Context,
    size: Size,
    stats: FrameStats,
}

impl<S> Harness<S> {
    fn start(size: Size, state: S, runner: Runner<S>) -> Self {
        let mut harness = Self {
            state,
            runner,
            context: Context::new(),
            size,
            stats: FrameStats::default(),
        };
        harness.run();
        harness
    }

    /// One batch: the app updates on the batch's messages and declares at
    /// most once.
    pub fn run(&mut self) -> FrameStats {
        let Runner { driver, frame } = &mut self.runner;
        let stats = frame(&mut self.state, &mut **driver, &mut self.context, self.size);
        self.stats = stats;
        stats
    }

    /// What the last batch did.
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    pub fn state(&self) -> &S {
        &self.state
    }
    pub fn context(&self) -> &Context {
        &self.context
    }
    /// Moves the host clock the context reads for timed behaviour, such as a
    /// grid's filter delay. The harness never moves it by itself.
    pub fn set_now(&mut self, now: std::time::Duration) {
        self.context.set_now(now);
    }
    pub fn resize(&mut self, size: Size) -> FrameStats {
        self.size = size;
        self.run()
    }

    /// Explicit key path from the root; panics listing the keys present at the failing level.
    pub fn find(&self, path: &[&str]) -> NodeId {
        let mut id = self.context.root();
        for (depth, key) in path.iter().enumerate() {
            let children = self
                .context
                .node(id)
                .map(|node| node.children)
                .unwrap_or_default();
            let keys: Vec<String> = children
                .iter()
                .filter_map(|child| self.context.details(*child).map(|details| details.key))
                .collect();
            id = children
                .into_iter()
                .zip(&keys)
                .find(|(_, candidate)| *candidate == key)
                .map(|(child, _)| child)
                .unwrap_or_else(|| {
                    panic!(
                        "no child {key:?} under {:?}; available: {keys:?}",
                        &path[..depth]
                    )
                });
        }
        id
    }

    /// The unique node whose semantic label is exactly `label`.
    pub fn by_label(&self, label: &str) -> NodeId {
        let matches = self.labelled(label);
        match matches.as_slice() {
            [(id, _)] => *id,
            [] => {
                let near: Vec<_> = self
                    .snapshots()
                    .filter(|node| node.semantics.label.contains(label))
                    .map(|node| format!("{:?} at {}", node.semantics.label, self.path(node.id)))
                    .collect();
                panic!("no node labelled {label:?}; near matches: {near:?}")
            }
            many => {
                let paths: Vec<_> = many.iter().map(|(_, path)| path.clone()).collect();
                panic!("label {label:?} is ambiguous: {paths:?}")
            }
        }
    }
    pub fn query_label(&self, label: &str) -> Option<NodeId> {
        let matches = self.labelled(label);
        (matches.len() == 1).then(|| matches[0].0)
    }
    fn labelled(&self, label: &str) -> Vec<(NodeId, String)> {
        self.snapshots()
            .filter(|node| node.semantics.label == label)
            .map(|node| (node.id, self.path(node.id)))
            .collect()
    }
    fn snapshots(&self) -> impl Iterator<Item = NodeSnapshot> + '_ {
        let root = self.context.root();
        self.context
            .snapshots()
            .into_iter()
            .filter(move |node| node.id != root)
    }
    /// Key path such as `page/actions/save`.
    pub fn path(&self, id: NodeId) -> String {
        let mut keys = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next.filter(|id| *id != self.context.root()) {
            keys.push(self.context.details(id).map(|d| d.key).unwrap_or_default());
            next = self.context.node(id).and_then(|node| node.parent);
        }
        keys.reverse();
        keys.join("/")
    }

    pub fn event(&mut self, event: Event) -> bool {
        self.context.event(event)
    }
    pub fn action(&mut self, id: NodeId, action: Action) -> bool {
        self.context.action(id, action)
    }
    pub fn focus(&mut self, id: NodeId) -> FrameStats {
        self.context.action(id, Action::Focus);
        self.run()
    }
    pub fn hover(&mut self, id: NodeId) -> FrameStats {
        self.context.event(Event::PointerMoved(self.center(id)));
        self.run()
    }
    pub fn click(&mut self, id: NodeId) -> FrameStats {
        self.context.event(Event::PointerMoved(self.center(id)));
        self.context.event(Event::PointerDown);
        self.context.event(Event::PointerUp);
        self.run()
    }
    pub fn press(&mut self, key: Key) -> FrameStats {
        self.press_with(key, false)
    }
    pub fn press_shift(&mut self, key: Key) -> FrameStats {
        self.press_with(key, true)
    }
    fn press_with(&mut self, key: Key, shift: bool) -> FrameStats {
        self.context.event(Event::KeyDown {
            key,
            shift,
            repeat: false,
        });
        self.context.event(Event::KeyUp(key));
        self.run()
    }
    pub fn type_text(&mut self, text: &str) -> FrameStats {
        self.context
            .event(Event::Text(TextEdit::Insert(text.into())));
        self.run()
    }
    fn center(&self, id: NodeId) -> Point {
        let bounds = self
            .context
            .node(id)
            .unwrap_or_else(|| panic!("{id:?} is not a live node"))
            .bounds;
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    }

    /// `key kind role "label" [disabled] [focusable]`, parent-first, indented by depth.
    pub fn snapshot_tree(&self) -> String {
        self.lines(|prefix, _| prefix.to_owned())
    }
    /// Tree lines plus `@ x,y w×h` and `clip …` when the clip differs from the bounds.
    pub fn snapshot_layout(&self) -> String {
        self.lines(|prefix, node| {
            let mut line = format!("{prefix} @ {}", rect(node.bounds));
            if rect(node.clip) != rect(node.bounds) {
                line += &format!(" clip {}", rect(node.clip));
            }
            line
        })
    }
    /// What assistive technology sees: role, label and present state fields.
    pub fn snapshot_accessibility(&self) -> String {
        self.lines(|_, node| {
            let mut line = format!("{:?} {:?}", node.semantics.role, node.semantics.label);
            if let Some(value) = &node.value {
                line += &format!(" value={value:?}");
            }
            for (name, flag) in [
                ("checked", node.checked),
                ("selected", node.selected),
                ("expanded", node.expanded),
            ] {
                if let Some(flag) = flag {
                    line += &format!(" {name}={flag}");
                }
            }
            if let Some(numeric) = node.numeric {
                line += &format!(" number={:.1}", numeric.value);
            }
            if node.focused {
                line += " [focused]";
            }
            if node.semantics.disabled {
                line += " [disabled]";
            }
            line
        })
    }
    fn lines(&self, format: impl Fn(&str, &NodeSnapshot) -> String) -> String {
        let root = self.context.root();
        let mut out = String::new();
        for node in self.context.snapshots() {
            if node.id == root {
                continue;
            }
            let depth = self.path(node.id).matches('/').count();
            let details = self.context.details(node.id).unwrap();
            // A call-site key names a source line, which moves whenever the
            // file is edited; the place among siblings keeps snapshots stable.
            let key = if details.explicit {
                details.key
            } else {
                let index = node
                    .parent
                    .and_then(|parent| self.context.node(parent))
                    .and_then(|parent| parent.children.iter().position(|id| *id == node.id))
                    .unwrap_or_default();
                format!("#{index}")
            };
            let mut prefix = format!(
                "{} {} {:?} {:?}",
                key, details.kind, node.semantics.role, node.semantics.label
            );
            if node.semantics.disabled {
                prefix += " [disabled]";
            }
            if node.semantics.focusable {
                prefix += " [focusable]";
            }
            out += &"  ".repeat(depth);
            out += &format(&prefix, &node);
            out.push('\n');
        }
        out
    }
    /// One primitive per line, indented by clip depth.
    pub fn snapshot_scene(&self) -> String {
        let mut out = String::new();
        let mut depth = 0usize;
        for primitive in &self.context.scene().primitives {
            if matches!(primitive, Primitive::PopClip | Primitive::PopLayer) {
                depth = depth.saturating_sub(1);
            }
            out += &"  ".repeat(depth);
            out += &match primitive {
                Primitive::Quad {
                    rect: r,
                    color,
                    radius,
                } => format!("Quad {} {} r={radius:.1}", rect(*r), hex(*color)),
                Primitive::Gradient {
                    rect: r,
                    radius,
                    from,
                    to,
                    stops,
                    radial,
                } => format!(
                    "Gradient {} r={radius:.1} {}{:.2},{:.2}..{:.2},{:.2} {}",
                    rect(*r),
                    if *radial { "radial " } else { "" },
                    from.x,
                    from.y,
                    to.x,
                    to.y,
                    stops
                        .iter()
                        .map(|stop| format!("{:.2}:{}", stop.at, hex(stop.color)))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                Primitive::Shadow {
                    rect: r,
                    radius,
                    color,
                    blur,
                    spread,
                    offset,
                    inset,
                } => format!(
                    "Shadow {} r={radius:.1} {} blur={blur:.1} spread={spread:.1} at={:.1},{:.1}{}",
                    rect(*r),
                    hex(*color),
                    offset.x,
                    offset.y,
                    if *inset { " inset" } else { "" }
                ),
                Primitive::Custom { rect: r, id } => format!("Custom {} #{id}", rect(*r)),
                Primitive::Path { path, fill, stroke } => format!(
                    "Path {} segments={}{}{}",
                    rect(path.bounds()),
                    path.segments().len(),
                    fill.map_or_else(String::new, |fill| format!(" fill={}", hex(fill))),
                    stroke.map_or_else(String::new, |stroke| format!(
                        " stroke={:.1} {}",
                        stroke.width,
                        hex(stroke.color)
                    ))
                ),
                Primitive::Text {
                    rect: r,
                    text,
                    font_size,
                    color,
                } => format!("Text {} {font_size:.1} {text:?} {}", rect(*r), hex(*color)),
                Primitive::StyledText {
                    rect: r,
                    text,
                    style,
                    color,
                } => format!(
                    "StyledText {} {:.1} {:?} {text:?} {}",
                    rect(*r),
                    style.font_size,
                    style.family,
                    hex(*color)
                ),
                Primitive::RichText {
                    rect: r,
                    spans,
                    style,
                    color,
                } => format!(
                    "RichText {} {:.1} {:?} {:?} {}",
                    rect(*r),
                    style.font_size,
                    style.family,
                    spans
                        .iter()
                        .map(|span| span.text.as_str())
                        .collect::<String>(),
                    hex(*color)
                ),
                Primitive::Image {
                    rect: r,
                    image,
                    source,
                    tint,
                    radius,
                } => format!(
                    "Image {} {}x{}{} source={} r={radius:.1}{}",
                    rect(*r),
                    image.size().width,
                    image.size().height,
                    if image.is_vector() { " vector" } else { "" },
                    rect(*source),
                    tint.map_or_else(String::new, |tint| format!(" tint={}", hex(tint)))
                ),
                Primitive::PushClip(r) => format!("PushClip {}", rect(*r)),
                Primitive::PopClip => "PopClip".into(),
                Primitive::PushLayer {
                    bounds,
                    opacity,
                    transform: t,
                } => format!(
                    "PushLayer {} opacity={opacity:.2} transform=[{:.3} {:.3} {:.3} {:.3} {:.1} {:.1}]",
                    rect(*bounds),
                    t.a,
                    t.b,
                    t.c,
                    t.d,
                    t.e,
                    t.f
                ),
                Primitive::PopLayer => "PopLayer".into(),
            };
            out.push('\n');
            if matches!(
                primitive,
                Primitive::PushClip(_) | Primitive::PushLayer { .. }
            ) {
                depth += 1;
            }
        }
        out
    }
}

fn rect(r: Rect) -> String {
    format!("{:.1},{:.1} {:.1}×{:.1}", r.x, r.y, r.width, r.height)
}
fn hex(c: Color) -> String {
    if c.3 == 255 {
        format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c.0, c.1, c.2, c.3)
    }
}

/// Compares `actual` with `<dir>/<name>.snap`. `UPDATE_SNAPSHOTS` rewrites the
/// file; a missing file is written and reported so it can be reviewed.
impl<A: App> Harness<A> {
    /// Runs `app` on a real `Context` with synthetic text metrics: each input
    /// helper is one batch, which updates on its messages and declares the
    /// view at most once.
    pub fn app(size: Size, app: A) -> Self {
        Self::start(
            size,
            app,
            Runner {
                driver: Box::new(AppDriver::<A::Message>::new(|| true)),
                frame: app_frame::<A>,
            },
        )
    }

    /// Runs `update` with `message`, applies what it asked for, and declares
    /// if the view needs to: one batch.
    pub fn send(&mut self, message: A::Message) -> FrameStats {
        if let Some(driver) = self.driver() {
            driver.send(message);
        }
        self.run()
    }
    /// The app, to change outside `update`; `state()` reads it (`app` is
    /// the constructor). The next batch declares the view.
    pub fn app_mut(&mut self) -> &mut A {
        if let Some(driver) = self.driver() {
            driver.invalidate();
        }
        &mut self.state
    }
    /// Same as [`app_mut`](Self::app_mut): the next batch declares the view.
    #[deprecated(note = "use `app_mut`")]
    pub fn state_mut(&mut self) -> &mut A {
        self.app_mut()
    }
    fn driver(&mut self) -> Option<&mut AppDriver<A::Message>> {
        self.runner.driver.downcast_mut()
    }
}

fn app_frame<A: App>(
    app: &mut A,
    driver: &mut dyn Any,
    context: &mut Context,
    size: Size,
) -> FrameStats {
    let driver = driver
        .downcast_mut::<AppDriver<A::Message>>()
        .expect("the driver of the app's message");
    driver.frame(app, context, size, &mut MonospaceMeasurer)
}

/// Compares `actual` with `<dir>/<name>.snap`. `UPDATE_SNAPSHOTS` rewrites the
/// file; a missing file is written and reported so it can be reviewed.
pub fn assert_snapshot(dir: impl AsRef<Path>, name: &str, actual: &str) {
    let path = dir.as_ref().join(format!("{name}.snap"));
    let write = || {
        std::fs::create_dir_all(dir.as_ref()).unwrap();
        std::fs::write(&path, actual).unwrap();
    };
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        return write();
    }
    let Ok(expected) = std::fs::read_to_string(&path) else {
        write();
        panic!(
            "snapshot {name} created at {}; review it and rerun",
            path.display()
        );
    };
    let expected = expected.replace("\r\n", "\n");
    if expected == actual {
        return;
    }
    let (line, old, new) = expected
        .lines()
        .map(Some)
        .chain(std::iter::repeat(None))
        .zip(actual.lines().map(Some).chain(std::iter::repeat(None)))
        .enumerate()
        .find(|(_, (old, new))| old != new)
        .map(|(index, (old, new))| (index + 1, old.unwrap_or("<end>"), new.unwrap_or("<end>")))
        .expect("differing snapshots differ on some line");
    panic!(
        "snapshot {name} mismatch at line {line}:\n  expected: {old}\n  actual:   {new}\nset UPDATE_SNAPSHOTS=1 to accept the new output"
    );
}
