//! What a point is over when views overlap, and where the wheel goes: the
//! innermost and the latest declared first, floating panels over the page.
mod support;

use glassmere_core::{
    Action, Block, BlockKind, Command, Context, Event, Key, Label, Length, Menu, MenuEntry, NodeId,
    PopoverOptions, RichDocument, RichEditorOptions, RichOp, Style, Tab, Ui,
};
use glassmere_scene::{Point, Rect, Size};
use support::{Driven, center, on};

const ROOT: Size = Size::new(700.0, 400.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = glassmere_core::app::Action<S>;

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

fn inside(rect: Rect, point: Point) -> bool {
    point.x >= rect.x
        && point.x < rect.x + rect.width
        && point.y >= rect.y
        && point.y < rect.y + rect.height
}

/// Whether `id` is `ancestor` or below it.
fn under(context: &Context, mut id: NodeId, ancestor: NodeId) -> bool {
    loop {
        if id == ancestor {
            return true;
        }
        match context.node(id).and_then(|node| node.parent) {
            Some(parent) => id = parent,
            None => return false,
        }
    }
}

#[derive(Default)]
struct Page {
    first: bool,
    second: bool,
}

fn popovers(state: &Page, ui: &mut Ui<'_, Handler<Page>>) {
    ui.keyed("bar", |ui| {
        ui.row(Style::default(), |ui| {
            ui.keyed("share", |ui| ui.button("Share"));
            ui.keyed("first", |ui| {
                ui.popover(
                    state.first,
                    PopoverOptions::default(),
                    on(|s: &mut Page, open: bool| s.first = open),
                    |ui| {
                        ui.keyed("tall", |ui| ui.button("Tall"));
                    },
                );
            });
            ui.keyed("more", |ui| ui.button("More"));
            ui.keyed("second", |ui| {
                ui.popover(
                    state.second,
                    PopoverOptions::default(),
                    on(|s: &mut Page, open: bool| s.second = open),
                    |ui| {
                        ui.keyed("extra", |ui| ui.button("Extra"));
                    },
                );
            });
        })
    });
    ui.keyed("below", |ui| ui.button("Below"));
}

#[test]
fn of_two_popovers_over_each_other_the_later_one_takes_the_point() {
    let mut driven = Driven::new(ROOT, Page::default(), popovers);
    driven.state_mut().first = true;
    driven.state_mut().second = true;
    driven.frame();
    let context = &driven.context;
    let first = context.find(&["bar", "share", "first"]).unwrap();
    let second = context.find(&["bar", "more", "second"]).unwrap();
    let (one, two) = (
        bounds(context, &["bar", "share", "first"]),
        bounds(context, &["bar", "more", "second"]),
    );
    // Where the two panels lie over each other.
    let x = two.x + 1.0;
    let y = two.y + 1.0;
    assert!(inside(one, Point { x, y }) && inside(two, Point { x, y }));
    let hit = context.hit_test(Point { x, y }).unwrap();
    assert!(under(context, hit, second) && !under(context, hit, first));
    // Only the first covers a point left of the second.
    let at = Point {
        x: one.x + 1.0,
        y: one.y + 1.0,
    };
    assert!(!inside(two, at));
    let hit = context.hit_test(at).unwrap();
    assert!(under(context, hit, first));
}

#[test]
fn a_popover_takes_the_point_over_a_sibling_declared_after_its_anchor() {
    let mut driven = Driven::new(ROOT, Page::default(), popovers);
    driven.state_mut().first = true;
    driven.frame();
    let context = &driven.context;
    let first = context.find(&["bar", "share", "first"]).unwrap();
    let panel = bounds(context, &["bar", "share", "first"]);
    let below = bounds(context, &["below"]);
    let at = center(context, &["below"]);
    assert!(inside(panel, at) && inside(below, at));
    let hit = context.hit_test(at).unwrap();
    assert!(under(context, hit, first));
    assert_ne!(Some(hit), context.find(&["below"]));
}

/// Recent opens beside the menu the File button opened, lapping it.
fn file_menu(_: &Page, ui: &mut Ui<'_, Handler<Page>>) {
    let command = |id: &str, label: &str| MenuEntry::Command(Command::with_id(id, label, None));
    let menu = Menu::new([
        command("open", "Open..."),
        MenuEntry::Separator,
        MenuEntry::submenu("Recent", [command("a", "a.txt"), command("b", "b.txt")]),
    ]);
    ui.keyed("file", |ui| {
        ui.command_menu("File", &menu);
    });
}

#[test]
fn a_submenu_is_over_the_menu_it_opened_from() {
    let mut driven = Driven::new(Size::new(900.0, 400.0), Page::default(), file_menu);
    let button = driven.context.find(&["file"]).unwrap();
    assert!(driven.context.action(button, Action::Expand));
    driven.frame();
    for key in [Key::ArrowDown, Key::ArrowRight] {
        driven.context.event(Event::KeyDown {
            key,
            shift: false,
            repeat: false,
        });
    }
    driven.frame();
    let context = &driven.context;
    let menu = context.find(&["file", "$popup"]).unwrap();
    let recent = context.find(&["file", "$popup", "2", "$popup"]).unwrap();
    let row = bounds(context, &["file", "$popup", "2"]);
    // The submenu laps the row it opened from, so that a point there is in
    // both panels.
    let at = Point {
        x: row.x + row.width - 1.0,
        y: row.y + row.height / 2.0,
    };
    assert!(inside(bounds(context, &["file", "$popup"]), at));
    assert!(inside(
        bounds(context, &["file", "$popup", "2", "$popup"]),
        at
    ));
    let hit = context.hit_test(at).unwrap();
    assert!(under(context, hit, recent), "{hit:?}");
    // The menu itself is still what is hit where the submenu is not.
    let away = Point {
        x: row.x + 4.0,
        y: row.y + row.height / 2.0,
    };
    let hit = context.hit_test(away).unwrap();
    assert!(under(context, hit, menu) && !under(context, hit, recent));
}

/// Recent is the first entry, with commands after it: the submenu laps
/// rows that come after the row it opened from.
fn recent_first_menu(_: &Page, ui: &mut Ui<'_, Handler<Page>>) {
    let command = |id: &str, label: &str| MenuEntry::Command(Command::with_id(id, label, None));
    let menu = Menu::new([
        MenuEntry::submenu(
            "Recent",
            [
                command("a", "a.txt"),
                command("b", "b.txt"),
                command("c", "c.txt"),
                command("d", "d.txt"),
            ],
        ),
        command("open", "Open..."),
        command("save", "Save"),
        command("close", "Close"),
    ]);
    ui.keyed("file", |ui| {
        ui.command_menu("File", &menu);
    });
}

#[test]
fn a_submenu_is_over_the_rows_after_the_one_it_opened_from() {
    let mut driven = Driven::new(Size::new(900.0, 400.0), Page::default(), recent_first_menu);
    let button = driven.context.find(&["file"]).unwrap();
    assert!(driven.context.action(button, Action::Expand));
    driven.frame();
    driven.context.event(Event::KeyDown {
        key: Key::ArrowRight,
        shift: false,
        repeat: false,
    });
    driven.frame();
    let context = &driven.context;
    let menu_bounds = bounds(context, &["file", "$popup"]);
    let recent = context.find(&["file", "$popup", "0", "$popup"]).unwrap();
    let recent_bounds = bounds(context, &["file", "$popup", "0", "$popup"]);
    let mut probed = 0;
    let mut y = recent_bounds.y;
    while y < recent_bounds.y + recent_bounds.height {
        let mut x = recent_bounds.x;
        while x < recent_bounds.x + recent_bounds.width {
            let at = Point { x, y };
            if inside(menu_bounds, at) {
                probed += 1;
                let hit = context.hit_test(at).unwrap();
                assert!(under(context, hit, recent), "{at:?} hit {hit:?}");
            }
            x += 2.0;
        }
        y += 2.0;
    }
    // The submenu laps the menu over more than its first row.
    assert!(probed > 20, "{probed}");
}

/// An outer view over an inner one that has 100 px to go, then a spacer.
fn nested(ui: &mut Ui<'_>) {
    ui.keyed("outer", |ui| {
        ui.scroll(sized(300.0, 200.0), |ui| {
            ui.keyed("inner", |ui| {
                ui.scroll(sized(200.0, 100.0), |ui| {
                    ui.keyed("long", |ui| {
                        ui.label(Label::new("").style(sized(180.0, 200.0)))
                    });
                })
            });
            ui.keyed("rest", |ui| {
                ui.label(Label::new("").style(sized(280.0, 600.0)))
            });
        })
    });
}

fn offset(context: &Context, path: &[&str]) -> f32 {
    context
        .node(context.find(path).unwrap())
        .unwrap()
        .scroll
        .unwrap()
        .offset
}

fn wheel(context: &mut Context, delta_y: f32) {
    context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y,
    });
}

#[test]
fn a_wheel_turned_back_over_an_inner_view_at_its_start_moves_the_outer_one() {
    let mut context = Context::new();
    context.run(ROOT, &mut glassmere_core::MonospaceMeasurer, nested);
    context.event(Event::PointerMoved(Point { x: 50.0, y: 40.0 }));
    wheel(&mut context, 30.0);
    assert_eq!(offset(&context, &["outer", "inner"]), 30.0);
    assert_eq!(offset(&context, &["outer"]), 0.0);
    // Through to the end of the inner view, which then keeps the wheel.
    wheel(&mut context, 100.0);
    assert_eq!(offset(&context, &["outer", "inner"]), 100.0);
    assert_eq!(offset(&context, &["outer"]), 0.0);
    wheel(&mut context, 20.0);
    assert_eq!(offset(&context, &["outer", "inner"]), 100.0);
    assert_eq!(offset(&context, &["outer"]), 20.0);
    // Back: the inner view gives way at its start only after it is there.
    wheel(&mut context, -100.0);
    assert_eq!(offset(&context, &["outer", "inner"]), 0.0);
    assert_eq!(offset(&context, &["outer"]), 20.0);
    wheel(&mut context, -15.0);
    assert_eq!(offset(&context, &["outer", "inner"]), 0.0);
    assert_eq!(offset(&context, &["outer"]), 5.0);
    // A point beside the inner view belongs to the outer one alone.
    context.event(Event::PointerMoved(Point { x: 250.0, y: 40.0 }));
    wheel(&mut context, 10.0);
    assert_eq!(offset(&context, &["outer", "inner"]), 0.0);
    assert_eq!(offset(&context, &["outer"]), 15.0);
}

#[derive(Default)]
struct Tabs {
    selected: usize,
}

/// A tab strip of `count` tabs in a view that has room to scroll.
fn strip_in_scroll(count: usize) -> Driven<Tabs> {
    Driven::new(
        Size::new(300.0, 400.0),
        Tabs::default(),
        move |state, ui| {
            let tabs: Vec<Tab> = (0..count).map(|n| Tab::new(format!("Tab {n}"))).collect();
            ui.keyed("page", |ui| {
                ui.scroll(sized(300.0, 200.0), |ui| {
                    ui.keyed("tabs", |ui| {
                        ui.tabs(
                            state.selected,
                            &tabs,
                            on(|s: &mut Tabs, index: usize| s.selected = index),
                            |ui, _| ui.label(Label::new("").style(sized(100.0, 600.0))),
                        );
                    });
                })
            });
        },
    )
}

/// How far the first tab lies from where the strip starts.
fn strip_offset(context: &Context) -> f32 {
    let strip = bounds(context, &["page", "tabs", "$strip"]).x;
    strip - bounds(context, &["page", "tabs", "$strip", "0"]).x
}

#[test]
fn an_up_or_down_wheel_moves_a_tab_strip_that_overflows_and_otherwise_the_view() {
    let mut driven = strip_in_scroll(30);
    let context = &mut driven.context;
    let at = center(context, &["page", "tabs", "$strip"]);
    context.event(Event::PointerMoved(at));
    wheel(context, 60.0);
    assert_eq!(strip_offset(context), 60.0);
    assert_eq!(offset(context, &["page"]), 0.0);
    // Below the strip the wheel is the view's.
    context.event(Event::PointerMoved(Point { x: 50.0, y: 150.0 }));
    wheel(context, 30.0);
    assert_eq!(strip_offset(context), 60.0);
    assert_eq!(offset(context, &["page"]), 30.0);

    // Three tabs fit: the wheel goes on to the view around the strip.
    let mut driven = strip_in_scroll(3);
    let context = &mut driven.context;
    let at = center(context, &["page", "tabs", "$strip"]);
    context.event(Event::PointerMoved(at));
    wheel(context, 60.0);
    assert_eq!(strip_offset(context), 0.0);
    assert_eq!(offset(context, &["page"]), 60.0);
}

fn rich_in_scroll(document: &RichDocument, ui: &mut Ui<'_, Handler<RichDocument>>) {
    ui.keyed("page", |ui| {
        ui.scroll(sized(400.0, 300.0), |ui| {
            ui.keyed("rich", |ui| {
                ui.rich_editor(
                    document,
                    RichEditorOptions {
                        toolbar: false,
                        style: sized(380.0, 120.0).padding(8.0),
                        ..RichEditorOptions::default()
                    },
                    on(|s: &mut RichDocument, op: RichOp| s.apply(op)),
                );
            });
            ui.keyed("rest", |ui| {
                ui.label(Label::new("").style(sized(380.0, 600.0)))
            });
        })
    });
}

#[test]
fn a_wheel_over_a_rich_editor_with_more_lines_than_fit_moves_the_editor_first() {
    let blocks = (0..30)
        .map(|n| Block::new(BlockKind::Paragraph, &format!("line {n}")))
        .collect();
    let mut driven = Driven::new(ROOT, RichDocument::from_blocks(blocks), rich_in_scroll);
    let context = &mut driven.context;
    let editor = context.find(&["page", "rich", "$area"]).unwrap();
    assert!(context.node(editor).unwrap().bounds.height > 0.0);
    let at = center(context, &["page", "rich", "$area"]);
    context.event(Event::PointerMoved(at));
    wheel(context, 40.0);
    assert_eq!(offset(context, &["page"]), 0.0);
    // Past the end of the editor the wheel is the view's.
    wheel(context, 100_000.0);
    wheel(context, 40.0);
    assert_eq!(offset(context, &["page"]), 40.0);
}

fn combo(_: &Page, ui: &mut Ui<'_, Handler<Page>>) {
    let items: Vec<String> = (0..20).map(|n| format!("Item {n}")).collect();
    let items: Vec<&str> = items.iter().map(String::as_str).collect();
    ui.keyed("page", |ui| {
        ui.scroll(sized(300.0, 200.0), |ui| {
            ui.keyed("combo", |ui| {
                ui.combo_box("Pick", 0, &items, on(|_: &mut Page, _: usize| ()));
            });
            ui.keyed("rest", |ui| {
                ui.label(Label::new("").style(sized(280.0, 600.0)))
            });
        })
    });
}

#[test]
fn a_wheel_over_a_list_that_closed_before_the_next_frame_still_moves_that_list() {
    let mut driven = Driven::new(Size::new(400.0, 500.0), Page::default(), combo);
    let button = driven.context.find(&["page", "combo"]).unwrap();
    assert!(driven.context.action(button, Action::Expand));
    driven.frame();
    let list = driven.context.find(&["page", "combo", "$popup"]).unwrap();
    let rect = bounds(&driven.context, &["page", "combo", "$popup"]);
    let at = Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    };
    // The open list takes the wheel as a panel.
    driven.context.event(Event::PointerMoved(at));
    wheel(&mut driven.context, 30.0);
    driven.frame();
    assert_eq!(offset(&driven.context, &["page", "combo", "$popup"]), 30.0);
    assert_eq!(offset(&driven.context, &["page"]), 0.0);
    // Closed, it is no panel any more, but it is still in the tree and under
    // the pointer until the next frame declares without it.
    driven.context.event(Event::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    });
    assert!(driven.context.node(list).is_some());
    wheel(&mut driven.context, 30.0);
    assert_eq!(offset(&driven.context, &["page", "combo", "$popup"]), 60.0);
    assert_eq!(offset(&driven.context, &["page"]), 0.0);
}

/// A strip that overflows, and over it one that fits.
fn strips(_: &Tabs, ui: &mut Ui<'_, Handler<Tabs>>) {
    let many: Vec<Tab> = (0..30).map(|n| Tab::new(format!("Tab {n}"))).collect();
    let few: Vec<Tab> = (0..3).map(|n| Tab::new(format!("Tab {n}"))).collect();
    ui.keyed("page", |ui| {
        ui.scroll(sized(300.0, 200.0), |ui| {
            ui.keyed("under", |ui| {
                ui.tabs(
                    0,
                    &many,
                    on(|s: &mut Tabs, index: usize| s.selected = index),
                    |ui, _| ui.label(Label::new("").style(sized(10.0, 10.0))),
                );
            });
            ui.keyed("over", |ui| {
                // Laid over the first strip, not below it.
                ui.column(Style::default().offset(0.0, 10.0), |ui| {
                    ui.keyed("tabs", |ui| {
                        ui.tabs(
                            0,
                            &few,
                            on(|s: &mut Tabs, index: usize| s.selected = index),
                            |ui, _| ui.label(Label::new("").style(sized(10.0, 10.0))),
                        );
                    });
                })
            });
            ui.keyed("rest", |ui| {
                ui.label(Label::new("").style(sized(280.0, 600.0)))
            });
        })
    });
}

#[test]
fn a_wheel_over_two_strips_moves_the_one_that_overflows_though_the_other_is_in_front() {
    let mut driven = Driven::new(Size::new(300.0, 400.0), Tabs::default(), strips);
    let context = &mut driven.context;
    let under = bounds(context, &["page", "under", "$strip"]);
    let over = bounds(context, &["page", "over", "tabs", "$strip"]);
    let at = Point {
        x: under.x + 20.0,
        y: over.y + over.height / 2.0,
    };
    assert!(inside(under, at) && inside(over, at), "{under:?} {over:?}");
    context.event(Event::PointerMoved(at));
    wheel(context, 60.0);
    let moved = under.x - bounds(context, &["page", "under", "$strip", "0"]).x;
    assert_eq!(moved, 60.0);
    assert_eq!(offset(context, &["page"]), 0.0);
}
