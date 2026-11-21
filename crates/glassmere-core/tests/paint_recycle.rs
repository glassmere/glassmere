//! A node that is painted again has its chunk written over the one it had, and
//! draws what it draws when nothing is written over: the scene, the damage and
//! what the nodes cache are those of a context that paints every node from
//! nothing.

use glassmere_core::{
    Action, Button, Context, Direction, DockLayout, DockNode, DockOptions, DockPanel, DockTarget,
    DockZone, Easing, Effect, Event, Key, Label, Length, MonospaceMeasurer, NodeId, Style, Theme,
    Transition, Ui,
};
use glassmere_scene::{Color, Point, Primitive, Size};
use std::time::Duration;

const WINDOW: Size = Size::new(480.0, 360.0);
const DOCK_WINDOW: Size = Size::new(800.0, 600.0);

fn middle(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

fn press(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

fn point_at(context: &mut Context, path: &[&str]) {
    let at = middle(context, path);
    context.event(Event::PointerMoved(at));
}

fn focus(context: &mut Context, path: &[&str]) {
    let id = context.find(path).unwrap();
    assert!(context.action(id, Action::Focus));
}

fn wheel(context: &mut Context, delta_y: f32) {
    context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y,
    });
}

/// Two contexts that are given the same views and the same input. The second
/// is made to drop the paint of every node before it is told anything, so that
/// no chunk is left for what it paints to be written over: it paints every node
/// from nothing. The first paints again only the nodes that were invalidated,
/// into the chunks they had.
struct Pair {
    reused: Context,
    fresh: Context,
    /// Which of two themes `fresh` has. They differ in their name and in
    /// nothing that is drawn, and a new theme drops the paint of every node.
    other: bool,
    /// Whether the nodes that cache a chunk are compared too. A node that is
    /// parked keeps the chunk it had in `reused`, and has none in `fresh`.
    caches: bool,
}

impl Pair {
    fn new() -> Self {
        Self {
            reused: Context::new(),
            fresh: Context::new(),
            other: false,
            caches: true,
        }
    }

    fn parking() -> Self {
        Self {
            caches: false,
            ..Self::new()
        }
    }

    fn forget(&mut self) {
        self.other = !self.other;
        let name = if self.other { "other" } else { "dark" };
        self.fresh.set_theme(Theme {
            name: name.into(),
            ..Theme::dark()
        });
    }

    fn at(&mut self, now: Duration) {
        self.reused.set_now(now);
        self.fresh.set_now(now);
    }

    /// What reaches both before the next frame.
    fn tell(&mut self, input: impl Fn(&mut Context)) {
        input(&mut self.reused);
        self.forget();
        input(&mut self.fresh);
    }

    /// A frame of `view` in each, after which they have painted the same.
    fn frame(&mut self, size: Size, view: impl Fn(&mut Ui<'_>), step: &str) {
        self.forget();
        self.reused.run(size, &mut MonospaceMeasurer, &view);
        self.fresh.run(size, &mut MonospaceMeasurer, &view);
        self.same(step);
    }

    fn same(&self, step: &str) {
        let (reused, fresh) = (&self.reused, &self.fresh);
        assert_eq!(reused.scene(), fresh.scene(), "{step}");
        // For a NaN or a negative zero, what they print is the same as well.
        assert_eq!(
            format!("{:?}", reused.scene()),
            format!("{:?}", fresh.scene()),
            "{step}"
        );
        assert_eq!(reused.damage_regions(), fresh.damage_regions(), "{step}");
        if self.caches {
            assert_eq!(reused.memory(), fresh.memory(), "{step}");
        }
        assert_eq!(reused.hovered(), fresh.hovered(), "{step}");
        assert_eq!(reused.focused(), fresh.focused(), "{step}");
    }

    fn primitives(&self) -> usize {
        self.reused.scene().primitives.len()
    }
}

/// A field, and under it a scroll view of rows of a label, a check box and a
/// button.
#[derive(Clone, Copy)]
struct Form {
    rows: usize,
    text: &'static str,
    /// Every other check box is ticked.
    ticked: bool,
    /// The red of the label of every third row, or none at 0.
    tint: u8,
    /// The label of every other row is bold, which makes it a styled text.
    bold: bool,
}

const FORM: Form = Form {
    rows: 8,
    text: "short",
    ticked: false,
    tint: 0,
    bold: false,
};

const BOLD: Form = Form { bold: true, ..FORM };

fn form(ui: &mut Ui<'_>, form: Form) {
    ui.keyed("name", |ui| {
        ui.text_input("Name", form.text, |_| ());
    });
    ui.keyed("list", |ui| {
        ui.scroll(
            Style::default()
                .width(Length::Px(300.0))
                .height(Length::Px(120.0)),
            |ui| {
                for row in 0..form.rows {
                    ui.keyed(format!("row{row}"), |ui| {
                        ui.row(Style::default(), |ui| {
                            let mut style = Style::default();
                            if form.tint > 0 && row % 3 == 0 {
                                style = style.foreground(Color(form.tint, 100, 100, 255));
                            }
                            if form.bold && row % 2 == 1 {
                                style = style.font_weight(700);
                            }
                            ui.keyed("label", |ui| {
                                ui.label(Label::new(format!("{} {row}", form.text)).style(style))
                            });
                            ui.keyed("check", |ui| {
                                ui.checkbox("Tick", form.ticked && row % 2 == 0, |_| ());
                            });
                            ui.keyed("button", |ui| {
                                ui.button("Press");
                            });
                        });
                    });
                }
            },
        );
    });
}

enum Move {
    Edit(Form),
    Tab,
    Hover(&'static [&'static str]),
    Wheel(f32),
    FocusLost,
}

/// Plays `moves` in a pair, with a frame after each, and says how long the
/// scene was after each.
fn play(pair: &mut Pair, moves: &[(&str, Move)]) -> Vec<usize> {
    let mut shown = FORM;
    let mut lengths = Vec::new();
    for (step, change) in moves {
        match change {
            Move::Edit(edited) => shown = *edited,
            Move::Tab => pair.tell(|context| press(context, Key::Tab)),
            Move::Hover(path) => pair.tell(|context| point_at(context, path)),
            Move::Wheel(delta) => pair.tell(|context| wheel(context, *delta)),
            Move::FocusLost => pair.tell(|context| {
                context.event(Event::FocusLost);
            }),
        }
        pair.frame(WINDOW, |ui| form(ui, shown), step);
        lengths.push(pair.primitives());
    }
    lengths
}

#[test]
fn a_run_of_edits_paints_what_a_context_that_writes_over_nothing_paints() {
    let long = "a label that is a good deal longer than the ones before it";
    let moves = [
        ("the first frame", Move::Edit(FORM)),
        ("a longer text", Move::Edit(Form { text: long, ..FORM })),
        ("a shorter one", Move::Edit(Form { text: "no", ..FORM })),
        ("an empty one", Move::Edit(Form { text: "", ..FORM })),
        ("a text again", Move::Edit(FORM)),
        ("a colour", Move::Edit(Form { tint: 200, ..FORM })),
        ("another colour", Move::Edit(Form { tint: 90, ..FORM })),
        ("no colour", Move::Edit(FORM)),
        // The same edits to labels that are bold, which are drawn as styled
        // text, and the change from one kind of text to the other.
        ("bold labels", Move::Edit(BOLD)),
        ("longer bold ones", Move::Edit(Form { text: long, ..BOLD })),
        ("shorter bold ones", Move::Edit(Form { text: "no", ..BOLD })),
        ("empty bold ones", Move::Edit(Form { text: "", ..BOLD })),
        ("bold ones again", Move::Edit(BOLD)),
        ("plain ones again", Move::Edit(FORM)),
        ("the keyboard on the field", Move::Tab),
        ("on to a check box", Move::Tab),
        ("on to its button", Move::Tab),
        (
            "ticks",
            Move::Edit(Form {
                ticked: true,
                ..FORM
            }),
        ),
        ("no ticks", Move::Edit(FORM)),
        ("the keyboard gone", Move::FocusLost),
        ("fewer rows", Move::Edit(Form { rows: 3, ..FORM })),
        ("more rows", Move::Edit(Form { rows: 12, ..FORM })),
        ("the pointer on the list", Move::Hover(&["list"])),
        ("scrolled down", Move::Wheel(80.0)),
        ("scrolled on", Move::Wheel(80.0)),
        ("scrolled back", Move::Wheel(-160.0)),
        (
            "the pointer on a button",
            Move::Hover(&["list", "row1", "button"]),
        ),
        (
            "the pointer on a label",
            Move::Hover(&["list", "row2", "label"]),
        ),
        ("the pointer on the field", Move::Hover(&["name"])),
    ];
    let lengths = play(&mut Pair::new(), &moves);
    // The scene grew and shrank along the way.
    assert!(
        lengths.windows(2).any(|pair| pair[0] < pair[1]),
        "{lengths:?}"
    );
    assert!(
        lengths.windows(2).any(|pair| pair[0] > pair[1]),
        "{lengths:?}"
    );
}

/// A check box, which has a ring while it has the keyboard and a mark while it
/// is ticked, and a column of labels.
fn shrinking(ui: &mut Ui<'_>, ticked: bool, labels: usize) {
    ui.keyed("tick", |ui| {
        ui.checkbox("Tick", ticked, |_| ());
    });
    ui.keyed("column", |ui| {
        ui.column(Style::default(), |ui| {
            for index in 0..labels {
                ui.keyed(format!("label{index}"), |ui| {
                    ui.label(format!("label {index}"))
                });
            }
        })
    });
}

#[test]
fn a_node_that_loses_its_ring_its_mark_and_half_its_children_is_painted_as_it_now_is() {
    let mut pair = Pair::new();
    pair.frame(WINDOW, |ui| shrinking(ui, true, 8), "ticked, eight labels");
    pair.tell(|context| focus(context, &["tick"]));
    pair.frame(WINDOW, |ui| shrinking(ui, true, 8), "with the keyboard");
    let (drawn, cached) = (pair.primitives(), pair.reused.memory().cached_primitives);
    pair.tell(|context| {
        context.event(Event::FocusLost);
    });
    pair.frame(WINDOW, |ui| shrinking(ui, true, 8), "without the keyboard");
    pair.frame(WINDOW, |ui| shrinking(ui, false, 8), "not ticked");
    pair.frame(WINDOW, |ui| shrinking(ui, false, 4), "four labels");
    // What the check box drew and what the other four labels drew is gone, from
    // the scene and from what the nodes cache.
    assert!(
        pair.primitives() + 12 < drawn,
        "{} of {drawn}",
        pair.primitives()
    );
    let after = pair.reused.memory().cached_primitives;
    assert!(after + 12 < cached, "{after} of {cached}");
    // And it all comes back.
    pair.tell(|context| focus(context, &["tick"]));
    pair.frame(WINDOW, |ui| shrinking(ui, true, 8), "all of it again");
    assert_eq!(pair.primitives(), drawn);
}

const STRIP: [&str; 4] = ["dock", "$group", "$tabs", "$strip"];

/// A dock of one group with more tabs than fit its strip.
fn crowded(ui: &mut Ui<'_>, layout: &DockLayout) {
    let panels: Vec<DockPanel> = (0..30)
        .map(|n| DockPanel::new(format!("panel-{n}"), format!("Panel {n}")))
        .collect();
    ui.keyed("dock", |ui| {
        ui.dock(
            layout,
            &panels,
            DockOptions::default(),
            |_| (),
            |ui, id| ui.label(format!("content:{id}")),
        )
    });
}

/// Whether any of the button that scrolls the strip, named `key`, is in view.
fn shown(context: &Context, key: &str) -> bool {
    let path: Vec<&str> = STRIP.iter().copied().chain([key]).collect();
    context
        .node(context.find(&path).unwrap())
        .unwrap()
        .clip
        .width
        > 0.0
}

#[test]
fn a_strip_button_parked_after_it_was_drawn_is_painted_empty_and_drawn_again_when_it_shows() {
    let layout = DockLayout::new(DockNode::tabs((0..30).map(|n| format!("panel-{n}"))));
    let view = |ui: &mut Ui<'_>| crowded(ui, &layout);
    let mut pair = Pair::new();
    pair.frame(DOCK_WINDOW, view, "the first frame");
    assert!(!shown(&pair.reused, "$scroll-start") && shown(&pair.reused, "$scroll-end"));
    pair.tell(|context| point_at(context, &STRIP));
    pair.frame(DOCK_WINDOW, view, "the pointer on the strip");
    // The button that scrolls back is drawn once the tabs have moved, parked
    // when they are back, and drawn once more.
    for (delta, showing) in [
        (120.0, true),
        (-120.0, false),
        (120.0, true),
        (-120.0, false),
    ] {
        pair.tell(|context| wheel(context, delta));
        pair.frame(DOCK_WINDOW, view, &format!("scrolled by {delta}"));
        assert_eq!(shown(&pair.reused, "$scroll-start"), showing, "{delta}");
    }
}

/// A combo box over rows.
fn picking(ui: &mut Ui<'_>) {
    ui.keyed("combo", |ui| {
        ui.combo_box("Choice", 1, &["One", "Two", "Three", "Four"], |_| ());
    });
    for row in 0..6 {
        ui.keyed(format!("row{row}"), |ui| ui.label(format!("row {row}")));
    }
}

fn act(context: &mut Context, path: &[&str], action: Action) {
    let id = context.find(path).unwrap();
    assert!(context.action(id, action));
}

#[test]
fn a_popup_opened_moved_through_closed_and_opened_again_is_painted_as_it_is() {
    let mut pair = Pair::new();
    pair.frame(WINDOW, picking, "the first frame");
    pair.tell(|context| focus(context, &["combo"]));
    pair.frame(WINDOW, picking, "the keyboard on the combo box");
    let closed = pair.primitives();
    for round in ["first", "second"] {
        pair.tell(|context| act(context, &["combo"], Action::Expand));
        pair.frame(WINDOW, picking, &format!("{round} time open"));
        assert!(pair.reused.find(&["combo", "$popup"]).is_some());
        assert!(pair.primitives() > closed);
        for down in 1..=3 {
            pair.tell(|context| press(context, Key::ArrowDown));
            pair.frame(WINDOW, picking, &format!("{round} time, {down} down"));
        }
        pair.tell(|context| act(context, &["combo"], Action::Collapse));
        pair.frame(WINDOW, picking, &format!("{round} time closed"));
        assert!(pair.reused.find(&["combo", "$popup"]).is_none());
        assert_eq!(pair.primitives(), closed);
    }
}

fn workspace() -> DockLayout {
    DockLayout::new(DockNode::row(
        0.25,
        DockNode::tabs(["files"]),
        DockNode::column(
            0.7,
            DockNode::tabs(["editor", "preview"]),
            DockNode::tabs(["output"]),
        ),
    ))
}

/// A dock whose panels are a button each.
fn docked(ui: &mut Ui<'_>, layout: &DockLayout) {
    let panels = [
        DockPanel::new("files", "Files"),
        DockPanel::new("editor", "Editor"),
        DockPanel::new("preview", "Preview"),
        DockPanel::new("output", "Output"),
    ];
    ui.keyed("dock", |ui| {
        ui.dock(
            layout,
            &panels,
            DockOptions::default(),
            |_| (),
            |ui, id| {
                ui.button(format!("Button of {id}"));
            },
        )
    });
}

/// The node that is labelled `label`.
fn labelled(context: &Context, label: &str) -> glassmere_core::NodeSnapshot {
    context
        .snapshots()
        .into_iter()
        .find(|node| node.semantics.label == label)
        .unwrap_or_else(|| panic!("nothing is labelled {label:?}"))
}

/// The nodes the node labelled `label` hangs from, nearest first. They change
/// when the panel it is in is moved to another group.
fn ancestry(context: &Context, label: &str) -> Vec<NodeId> {
    let mut chain = Vec::new();
    let mut next = labelled(context, label).parent;
    while let Some(id) = next {
        chain.push(id);
        next = context.node(id).and_then(|node| node.parent);
    }
    chain
}

fn point_at_label(context: &mut Context, label: &str) {
    let bounds = labelled(context, label).bounds;
    context.event(Event::PointerMoved(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }));
}

#[test]
fn a_panel_moved_to_another_group_after_its_content_was_invalidated_is_painted_in_its_new_place() {
    let mut layout = workspace();
    let mut pair = Pair::parking();
    pair.frame(DOCK_WINDOW, |ui| docked(ui, &layout), "the first frame");
    let moves = [
        (
            "editor",
            DockTarget::Group {
                anchor: "output".into(),
                zone: DockZone::Center,
            },
        ),
        (
            "editor",
            DockTarget::Group {
                anchor: "preview".into(),
                zone: DockZone::Center,
            },
        ),
        (
            "files",
            DockTarget::Group {
                anchor: "output".into(),
                zone: DockZone::Right,
            },
        ),
        (
            "output",
            DockTarget::Group {
                anchor: "editor".into(),
                zone: DockZone::Bottom,
            },
        ),
    ];
    for (panel, target) in moves {
        // The pointer is on the panel's button, which that paints in its hover
        // colour, in the frame that moves the panel.
        let content = format!("Button of {panel}");
        pair.tell(|context| point_at_label(context, &content));
        let hung = ancestry(&pair.reused, &content);
        assert!(
            layout.move_panel(panel, target.clone()),
            "{panel} to {target:?}"
        );
        pair.frame(
            DOCK_WINDOW,
            |ui| docked(ui, &layout),
            &format!("{panel} to {target:?}"),
        );
        assert_ne!(ancestry(&pair.reused, &content), hung, "{panel}");
    }
}

/// A label, a button that slides out when it is removed, and a button below.
fn toast(ui: &mut Ui<'_>, shown: bool) {
    ui.label("always");
    if shown {
        ui.keyed("toast", |ui| {
            ui.button(
                Button::new("Saved").style(Style::button().exit(Effect::slide(0.0, 20.0, 200))),
            );
        });
    }
    ui.keyed("below", |ui| {
        ui.button("Below");
    });
}

#[test]
fn a_node_that_is_hovered_and_removed_in_one_frame_leaves_the_ghost_of_what_it_looked_like_hovered()
{
    let at = Duration::from_millis;
    let mut pair = Pair::new();
    pair.at(at(0));
    pair.frame(WINDOW, |ui| toast(ui, true), "with the toast");
    let with_toast = pair.primitives();
    // The pointer comes onto the button, which invalidates what it drew, and
    // the frame that follows removes it, and paints it for its ghost.
    pair.tell(|context| point_at(context, &["toast"]));
    pair.at(at(100));
    pair.frame(WINDOW, |ui| toast(ui, false), "the toast goes");
    assert!(pair.reused.animating());
    // The ghost is the end of the scene, and the button in it is the one with
    // the pointer on it.
    let hover = pair.reused.theme().control_hover;
    let primitives = &pair.reused.scene().primitives;
    assert!(
        primitives[primitives.len().saturating_sub(with_toast)..]
            .iter()
            .any(|primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == hover)),
        "the ghost is not painted hovered"
    );
    for millis in [150, 200, 250, 400] {
        pair.at(at(millis));
        pair.frame(WINDOW, |ui| toast(ui, false), &format!("{millis} ms"));
    }
    assert!(!pair.reused.animating());
}

/// Cells that ease to a colour that the view changes.
fn wall(ui: &mut Ui<'_>, lit: bool) {
    ui.keyed("wall", |ui| {
        ui.row(Style::default(), |ui| {
            for index in 0..12 {
                let shade = (index % 4) as u8 * 24;
                ui.keyed(format!("cell{index}"), |ui| {
                    ui.label(
                        Label::new("*").style(
                            Style::default()
                                .width(Length::Px(20.0))
                                .background(if lit {
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

/// The colour of the first cell's fill.
fn fill(context: &Context) -> Color {
    let bounds = context
        .node(context.find(&["wall", "cell0"]).unwrap())
        .unwrap()
        .bounds;
    context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::Quad { rect, color, .. } if *rect == bounds => Some(*color),
            _ => None,
        })
        .expect("the cell is filled")
}

#[test]
fn a_cell_that_eases_to_a_colour_is_painted_in_a_new_colour_every_frame() {
    let mut pair = Pair::new();
    pair.at(Duration::ZERO);
    pair.frame(WINDOW, |ui| wall(ui, true), "the first frame");
    let mut fills = vec![fill(&pair.reused)];
    // Sixteen milliseconds a frame, for longer than the transition takes.
    for frame in 1..=30u64 {
        pair.at(Duration::from_millis(frame * 16));
        pair.frame(WINDOW, |ui| wall(ui, false), &format!("frame {frame}"));
        fills.push(fill(&pair.reused));
    }
    // The frame that gives the view's new colour starts the ease, and every
    // frame after it, while it eases, is a colour of its own. It ends where the
    // view said.
    let easing = 300 / 16 + 1;
    assert!(
        fills[1..easing].windows(2).all(|pair| pair[0] != pair[1]),
        "{fills:?}"
    );
    assert_eq!(fills[fills.len() - 1], Color::rgb(40, 60, 220));
    assert!(!pair.reused.animating());
}

/// The nodes that cache a chunk and the primitives in those chunks.
fn cached(context: &Context) -> (usize, usize) {
    let report = context.memory();
    (report.cached_nodes, report.cached_primitives)
}

#[test]
fn a_node_is_cached_until_its_paint_is_invalidated_and_not_while_its_old_chunk_waits() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| form(ui, FORM);
    let mut asked = Vec::new();
    let frame = |context: &mut Context, asked: &mut Vec<(usize, usize)>| {
        let stats = context.run(WINDOW, &mut MonospaceMeasurer, view);
        asked.push(cached(context));
        asked.push((stats.paint_nodes, stats.paint_cache_hits));
    };
    frame(&mut context, &mut asked);
    // The pointer comes onto a button. The button, the row, the list and the
    // root are not cached from then on, and the chunks they had are not
    // counted whether they wait to be written over or not.
    point_at(&mut context, &["list", "row1", "button"]);
    asked.push(cached(&context));
    frame(&mut context, &mut asked);
    // The same again, with nothing to invalidate.
    frame(&mut context, &mut asked);
    assert_eq!(asked, RECORDED_CACHES, "{asked:?}");
}

/// The nodes caching a chunk and the primitives in them, and then the nodes
/// painted and the chunks reused, after the first frame, between the pointer
/// and the second frame, after the second, and after a third that changes
/// nothing and paints nothing.
const RECORDED_CACHES: [(usize, usize); 7] = [
    (35, 420),
    (35, 0),
    (31, 181),
    (35, 420),
    (4, 10),
    (35, 420),
    (0, 0),
];

/// Something that drops the paint of every node.
type Clear = fn(&mut Context);

#[test]
fn chunks_left_by_invalidations_before_a_bulk_clear_are_painted_over_like_any_other() {
    let clears: [(&str, Clear); 3] = [
        ("a new direction", |context| {
            context.set_direction(Direction::Rtl)
        }),
        ("reduced motion", |context| context.set_reduced_motion(true)),
        ("a new theme", |context| context.set_theme(Theme::light())),
    ];
    for (name, clear) in clears {
        let view = |ui: &mut Ui<'_>| form(ui, FORM);
        let at = ["list", "row1", "button"];
        // The pointer invalidates a button and what it is in, and then every
        // node loses its paint.
        let mut late = Context::new();
        late.run(WINDOW, &mut MonospaceMeasurer, view);
        point_at(&mut late, &at);
        clear(&mut late);
        late.run(WINDOW, &mut MonospaceMeasurer, view);
        // Every node loses its paint first, and the pointer has nothing to
        // invalidate.
        let mut early = Context::new();
        early.run(WINDOW, &mut MonospaceMeasurer, view);
        clear(&mut early);
        point_at(&mut early, &at);
        early.run(WINDOW, &mut MonospaceMeasurer, view);
        assert_eq!(late.hovered(), early.hovered(), "{name}");
        assert_eq!(late.scene(), early.scene(), "{name}");
        assert_eq!(late.memory(), early.memory(), "{name}");
    }
}
