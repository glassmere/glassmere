mod support;

use glassmere_core::app::Action as Msg;
use glassmere_core::{
    Action, Button, Collapsible, Context, Direction, Event, Key, Length, MonospaceMeasurer,
    Progress, Role, Spinner, Style, Switch, Transition, Ui,
};
use glassmere_scene::{Color, Point, Primitive, Rect, Segment, Size};
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
