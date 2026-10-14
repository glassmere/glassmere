use rust_gui_core::{
    Action, Axis, Context, Event, Key, Length, MenuItem, MonospaceMeasurer, PopoverOptions, Role,
    SplitOptions, Style, TOOLTIP_DELAY, Tab, TabsResponse, Ui,
};
use rust_gui_scene::{Point, Primitive, Size};
use std::time::Duration;

const ROOT: Size = Size::new(500.0, 400.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> rust_gui_core::FrameStats {
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

fn bounds(context: &Context, path: &[&str]) -> rust_gui_scene::Rect {
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

fn tabs_frame(context: &mut Context, selected: &mut usize) -> TabsResponse {
    let tabs = [Tab::new("One"), Tab::closable("Two"), Tab::new("Three")];
    let mut response = TabsResponse::default();
    frame(context, |ui| {
        ui.keyed("before", |ui| ui.button("Before"));
        response = ui.keyed("tabs", |ui| {
            ui.tabs(selected, &tabs, |ui, index| {
                ui.keyed("body", |ui| ui.label(format!("panel {index}")));
                ui.keyed("inner", |ui| ui.button("Inner"));
            })
        });
        ui.keyed("after", |ui| ui.button("After"));
    });
    response
}

#[test]
fn tabs_select_by_click_and_arrows_expose_roles_and_report_closes() {
    let mut context = Context::new();
    let mut selected = 0usize;
    tabs_frame(&mut context, &mut selected);
    let strip = context.find(&["tabs", "$strip"]).unwrap();
    assert_eq!(context.node(strip).unwrap().semantics.role, Role::TabList);
    let first = context.find(&["tabs", "$strip", "0"]).unwrap();
    assert_eq!(context.node(first).unwrap().semantics.role, Role::Tab);
    assert_eq!(context.node(first).unwrap().selected, Some(true));
    let panel = context.find(&["tabs", "$panel"]).unwrap();
    assert_eq!(context.node(panel).unwrap().semantics.role, Role::TabPanel);
    assert!(scene_text(&context, "panel 0"));
    // Click the second tab.
    click_at(&mut context, &["tabs", "$strip", "1"]);
    let response = tabs_frame(&mut context, &mut selected);
    assert_eq!(selected, 1);
    assert!(response.changed);
    assert!(scene_text(&context, "panel 1"));
    assert!(!tabs_frame(&mut context, &mut selected).changed);
    // Only the selected tab is in the Tab order; Tab then enters the panel.
    context.action(context.find(&["before"]).unwrap(), Action::Focus);
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "1");
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "$close-1");
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "inner");
    // Arrows on the focused tab switch immediately and keep focus on the strip.
    context.action(
        context.find(&["tabs", "$strip", "1"]).unwrap(),
        Action::Focus,
    );
    key(&mut context, Key::ArrowRight);
    tabs_frame(&mut context, &mut selected);
    assert_eq!((selected, focused_key(&context).as_str()), (2, "2"));
    key(&mut context, Key::Home);
    tabs_frame(&mut context, &mut selected);
    assert_eq!((selected, focused_key(&context).as_str()), (0, "0"));
    key(&mut context, Key::End);
    tabs_frame(&mut context, &mut selected);
    assert_eq!(selected, 2);
    key(&mut context, Key::ArrowLeft);
    tabs_frame(&mut context, &mut selected);
    assert_eq!(selected, 1);
    // Close through the button, and through Delete on the focused closable tab.
    click_at(&mut context, &["tabs", "$strip", "$close-1"]);
    assert_eq!(tabs_frame(&mut context, &mut selected).closed, Some(1));
    assert_eq!(tabs_frame(&mut context, &mut selected).closed, None);
    context.action(
        context.find(&["tabs", "$strip", "1"]).unwrap(),
        Action::Focus,
    );
    key(&mut context, Key::Delete);
    assert_eq!(tabs_frame(&mut context, &mut selected).closed, Some(1));
    // Delete on a non-closable tab does nothing.
    key(&mut context, Key::ArrowRight);
    tabs_frame(&mut context, &mut selected);
    key(&mut context, Key::Delete);
    assert_eq!(tabs_frame(&mut context, &mut selected).closed, None);
}

fn split_frame(context: &mut Context, fraction: &mut f32) {
    frame(context, |ui| {
        ui.keyed("split", |ui| {
            ui.split(
                fraction,
                Axis::Horizontal,
                SplitOptions {
                    min_first: 100.0,
                    min_second: 100.0,
                    divider: 10.0,
                    style: Style::default()
                        .width(Length::Px(500.0))
                        .height(Length::Px(200.0)),
                },
                |ui| ui.keyed("a", |ui| ui.label("A")),
                |ui| ui.keyed("b", |ui| ui.label("B")),
            )
        });
    });
}

#[test]
fn split_dividers_drag_within_minimums_move_with_keys_and_accept_semantic_values() {
    let mut context = Context::new();
    let mut fraction = 0.5f32;
    split_frame(&mut context, &mut fraction);
    let first = bounds(&context, &["split", "$first"]);
    let divider = context.find(&["split", "$divider"]).unwrap();
    let second = bounds(&context, &["split", "$second"]);
    assert_eq!((first.x, first.width), (0.0, 250.0));
    assert_eq!(context.node(divider).unwrap().bounds.x, 250.0);
    assert_eq!((second.x, second.width), (260.0, 240.0));
    let snapshot = context.node(divider).unwrap();
    assert_eq!(snapshot.semantics.role, Role::Splitter);
    assert_eq!(snapshot.orientation, Some(Axis::Horizontal));
    assert_eq!(snapshot.numeric.unwrap().value, 50.0);
    // Drag to the right, then past the second pane's minimum.
    context.event(Event::PointerMoved(Point { x: 255.0, y: 100.0 }));
    context.event(Event::PointerDown);
    assert_eq!(context.captured(), Some(divider));
    context.event(Event::PointerMoved(Point { x: 300.0, y: 100.0 }));
    split_frame(&mut context, &mut fraction);
    assert!((fraction - 295.0 / 490.0).abs() < 0.001, "{fraction}");
    let first = bounds(&context, &["split", "$first"]);
    assert!((first.width - 500.0 * fraction).abs() < 1.0);
    context.event(Event::PointerMoved(Point { x: 480.0, y: 100.0 }));
    split_frame(&mut context, &mut fraction);
    assert!(
        (fraction - (1.0 - 100.0 / 490.0)).abs() < 0.001,
        "clamped: {fraction}"
    );
    context.event(Event::PointerUp);
    // Keyboard: ten logical pixels per step, Home to the first minimum.
    context.action(divider, Action::Focus);
    let before = fraction;
    key(&mut context, Key::ArrowLeft);
    split_frame(&mut context, &mut fraction);
    assert!((before - fraction - 10.0 / 500.0).abs() < 0.001);
    key(&mut context, Key::Home);
    split_frame(&mut context, &mut fraction);
    assert!((fraction - 100.0 / 490.0).abs() < 0.001);
    // Accessibility sets a percentage.
    assert!(context.action(divider, Action::SetNumber(50.0)));
    split_frame(&mut context, &mut fraction);
    assert_eq!(fraction, 0.5);
}

fn menu_frame(context: &mut Context) -> Option<usize> {
    let items = [
        MenuItem::new("Copy"),
        MenuItem::disabled("Cut"),
        MenuItem::new("Paste"),
    ];
    let mut chosen = None;
    frame(context, |ui| {
        ui.keyed("outside", |ui| ui.button("Outside"));
        chosen = ui.keyed("area", |ui| {
            ui.context_menu(
                &items,
                Style::default()
                    .width(Length::Px(400.0))
                    .height(Length::Px(200.0)),
                |ui| {
                    ui.keyed("text", |ui| ui.label("Right-click me"));
                    ui.keyed("inner", |ui| ui.button("Inner"));
                },
            )
        });
    });
    chosen
}

#[test]
fn context_menus_open_at_the_pointer_or_focus_navigate_and_choose_once() {
    let mut context = Context::new();
    menu_frame(&mut context);
    assert!(context.find(&["area", "$popup"]).is_none());
    assert!(
        !context.event(Event::ContextMenu(Point { x: 10.0, y: 10.0 })),
        "outside the area"
    );
    assert!(context.event(Event::ContextMenu(Point { x: 100.0, y: 120.0 })));
    menu_frame(&mut context);
    let popup = context.find(&["area", "$popup"]).unwrap();
    let rect = context.node(popup).unwrap().bounds;
    assert_eq!((rect.x, rect.y), (100.0, 120.0), "anchored at the pointer");
    assert!(context.node(popup).unwrap().is_popup);
    key(&mut context, Key::ArrowDown);
    key(&mut context, Key::Enter);
    assert_eq!(menu_frame(&mut context), Some(2), "skips the disabled item");
    assert_eq!(menu_frame(&mut context), None);
    assert!(context.find(&["area", "$popup"]).is_none());
    // Escape closes without a choice; near the right edge the menu stays on screen.
    context.event(Event::ContextMenu(Point { x: 395.0, y: 100.0 }));
    menu_frame(&mut context);
    let rect = bounds(&context, &["area", "$popup"]);
    assert!(rect.x + rect.width <= ROOT.width + 0.5);
    key(&mut context, Key::Escape);
    assert_eq!(menu_frame(&mut context), None);
    assert!(context.find(&["area", "$popup"]).is_none());
    // The keyboard opens the menu below the focused control inside the area.
    let inner = context.find(&["area", "inner"]).unwrap();
    context.action(inner, Action::Focus);
    let inner_bounds = context.node(inner).unwrap().bounds;
    key(&mut context, Key::ContextMenu);
    menu_frame(&mut context);
    let rect = bounds(&context, &["area", "$popup"]);
    assert_eq!(
        (rect.x, rect.y),
        (inner_bounds.x, inner_bounds.y + inner_bounds.height)
    );
    key(&mut context, Key::Enter);
    assert_eq!(menu_frame(&mut context), Some(0));
    assert_eq!(
        focused_key(&context),
        "inner",
        "focus returns to where the menu was opened"
    );
}

fn popover_frame(context: &mut Context, open: &mut bool, second: &mut bool) -> bool {
    let mut dismissed = false;
    frame(context, |ui| {
        ui.keyed("bar", |ui| {
            ui.row(Style::default(), |ui| {
                ui.keyed("share", |ui| ui.button("Share"));
                dismissed = ui
                    .keyed("pop", |ui| {
                        ui.popover(open, PopoverOptions::default(), |ui| {
                            ui.keyed("hint", |ui| ui.label("Share this"));
                            ui.keyed("copy", |ui| ui.button("Copy link"));
                        })
                    })
                    .dismissed;
                ui.keyed("more", |ui| ui.button("More"));
                ui.keyed("pop2", |ui| {
                    ui.popover(second, PopoverOptions::default(), |ui| {
                        ui.keyed("extra", |ui| ui.button("Extra"));
                    })
                });
                ui.keyed("other", |ui| ui.button("Other"));
            })
        });
    });
    dismissed
}

#[test]
fn popovers_anchor_below_their_widget_take_focus_and_dismiss_on_escape_or_outside_press() {
    let mut context = Context::new();
    let mut open = false;
    let mut second = false;
    popover_frame(&mut context, &mut open, &mut second);
    assert!(context.find(&["bar", "share", "pop"]).is_none());
    open = true;
    popover_frame(&mut context, &mut open, &mut second);
    let share = bounds(&context, &["bar", "share"]);
    let popover = context.find(&["bar", "share", "pop"]).unwrap();
    let rect = context.node(popover).unwrap().bounds;
    assert_eq!((rect.x, rect.y), (share.x, share.y + share.height));
    assert!(context.node(popover).unwrap().is_popup);
    assert_eq!(focused_key(&context), "copy", "focus moves inside");
    assert!(scene_text(&context, "Share this"));
    // The panel blocks clicks to whatever is under it, and a press inside keeps it open.
    assert_eq!(
        context.hit_test(Point {
            x: rect.x + 4.0,
            y: rect.y + 4.0
        }),
        Some(popover)
    );
    click_at(&mut context, &["bar", "share", "pop", "copy"]);
    let dismissed = popover_frame(&mut context, &mut open, &mut second);
    assert!(open && !dismissed);
    // Escape dismisses and restores focus to the anchor.
    key(&mut context, Key::Escape);
    let dismissed = popover_frame(&mut context, &mut open, &mut second);
    assert!(!open && dismissed);
    assert_eq!(focused_key(&context), "share");
    assert!(context.find(&["bar", "share", "pop"]).is_none());
    // An outside press dismisses too; the pressed control takes focus.
    open = true;
    popover_frame(&mut context, &mut open, &mut second);
    click_at(&mut context, &["bar", "other"]);
    let dismissed = popover_frame(&mut context, &mut open, &mut second);
    assert!(!open && dismissed);
    assert_eq!(focused_key(&context), "other");
    // Two popovers can be open at once; pressing the first's anchor keeps it.
    open = true;
    second = true;
    popover_frame(&mut context, &mut open, &mut second);
    assert!(context.find(&["bar", "share", "pop"]).is_some());
    assert!(context.find(&["bar", "more", "pop2"]).is_some());
    click_at(&mut context, &["bar", "share"]);
    popover_frame(&mut context, &mut open, &mut second);
    assert!(open, "pressing the anchor does not dismiss");
    assert!(!second, "but it is outside the other popover");
}
