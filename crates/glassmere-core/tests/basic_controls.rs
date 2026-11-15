mod support;

use glassmere_core::app::Action as Msg;
use glassmere_core::{
    Action, Context, Direction, Event, Key, Length, MonospaceMeasurer, Progress, Role, Spinner,
    Style, Switch, Transition, Ui,
};
use glassmere_scene::{Color, Primitive, Rect, Size};
use std::time::Duration;
use support::{Driven, center, click, on};

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

/// The dots a spinner draws, as (rectangle, opacity), in paint order.
fn dots(context: &Context) -> Vec<(Rect, u8)> {
    let accent = context.theme().accent;
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Quad { rect, color, .. }
                if (color.0, color.1, color.2) == (accent.0, accent.1, accent.2) =>
            {
                Some((*rect, color.3))
            }
            _ => None,
        })
        .collect()
}

fn spin(ui: &mut Ui<'_>) {
    ui.keyed("page", |ui| {
        ui.column(Style::default().gap(20.0), |ui| {
            ui.label("Static text above");
            ui.keyed("busy", |ui| ui.spinner("Loading"));
            ui.label("Static text below");
        });
    });
}

#[test]
fn a_spinner_is_a_progress_bar_without_a_value_and_cannot_be_focused() {
    let mut context = Context::new();
    frame_at(&mut context, 0, spin);
    let node = context
        .node(context.find(&["page", "busy"]).unwrap())
        .unwrap();
    assert_eq!(node.semantics.role, Role::ProgressBar);
    assert_eq!(node.semantics.label, "Loading");
    assert!(!node.semantics.focusable);
    assert_eq!((node.value, node.numeric), (None, None));
    let size = bounds(&context, &["page", "busy"]);
    assert_eq!((size.width, size.height), (24.0, 24.0));
    // The label is a name and is not drawn.
    assert!(!support::shows(&context, "Loading"));
}

#[test]
fn a_spinner_turns_and_repaints_only_its_own_box() {
    let mut context = Context::new();
    frame_at(&mut context, 0, spin);
    assert!(context.animating());
    let spinner = bounds(&context, &["page", "busy"]);
    let first = dots(&context);
    assert_eq!(first.len(), 10);
    assert!(first.iter().all(|(rect, _)| {
        rect.x >= spinner.x
            && rect.y >= spinner.y
            && rect.x + rect.width <= spinner.x + spinner.width
            && rect.y + rect.height <= spinner.y + spinner.height
    }));
    let strongest = |dots: &[(Rect, u8)]| {
        dots.iter()
            .enumerate()
            .max_by_key(|(_, (_, alpha))| *alpha)
            .unwrap()
            .0
    };
    frame_at(&mut context, 300, spin);
    let later = dots(&context);
    assert_ne!(strongest(&first), strongest(&later), "the trail has turned");
    let damage = context.damage_regions();
    assert!(!damage.is_empty());
    assert!(
        damage.iter().all(|region| {
            region.x >= spinner.x - 1.0
                && region.y >= spinner.y - 1.0
                && region.x + region.width <= spinner.x + spinner.width + 1.0
                && region.y + region.height <= spinner.y + spinner.height + 1.0
        }),
        "only the spinner is damaged: {damage:?} outside {spinner:?}"
    );
}

#[test]
fn reduced_motion_stops_a_spinner_and_the_window_goes_idle() {
    let mut context = Context::new();
    context.set_reduced_motion(true);
    frame_at(&mut context, 0, spin);
    let still = dots(&context);
    assert_eq!(still.len(), 10);
    assert!(!context.animating());
    frame_at(&mut context, 450, spin);
    assert_eq!(dots(&context), still);
    assert!(!context.animating());
    assert!(context.damage_regions().is_empty());
}

#[test]
fn a_spinner_that_is_removed_leaves_the_window_idle() {
    let mut context = Context::new();
    frame_at(&mut context, 0, spin);
    assert!(context.animating());
    let without = |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| ui.label("Done"));
    };
    frame_at(&mut context, 16, without);
    frame_at(&mut context, 32, without);
    assert!(!context.animating());
}

#[test]
fn a_disabled_spinner_is_dimmed_and_a_style_sizes_and_colors_it() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.enabled(false, |ui| ui.spinner("Loading"));
    });
    assert!(dots(&context).is_empty());
    assert_eq!(quads(&context, context.theme().text_disabled).len(), 1);

    let mut context = Context::new();
    let red = Color(200, 0, 0, 255);
    frame_at(&mut context, 0, |ui| {
        ui.keyed("big", |ui| {
            ui.spinner(Spinner::new("Loading").style(Style {
                width: Length::Px(48.0),
                height: Length::Px(48.0),
                foreground: Some(red),
                ..Style::default()
            }))
        });
    });
    assert_eq!(bounds(&context, &["big"]).width, 48.0);
    assert_eq!(
        quads(&context, red).len(),
        1,
        "only the head is at full strength"
    );
}

struct Toggles {
    on: bool,
    flips: u32,
}

fn toggles_view(state: &Toggles, ui: &mut Ui<'_, Msg<Toggles>>) {
    ui.keyed("page", |ui| {
        ui.column(Style::default().gap(12.0), |ui| {
            ui.keyed("wifi", |ui| {
                ui.switch(
                    "Wi-Fi",
                    state.on,
                    on(|s: &mut Toggles, on: bool| {
                        s.on = on;
                        s.flips += 1;
                    }),
                )
            });
            ui.enabled(false, |ui| {
                ui.keyed("locked", |ui| {
                    ui.switch(
                        "Locked",
                        true,
                        on(|s: &mut Toggles, _: bool| s.flips += 100),
                    )
                });
            });
        });
    });
}

fn toggles() -> Driven<Toggles> {
    Driven::new(
        ROOT,
        Toggles {
            on: false,
            flips: 0,
        },
        toggles_view,
    )
}

/// Moves to the middle of the node at `path` and presses and releases.
fn click_on(context: &mut Context, path: &[&str]) {
    let at = center(context, path);
    click(context, at);
}

fn press(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

#[test]
fn a_switch_sends_the_opposite_of_what_it_shows_for_a_click() {
    let mut app = toggles();
    click_on(&mut app.context, &["page", "wifi"]);
    app.frame();
    assert!(app.state().on);
    assert_eq!(app.state().flips, 1);
    click_on(&mut app.context, &["page", "wifi"]);
    app.frame();
    assert!(!app.state().on);
    assert_eq!(app.state().flips, 2);
    app.frame();
    assert_eq!(app.state().flips, 2, "one message per click");
}

#[test]
fn a_switch_toggles_from_the_keyboard_and_from_assistive_technology() {
    let mut app = toggles();
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&["page", "wifi"]));
    press(&mut app.context, Key::Space);
    app.frame();
    assert!(app.state().on);
    press(&mut app.context, Key::Enter);
    app.frame();
    assert!(!app.state().on);
    let wifi = app.context.find(&["page", "wifi"]).unwrap();
    assert!(app.context.action(wifi, Action::Activate));
    app.frame();
    assert!(app.state().on);
    assert_eq!(app.state().flips, 3);
}

#[test]
fn a_disabled_switch_cannot_be_focused_or_toggled_and_says_so() {
    let mut app = toggles();
    let locked = app.context.find(&["page", "locked"]).unwrap();
    assert!(!app.context.action(locked, Action::Activate));
    click_on(&mut app.context, &["page", "locked"]);
    press(&mut app.context, Key::Tab);
    press(&mut app.context, Key::Tab);
    assert_ne!(app.context.focused(), Some(locked));
    app.frame();
    assert_eq!(app.state().flips, 0);
    let node = app.context.node(locked).unwrap();
    assert!(node.semantics.disabled);
    assert!(!node.semantics.focusable);
    assert_eq!(node.checked, Some(true));
}

#[test]
fn a_switch_is_a_switch_with_a_name_and_a_checked_state() {
    let mut app = toggles();
    let wifi = app.context.find(&["page", "wifi"]).unwrap();
    let node = app.context.node(wifi).unwrap();
    assert_eq!(node.semantics.role, Role::Switch);
    assert_eq!(node.semantics.label, "Wi-Fi");
    assert!(node.semantics.focusable);
    assert_eq!(node.checked, Some(false));
    app.send(|s: &mut Toggles| s.on = true);
    app.frame();
    assert_eq!(app.context.node(wifi).unwrap().checked, Some(true));
}

/// The track and the thumb of the one switch drawn, in that order.
fn switch_parts(context: &Context) -> (Rect, Rect) {
    let thumb = quads(context, context.theme().thumb);
    let track: Vec<Rect> = context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Quad { rect, .. } if (rect.width, rect.height) == (40.0, 22.0) => {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!((thumb.len(), track.len()), (1, 1), "one switch is drawn");
    (track[0], thumb[0])
}

fn one_switch(ui: &mut Ui<'_>, on: bool) {
    ui.keyed("wifi", |ui| ui.switch("Wi-Fi", on, |_| ()));
}

fn label_rect(context: &Context, label: &str) -> Rect {
    context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::Text { rect, text, .. } | Primitive::StyledText { rect, text, .. }
                if text == label =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the label is drawn")
}

#[test]
fn a_switch_puts_its_thumb_at_the_start_when_off_and_the_end_when_on() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| one_switch(ui, false));
    let (track, thumb) = switch_parts(&context);
    assert_eq!(thumb.x, track.x + 3.0);
    assert_eq!(thumb.y, track.y + 3.0);
    frame_at(&mut context, 1000, |ui| one_switch(ui, true));
    frame_at(&mut context, 2000, |ui| one_switch(ui, true));
    let (track, thumb) = switch_parts(&context);
    assert_eq!(thumb.x + thumb.width, track.x + track.width - 3.0);
    // The pill is the accent when on.
    assert_eq!(quads(&context, context.theme().accent), vec![track]);
}

#[test]
fn a_switch_thumb_eases_with_the_style_transition_and_only_its_box_is_repainted() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| one_switch(ui, false));
    let node = bounds(&context, &["wifi"]);
    let (_, start) = switch_parts(&context);
    frame_at(&mut context, 1000, |ui| one_switch(ui, true));
    frame_at(&mut context, 1050, |ui| one_switch(ui, true));
    let (track, middle) = switch_parts(&context);
    assert!(
        middle.x > start.x && middle.x + middle.width < track.x + track.width - 3.0,
        "the thumb is on its way: {middle:?}"
    );
    assert!(context.animating());
    for region in context.damage_regions() {
        assert!(
            region.x >= node.x - 1.0
                && region.y >= node.y - 1.0
                && region.x + region.width <= node.x + node.width + 1.0
                && region.y + region.height <= node.y + node.height + 1.0,
            "{region:?} is outside the switch {node:?}"
        );
    }
    frame_at(&mut context, 1200, |ui| one_switch(ui, true));
    let (track, end) = switch_parts(&context);
    assert_eq!(end.x + end.width, track.x + track.width - 3.0);
    assert!(!context.animating());
}

#[test]
fn a_switch_without_a_transition_or_with_reduced_motion_jumps() {
    let plain = |ui: &mut Ui<'_>, on: bool| {
        ui.keyed("wifi", |ui| {
            ui.switch(Switch::new("Wi-Fi").style(Style::button()), on, |_| ())
        });
    };
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| plain(ui, false));
    frame_at(&mut context, 1000, |ui| plain(ui, true));
    let (track, thumb) = switch_parts(&context);
    assert_eq!(thumb.x + thumb.width, track.x + track.width - 3.0);
    assert!(!context.animating());

    let mut context = Context::new();
    context.set_reduced_motion(true);
    frame_at(&mut context, 0, |ui| one_switch(ui, false));
    frame_at(&mut context, 1000, |ui| one_switch(ui, true));
    let (track, thumb) = switch_parts(&context);
    assert_eq!(thumb.x + thumb.width, track.x + track.width - 3.0);
    assert!(!context.animating());
}

#[test]
fn a_switch_that_does_not_change_is_idle_whatever_its_transition() {
    let slow = |ui: &mut Ui<'_>| {
        ui.keyed("wifi", |ui| {
            ui.switch(
                Switch::new("Wi-Fi").style(Style {
                    transition: Some(Transition::new(400)),
                    ..Style::button()
                }),
                false,
                |_| (),
            )
        });
    };
    let mut context = Context::new();
    frame_at(&mut context, 0, slow);
    frame_at(&mut context, 100, slow);
    assert!(!context.animating());
    assert!(context.damage_regions().is_empty());
}

#[test]
fn a_switch_is_mirrored_in_right_to_left_text() {
    let mut context = Context::new();
    context.set_direction(Direction::Rtl);
    frame_at(&mut context, 0, |ui| one_switch(ui, false));
    let node = bounds(&context, &["wifi"]);
    let (track, thumb) = switch_parts(&context);
    assert_eq!(
        track.x + track.width,
        node.x + node.width - 12.0,
        "the pill is at the start edge, the right"
    );
    assert_eq!(thumb.x + thumb.width, track.x + track.width - 3.0);
    let text = label_rect(&context, "Wi-Fi");
    assert!(
        text.x + text.width <= track.x,
        "the label is beside the pill: {text:?}"
    );
    frame_at(&mut context, 1000, |ui| one_switch(ui, true));
    frame_at(&mut context, 2000, |ui| one_switch(ui, true));
    let (track, thumb) = switch_parts(&context);
    assert_eq!(thumb.x, track.x + 3.0, "on is at the far end, the left");
}

#[test]
fn a_switch_label_is_written_after_the_pill_inside_the_box() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| one_switch(ui, false));
    let node = bounds(&context, &["wifi"]);
    let (track, _) = switch_parts(&context);
    assert_eq!(track.x, node.x + 12.0);
    let text = label_rect(&context, "Wi-Fi");
    assert!(text.x >= track.x + track.width, "{text:?} after {track:?}");
    assert!(
        text.x + text.width <= node.x + node.width,
        "the label fits the box"
    );
}
