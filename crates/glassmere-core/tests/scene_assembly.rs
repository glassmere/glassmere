//! The scene is assembled over the one before it, and a chunk that is where it
//! was is not written again. Whatever the buffer held, the scene is what a
//! context makes the first time it paints, when there is nothing to write over.

use glassmere_core::{
    Action, Button, Context, CustomWidget, Effect, Event, FrameStats, Key, Label, Length,
    MonospaceMeasurer, NodeId, Style, TOOLTIP_DELAY, Transform, Transition, Ui, WidgetBuilder,
    WidgetInput,
};
use glassmere_scene::{
    Color, Point, Primitive, Rect, Scene, Size, TextGeometry, TextMeasurer, TextStyle,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

const WINDOW: Size = Size::new(480.0, 360.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> FrameStats {
    context.run(WINDOW, &mut MonospaceMeasurer, view)
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

fn middle(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

/// What a context makes of `view` the first time it paints, with no scene to
/// write over.
fn painted_once(view: impl FnOnce(&mut Ui<'_>)) -> Scene {
    let mut context = Context::new();
    frame(&mut context, view);
    context.scene().clone()
}

/// The scenes are the same, which for a NaN or a negative zero takes the
/// text they print as well.
fn assert_same(scene: &Scene, wanted: &Scene, step: &str) {
    assert_eq!(scene, wanted, "{step}");
    assert_eq!(format!("{scene:?}"), format!("{wanted:?}"), "{step}");
}

#[derive(Clone, Copy)]
struct Page {
    rows: usize,
    text: &'static str,
    /// The first label is bold, which makes it styled text.
    bold_first: bool,
    /// Every other check box is ticked.
    ticked: bool,
    /// The colour of every third label, red from 1 to 255, or none at 0.
    tint: u8,
    /// The rows are in a memoized container.
    memo: bool,
}

const PLAIN: Page = Page {
    rows: 6,
    text: "short",
    bold_first: false,
    ticked: false,
    tint: 0,
    memo: false,
};

/// Rows of a label, a check box and a button: text, styled text, quads,
/// check marks and clips.
fn page(ui: &mut Ui<'_>, page: Page) {
    let rows = |ui: &mut Ui<'_>| {
        for row in 0..page.rows {
            ui.keyed(format!("row{row}"), |ui| {
                ui.row(Style::default(), |ui| {
                    let mut style = Style::default();
                    if page.bold_first && row == 0 {
                        style = style.font_weight(700);
                    }
                    if page.tint > 0 && row % 3 == 0 {
                        style = style.foreground(Color(page.tint, 100, 100, 255));
                    }
                    ui.keyed("label", |ui| {
                        ui.label(Label::new(format!("{} {row}", page.text)).style(style))
                    });
                    ui.keyed("check", |ui| {
                        ui.checkbox("Tick", page.ticked && row % 2 == 0, |_| ());
                    });
                    ui.keyed("button", |ui| {
                        ui.button("Press");
                    });
                });
            });
        }
    };
    if page.memo {
        let state = (
            page.rows,
            page.text,
            page.bold_first,
            page.ticked,
            page.tint,
        );
        ui.memo(&state, Style::default(), rows);
    } else {
        rows(ui);
    }
}

#[test]
fn a_scene_written_over_the_last_is_the_one_a_context_makes_by_painting_once() {
    let long = "a label that is a good deal longer than the ones before it";
    let steps = [
        ("the same again", PLAIN),
        (
            "longer labels",
            Page {
                text: long,
                ..PLAIN
            },
        ),
        ("empty labels", Page { text: "", ..PLAIN }),
        ("fewer rows", Page { rows: 2, ..PLAIN }),
        ("more rows", Page { rows: 11, ..PLAIN }),
        (
            "a bold label",
            Page {
                rows: 11,
                bold_first: true,
                ..PLAIN
            },
        ),
        ("a plain one again", Page { rows: 11, ..PLAIN }),
        (
            "ticks",
            Page {
                rows: 11,
                ticked: true,
                ..PLAIN
            },
        ),
        (
            "fewer rows without them",
            Page {
                rows: 4,
                text: long,
                ..PLAIN
            },
        ),
        ("one row", Page { rows: 1, ..PLAIN }),
        ("none", Page { rows: 0, ..PLAIN }),
        ("many", Page { rows: 20, ..PLAIN }),
        (
            "many, bold and ticked",
            Page {
                rows: 20,
                bold_first: true,
                ticked: true,
                ..PLAIN
            },
        ),
    ];
    let mut context = Context::new();
    let mut lengths = Vec::new();
    for (step, wanted) in steps {
        frame(&mut context, |ui| page(ui, wanted));
        assert_same(context.scene(), &painted_once(|ui| page(ui, wanted)), step);
        lengths.push(context.scene().primitives.len());
    }
    // The steps did change the scene: it grew, shrank and came back.
    assert!(
        lengths.windows(2).any(|pair| pair[0] < pair[1]),
        "{lengths:?}"
    );
    assert!(
        lengths.windows(2).any(|pair| pair[0] > pair[1]),
        "{lengths:?}"
    );
}

/// A small generator, so that a failure comes again.
struct Dice(u64);

impl Dice {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

#[test]
fn a_random_run_of_edits_leaves_the_scene_a_context_makes_by_painting_once() {
    let texts = [
        "short",
        "",
        "a label that is a good deal longer than the others",
    ];
    for seed in 1..=4u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut context = Context::new();
        let mut wanted = PLAIN;
        for step in 0..40 {
            for _ in 0..=dice.below(2) {
                match dice.below(7) {
                    0 => wanted.rows = dice.below(14),
                    1 => wanted.text = texts[dice.below(3)],
                    2 => wanted.bold_first = !wanted.bold_first,
                    3 => wanted.ticked = !wanted.ticked,
                    4 => wanted.tint = 60 * dice.below(4) as u8,
                    5 => wanted.memo = !wanted.memo,
                    _ => {}
                }
            }
            frame(&mut context, |ui| page(ui, wanted));
            assert_same(
                context.scene(),
                &painted_once(|ui| page(ui, wanted)),
                &format!("seed {seed}, step {step}"),
            );
        }
    }
}

#[test]
fn a_focus_ring_that_comes_and_goes_shifts_what_follows_it_and_leaves_nothing_behind() {
    let wanted = Page { rows: 8, ..PLAIN };
    let view = |ui: &mut Ui<'_>| page(ui, wanted);
    let mut context = Context::new();
    frame(&mut context, view);
    let unfocused = context.scene().clone();
    let mut lengths = vec![unfocused.primitives.len()];
    // Focus goes from the first check box to its button, to the next row's
    // check box, and on.
    for tabs in 1..=5 {
        key(&mut context, Key::Tab);
        frame(&mut context, view);
        let mut reference = Context::new();
        frame(&mut reference, view);
        for _ in 0..tabs {
            key(&mut reference, Key::Tab);
        }
        frame(&mut reference, view);
        assert_same(
            context.scene(),
            reference.scene(),
            &format!("after {tabs} presses of Tab"),
        );
        lengths.push(context.scene().primitives.len());
    }
    assert!(
        lengths[1..].iter().all(|length| *length > lengths[0]),
        "a ring adds primitives: {lengths:?}"
    );
    // The ring goes, and the scene is what it was before there was one.
    context.event(Event::FocusLost);
    frame(&mut context, view);
    assert_eq!(context.focused(), None);
    assert_same(context.scene(), &unfocused, "focus lost");
}

/// A button that has a tooltip, and rows below it.
fn tipped(ui: &mut Ui<'_>, rows: usize) {
    ui.keyed("first", |ui| {
        ui.button("Hover me");
    });
    ui.tooltip("Saves the document");
    for row in 1..rows {
        ui.keyed(format!("row{row}"), |ui| {
            ui.label(format!("a row of text, number {row}"))
        });
    }
}

#[test]
fn a_tooltip_stays_the_end_of_a_scene_that_shrinks_and_grows() {
    let due = TOOLTIP_DELAY + Duration::from_millis(100);
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    frame(&mut context, |ui| tipped(ui, 12));
    context.event(Event::PointerMoved(middle(&context, &["first"])));
    context.set_now(due);
    for rows in [3, 9, 9, 2, 14, 14] {
        frame(&mut context, |ui| tipped(ui, rows));
        let mut reference = Context::new();
        reference.set_now(Duration::ZERO);
        frame(&mut reference, |ui| tipped(ui, rows));
        reference.event(Event::PointerMoved(middle(&reference, &["first"])));
        reference.set_now(due);
        frame(&mut reference, |ui| tipped(ui, rows));
        let step = format!("{rows} rows");
        assert_same(context.scene(), reference.scene(), &step);
        // The bubble and its text are the last two primitives, whatever the
        // scene held there before.
        let [
            ..,
            Primitive::Quad { rect, .. },
            Primitive::StyledText { text, .. },
        ] = context.scene().primitives.as_slice()
        else {
            panic!("{step}: the scene does not end in the tooltip");
        };
        assert_eq!(text, "Saves the document");
        assert!(rect.width > 0.0);
    }
    // It goes, with nothing of it left.
    context.event(Event::PointerLeft);
    frame(&mut context, |ui| tipped(ui, 14));
    let bare = painted_once(|ui| tipped(ui, 14));
    assert_same(context.scene(), &bare, "without the tooltip");
}

/// A measurer that fails on the text of the tooltip while it is armed, as an
/// application's can.
struct Failing {
    armed: bool,
}

impl TextMeasurer for Failing {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size {
        if self.armed && text == "Saves the document" {
            panic!("the measurer failed on {text:?}");
        }
        MonospaceMeasurer.measure_styled(text, style, width)
    }

    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry {
        MonospaceMeasurer.geometry(text, style, width)
    }
}

#[test]
fn a_frame_that_unwinds_in_the_measurer_leaves_the_scene_there_was() {
    let due = TOOLTIP_DELAY + Duration::from_millis(100);
    let mut measurer = Failing { armed: false };
    let mut context = Context::new();
    context.set_now(Duration::ZERO);
    context.run(WINDOW, &mut measurer, |ui| tipped(ui, 8));
    context.event(Event::PointerMoved(middle(&context, &["first"])));
    context.set_now(due);
    let before = context.scene().clone();
    // The frame that shows the tooltip measures its text, and fails.
    measurer.armed = true;
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        context.run(WINDOW, &mut measurer, |ui| tipped(ui, 5))
    }));
    measurer.armed = false;
    assert!(unwound.is_err(), "the tooltip was not measured");
    assert!(!before.primitives.is_empty());
    assert_same(context.scene(), &before, "after the frame that unwound");
    let report = context.memory();
    assert_eq!(report.scene_primitives, before.primitives.len());
    assert_eq!(
        report.scene_bytes,
        before.primitives.len() * std::mem::size_of::<Primitive>()
    );
    // The host goes on with the same context, and the next frame makes the
    // scene of a context that did not fail.
    context.run(WINDOW, &mut measurer, |ui| tipped(ui, 5));
    let mut reference = Context::new();
    reference.set_now(Duration::ZERO);
    frame(&mut reference, |ui| tipped(ui, 5));
    reference.event(Event::PointerMoved(middle(&reference, &["first"])));
    reference.set_now(due);
    frame(&mut reference, |ui| tipped(ui, 5));
    assert_ne!(*reference.scene(), before);
    assert_same(context.scene(), reference.scene(), "the frame after");
}

/// A combo box over rows, and a label that is only there some of the time.
fn picking(ui: &mut Ui<'_>, note: bool) {
    ui.keyed("combo", |ui| {
        ui.combo_box("Choice", 1, &["One", "Two", "Three", "Four"], |_| ());
    });
    for row in 0..6 {
        ui.keyed(format!("row{row}"), |ui| ui.label(format!("row {row}")));
    }
    if note {
        ui.keyed("note", |ui| ui.label("a note"));
    }
}

/// What a context makes of `picking` the first time it paints it, and then
/// the second time, after `events` have reached the combo box.
fn picked(note: bool, events: impl FnOnce(&mut Context, NodeId)) -> Scene {
    let mut context = Context::new();
    frame(&mut context, |ui| picking(ui, note));
    let combo = context.find(&["combo"]).unwrap();
    events(&mut context, combo);
    frame(&mut context, |ui| picking(ui, note));
    context.scene().clone()
}

#[test]
fn a_popup_opened_again_where_it_was_is_painted_as_it_was() {
    let focused = picked(false, |context, combo| {
        context.action(combo, Action::Focus);
    });
    let open = picked(false, |context, combo| {
        context.action(combo, Action::Focus);
        context.action(combo, Action::Expand);
    });
    let noted = picked(true, |context, combo| {
        context.action(combo, Action::Focus);
    });
    assert!(open.primitives.len() > focused.primitives.len());

    let mut context = Context::new();
    frame(&mut context, |ui| picking(ui, false));
    let combo = context.find(&["combo"]).unwrap();
    context.action(combo, Action::Focus);
    frame(&mut context, |ui| picking(ui, false));
    assert_same(context.scene(), &focused, "focused");

    context.action(combo, Action::Expand);
    frame(&mut context, |ui| picking(ui, false));
    assert!(context.find(&["combo", "$popup"]).is_some());
    assert_same(context.scene(), &open, "open");

    context.action(combo, Action::Collapse);
    frame(&mut context, |ui| picking(ui, false));
    assert_same(context.scene(), &focused, "closed");

    // Something else is where the popup was.
    frame(&mut context, |ui| picking(ui, true));
    assert_same(context.scene(), &noted, "a note where the popup was");
    frame(&mut context, |ui| picking(ui, false));
    assert_same(context.scene(), &focused, "the note gone");

    context.action(combo, Action::Expand);
    frame(&mut context, |ui| picking(ui, false));
    assert_same(context.scene(), &open, "open again");
}

#[test]
fn a_scene_that_shrank_does_not_keep_the_room_of_the_big_one() {
    let mut context = Context::new();
    frame(&mut context, |ui| page(ui, Page { rows: 400, ..PLAIN }));
    let big = context.scene().primitives.len();
    assert!(big > 4096, "{big} primitives");
    // A scene as big as the one before may keep all the room it has.
    frame(&mut context, |ui| {
        page(
            ui,
            Page {
                rows: 400,
                text: "other",
                ..PLAIN
            },
        )
    });
    assert_eq!(context.scene().primitives.len(), big);

    for rows in [60, 3, 0] {
        frame(&mut context, |ui| page(ui, Page { rows, ..PLAIN }));
        let primitives = &context.scene().primitives;
        assert!(
            primitives.capacity() <= 4 * primitives.len().max(256),
            "room for {} primitives, {} held, after {big}",
            primitives.capacity(),
            primitives.len()
        );
    }
}

/// A label over a node that is scaled about its middle, so that it is drawn
/// past its own box, or not scaled at all. The node has a transition, which is
/// what makes a transformed node the scene is damaged whole for.
fn scaled(ui: &mut Ui<'_>, label: &str, scale: f32) {
    ui.label("above");
    ui.keyed("moved", |ui| {
        ui.container(
            Style::default()
                .width(Length::Px(100.0))
                .height(Length::Px(40.0))
                .transform(Transform::scale(scale))
                .transition(Transition::new(100)),
            |ui| ui.label(label.to_owned()),
        )
    });
}

#[test]
fn a_transformed_node_painted_again_damages_the_whole_scene() {
    let whole = Rect {
        x: 0.0,
        y: 0.0,
        width: WINDOW.width,
        height: WINDOW.height,
    };
    let mut context = Context::new();
    let painted = |context: &mut Context, label, scale| {
        frame(context, |ui| scaled(ui, label, scale));
        context.damage_regions().to_vec()
    };
    painted(&mut context, "one", 1.1);
    // Only the label of a scaled node changes, and the scaled pixels land
    // anywhere, so the scene is damaged whole, as big as the last one was.
    assert_eq!(painted(&mut context, "two", 1.1), [whole]);
    // A node that is not scaled damages its own place.
    let mut plain = Context::new();
    painted(&mut plain, "one", 1.0);
    let own = painted(&mut plain, "two", 1.0);
    assert!(own.iter().all(|rect| *rect != whole), "{own:?}");
}

/// A custom widget that draws a rectangle with NaN in it, under a label.
fn nan(ui: &mut Ui<'_>, label: &str) {
    ui.keyed("custom", |ui| {
        ui.widget(
            Style::default(),
            WidgetBuilder::new(|_: &WidgetInput| CustomWidget {
                size: Size::new(40.0, 20.0),
                primitives: vec![Primitive::Quad {
                    rect: Rect {
                        x: f32::NAN,
                        y: 1.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    color: Color::WHITE,
                    radius: f32::NAN,
                }],
                ..CustomWidget::default()
            }),
        );
    });
    ui.keyed("label", |ui| ui.label(label.to_owned()));
}

#[test]
fn a_primitive_with_a_nan_in_it_is_assembled_like_any_other() {
    let mut context = Context::new();
    // The label is written again each frame, and the widget stays where it is.
    for label in ["one", "two", "three", ""] {
        frame(&mut context, |ui| nan(ui, label));
        let printed = format!("{:?}", context.scene());
        assert!(printed.contains("NaN"), "{printed}");
        let wanted = painted_once(|ui| nan(ui, label));
        assert_eq!(printed, format!("{wanted:?}"), "{label:?}");
    }
}

#[test]
fn contexts_that_paint_in_turn_keep_each_its_own_scene() {
    let (mut one, mut other) = (Context::new(), Context::new());
    for step in 0..8 {
        let first = Page {
            rows: 3 + step * 2,
            text: "the first context",
            ..PLAIN
        };
        let second = Page {
            rows: 12 - step,
            text: "second",
            ticked: step % 2 == 0,
            ..PLAIN
        };
        frame(&mut one, |ui| page(ui, first));
        frame(&mut other, |ui| page(ui, second));
        assert_same(one.scene(), &painted_once(|ui| page(ui, first)), "one");
        assert_same(other.scene(), &painted_once(|ui| page(ui, second)), "other");
    }
}

fn toast(ui: &mut Ui<'_>, shown: bool) {
    ui.label("always");
    if shown {
        ui.keyed("toast", |ui| {
            ui.label(
                Label::new("Saved").style(Style::default().exit(Effect::slide(0.0, 20.0, 200))),
            )
        });
    }
    ui.keyed("below", |ui| {
        ui.button(Button::new("Below"));
    });
}

#[test]
fn a_node_that_fades_out_leaves_its_ghost_after_the_tree_and_then_nothing() {
    let mut context = Context::new();
    let at = |millis| Duration::from_millis(millis);
    context.set_now(at(0));
    frame(&mut context, |ui| toast(ui, true));
    let with_toast = context.scene().primitives.len();
    context.set_now(at(1000));
    frame(&mut context, |ui| toast(ui, false));
    let without = painted_once(|ui| toast(ui, false));
    assert!(context.animating());
    // The tree is the one of a context that never had the toast, and the ghost
    // is drawn over it, in a layer, inside the clip of the node it was.
    let primitives = &context.scene().primitives;
    assert_eq!(primitives[..without.primitives.len()], without.primitives);
    assert!(matches!(
        primitives[without.primitives.len()..],
        [
            Primitive::PushClip(_),
            Primitive::PushLayer { .. },
            ..,
            Primitive::PopLayer,
            Primitive::PopClip
        ]
    ));
    assert!(primitives.len() > without.primitives.len() + 4);
    assert!(with_toast > without.primitives.len());
    // The fade goes on, and then it is over.
    context.set_now(at(1100));
    frame(&mut context, |ui| toast(ui, false));
    assert!(context.animating());
    context.set_now(at(1300));
    frame(&mut context, |ui| toast(ui, false));
    assert!(!context.animating());
    assert_same(context.scene(), &without, "after the fade");
}

#[test]
fn the_memory_report_counts_the_scene_and_what_the_nodes_cache() {
    let mut context = Context::new();
    let mut counts = Vec::new();
    for rows in [3, 5, 2] {
        frame(&mut context, |ui| page(ui, Page { rows, ..PLAIN }));
        let report = context.memory();
        assert_eq!(report.scene_primitives, context.scene().primitives.len());
        assert_eq!(
            report.scene_bytes,
            report.scene_primitives * std::mem::size_of::<Primitive>()
        );
        counts.push((report.cached_nodes, report.cached_primitives));
    }
    assert_eq!(counts, RECORDED_COUNTS, "{counts:?}");
}

/// The nodes holding a chunk and the primitives in those chunks, after each
/// page above.
const RECORDED_COUNTS: [(usize, usize); 3] = [(13, 113), (21, 187), (9, 76)];

#[test]
fn a_frame_paints_the_nodes_that_changed_and_reuses_the_chunks_of_the_rest() {
    let mut context = Context::new();
    let mut asked = Vec::new();
    let mut run = |context: &mut Context, rows, text| {
        let stats = frame(context, |ui| {
            page(
                ui,
                Page {
                    rows,
                    text,
                    ..PLAIN
                },
            )
        });
        asked.push((
            stats.paint_nodes,
            stats.paint_cache_hits,
            stats.scene_rebuilt,
        ));
    };
    run(&mut context, 4, "one");
    run(&mut context, 4, "one");
    run(&mut context, 4, "two");
    context.event(Event::PointerMoved(middle(&context, &["row1", "button"])));
    run(&mut context, 4, "two");
    key(&mut context, Key::Tab);
    run(&mut context, 4, "two");
    run(&mut context, 2, "two");
    assert_eq!(asked, RECORDED_ASKS, "{asked:?}");
}

/// The nodes painted, the chunks reused and whether the scene was assembled,
/// for each frame above.
const RECORDED_ASKS: [(usize, usize, bool); 6] = [
    (17, 0, true),
    (0, 0, false),
    (9, 8, true),
    (3, 5, true),
    (3, 5, true),
    (1, 2, true),
];
