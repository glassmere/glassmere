mod support;

use glassmere_core::{Context, Direction, MonospaceMeasurer, Progress, Role, Style, Ui};
use glassmere_scene::{Color, Primitive, Rect, Size};
use std::time::Duration;

const ROOT: Size = Size::new(400.0, 300.0);

fn frame_at(context: &mut Context, milliseconds: u64, view: impl FnOnce(&mut Ui<'_>)) {
    context.set_now(Duration::from_millis(milliseconds));
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

/// The rectangles of the quads drawn in `color`.
fn quads(context: &Context, color: Color) -> Vec<Rect> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Quad {
                rect, color: shade, ..
            } if *shade == color => Some(*rect),
            _ => None,
        })
        .collect()
}

fn bar(ui: &mut Ui<'_>, name: &str, value: Option<f64>) {
    ui.keyed(name, |ui| ui.progress(Progress::new(name), value));
}

#[test]
fn a_progress_bar_reads_as_a_percentage_clamped_to_its_range() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default().gap(4.0), |ui| {
                bar(ui, "quarter", Some(0.25));
                bar(ui, "over", Some(3.0));
                bar(ui, "under", Some(-1.0));
                bar(ui, "unknown", None);
            });
        });
    });
    let snapshot = |name: &str| {
        context
            .node(context.find(&["page", name]).unwrap())
            .unwrap()
    };
    let quarter = snapshot("quarter");
    assert_eq!(quarter.semantics.role, Role::ProgressBar);
    assert_eq!(quarter.semantics.label, "quarter");
    assert_eq!(quarter.value.as_deref(), Some("25%"));
    assert_eq!(
        quarter.numeric.map(|n| (n.value, n.min, n.max)),
        Some((25.0, 0.0, 100.0))
    );
    assert!(!quarter.semantics.focusable);
    assert_eq!(snapshot("over").value.as_deref(), Some("100%"));
    assert_eq!(snapshot("under").value.as_deref(), Some("0%"));
    let unknown = snapshot("unknown");
    assert_eq!(unknown.semantics.role, Role::ProgressBar);
    assert_eq!((unknown.value, unknown.numeric), (None, None));
}

#[test]
fn a_progress_bar_fills_its_share_of_the_track_from_the_start_edge() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| bar(ui, "upload", Some(0.25));
    frame_at(&mut context, 0, view);
    let track = bounds(&context, &["upload"]);
    let accent = context.theme().accent;
    let fill = quads(&context, accent);
    assert_eq!(fill.len(), 1);
    assert_eq!((fill[0].x, fill[0].width), (track.x, track.width * 0.25));
    assert_eq!(quads(&context, context.theme().track), vec![track]);

    // Right to left starts at the other edge.
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.keyed("page", |ui| {
            ui.container(Style::default().direction(Direction::Rtl), |ui| {
                bar(ui, "upload", Some(0.25));
            });
        });
    });
    let track = bounds(&context, &["page", "upload"]);
    let fill = quads(&context, context.theme().accent);
    assert_eq!(fill[0].x + fill[0].width, track.x + track.width);
}

#[test]
fn a_disabled_progress_bar_is_dimmed_and_says_so() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.enabled(false, |ui| bar(ui, "upload", Some(0.5)));
    });
    let node = context.node(context.find(&["upload"]).unwrap()).unwrap();
    assert!(node.semantics.disabled);
    assert!(quads(&context, context.theme().accent).is_empty());
    assert_eq!(quads(&context, context.theme().text_disabled).len(), 1);
}

#[test]
fn an_unknown_progress_bar_moves_and_repaints_only_its_own_box() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(Style::default().gap(20.0), |ui| {
                ui.label("Static text above");
                bar(ui, "busy", None);
                bar(ui, "done", Some(1.0));
            });
        });
    };
    frame_at(&mut context, 0, view);
    assert!(context.animating());
    let track = bounds(&context, &["page", "busy"]);
    let segment = |context: &Context| quads(context, context.theme().accent)[0];
    let first = segment(&context);

    frame_at(&mut context, 400, view);
    assert_ne!(segment(&context), first, "the segment has moved");
    assert!(context.animating());
    let damage = context.damage_regions();
    assert!(!damage.is_empty());
    assert!(
        damage.iter().all(|region| {
            region.x >= track.x - 1.0
                && region.y >= track.y - 1.0
                && region.x + region.width <= track.x + track.width + 1.0
                && region.y + region.height <= track.y + track.height + 1.0
        }),
        "only the bar is damaged: {damage:?} outside {track:?}"
    );
}

#[test]
fn reduced_motion_keeps_an_unknown_progress_bar_still_and_the_window_idle() {
    let mut context = Context::new();
    context.set_reduced_motion(true);
    let view = |ui: &mut Ui<'_>| bar(ui, "busy", None);
    frame_at(&mut context, 0, view);
    let still = quads(&context, context.theme().accent);
    assert_eq!(still.len(), 1, "a segment is still drawn");
    assert!(!context.animating());
    frame_at(&mut context, 700, view);
    assert_eq!(quads(&context, context.theme().accent), still);
    assert!(!context.animating());
}

#[test]
fn a_known_progress_bar_and_a_window_without_one_stay_idle() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        bar(ui, "done", Some(0.5));
        ui.label("text");
    };
    frame_at(&mut context, 0, view);
    assert!(!context.animating());
    frame_at(&mut context, 900, view);
    assert!(!context.animating());
    assert!(context.damage_regions().is_empty());
}
