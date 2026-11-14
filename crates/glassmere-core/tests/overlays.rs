mod support;

use glassmere_core::{
    Action, Axis, Context, Event, Key, Length, MenuItem, MonospaceMeasurer, PopoverOptions, Role,
    SplitOptions, Style, TOOLTIP_DELAY, Tab, Ui,
};
use glassmere_scene::{Point, Primitive, Size};
use std::time::Duration;
use support::{Driven, center, click, on};

const ROOT: Size = Size::new(500.0, 400.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = glassmere_core::app::Action<S>;

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> glassmere_core::FrameStats {
    context.run(ROOT, &mut MonospaceMeasurer, view)
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

fn focused_key(context: &Context) -> String {
    context
        .focused()
        .and_then(|id| context.details(id))
        .map_or_else(|| "none".into(), |details| details.key)
}

fn bounds(context: &Context, path: &[&str]) -> glassmere_scene::Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

fn click_at(context: &mut Context, path: &[&str]) {
    let point = center(context, path);
    click(context, point);
}

fn scene_text(context: &Context, needle: &str) -> bool {
    context.scene().primitives.iter().any(|primitive| {
        matches!(primitive, Primitive::StyledText { text, .. } | Primitive::Text { text, .. } if text == needle)
    })
}

#[test]
fn tooltips_appear_after_a_dwell_request_a_wake_and_hide_on_movement_or_input() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("save", |ui| ui.button("Save"));
                ui.tooltip("Saves the document");
                ui.keyed("plain", |ui| ui.button("Plain"));
            })
        });
    };
    context.set_now(Duration::ZERO);
    frame(&mut context, view);
    let save = context.find(&["page", "save"]).unwrap();
    assert_eq!(
        context.node(save).unwrap().description.as_deref(),
        Some("Saves the document")
    );
    assert_eq!(context.next_wake(), None);
    context.event(Event::PointerMoved(center(&context, &["page", "save"])));
    assert_eq!(context.next_wake(), Some(TOOLTIP_DELAY));
    frame(&mut context, view);
    assert!(!scene_text(&context, "Saves the document"), "not yet due");
    context.set_now(TOOLTIP_DELAY + Duration::from_millis(100));
    let stats = frame(&mut context, view);
    assert!(stats.scene_rebuilt);
    assert!(scene_text(&context, "Saves the document"));
    assert_eq!(context.next_wake(), None, "nothing pending while shown");
    // Moving onto a widget without a tooltip hides it and requests no wake.
    context.event(Event::PointerMoved(center(&context, &["page", "plain"])));
    frame(&mut context, view);
    assert!(!scene_text(&context, "Saves the document"));
    assert_eq!(context.next_wake(), None);
    // Pressing hides an active tooltip and does not re-arm until the pointer moves on.
    context.event(Event::PointerMoved(center(&context, &["page", "save"])));
    context.set_now(Duration::from_secs(2));
    frame(&mut context, view);
    assert!(scene_text(&context, "Saves the document"));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    frame(&mut context, view);
    assert!(!scene_text(&context, "Saves the document"));
    assert_eq!(context.next_wake(), None);
}

#[derive(Default)]
struct Tabs {
    selected: usize,
    closed: Vec<usize>,
}

fn tabs_page(state: &Tabs, ui: &mut Ui<'_, Handler<Tabs>>) {
    let tabs = [Tab::new("One"), Tab::closable("Two"), Tab::new("Three")];
    ui.keyed("before", |ui| ui.button("Before"));
    ui.keyed("tabs", |ui| {
        ui.tabs_close(
            state.selected,
            &tabs,
            on(|s: &mut Tabs, index: usize| s.selected = index),
            on(|s: &mut Tabs, index: usize| s.closed.push(index)),
            |ui, index| {
                ui.keyed("body", |ui| ui.label(format!("panel {index}")));
                ui.keyed("inner", |ui| ui.button("Inner"));
            },
        );
    });
    ui.keyed("after", |ui| ui.button("After"));
}

#[test]
fn tabs_select_by_click_and_arrows_expose_roles_and_report_closes() {
    let mut driven = Driven::new(ROOT, Tabs::default(), tabs_page);
    let context = &driven.context;
    let strip = context.find(&["tabs", "$strip"]).unwrap();
    assert_eq!(context.node(strip).unwrap().semantics.role, Role::TabList);
    let first = context.find(&["tabs", "$strip", "0"]).unwrap();
    assert_eq!(context.node(first).unwrap().semantics.role, Role::Tab);
    assert_eq!(context.node(first).unwrap().selected, Some(true));
    let panel = context.find(&["tabs", "$panel"]).unwrap();
    assert_eq!(context.node(panel).unwrap().semantics.role, Role::TabPanel);
    assert!(scene_text(context, "panel 0"));
    // Click the second tab.
    click_at(&mut driven.context, &["tabs", "$strip", "1"]);
    driven.frame();
    assert_eq!(driven.state().selected, 1);
    assert!(scene_text(&driven.context, "panel 1"));
    // Only the selected tab is in the Tab order; Tab then enters the panel.
    let before = driven.context.find(&["before"]).unwrap();
    driven.context.action(before, Action::Focus);
    key(&mut driven.context, Key::Tab);
    assert_eq!(focused_key(&driven.context), "1");
    key(&mut driven.context, Key::Tab);
    assert_eq!(focused_key(&driven.context), "$close-1");
    key(&mut driven.context, Key::Tab);
    assert_eq!(focused_key(&driven.context), "inner");
    // Arrows on the focused tab switch immediately and keep focus on the strip.
    let tab = driven.context.find(&["tabs", "$strip", "1"]).unwrap();
    driven.context.action(tab, Action::Focus);
    key(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert_eq!(
        (
            driven.state().selected,
            focused_key(&driven.context).as_str()
        ),
        (2, "2")
    );
    key(&mut driven.context, Key::Home);
    driven.frame();
    assert_eq!(
        (
            driven.state().selected,
            focused_key(&driven.context).as_str()
        ),
        (0, "0")
    );
    key(&mut driven.context, Key::End);
    driven.frame();
    assert_eq!(driven.state().selected, 2);
    key(&mut driven.context, Key::ArrowLeft);
    driven.frame();
    assert_eq!(driven.state().selected, 1);
    assert!(driven.state().closed.is_empty());
    // Close through the button, and through Delete on the focused closable tab.
    click_at(&mut driven.context, &["tabs", "$strip", "$close-1"]);
    driven.frame();
    assert_eq!(driven.state().closed, [1]);
    driven.frame();
    assert_eq!(driven.state().closed, [1], "a close is sent once");
    let tab = driven.context.find(&["tabs", "$strip", "1"]).unwrap();
    driven.context.action(tab, Action::Focus);
    key(&mut driven.context, Key::Delete);
    driven.frame();
    assert_eq!(driven.state().closed, [1, 1]);
    // Delete on a non-closable tab does nothing.
    key(&mut driven.context, Key::ArrowRight);
    driven.frame();
    key(&mut driven.context, Key::Delete);
    driven.frame();
    assert_eq!(driven.state().closed, [1, 1]);
}

#[derive(Default)]
struct Split {
    fraction: f32,
}

fn split_page(state: &Split, ui: &mut Ui<'_, Handler<Split>>) {
    ui.keyed("split", |ui| {
        ui.split(
            state.fraction,
            Axis::Horizontal,
            SplitOptions {
                min_first: 100.0,
                min_second: 100.0,
                divider: 10.0,
                style: Style::default()
                    .width(Length::Px(500.0))
                    .height(Length::Px(200.0)),
            },
            on(|s: &mut Split, fraction: f32| s.fraction = fraction),
            |ui| ui.keyed("a", |ui| ui.label("A")),
            |ui| ui.keyed("b", |ui| ui.label("B")),
        );
    });
}

#[test]
fn split_dividers_drag_within_minimums_move_with_keys_and_accept_semantic_values() {
    let mut driven = Driven::new(ROOT, Split { fraction: 0.5 }, split_page);
    let first = bounds(&driven.context, &["split", "$first"]);
    let divider = driven.context.find(&["split", "$divider"]).unwrap();
    let second = bounds(&driven.context, &["split", "$second"]);
    assert_eq!((first.x, first.width), (0.0, 250.0));
    assert_eq!(driven.context.node(divider).unwrap().bounds.x, 250.0);
    assert_eq!((second.x, second.width), (260.0, 240.0));
    let snapshot = driven.context.node(divider).unwrap();
    assert_eq!(snapshot.semantics.role, Role::Splitter);
    assert_eq!(snapshot.orientation, Some(Axis::Horizontal));
    assert_eq!(snapshot.numeric.unwrap().value, 50.0);
    // Drag to the right, then past the second pane's minimum.
    driven
        .context
        .event(Event::PointerMoved(Point { x: 255.0, y: 100.0 }));
    driven.context.event(Event::PointerDown);
    assert_eq!(driven.context.captured(), Some(divider));
    driven
        .context
        .event(Event::PointerMoved(Point { x: 300.0, y: 100.0 }));
    driven.frame();
    let fraction = driven.state().fraction;
    assert!((fraction - 295.0 / 490.0).abs() < 0.001, "{fraction}");
    let first = bounds(&driven.context, &["split", "$first"]);
    assert!((first.width - 500.0 * fraction).abs() < 1.0);
    driven
        .context
        .event(Event::PointerMoved(Point { x: 480.0, y: 100.0 }));
    driven.frame();
    let fraction = driven.state().fraction;
    assert!(
        (fraction - (1.0 - 100.0 / 490.0)).abs() < 0.001,
        "clamped: {fraction}"
    );
    driven.context.event(Event::PointerUp);
    // Keyboard: ten logical pixels per step, Home to the first minimum.
    driven.context.action(divider, Action::Focus);
    let before = driven.state().fraction;
    key(&mut driven.context, Key::ArrowLeft);
    driven.frame();
    assert!((before - driven.state().fraction - 10.0 / 500.0).abs() < 0.001);
    key(&mut driven.context, Key::Home);
    driven.frame();
    assert!((driven.state().fraction - 100.0 / 490.0).abs() < 0.001);
    // Accessibility sets a percentage.
    assert!(driven.context.action(divider, Action::SetNumber(50.0)));
    driven.frame();
    assert_eq!(driven.state().fraction, 0.5);
}

#[derive(Default)]
struct Menu {
    picks: Vec<usize>,
}

fn menu_page(_: &Menu, ui: &mut Ui<'_, Handler<Menu>>) {
    let items = [
        MenuItem::new("Copy"),
        MenuItem::disabled("Cut"),
        MenuItem::new("Paste"),
    ];
    ui.keyed("outside", |ui| ui.button("Outside"));
    ui.keyed("area", |ui| {
        ui.context_menu(
            &items,
            Style::default()
                .width(Length::Px(400.0))
                .height(Length::Px(200.0)),
            on(|s: &mut Menu, index: usize| s.picks.push(index)),
            |ui| {
                ui.keyed("text", |ui| ui.label("Right-click me"));
                ui.keyed("inner", |ui| ui.button("Inner"));
            },
        );
    });
}

#[test]
fn context_menus_open_at_the_pointer_or_focus_navigate_and_choose_once() {
    let mut driven = Driven::new(ROOT, Menu::default(), menu_page);
    assert!(driven.context.find(&["area", "$popup"]).is_none());
    assert!(
        !driven
            .context
            .event(Event::ContextMenu(Point { x: 10.0, y: 10.0 })),
        "outside the area"
    );
    assert!(
        driven
            .context
            .event(Event::ContextMenu(Point { x: 100.0, y: 120.0 }))
    );
    driven.frame();
    let popup = driven.context.find(&["area", "$popup"]).unwrap();
    let rect = driven.context.node(popup).unwrap().bounds;
    assert_eq!((rect.x, rect.y), (100.0, 120.0), "anchored at the pointer");
    assert!(driven.context.node(popup).unwrap().is_popup);
    key(&mut driven.context, Key::ArrowDown);
    key(&mut driven.context, Key::Enter);
    driven.frame();
    assert_eq!(driven.state().picks, [2], "skips the disabled item, once");
    driven.frame();
    assert_eq!(driven.state().picks, [2], "a choice is sent once");
    assert!(driven.context.find(&["area", "$popup"]).is_none());
    // Escape closes without a choice; near the right edge the menu stays on screen.
    driven
        .context
        .event(Event::ContextMenu(Point { x: 395.0, y: 100.0 }));
    driven.frame();
    let rect = bounds(&driven.context, &["area", "$popup"]);
    assert!(rect.x + rect.width <= ROOT.width + 0.5);
    key(&mut driven.context, Key::Escape);
    driven.frame();
    assert_eq!(driven.state().picks, [2]);
    assert!(driven.context.find(&["area", "$popup"]).is_none());
    // The keyboard opens the menu below the focused control inside the area.
    let inner = driven.context.find(&["area", "inner"]).unwrap();
    driven.context.action(inner, Action::Focus);
    let inner_bounds = driven.context.node(inner).unwrap().bounds;
    key(&mut driven.context, Key::ContextMenu);
    driven.frame();
    let rect = bounds(&driven.context, &["area", "$popup"]);
    assert_eq!(
        (rect.x, rect.y),
        (inner_bounds.x, inner_bounds.y + inner_bounds.height)
    );
    key(&mut driven.context, Key::Enter);
    driven.frame();
    assert_eq!(driven.state().picks, [2, 0]);
    assert_eq!(
        focused_key(&driven.context),
        "inner",
        "focus returns to where the menu was opened"
    );
}

#[derive(Default)]
struct Pops {
    open: bool,
    second: bool,
    dismissed: usize,
}

fn pop_page(state: &Pops, ui: &mut Ui<'_, Handler<Pops>>) {
    ui.keyed("bar", |ui| {
        ui.row(Style::default(), |ui| {
            ui.keyed("share", |ui| ui.button("Share"));
            ui.keyed("pop", |ui| {
                ui.popover(
                    state.open,
                    PopoverOptions::default(),
                    on(|s: &mut Pops, open: bool| {
                        if !open {
                            s.dismissed += 1;
                        }
                        s.open = open;
                    }),
                    |ui| {
                        ui.keyed("hint", |ui| ui.label("Share this"));
                        ui.keyed("copy", |ui| ui.button("Copy link"));
                    },
                );
            });
            ui.keyed("more", |ui| ui.button("More"));
            ui.keyed("pop2", |ui| {
                ui.popover(
                    state.second,
                    PopoverOptions::default(),
                    on(|s: &mut Pops, open: bool| s.second = open),
                    |ui| {
                        ui.keyed("extra", |ui| ui.button("Extra"));
                    },
                );
            });
            ui.keyed("other", |ui| ui.button("Other"));
        })
    });
}

#[test]
fn popovers_anchor_below_their_widget_take_focus_and_dismiss_on_escape_or_outside_press() {
    let mut driven = Driven::new(ROOT, Pops::default(), pop_page);
    assert!(driven.context.find(&["bar", "share", "pop"]).is_none());
    driven.state_mut().open = true;
    driven.frame();
    let share = bounds(&driven.context, &["bar", "share"]);
    let popover = driven.context.find(&["bar", "share", "pop"]).unwrap();
    let rect = driven.context.node(popover).unwrap().bounds;
    assert_eq!((rect.x, rect.y), (share.x, share.y + share.height));
    assert!(driven.context.node(popover).unwrap().is_popup);
    assert_eq!(focused_key(&driven.context), "copy", "focus moves inside");
    assert!(scene_text(&driven.context, "Share this"));
    // The panel blocks clicks to whatever is under it, and a press inside keeps it open.
    assert_eq!(
        driven.context.hit_test(Point {
            x: rect.x + 4.0,
            y: rect.y + 4.0
        }),
        Some(popover)
    );
    click_at(&mut driven.context, &["bar", "share", "pop", "copy"]);
    driven.frame();
    assert!(driven.state().open && driven.state().dismissed == 0);
    // Escape dismisses (one message) and restores focus to the anchor.
    key(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(!driven.state().open);
    assert_eq!(driven.state().dismissed, 1);
    assert_eq!(focused_key(&driven.context), "share");
    assert!(driven.context.find(&["bar", "share", "pop"]).is_none());
    // An outside press dismisses too; the pressed control takes focus.
    driven.state_mut().open = true;
    driven.frame();
    click_at(&mut driven.context, &["bar", "other"]);
    driven.frame();
    assert!(!driven.state().open);
    assert_eq!(driven.state().dismissed, 2);
    assert_eq!(focused_key(&driven.context), "other");
    // Two popovers can be open at once; pressing the first's anchor keeps it.
    driven.state_mut().open = true;
    driven.state_mut().second = true;
    driven.frame();
    assert!(driven.context.find(&["bar", "share", "pop"]).is_some());
    assert!(driven.context.find(&["bar", "more", "pop2"]).is_some());
    click_at(&mut driven.context, &["bar", "share"]);
    driven.frame();
    assert!(driven.state().open, "pressing the anchor does not dismiss");
    assert!(
        !driven.state().second,
        "but it is outside the other popover"
    );
}

#[test]
fn a_plain_tab_strip_with_more_tabs_than_fit_scrolls_to_the_selected_tab() {
    let mut driven = Driven::new(ROOT, Tabs::default(), |state: &Tabs, ui| {
        let tabs: Vec<Tab> = (0..30).map(|n| Tab::new(format!("Tab {n}"))).collect();
        ui.keyed("tabs", |ui| {
            ui.tabs(
                state.selected,
                &tabs,
                on(|s: &mut Tabs, index: usize| s.selected = index),
                |ui, index| ui.label(format!("panel {index}")),
            );
        });
    });
    let strip = bounds(&driven.context, &["tabs", "$strip"]);
    assert!(strip.width <= ROOT.width);
    assert_eq!(bounds(&driven.context, &["tabs", "$strip", "0"]).x, strip.x);
    let last = bounds(&driven.context, &["tabs", "$strip", "29"]);
    assert!(last.x > strip.x + strip.width);
    // No scrollbar is drawn for it.
    let bars = |context: &Context| {
        let color = context.theme().scrollbar;
        context
            .scene()
            .primitives
            .iter()
            .filter(
                |primitive| matches!(primitive, Primitive::Quad { color: c, .. } if *c == color),
            )
            .count()
    };
    assert_eq!(bars(&driven.context), 0);

    // The app selects the last tab: it is shown, up to the end of the strip.
    driven.state_mut().selected = 29;
    driven.frame();
    let last = bounds(&driven.context, &["tabs", "$strip", "29"]);
    assert!(last.x >= strip.x && last.x + last.width <= strip.x + strip.width + 0.5);
    assert!(bounds(&driven.context, &["tabs", "$strip", "0"]).x < strip.x);
    assert_eq!(bars(&driven.context), 0);
    assert!(scene_text(&driven.context, "panel 29"));

    // A sideways wheel moves it back too.
    let point = center(&driven.context, &["tabs", "$strip"]);
    driven
        .context
        .event(glassmere_core::Event::PointerMoved(point));
    driven.context.event(glassmere_core::Event::Scroll {
        delta_x: -100_000.0,
        delta_y: 0.0,
    });
    driven.frame();
    assert_eq!(bounds(&driven.context, &["tabs", "$strip", "0"]).x, strip.x);
}

/// A plain tab strip of `count` tabs, in a 500 px wide window.
fn strip_of(count: usize) -> Driven<Tabs> {
    Driven::new(ROOT, Tabs::default(), move |state: &Tabs, ui| {
        let tabs: Vec<Tab> = (0..count).map(|n| Tab::new(format!("Tab {n}"))).collect();
        ui.keyed("tabs", |ui| {
            ui.tabs(
                state.selected,
                &tabs,
                on(|s: &mut Tabs, index: usize| s.selected = index),
                |ui, index| ui.label(format!("panel {index}")),
            );
        });
    })
}

const START: [&str; 3] = ["tabs", "$strip", "$scroll-start"];
const END: [&str; 3] = ["tabs", "$strip", "$scroll-end"];

/// Whether the strip shows this scroll button: it takes room, and is both
/// reported to assistive technology and reachable by the pointer.
fn button_shown(context: &Context, path: &[&str]) -> bool {
    let id = context.find(path).unwrap();
    let node = context.node(id).unwrap();
    let shown = node.clip.width > 0.0 && node.clip.height > 0.0;
    let reported = context.snapshots().iter().any(|snapshot| snapshot.id == id);
    let reachable = context.hit_test(center(context, path)) == Some(id);
    assert_eq!(shown, reported, "{path:?} is reported only while shown");
    assert_eq!(shown, reachable, "{path:?} is reachable only while shown");
    shown
}

fn shades(context: &Context) -> usize {
    context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Gradient { .. }))
        .count()
}

fn first_tab_x(context: &Context) -> f32 {
    bounds(context, &["tabs", "$strip", "0"]).x
}

#[test]
fn a_tab_strip_whose_tabs_fit_has_no_scroll_buttons_or_shades() {
    let driven = strip_of(3);
    assert!(!button_shown(&driven.context, &START));
    assert!(!button_shown(&driven.context, &END));
    assert_eq!(shades(&driven.context), 0);
    assert!(!scene_text(&driven.context, "‹") && !scene_text(&driven.context, "›"));
    // The buttons take no room: the tabs are as wide as ever.
    let strip = bounds(&driven.context, &["tabs", "$strip"]);
    let last = bounds(&driven.context, &["tabs", "$strip", "2"]);
    assert!(last.x + last.width <= strip.x + strip.width);
}

#[test]
fn scroll_buttons_and_shades_show_only_on_the_sides_with_hidden_tabs_and_are_named() {
    let mut driven = strip_of(30);
    let strip = bounds(&driven.context, &["tabs", "$strip"]);
    // At the start only the end has tabs past it.
    assert!(!button_shown(&driven.context, &START));
    assert!(button_shown(&driven.context, &END));
    assert_eq!(shades(&driven.context), 1);
    let end = bounds(&driven.context, &END);
    assert!(
        (end.x + end.width - (strip.x + strip.width)).abs() < 0.5,
        "{end:?}"
    );
    assert!((end.height - strip.height).abs() < 0.5, "{end:?}");
    assert!(scene_text(&driven.context, "›") && !scene_text(&driven.context, "‹"));
    let named = |context: &Context, path: &[&str]| {
        let node = context.node(context.find(path).unwrap()).unwrap();
        (
            node.semantics.role,
            node.semantics.label,
            node.semantics.focusable,
        )
    };
    assert_eq!(
        named(&driven.context, &END),
        (Role::Button, "Show later tabs".into(), false)
    );

    // In the middle both have.
    let point = center(&driven.context, &["tabs", "$strip"]);
    driven.context.event(Event::PointerMoved(point));
    driven.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 200.0,
    });
    driven.frame();
    assert!(button_shown(&driven.context, &START));
    assert!(button_shown(&driven.context, &END));
    assert_eq!(shades(&driven.context), 2);
    assert_eq!(
        named(&driven.context, &START),
        (Role::Button, "Show earlier tabs".into(), false)
    );
    let start = bounds(&driven.context, &START);
    assert!((start.x - strip.x).abs() < 0.5, "{start:?}");

    // At the end only the start has.
    driven.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 100_000.0,
    });
    driven.frame();
    assert!(button_shown(&driven.context, &START));
    assert!(!button_shown(&driven.context, &END));
    assert_eq!(shades(&driven.context), 1);
    assert!(scene_text(&driven.context, "‹") && !scene_text(&driven.context, "›"));
}

#[test]
fn a_click_on_a_scroll_button_scrolls_by_about_a_tab_and_neither_selects_nor_takes_focus() {
    let mut driven = strip_of(30);
    let tab = driven.context.find(&["tabs", "$strip", "0"]).unwrap();
    driven.context.action(tab, Action::Focus);
    assert_eq!(focused_key(&driven.context), "0");
    let width = bounds(&driven.context, &["tabs", "$strip", "29"]).width;
    let strip = bounds(&driven.context, &["tabs", "$strip"]);

    click_at(&mut driven.context, &END);
    driven.frame();
    let moved = strip.x - first_tab_x(&driven.context);
    assert!(
        moved > 0.0 && moved <= width + 2.0,
        "moved {moved} for tabs {width} wide"
    );
    assert_eq!(driven.state().selected, 0, "no tab was selected");
    assert_eq!(
        focused_key(&driven.context),
        "0",
        "the strip keeps the keyboard"
    );
    assert!(
        button_shown(&driven.context, &START),
        "tabs are hidden before now"
    );

    // The start button goes back.
    click_at(&mut driven.context, &START);
    driven.frame();
    assert_eq!(first_tab_x(&driven.context), strip.x);
    assert!(!button_shown(&driven.context, &START));
    assert_eq!(driven.state().selected, 0);
    assert_eq!(focused_key(&driven.context), "0");
}

#[test]
fn the_scroll_buttons_are_not_in_the_tab_order_and_assistive_technology_can_press_them() {
    let mut driven = strip_of(30);
    let tab = driven.context.find(&["tabs", "$strip", "0"]).unwrap();
    driven.context.action(tab, Action::Focus);
    key(&mut driven.context, Key::Tab);
    assert_ne!(focused_key(&driven.context), "$scroll-end");
    assert_ne!(focused_key(&driven.context), "$scroll-start");
    let end = driven.context.find(&END).unwrap();
    assert!(!driven.context.action(end, Action::Focus));
    assert!(driven.context.action(end, Action::Activate));
    driven.frame();
    assert!(first_tab_x(&driven.context) < bounds(&driven.context, &["tabs", "$strip"]).x);
}

#[test]
fn scroll_buttons_reach_both_ends_and_then_give_way() {
    let mut driven = strip_of(30);
    let strip = bounds(&driven.context, &["tabs", "$strip"]);
    let mut clicks = 0;
    while button_shown(&driven.context, &END) {
        click_at(&mut driven.context, &END);
        driven.frame();
        clicks += 1;
        assert!(clicks < 40, "the end button never went away");
    }
    assert!(
        clicks > 3,
        "a click moves by about a tab, not the whole strip"
    );
    let last = bounds(&driven.context, &["tabs", "$strip", "29"]);
    assert!(
        last.x + last.width <= strip.x + strip.width + 0.5,
        "{last:?}"
    );
    assert!(last.x >= strip.x);
    assert!(button_shown(&driven.context, &START));
    assert_eq!(driven.state().selected, 0);

    while button_shown(&driven.context, &START) {
        click_at(&mut driven.context, &START);
        driven.frame();
        clicks -= 1;
        assert!(clicks > -40, "the start button never went away");
    }
    assert_eq!(first_tab_x(&driven.context), strip.x);
    assert!(button_shown(&driven.context, &END));
    assert_eq!(driven.state().selected, 0);
}

#[test]
fn a_tab_beside_a_scroll_button_still_takes_the_click() {
    let mut driven = strip_of(30);
    let end = bounds(&driven.context, &END);
    // The tab just before the button, cut by it.
    let beside = Point {
        x: end.x - 3.0,
        y: end.y + end.height / 2.0,
    };
    let under = (0..30)
        .find(|n| {
            let tab = bounds(&driven.context, &["tabs", "$strip", &n.to_string()]);
            tab.x <= beside.x && beside.x < tab.x + tab.width
        })
        .expect("a tab ends under the button");
    click(&mut driven.context, beside);
    driven.frame();
    assert_eq!(driven.state().selected, under);
    // The selected tab is brought clear of the button, and a click on the
    // button itself selects nothing.
    let tab = bounds(&driven.context, &["tabs", "$strip", &under.to_string()]);
    if button_shown(&driven.context, &END) {
        let end = bounds(&driven.context, &END);
        assert!(tab.x + tab.width <= end.x + 0.5, "{tab:?} under {end:?}");
        click_at(&mut driven.context, &END);
        driven.frame();
        assert_eq!(driven.state().selected, under);
    }
}

#[test]
fn a_press_held_on_a_scroll_button_repeats_like_a_scrollbar_track() {
    let mut driven = strip_of(30);
    let at = |driven: &mut Driven<Tabs>, milliseconds: u64| {
        driven.context.set_now(Duration::from_millis(milliseconds));
        driven.frame();
    };
    at(&mut driven, 1000);
    let point = center(&driven.context, &END);
    driven.context.event(Event::PointerMoved(point));
    driven.context.event(Event::PointerDown);
    driven.frame();
    let once = first_tab_x(&driven.context);
    assert!(once < bounds(&driven.context, &["tabs", "$strip"]).x);
    assert_eq!(
        driven.context.next_wake(),
        Some(Duration::from_millis(1400))
    );
    at(&mut driven, 1399);
    assert_eq!(
        first_tab_x(&driven.context),
        once,
        "nothing before the delay"
    );
    at(&mut driven, 1400);
    let twice = first_tab_x(&driven.context);
    assert!(twice < once, "scrolled again after the delay");
    at(&mut driven, 1450);
    assert!(
        first_tab_x(&driven.context) < twice,
        "then at a steady interval"
    );
    driven.context.event(Event::PointerUp);
    assert_eq!(driven.context.next_wake(), None);
    let held = first_tab_x(&driven.context);
    at(&mut driven, 5000);
    assert_eq!(first_tab_x(&driven.context), held, "release stops it");
    assert_eq!(driven.state().selected, 0);

    // It stops where the button goes away: the end of the strip.
    let point = center(&driven.context, &END);
    driven.context.event(Event::PointerMoved(point));
    driven.context.event(Event::PointerDown);
    for step in 0..60 {
        at(&mut driven, 6000 + step * 50);
    }
    assert!(!button_shown(&driven.context, &END));
    assert_eq!(driven.context.next_wake(), None);
}

#[test]
fn a_right_to_left_strip_mirrors_its_buttons_and_arrows() {
    let mut driven = strip_of(30);
    driven.context.set_direction(glassmere_core::Direction::Rtl);
    driven.frame();
    let strip = bounds(&driven.context, &["tabs", "$strip"]);
    // The first tab is at the right, so the hidden ones are past the left.
    assert!(!button_shown(&driven.context, &START));
    assert!(button_shown(&driven.context, &END));
    let end = bounds(&driven.context, &END);
    assert!((end.x - strip.x).abs() < 0.5, "{end:?} in {strip:?}");
    assert!(scene_text(&driven.context, "‹") && !scene_text(&driven.context, "›"));
    click_at(&mut driven.context, &END);
    driven.frame();
    assert!(button_shown(&driven.context, &START));
    let start = bounds(&driven.context, &START);
    assert!(
        (start.x + start.width - (strip.x + strip.width)).abs() < 0.5,
        "{start:?}"
    );
    click_at(&mut driven.context, &START);
    driven.frame();
    assert!(!button_shown(&driven.context, &START));
    let first = bounds(&driven.context, &["tabs", "$strip", "0"]);
    assert!((first.x + first.width - (strip.x + strip.width)).abs() < 0.5);
}
