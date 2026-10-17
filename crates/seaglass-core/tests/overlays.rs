mod support;

use seaglass_core::{
    Action, Axis, Context, Event, Key, Length, MenuItem, MonospaceMeasurer, PopoverOptions, Role,
    SplitOptions, Style, TOOLTIP_DELAY, Tab, Ui,
};
use seaglass_scene::{Point, Primitive, Size};
use std::time::Duration;
use support::{Driven, on};

const ROOT: Size = Size::new(500.0, 400.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = seaglass_core::app::Action<S>;

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> seaglass_core::FrameStats {
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

fn bounds(context: &Context, path: &[&str]) -> seaglass_scene::Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

fn center(context: &Context, path: &[&str]) -> Point {
    let bounds = bounds(context, path);
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

fn click(context: &mut Context, point: Point) {
    context.event(Event::PointerMoved(point));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
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
