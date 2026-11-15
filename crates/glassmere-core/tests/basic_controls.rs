mod support;

use glassmere_core::app::Action as Msg;
use glassmere_core::{
    Action, Button, Collapsible, Context, Direction, Event, Key, Length, Link, MonospaceMeasurer,
    Progress, Role, Spinner, Style, Switch, Transition, Ui,
};
use glassmere_scene::{Color, Point, Primitive, Rect, Segment, Size};
use std::ops::RangeInclusive;
use std::time::Duration;
use support::{Driven, act, center, click, on};

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

struct Sections {
    open: bool,
    toggles: Vec<bool>,
    resets: u32,
}

fn sections_view(state: &Sections, ui: &mut Ui<'_, Msg<Sections>>) {
    ui.keyed("advanced", |ui| {
        ui.collapsible(
            "Advanced",
            state.open,
            on(|s: &mut Sections, open: bool| {
                s.open = open;
                s.toggles.push(open);
            }),
            |ui| {
                ui.keyed("reset", |ui| {
                    ui.button(Button::new("Reset").on_click(act(|s: &mut Sections| s.resets += 1)))
                });
            },
        )
    });
}

fn sections(open: bool) -> Driven<Sections> {
    Driven::new(
        ROOT,
        Sections {
            open,
            toggles: Vec::new(),
            resets: 0,
        },
        sections_view,
    )
}

const HEADER: [&str; 2] = ["advanced", "$header"];
const RESET: [&str; 3] = ["advanced", "$content", "reset"];

#[test]
fn a_collapsible_declares_its_content_only_while_open() {
    let mut app = sections(false);
    assert!(app.context.find(&HEADER).is_some());
    assert!(app.context.find(&RESET).is_none());
    app.state_mut().open = true;
    app.frame();
    assert!(app.context.find(&RESET).is_some());
    assert!(support::shows(&app.context, "Reset"));
    app.state_mut().open = false;
    app.frame();
    assert!(app.context.find(&RESET).is_none());
    assert!(!support::shows(&app.context, "Reset"));
}

#[test]
fn a_collapsible_header_is_a_button_that_reports_whether_it_is_expanded() {
    let mut app = sections(false);
    let header = app.context.find(&HEADER).unwrap();
    let node = app.context.node(header).unwrap();
    assert_eq!(node.semantics.role, Role::Button);
    assert_eq!(node.semantics.label, "Advanced");
    assert!(node.semantics.focusable);
    assert_eq!(node.expanded, Some(false));
    app.state_mut().open = true;
    app.frame();
    assert_eq!(app.context.node(header).unwrap().expanded, Some(true));
    assert!(support::shows(&app.context, "Advanced"));
}

#[test]
fn clicking_the_header_sends_the_state_it_asks_for_once() {
    let mut app = sections(false);
    click_on(&mut app.context, &HEADER);
    app.frame();
    assert_eq!(app.state().toggles, [true]);
    assert!(app.context.find(&RESET).is_some());
    click_on(&mut app.context, &HEADER);
    app.frame();
    assert_eq!(app.state().toggles, [true, false]);
    assert!(app.context.find(&RESET).is_none());
    app.frame();
    assert_eq!(app.state().toggles.len(), 2);
}

#[test]
fn enter_and_space_toggle_and_the_content_is_reachable_by_tab_only_while_open() {
    let mut app = sections(false);
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&HEADER));
    press(&mut app.context, Key::Space);
    app.frame();
    assert_eq!(app.state().toggles, [true]);
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&RESET));
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&HEADER));
    press(&mut app.context, Key::Enter);
    app.frame();
    assert_eq!(app.state().toggles, [true, false]);
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&HEADER));
}

#[test]
fn right_opens_and_left_closes_and_the_other_arrow_does_nothing() {
    let mut app = sections(false);
    press(&mut app.context, Key::Tab);
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert!(app.state().toggles.is_empty(), "Left on a closed section");
    press(&mut app.context, Key::ArrowRight);
    app.frame();
    assert_eq!(app.state().toggles, [true]);
    press(&mut app.context, Key::ArrowRight);
    app.frame();
    assert_eq!(app.state().toggles, [true], "Right on an open section");
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert_eq!(app.state().toggles, [true, false]);
}

#[test]
fn the_arrows_are_not_mirrored_in_right_to_left_text() {
    let mut app = sections(false);
    app.context.set_direction(Direction::Rtl);
    app.frame();
    press(&mut app.context, Key::Tab);
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert!(app.state().toggles.is_empty());
    press(&mut app.context, Key::ArrowRight);
    app.frame();
    assert_eq!(app.state().toggles, [true]);
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert_eq!(app.state().toggles, [true, false]);
}

#[test]
fn assistive_technology_expands_and_collapses_only_when_that_changes_something() {
    let mut app = sections(false);
    let header = app.context.find(&HEADER).unwrap();
    assert!(!app.context.action(header, Action::Collapse));
    assert!(app.context.action(header, Action::Expand));
    app.frame();
    assert!(!app.context.action(header, Action::Expand));
    assert!(app.context.action(header, Action::Collapse));
    app.frame();
    assert_eq!(app.state().toggles, [true, false]);
    assert!(app.context.action(header, Action::Activate));
    app.frame();
    assert_eq!(app.state().toggles, [true, false, true]);
}

#[test]
fn a_disabled_collapsible_cannot_be_toggled_and_disables_what_it_shows() {
    let mut app = Driven::new(
        ROOT,
        Sections {
            open: true,
            toggles: Vec::new(),
            resets: 0,
        },
        |state, ui| ui.enabled(false, |ui| sections_view(state, ui)),
    );
    let header = app.context.find(&HEADER).unwrap();
    let node = app.context.node(header).unwrap();
    assert!(node.semantics.disabled);
    assert!(!node.semantics.focusable);
    assert!(!app.context.action(header, Action::Collapse));
    click_on(&mut app.context, &HEADER);
    click_on(&mut app.context, &RESET);
    app.frame();
    assert!(app.state().toggles.is_empty());
    assert_eq!(app.state().resets, 0);
}

#[test]
fn the_content_keeps_working_while_the_section_is_open() {
    let mut app = sections(true);
    click_on(&mut app.context, &RESET);
    app.frame();
    assert_eq!(app.state().resets, 1);
}

/// The points of the chevron the header draws.
fn chevron(context: &Context) -> Vec<Point> {
    let paths: Vec<_> = context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Path { path, .. } => Some(path.segments().to_vec()),
            _ => None,
        })
        .collect();
    assert_eq!(paths.len(), 1, "one chevron is drawn");
    paths[0]
        .iter()
        .filter_map(|segment| match segment {
            Segment::Move(point) | Segment::Line(point) => Some(*point),
            _ => None,
        })
        .collect()
}

#[test]
fn the_chevron_points_along_the_line_when_closed_and_down_when_open() {
    let mut app = sections(false);
    let header = bounds(&app.context, &HEADER);
    let closed = chevron(&app.context);
    assert_eq!(closed.len(), 3);
    assert!(closed[1].x > closed[0].x, "the tip is toward the end");
    assert_eq!(closed[0].y + closed[2].y, closed[1].y * 2.0);
    assert!(closed.iter().all(|point| point.x < header.x + 20.0));
    app.state_mut().open = true;
    app.frame();
    let open = chevron(&app.context);
    assert!(open[1].y > open[0].y, "the tip is down");
    assert_eq!(open[0].x + open[2].x, open[1].x * 2.0);
}

#[test]
fn the_chevron_and_the_label_swap_sides_in_right_to_left_text() {
    let mut app = sections(false);
    app.context.set_direction(Direction::Rtl);
    app.frame();
    let header = bounds(&app.context, &HEADER);
    let closed = chevron(&app.context);
    assert!(
        closed[1].x < closed[0].x,
        "the tip is toward the end, the left"
    );
    assert!(
        closed
            .iter()
            .all(|point| point.x > header.x + header.width - 20.0)
    );
    let text = label_rect(&app.context, "Advanced");
    assert!(
        text.x + text.width <= header.x + header.width - 20.0,
        "the label is beside the chevron: {text:?}"
    );
}

#[test]
fn hovering_the_header_paints_a_background_and_a_header_style_replaces_the_default() {
    let mut app = sections(false);
    assert!(quads(&app.context, app.context.theme().control_hover).is_empty());
    let at = center(&app.context, &HEADER);
    app.context.event(Event::PointerMoved(at));
    app.frame();
    let header = bounds(&app.context, &HEADER);
    assert_eq!(
        quads(&app.context, app.context.theme().control_hover),
        [header]
    );

    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.keyed("advanced", |ui| {
            ui.collapsible(
                Collapsible::new("Advanced").style(Style {
                    width: Length::Px(300.0),
                    padding: 20.0,
                    ..Style::default()
                }),
                false,
                |_| (),
                |_| (),
            )
        });
    });
    let header = bounds(&context, &HEADER);
    assert_eq!(header.width, 300.0);
}

struct Links {
    opened: u32,
}

fn links_view(_: &Links, ui: &mut Ui<'_, Msg<Links>>) {
    ui.keyed("page", |ui| links_column(ui));
}

fn links_column(ui: &mut Ui<'_, Msg<Links>>) {
    ui.column(Style::default().gap(12.0), |ui| {
        ui.keyed("docs", |ui| {
            ui.link(Link::new("Documentation").on_click(act(|s: &mut Links| s.opened += 1)))
        });
        ui.enabled(false, |ui| {
            ui.keyed("locked", |ui| {
                ui.link(Link::new("Locked").on_click(act(|s: &mut Links| s.opened += 100)))
            });
        });
        ui.keyed("off", |ui| {
            ui.link(
                Link::new("Off")
                    .enabled(false)
                    .on_click(act(|s: &mut Links| s.opened += 1000)),
            )
        });
        ui.keyed("quiet", |ui| ui.link("No message"));
    });
}

fn links() -> Driven<Links> {
    Driven::new(ROOT, Links { opened: 0 }, links_view)
}

#[test]
fn a_link_sends_its_message_for_a_click_and_for_assistive_technology() {
    let mut app = links();
    click_on(&mut app.context, &["page", "docs"]);
    app.frame();
    assert_eq!(app.state().opened, 1);
    let docs = app.context.find(&["page", "docs"]).unwrap();
    assert!(app.context.action(docs, Action::Activate));
    app.frame();
    assert_eq!(app.state().opened, 2);
    app.frame();
    assert_eq!(app.state().opened, 2, "one message per activation");
}

#[test]
fn enter_activates_a_focused_link_and_space_does_not() {
    let mut app = links();
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&["page", "docs"]));
    press(&mut app.context, Key::Space);
    app.frame();
    assert_eq!(app.state().opened, 0);
    press(&mut app.context, Key::Enter);
    app.frame();
    assert_eq!(app.state().opened, 1);
}

#[test]
fn a_disabled_link_cannot_be_focused_or_activated_and_says_so() {
    let mut app = links();
    for name in ["locked", "off"] {
        let id = app.context.find(&["page", name]).unwrap();
        assert!(!app.context.action(id, Action::Activate));
        click_on(&mut app.context, &["page", name]);
        let node = app.context.node(id).unwrap();
        assert!(node.semantics.disabled, "{name}");
        assert!(!node.semantics.focusable, "{name}");
    }
    press(&mut app.context, Key::Tab);
    press(&mut app.context, Key::Tab);
    assert_eq!(
        app.context.focused(),
        app.context.find(&["page", "quiet"]),
        "Tab skips the disabled links"
    );
    app.frame();
    assert_eq!(app.state().opened, 0);
}

#[test]
fn a_link_is_a_link_with_a_name_and_takes_the_keyboard_and_the_pointing_hand() {
    let app = links();
    let docs = app.context.find(&["page", "docs"]).unwrap();
    let node = app.context.node(docs).unwrap();
    assert_eq!(node.semantics.role, Role::Link);
    assert_eq!(node.semantics.label, "Documentation");
    // The window shows the pointing hand over anything focusable.
    assert!(node.semantics.focusable);
    assert!(!node.semantics.disabled);
    // Without a message it is still a link: the application may only listen.
    let quiet = app.context.find(&["page", "quiet"]).unwrap();
    assert_eq!(app.context.node(quiet).unwrap().semantics.role, Role::Link);
}

/// The color and the font size of the text drawn for `label`.
fn text_style(context: &Context, label: &str) -> (Color, f32) {
    context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::Text {
                text,
                color,
                font_size,
                ..
            } if text == label => Some((*color, *font_size)),
            Primitive::StyledText {
                text, color, style, ..
            } if text == label => Some((*color, style.font_size)),
            _ => None,
        })
        .expect("the text is drawn")
}

/// The one-pixel rule drawn in `color`, which is how a link is underlined.
fn rule(context: &Context, color: Color) -> Rect {
    let rules: Vec<Rect> = quads(context, color)
        .into_iter()
        .filter(|rect| rect.height == 1.0)
        .collect();
    assert_eq!(rules.len(), 1, "one underline is drawn: {rules:?}");
    rules[0]
}

fn one_link(ui: &mut Ui<'_>, style: Option<Style>) {
    ui.keyed("docs", |ui| {
        let link = Link::new("Documentation");
        ui.link(match style {
            Some(style) => link.style(style),
            None => link,
        })
    });
}

#[test]
fn a_link_is_underlined_text_in_the_accent_color() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| one_link(ui, None));
    let accent = context.theme().accent;
    let (color, font_size) = text_style(&context, "Documentation");
    assert_eq!(color, accent);
    let text = label_rect(&context, "Documentation");
    let line = rule(&context, accent);
    let width = "Documentation".chars().count() as f32 * font_size * 0.6;
    assert_eq!(line.x, text.x);
    assert_eq!(line.width, width, "the rule is as wide as the text");
    assert!(
        line.y > text.y && line.y < text.y + text.height,
        "{line:?} is under the text {text:?}"
    );
    let node = bounds(&context, &["docs"]);
    assert!(
        line.x >= node.x && line.x + line.width <= node.x + node.width,
        "the underline stays inside the link's box {node:?}"
    );
}

#[test]
fn a_link_style_may_choose_the_color_and_never_wraps() {
    let mut context = Context::new();
    let green = Color::rgb(30, 160, 60);
    frame_at(&mut context, 0, |ui| {
        one_link(
            ui,
            Some(Style {
                foreground: Some(green),
                width: Length::Px(60.0),
                ..Style::default()
            }),
        )
    });
    assert_eq!(text_style(&context, "Documentation").0, green);
    let line = rule(&context, green);
    assert!(
        line.x + line.width <= bounds(&context, &["docs"]).x + 60.0,
        "{line:?} is cut to the box"
    );
    assert!(quads(&context, context.theme().accent).is_empty());
}

#[test]
fn a_disabled_link_is_dimmed_with_its_underline() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.keyed("off", |ui| ui.link(Link::new("Off").enabled(false)));
    });
    let dim = context.theme().text_disabled;
    assert_eq!(text_style(&context, "Off").0, dim);
    rule(&context, dim);
    assert!(quads(&context, context.theme().accent).is_empty());
}

#[test]
fn a_link_in_right_to_left_text_is_underlined_where_its_text_is_at_the_end_edge() {
    let mut context = Context::new();
    context.set_direction(Direction::Rtl);
    frame_at(&mut context, 0, |ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default().width(Length::Px(300.0)), |ui| {
                ui.keyed("docs", |ui| {
                    ui.link(Link::new("Documentation").style(Style {
                        width: Length::Px(200.0),
                        padding: 3.0,
                        ..Style::default()
                    }));
                });
            })
        });
    });
    let (_, font_size) = text_style(&context, "Documentation");
    let node = bounds(&context, &["page", "docs"]);
    let text = label_rect(&context, "Documentation");
    let line = rule(&context, context.theme().accent);
    let width = "Documentation".chars().count() as f32 * font_size * 0.6;
    assert_eq!(line.width, width);
    assert_eq!(line.x, text.x, "under the text");
    assert_eq!(
        line.x + line.width,
        node.x + node.width - 3.0,
        "the text ends at the end edge, the right"
    );
}

struct Prices {
    low: f64,
    high: f64,
    changes: Vec<(f64, f64)>,
}

fn price_slider(state: &Prices, ui: &mut Ui<'_, Msg<Prices>>) {
    ui.keyed("price", |ui| {
        ui.range_slider(
            "Price",
            state.low..=state.high,
            0.0..=100.0,
            10.0,
            on(|s: &mut Prices, range: RangeInclusive<f64>| {
                (s.low, s.high) = (*range.start(), *range.end());
                s.changes.push((s.low, s.high));
            }),
        );
    });
}

fn prices(low: f64, high: f64) -> Driven<Prices> {
    Driven::new(
        ROOT,
        Prices {
            low,
            high,
            changes: Vec::new(),
        },
        price_slider,
    )
}

const MINIMUM: [&str; 2] = ["price", "$minimum"];
const MAXIMUM: [&str; 2] = ["price", "$maximum"];

/// Where the thumb for `fraction` of the range is, in a box that runs left to right.
fn along(context: &Context, fraction: f32) -> Point {
    let bounds = bounds(context, &["price"]);
    Point {
        x: bounds.x + 12.0 + 256.0 * fraction,
        y: bounds.y + 49.0,
    }
}

fn press_at(app: &mut Driven<Prices>, fraction: f32) {
    let at = along(&app.context, fraction);
    click(&mut app.context, at);
    app.frame();
}

fn pressed(app: &Driven<Prices>) -> (f64, f64) {
    (app.state().low, app.state().high)
}

#[test]
fn a_range_slider_is_two_sliders_named_after_the_label_with_their_own_values() {
    let app = prices(20.0, 80.0);
    let node = |path: &[&str]| app.context.node(app.context.find(path).unwrap()).unwrap();
    let (low, high) = (node(&MINIMUM), node(&MAXIMUM));
    for (thumb, label) in [(&low, "Price minimum"), (&high, "Price maximum")] {
        assert_eq!(thumb.semantics.role, Role::Slider);
        assert_eq!(thumb.semantics.label, label);
        assert!(thumb.semantics.focusable && !thumb.semantics.disabled);
    }
    // Each can go as far as the other: that is what its range is.
    let numeric = |n: Option<glassmere_core::NumericValue>| {
        n.map(|n| (n.value, n.min, n.max, n.step)).unwrap()
    };
    assert_eq!(numeric(low.numeric), (20.0, 0.0, 80.0, 10.0));
    assert_eq!(numeric(high.numeric), (80.0, 20.0, 100.0, 10.0));
    let group = node(&["price"]);
    assert_eq!(group.semantics.role, Role::Group);
    assert_eq!(group.semantics.label, "Price");
    assert!(!group.semantics.focusable);
    assert!(support::shows(&app.context, "Price: 20.00 \u{2013} 80.00"));
}

#[test]
fn a_value_outside_the_range_or_in_the_wrong_order_is_shown_clamped_and_ordered() {
    let app = prices(90.0, 10.0);
    let value = |path: &[&str]| {
        app.context
            .node(app.context.find(path).unwrap())
            .unwrap()
            .numeric
            .unwrap()
            .value
    };
    assert_eq!((value(&MINIMUM), value(&MAXIMUM)), (10.0, 90.0));
    let app = prices(-5.0, 500.0);
    let value = |path: &[&str]| {
        app.context
            .node(app.context.find(path).unwrap())
            .unwrap()
            .numeric
            .unwrap()
            .value
    };
    assert_eq!((value(&MINIMUM), value(&MAXIMUM)), (0.0, 100.0));
}

#[test]
fn each_thumb_takes_the_keyboard_in_turn_and_sends_the_whole_range() {
    let mut app = prices(20.0, 80.0);
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&MINIMUM));
    press(&mut app.context, Key::ArrowRight);
    app.frame();
    assert_eq!(pressed(&app), (30.0, 80.0));
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), app.context.find(&MAXIMUM));
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert_eq!(pressed(&app), (30.0, 70.0));
    press(&mut app.context, Key::End);
    app.frame();
    assert_eq!(pressed(&app), (30.0, 100.0));
    press(&mut app.context, Key::PageDown);
    app.frame();
    assert_eq!(pressed(&app), (30.0, 30.0), "as low as the other thumb");
    assert_eq!(
        app.state().changes,
        [(30.0, 80.0), (30.0, 70.0), (30.0, 100.0), (30.0, 30.0)]
    );
}

#[test]
fn the_thumbs_never_cross() {
    let mut app = prices(50.0, 60.0);
    press(&mut app.context, Key::Tab);
    press(&mut app.context, Key::PageUp);
    app.frame();
    assert_eq!(pressed(&app), (60.0, 60.0));
    press(&mut app.context, Key::ArrowRight);
    app.frame();
    assert_eq!(app.state().changes, [(60.0, 60.0)], "no further");
    press(&mut app.context, Key::Tab);
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert_eq!(app.state().changes.len(), 1, "the other does not pass it");
    press(&mut app.context, Key::ArrowRight);
    app.frame();
    assert_eq!(pressed(&app), (60.0, 70.0));
}

#[test]
fn a_press_takes_the_nearer_thumb_and_moves_it_to_the_press() {
    let mut app = prices(20.0, 80.0);
    press_at(&mut app, 0.1);
    assert_eq!(pressed(&app), (10.0, 80.0));
    assert_eq!(app.context.focused(), app.context.find(&MINIMUM));
    press_at(&mut app, 0.9);
    assert_eq!(pressed(&app), (10.0, 90.0));
    assert_eq!(app.context.focused(), app.context.find(&MAXIMUM));
    // Beyond the end of the track is the end.
    press_at(&mut app, 1.0);
    assert_eq!(pressed(&app), (10.0, 100.0));
    // Between them, the one that is nearer.
    press_at(&mut app, 0.3);
    assert_eq!(pressed(&app), (30.0, 100.0));
}

#[test]
fn halfway_between_the_thumbs_a_press_takes_the_greater_and_on_both_the_one_that_can_move() {
    let mut app = prices(20.0, 80.0);
    press_at(&mut app, 0.5);
    assert_eq!(pressed(&app), (20.0, 50.0));

    let mut app = prices(60.0, 60.0);
    press_at(&mut app, 0.9);
    assert_eq!(pressed(&app), (60.0, 90.0), "right of both: the greater");
    let mut app = prices(60.0, 60.0);
    press_at(&mut app, 0.2);
    assert_eq!(pressed(&app), (20.0, 60.0), "left of both: the lesser");
}

#[test]
fn a_drag_follows_the_pointer_and_stops_at_the_other_thumb() {
    let mut app = prices(20.0, 80.0);
    let (from, to) = (along(&app.context, 0.2), along(&app.context, 0.95));
    app.context.event(Event::PointerMoved(from));
    app.context.event(Event::PointerDown);
    app.context
        .event(Event::PointerMoved(along(&app.context, 0.5)));
    app.frame();
    assert_eq!(pressed(&app), (50.0, 80.0));
    app.context.event(Event::PointerMoved(to));
    app.frame();
    assert_eq!(pressed(&app), (80.0, 80.0));
    app.context.event(Event::PointerUp);
    app.frame();
    assert!(app.state().changes.iter().all(|(low, high)| low <= high));
    assert_eq!(app.state().changes.last(), Some(&(80.0, 80.0)));
    // The drag took the thumb it began on, and keeps it.
    assert_eq!(app.context.focused(), app.context.find(&MINIMUM));
}

#[test]
fn a_drag_repaints_only_the_slider() {
    let mut app = prices(20.0, 80.0);
    let node = bounds(&app.context, &["price"]);
    let from = along(&app.context, 0.2);
    app.context.event(Event::PointerMoved(from));
    app.context.event(Event::PointerDown);
    app.context
        .event(Event::PointerMoved(along(&app.context, 0.5)));
    app.frame();
    let regions = app.context.damage_regions();
    assert!(!regions.is_empty());
    for region in regions {
        assert!(
            region.x >= node.x - 1.0
                && region.y >= node.y - 1.0
                && region.x + region.width <= node.x + node.width + 1.0
                && region.y + region.height <= node.y + node.height + 1.0,
            "{region:?} is outside the slider {node:?}"
        );
    }
}

#[test]
fn a_disabled_range_slider_takes_neither_keys_nor_presses() {
    let mut app = Driven::new(
        ROOT,
        Prices {
            low: 20.0,
            high: 80.0,
            changes: Vec::new(),
        },
        |state, ui| ui.enabled(false, |ui| price_slider(state, ui)),
    );
    for path in [MINIMUM, MAXIMUM] {
        let id = app.context.find(&path).unwrap();
        let node = app.context.node(id).unwrap();
        assert!(node.semantics.disabled && !node.semantics.focusable);
        assert!(!app.context.action(id, Action::Increment));
    }
    press(&mut app.context, Key::Tab);
    assert_eq!(app.context.focused(), None);
    press_at(&mut app, 0.5);
    press_at(&mut app, 0.1);
    assert!(app.state().changes.is_empty());
}

#[test]
fn assistive_technology_steps_and_sets_each_thumb() {
    let mut app = prices(20.0, 80.0);
    let (low, high) = (
        app.context.find(&MINIMUM).unwrap(),
        app.context.find(&MAXIMUM).unwrap(),
    );
    assert!(app.context.action(low, Action::Increment));
    app.frame();
    assert_eq!(pressed(&app), (30.0, 80.0));
    assert!(app.context.action(high, Action::SetNumber(55.0)));
    app.frame();
    assert_eq!(pressed(&app), (30.0, 60.0), "snapped to the step");
    assert!(app.context.action(low, Action::SetNumber(95.0)));
    app.frame();
    assert_eq!(pressed(&app), (60.0, 60.0), "no further than the other");
    assert!(!app.context.action(high, Action::Decrement));
}

/// The centers of the two thumbs, minimum first.
fn thumb_centers(context: &Context) -> Vec<f32> {
    let thumbs = quads(context, context.theme().thumb);
    assert_eq!(thumbs.len(), 2, "two thumbs are drawn");
    thumbs
        .iter()
        .map(|rect| {
            assert_eq!((rect.width, rect.height), (14.0, 14.0));
            rect.x + rect.width / 2.0
        })
        .collect()
}

/// The halos drawn around a focused thumb.
fn rings(context: &Context) -> Vec<Rect> {
    quads(context, context.theme().focus)
        .into_iter()
        .filter(|rect| (rect.width, rect.height) == (18.0, 18.0))
        .collect()
}

#[test]
fn the_track_is_filled_between_the_thumbs_and_the_focused_thumb_has_a_ring() {
    let mut app = prices(20.0, 80.0);
    let node = bounds(&app.context, &["price"]);
    let centers = thumb_centers(&app.context);
    assert_eq!(centers, [node.x + 12.0 + 51.2, node.x + 12.0 + 204.8]);
    let fill: Vec<Rect> = quads(&app.context, app.context.theme().accent)
        .into_iter()
        .filter(|rect| rect.height == 4.0)
        .collect();
    assert_eq!(fill.len(), 1);
    assert!((fill[0].x - centers[0]).abs() < 0.01);
    assert!((fill[0].x + fill[0].width - centers[1]).abs() < 0.01);
    assert!(rings(&app.context).is_empty());
    press(&mut app.context, Key::Tab);
    app.frame();
    let ring = rings(&app.context);
    assert_eq!(ring.len(), 1, "one ring, on the focused thumb");
    assert_eq!(
        ring[0].x + ring[0].width / 2.0,
        thumb_centers(&app.context)[0]
    );
    press(&mut app.context, Key::Tab);
    app.frame();
    let ring = rings(&app.context);
    assert_eq!(ring.len(), 1);
    assert_eq!(
        ring[0].x + ring[0].width / 2.0,
        thumb_centers(&app.context)[1]
    );
}

#[test]
fn in_right_to_left_text_the_minimum_is_at_the_right_and_the_arrows_follow_the_screen() {
    let mut app = prices(20.0, 80.0);
    app.context.set_direction(Direction::Rtl);
    app.frame();
    let node = bounds(&app.context, &["price"]);
    let centers = thumb_centers(&app.context);
    assert_eq!(centers, [node.x + 12.0 + 204.8, node.x + 12.0 + 51.2]);
    let fill: Vec<Rect> = quads(&app.context, app.context.theme().accent)
        .into_iter()
        .filter(|rect| rect.height == 4.0)
        .collect();
    assert_eq!(fill.len(), 1);
    assert!((fill[0].x - centers[1]).abs() < 0.01);
    assert!((fill[0].x + fill[0].width - centers[0]).abs() < 0.01);
    press(&mut app.context, Key::Tab);
    press(&mut app.context, Key::ArrowLeft);
    app.frame();
    assert_eq!(pressed(&app), (30.0, 80.0), "Left goes toward the greater");
    // A press near the right edge is near the minimum.
    let at = Point {
        x: node.x + node.width - 14.0,
        y: node.y + 49.0,
    };
    click(&mut app.context, at);
    app.frame();
    assert_eq!(pressed(&app), (0.0, 80.0));
}

#[test]
#[should_panic(expected = "range_slider requires")]
fn a_range_slider_without_a_step_is_misuse() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.range_slider("Price", 1.0..=2.0, 0.0..=10.0, 0.0, |_| ());
    });
}

#[test]
#[should_panic(expected = "range_slider requires")]
fn a_range_slider_over_an_empty_range_is_misuse() {
    let mut context = Context::new();
    frame_at(&mut context, 0, |ui| {
        ui.range_slider("Price", 5.0..=5.0, 5.0..=5.0, 1.0, |_| ());
    });
}
