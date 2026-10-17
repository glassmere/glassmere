mod support;

use seaglass_core::app::Action as Message;
use seaglass_core::{
    Axis, Context, DockArea, DockLayout, DockNode, DockOptions, DockPanel, DockPlace, DockTarget,
    DockWindowId, DockZone, Event, Key, Role, Ui,
};
use seaglass_scene::{Point, Primitive, Rect, Size};
use support::{Driven, act};

const ROOT: Size = Size::new(800.0, 600.0);

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

fn group<'a>(layout: &'a DockLayout, panel: &str) -> Vec<&'a str> {
    layout
        .group_of(panel)
        .unwrap()
        .iter()
        .map(String::as_str)
        .collect()
}

#[test]
fn panels_move_between_groups_edges_floats_and_windows_and_empty_containers_collapse() {
    let mut layout = workspace();
    // Into another group.
    assert!(layout.move_panel(
        "preview",
        DockTarget::Group {
            anchor: "output".into(),
            zone: DockZone::Center,
        },
    ));
    assert_eq!(group(&layout, "output"), ["output", "preview"]);
    // Beside a group: the group becomes a split with the panel on that side.
    assert!(layout.move_panel(
        "preview",
        DockTarget::Group {
            anchor: "editor".into(),
            zone: DockZone::Right,
        },
    ));
    assert_eq!(group(&layout, "preview"), ["preview"]);
    assert_eq!(layout.groups_in(DockArea::Main).len(), 4);
    // Whole-edge dock wraps the root.
    assert!(layout.move_panel(
        "output",
        DockTarget::Edge {
            area: DockArea::Main,
            zone: DockZone::Bottom,
        },
    ));
    let Some(DockNode::Split { axis, fraction, .. }) = &layout.root else {
        panic!("root should be a split");
    };
    assert_eq!((*axis, *fraction), (Axis::Vertical, 0.75));
    // Float and detach; the emptied groups disappear.
    assert!(layout.move_panel(
        "files",
        DockTarget::Float {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 200.0,
        },
    ));
    assert_eq!(layout.place_of("files"), Some(DockPlace::Float(0)));
    assert!(layout.move_panel(
        "preview",
        DockTarget::Window {
            width: 640.0,
            height: 480.0,
        },
    ));
    assert_eq!(
        layout.place_of("preview"),
        Some(DockPlace::Window(DockWindowId(1)))
    );
    assert_eq!(layout.groups_in(DockArea::Main).len(), 3);
    // A floating group takes tabs but never splits.
    assert!(layout.move_panel(
        "editor",
        DockTarget::Group {
            anchor: "files".into(),
            zone: DockZone::Left,
        },
    ));
    assert_eq!(group(&layout, "files"), ["files", "editor"]);
    // Closing the last panel of a window forgets the window.
    assert!(layout.close("preview"));
    assert!(layout.windows.is_empty());
    assert_eq!(layout.panels(), ["output", "files", "editor"]);
}

#[test]
fn pointless_and_unknown_moves_change_nothing_and_tab_positions_follow_the_visible_strip() {
    let mut layout = workspace();
    let before = layout.clone();
    for target in [
        // Alone in its group: splitting beside itself or tabbing into itself.
        DockTarget::Group {
            anchor: "files".into(),
            zone: DockZone::Left,
        },
        DockTarget::Tab {
            anchor: "files".into(),
            index: 0,
        },
        DockTarget::Group {
            anchor: "ghost".into(),
            zone: DockZone::Center,
        },
        DockTarget::Edge {
            area: DockArea::Window(DockWindowId(9)),
            zone: DockZone::Left,
        },
    ] {
        assert!(!layout.move_panel("files", target.clone()), "{target:?}");
        assert_eq!(layout, before);
    }
    assert!(!layout.move_panel(
        "ghost",
        DockTarget::Window {
            width: 1.0,
            height: 1.0
        }
    ));
    // Same group, centre: nothing to do.
    assert!(!layout.move_panel(
        "preview",
        DockTarget::Group {
            anchor: "editor".into(),
            zone: DockZone::Center,
        },
    ));
    // Reorder within the strip: "before index 0" and "after the last tab".
    assert!(layout.move_panel(
        "preview",
        DockTarget::Tab {
            anchor: "editor".into(),
            index: 0,
        },
    ));
    assert_eq!(group(&layout, "editor"), ["preview", "editor"]);
    assert!(layout.move_panel(
        "preview",
        DockTarget::Tab {
            anchor: "preview".into(),
            index: 2,
        },
    ));
    assert_eq!(group(&layout, "editor"), ["editor", "preview"]);
    // Dropping a tab where it already is changes nothing.
    assert!(!layout.move_panel(
        "preview",
        DockTarget::Tab {
            anchor: "editor".into(),
            index: 2,
        },
    ));
    // Splitting a group with one of its own panels uses a group mate as the anchor.
    assert!(layout.move_panel(
        "editor",
        DockTarget::Group {
            anchor: "editor".into(),
            zone: DockZone::Left,
        },
    ));
    assert_eq!(group(&layout, "editor"), ["editor"]);
}

#[test]
fn show_redock_and_retain_keep_a_layout_usable() {
    let mut layout = workspace();
    layout.show("search");
    assert_eq!(group(&layout, "search"), ["files", "search"]);
    layout.show("editor");
    assert!(layout.move_panel(
        "output",
        DockTarget::Window {
            width: 500.0,
            height: 300.0
        }
    ));
    assert!(layout.move_panel(
        "preview",
        DockTarget::Edge {
            area: DockArea::Window(DockWindowId(1)),
            zone: DockZone::Right,
        },
    ));
    assert_eq!(layout.groups_in(DockArea::Window(DockWindowId(1))).len(), 2);
    assert!(layout.redock(DockWindowId(1)));
    assert!(layout.windows.is_empty());
    assert_eq!(layout.place_of("output"), Some(DockPlace::Main));
    layout.retain_panels(|id| id != "files" && id != "search" && id != "output");
    assert_eq!(layout.panels(), ["preview", "editor"]);
    let mut empty = DockLayout::default();
    empty.show("first");
    assert_eq!(empty.root, Some(DockNode::tabs(["first"])));
}

#[test]
fn the_text_format_round_trips_tolerates_damage_and_reports_lines() {
    let mut layout = workspace();
    layout.activate("preview");
    layout.move_panel(
        "files",
        DockTarget::Float {
            x: 12.5,
            y: 40.0,
            width: 320.0,
            height: 240.0,
        },
    );
    layout.move_panel(
        "output",
        DockTarget::Window {
            width: 640.0,
            height: 480.0,
        },
    );
    layout.show("a \"quoted\" \\ id");
    let text = layout.to_text();
    assert_eq!(DockLayout::parse(&text).unwrap(), layout);

    let repaired = DockLayout::parse(
        "dock v1 // comment\n\
         main {\n\
           split horizontal 7 {\n\
             tabs 9 \"a\" \"b\" \"a\"\n\
             tabs 0\n\
           }\n\
         }\n\
         window 4 10 10 { tabs 0 \"b\" \"c\" }\n",
    )
    .unwrap();
    // The empty group collapses the split, duplicates go, numbers are repaired.
    assert_eq!(
        repaired.root,
        Some(DockNode::Tabs(seaglass_core::DockTabs {
            panels: vec!["a".into(), "b".into()],
            active: 1,
        }))
    );
    assert_eq!(repaired.windows[0].root, DockNode::tabs(["c"]));
    assert_eq!(
        (repaired.windows[0].width, repaired.windows[0].height),
        (200.0, 150.0)
    );
    assert_eq!(
        DockLayout::parse("dock v1\nmain { }\n").unwrap(),
        DockLayout::default()
    );

    for (text, line) in [
        ("dock v2", 1),
        ("dock v1\nmain {\n  split sideways 0.5 { }\n}", 3),
        (
            "dock v1\nmain {\n  split vertical 0.5 {\n    tabs 0 \"a\"\n  }\n}",
            3,
        ),
        ("dock v1\nmain {\n  tabs 0 \"a", 3),
        ("dock v1\nfloat 1 2 nan 4 { tabs 0 \"a\" }", 2),
        ("dock v1\nmain {", 2),
        ("dock v1\nmain { }\nmain { }", 3),
    ] {
        assert_eq!(DockLayout::parse(text).unwrap_err().line, line, "{text}");
    }
}

#[cfg(feature = "serde")]
#[test]
fn layouts_implement_serde() {
    fn assert_serde<T: serde::Serialize + for<'a> serde::Deserialize<'a>>() {}
    assert_serde::<DockLayout>();
}

/// What the application owns: the layout and the native windows option.
struct Docked {
    layout: DockLayout,
    native: bool,
}

fn panels() -> Vec<DockPanel> {
    vec![
        DockPanel::pinned("files", "Files"),
        DockPanel::new("editor", "Editor"),
        DockPanel::new("preview", "Preview"),
        DockPanel::new("output", "Output"),
    ]
}

fn view(state: &Docked, ui: &mut Ui<'_, Message<Docked>>) {
    ui.keyed("dock", |ui| {
        ui.dock(
            &state.layout,
            &panels(),
            DockOptions {
                native_windows: state.native,
                ..DockOptions::default()
            },
            |action| {
                act(move |s: &mut Docked| {
                    s.layout.apply(action.clone());
                })
            },
            |ui, id| ui.label(format!("content:{id}")),
        )
    });
}

struct App {
    driven: Driven<Docked>,
}

impl App {
    fn start(native: bool) -> Self {
        let state = Docked {
            layout: workspace(),
            native,
        };
        Self {
            driven: Driven::new(ROOT, state, view),
        }
    }

    /// One batch; whether the layout changed in it.
    fn frame(&mut self) -> bool {
        let before = self.layout().clone();
        self.driven.frame();
        *self.layout() != before
    }

    fn layout(&self) -> &DockLayout {
        &self.driven.state().layout
    }

    fn context(&mut self) -> &mut Context {
        &mut self.driven.context
    }
}

fn shows(context: &Context, needle: &str) -> bool {
    context.scene().primitives.iter().any(|primitive| {
        matches!(primitive, Primitive::StyledText { text, .. } | Primitive::Text { text, .. } if text == needle)
    })
}

/// Bounds of the first node with this role and label.
fn labelled(context: &Context, role: Role, label: &str) -> Rect {
    let mut stack = vec![context.root()];
    while let Some(id) = stack.pop() {
        let node = context.node(id).unwrap();
        if node.semantics.role == role && node.semantics.label == label {
            return node.bounds;
        }
        stack.extend(node.children.iter().rev());
    }
    panic!("no {role:?} labelled {label}");
}

fn center(rect: Rect) -> Point {
    Point {
        x: rect.x + rect.width / 2.0,
        y: rect.y + rect.height / 2.0,
    }
}

fn press(context: &mut Context, point: Point) {
    context.event(Event::PointerMoved(point));
    context.event(Event::PointerDown);
}

fn drag(context: &mut Context, from: Point, to: Point) {
    press(context, from);
    context.event(Event::PointerMoved(Point {
        x: from.x + 20.0,
        y: from.y + 20.0,
    }));
    context.event(Event::PointerMoved(to));
    context.event(Event::PointerUp);
}

/// The group (tab strip plus panel) showing `content:<panel>`.
fn group_bounds(context: &Context, panel: &str) -> Rect {
    let target = format!("content:{panel}");
    let mut stack = vec![context.root()];
    let mut label = None;
    while let Some(id) = stack.pop() {
        let node = context.node(id).unwrap();
        if node.semantics.label == target {
            label = Some(id);
            break;
        }
        stack.extend(node.children.iter().rev());
    }
    // label -> tab panel -> tabs -> dock group
    let mut id = label.expect("panel content is declared");
    for _ in 0..3 {
        id = context.node(id).unwrap().parent.unwrap();
    }
    context.node(id).unwrap().bounds
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

#[test]
fn the_dock_declares_active_panels_switches_tabs_and_closes_them() {
    let mut app = App::start(false);
    for (text, shown) in [
        ("content:files", true),
        ("content:editor", true),
        ("content:output", true),
        ("content:preview", false),
    ] {
        assert_eq!(shows(app.context(), text), shown, "{text}");
    }
    // Groups fill the dock: files on the left quarter, output at the bottom.
    let files = group_bounds(app.context(), "files");
    let output = group_bounds(app.context(), "output");
    assert_eq!((files.x, files.y, files.height), (0.0, 0.0, 600.0));
    assert!((files.width - 200.0).abs() < 8.0);
    assert!((output.y + output.height - 600.0).abs() < 0.5 && output.x > files.width);

    let preview = center(labelled(app.context(), Role::Tab, "Preview"));
    press(app.context(), preview);
    app.context().event(Event::PointerUp);
    assert!(app.frame());
    assert!(shows(app.context(), "content:preview") && !shows(app.context(), "content:editor"));

    // Delete closes the focused closable tab and reports it.
    app.context().event(Event::KeyDown {
        key: Key::Delete,
        shift: false,
        repeat: false,
    });
    app.frame();
    app.frame();
    assert!(!app.layout().contains("preview"));
    assert!(shows(app.context(), "content:editor"));
}

#[test]
fn dragging_a_tab_onto_indicators_strips_and_empty_space_docks_reorders_and_floats() {
    let mut app = App::start(false);
    let preview = center(labelled(app.context(), Role::Tab, "Preview"));
    let output = group_bounds(app.context(), "output");
    // Left compass indicator of the output group.
    let left = Point {
        x: output.x + output.width / 2.0 - 36.0,
        y: output.y + output.height / 2.0,
    };
    press(app.context(), preview);
    app.context().event(Event::PointerMoved(left));
    assert!(app.context().dock_dragging());
    app.frame();
    // The overlay shows the dragged title next to the pointer.
    assert!(shows(app.context(), "Preview"));
    app.context().event(Event::PointerUp);
    assert!(!app.context().dock_dragging());
    assert!(app.frame());
    let moved = group_bounds(app.context(), "preview");
    let output = group_bounds(app.context(), "output");
    assert!(moved.x < output.x && (moved.y - output.y).abs() < 0.5);

    // Escape cancels a drag.
    let editor = center(labelled(app.context(), Role::Tab, "Editor"));
    press(app.context(), editor);
    app.context().event(Event::PointerMoved(center(output)));
    app.context().event(Event::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    });
    app.context().event(Event::PointerUp);
    assert!(!app.frame());

    // Onto another strip, before its first tab.
    let output_tab = labelled(app.context(), Role::Tab, "Output");
    drag(
        app.context(),
        editor,
        Point {
            x: output_tab.x + 2.0,
            y: output_tab.y + output_tab.height / 2.0,
        },
    );
    app.frame();
    assert_eq!(group(app.layout(), "output"), ["editor", "output"]);

    // Over a group but on no indicator: the panel floats under the pointer.
    let editor = center(labelled(app.context(), Role::Tab, "Editor"));
    let files = group_bounds(app.context(), "files");
    drag(
        app.context(),
        editor,
        Point {
            x: files.x + 30.0,
            y: files.y + files.height - 40.0,
        },
    );
    app.frame();
    assert_eq!(app.layout().place_of("editor"), Some(DockPlace::Float(0)));
    app.frame();
    assert!(shows(app.context(), "content:editor"));

    // The whole dock's right edge indicator.
    let editor = center(labelled(app.context(), Role::Tab, "Editor"));
    drag(
        app.context(),
        editor,
        Point {
            x: 800.0 - 24.0,
            y: 300.0,
        },
    );
    app.frame();
    assert!(app.layout().floating.is_empty());
    app.frame();
    let editor = group_bounds(app.context(), "editor");
    assert_eq!((editor.y, editor.height), (0.0, 600.0));
    assert!((editor.x + editor.width - 800.0).abs() < 0.5 && (editor.width - 200.0).abs() < 8.0);
}

#[test]
fn floating_groups_move_resize_by_pointer_and_keys_block_clicks_and_rise_to_the_front() {
    let mut app = App::start(false);
    for (panel, x) in [("preview", 100.0), ("output", 160.0)] {
        app.driven.state_mut().layout.move_panel(
            panel,
            DockTarget::Float {
                x,
                y: 100.0,
                width: 300.0,
                height: 200.0,
            },
        );
    }
    app.frame();
    let float = group_bounds(app.context(), "preview");
    assert!(float.x >= 100.0 && float.y >= 100.0 && float.width <= 300.0);

    // Pressing inside the back float raises it.
    press(app.context(), Point { x: 110.0, y: 250.0 });
    app.context().event(Event::PointerUp);
    app.frame();
    assert_eq!(app.layout().floating[1].tabs.panels, ["preview"]);
    // Its surface swallows the press instead of reaching the editor tab below.
    assert_eq!(app.layout().place_of("editor"), Some(DockPlace::Main));

    app.frame();
    let grip = center(labelled(app.context(), Role::Group, "Move floating panel"));
    press(app.context(), grip);
    app.context().event(Event::PointerMoved(Point {
        x: grip.x + 50.0,
        y: grip.y + 30.0,
    }));
    app.context().event(Event::PointerUp);
    app.frame();
    let moved = app.layout().floating.last().unwrap();
    assert_eq!((moved.x, moved.y), (150.0, 130.0));

    // The grip keeps focus: arrows nudge by ten pixels.
    key(app.context(), Key::ArrowLeft);
    app.frame();
    assert_eq!(app.layout().floating.last().unwrap().x, 140.0);

    // Resizing the back float (the first in tree order) stops at the minimum
    // size and brings it to the front.
    app.frame();
    assert_eq!(app.layout().floating[0].tabs.panels, ["output"]);
    let resize = center(labelled(
        app.context(),
        Role::Group,
        "Resize floating panel",
    ));
    press(app.context(), resize);
    app.context()
        .event(Event::PointerMoved(Point { x: 0.0, y: 0.0 }));
    app.context().event(Event::PointerUp);
    app.frame();
    let float = app.layout().floating.last().unwrap();
    assert_eq!(float.tabs.panels, ["output"]);
    assert_eq!((float.width, float.height), (160.0, 100.0));
}

/// Opens the focused tab's panel menu and activates the item `downs` enabled
/// entries below the first; arrow keys skip disabled items.
fn choose(app: &mut App, downs: usize) {
    key(app.context(), Key::ContextMenu);
    app.frame();
    key(app.context(), Key::Home);
    for _ in 0..downs {
        key(app.context(), Key::ArrowDown);
    }
    key(app.context(), Key::Enter);
    app.frame();
    app.frame();
}

/// Focuses the editor tab again: a moved tab is a new node.
fn refocus(app: &mut App) {
    let tab = center(labelled(app.context(), Role::Tab, "Editor"));
    press(app.context(), tab);
    app.context().event(Event::PointerUp);
    app.frame();
}

#[test]
fn the_panel_menu_docks_floats_and_detaches_panels_from_the_keyboard() {
    let mut app = App::start(true);
    let editor = center(labelled(app.context(), Role::Tab, "Editor"));
    press(app.context(), editor);
    app.context().event(Event::PointerUp);
    app.frame();

    // Float, new window, (main window: disabled), dock left, dock right.
    choose(&mut app, 3);
    let bounds = group_bounds(app.context(), "editor");
    assert!((bounds.x + bounds.width - 800.0).abs() < 0.5 && bounds.height == 600.0);

    refocus(&mut app);
    choose(&mut app, 0);
    assert_eq!(app.layout().place_of("editor"), Some(DockPlace::Float(0)));

    // Item 1: a new window is created, and the panel leaves the main dock.
    // "Float" is disabled for a lone floating panel, so the first enabled item
    // is "Move to new window".
    refocus(&mut app);
    key(app.context(), Key::ContextMenu);
    app.frame();
    key(app.context(), Key::Home);
    key(app.context(), Key::Enter);
    assert!(app.frame());
    assert_eq!(
        app.layout()
            .windows
            .iter()
            .map(|w| w.id)
            .collect::<Vec<_>>(),
        [DockWindowId(1)]
    );
    app.frame();
    assert!(!shows(app.context(), "content:editor"));
}

#[test]
fn right_click_on_a_tab_opens_its_menu_and_a_release_outside_the_window_detaches() {
    let mut app = App::start(true);
    let preview = center(labelled(app.context(), Role::Tab, "Preview"));
    app.context().event(Event::ContextMenu(preview));
    app.frame();
    // The right-clicked tab became active and the menu is open.
    assert!(shows(app.context(), "content:preview"));
    assert!(shows(app.context(), "Move to new window"));
    key(app.context(), Key::Escape);
    app.frame();

    let preview = center(labelled(app.context(), Role::Tab, "Preview"));
    drag(app.context(), preview, Point { x: 900.0, y: 300.0 });
    app.frame();
    assert_eq!(
        app.layout()
            .windows
            .iter()
            .map(|w| w.id)
            .collect::<Vec<_>>(),
        [DockWindowId(1)]
    );

    // Without native windows the same release changes nothing.
    let mut app = App::start(false);
    let preview = center(labelled(app.context(), Role::Tab, "Preview"));
    drag(app.context(), preview, Point { x: 900.0, y: 300.0 });
    assert!(!app.frame());
    assert_eq!(*app.layout(), workspace());
}

mod detached {
    //! A detached window's dock is a second context: the app opens an OS
    //! window for it with `Effects::open_window` and closes it when the layout
    //! drops it, and `window_closed` is how a closed window comes back.
    use super::*;
    use seaglass_core::{WindowId, WindowRequest};
    use support::act_fx;

    const WINDOW: DockWindowId = DockWindowId(1);

    struct Detached {
        layout: DockLayout,
        window: Option<WindowId>,
    }

    fn open(app: &mut Driven<Detached>) -> WindowId {
        app.effects(|state, effects| {
            state.window = Some(effects.open_window(WindowRequest {
                title: "Docking (detached)".into(),
                size: Some(ROOT),
                modal_owner: None,
            }));
        });
        app.frame();
        app.state().window.expect("the window was opened")
    }

    fn start() -> (Driven<Detached>, WindowId) {
        let mut layout = workspace();
        layout.move_panel(
            "editor",
            DockTarget::Window {
                width: 640.0,
                height: 480.0,
            },
        );
        let state = Detached {
            layout,
            window: None,
        };
        let mut app = Driven::new(ROOT, state, |state, ui| {
            ui.keyed("dock", |ui| {
                ui.dock(
                    &state.layout,
                    &panels(),
                    DockOptions {
                        native_windows: true,
                        ..DockOptions::default()
                    },
                    |action| {
                        act(move |s: &mut Detached| {
                            s.layout.apply(action.clone());
                        })
                    },
                    |ui, id| ui.label(format!("content:{id}")),
                )
            });
        });
        app.set_window_view(|state, _, ui| {
            ui.keyed("dock", |ui| {
                ui.dock_window(
                    &state.layout,
                    WINDOW,
                    &panels(),
                    DockOptions {
                        native_windows: true,
                        ..DockOptions::default()
                    },
                    |action| {
                        // What changed the layout may have emptied the window.
                        act_fx(move |s: &mut Detached, effects| {
                            s.layout.apply(action.clone());
                            if s.layout.window(WINDOW).is_none()
                                && let Some(window) = s.window.take()
                            {
                                effects.close_window(window);
                            }
                        })
                    },
                    |ui, id| ui.label(format!("content:{id}")),
                )
            });
        });
        app.on_window_closed(|state, window, _| {
            if state.window == Some(window) {
                state.window = None;
                state.layout.redock(WINDOW);
            }
        });
        let window = open(&mut app);
        (app, window)
    }

    #[test]
    fn a_window_panel_menu_docks_it_back_into_the_main_window() {
        let (mut app, id) = start();
        assert!(shows(&app.window(id).context, "content:editor"));
        assert!(!shows(&app.context, "content:editor"));

        // Alone in its window, the panel's first enabled item is "Move to main window".
        let tab = center(labelled(&app.window(id).context, Role::Tab, "Editor"));
        press(&mut app.window(id).context, tab);
        app.window(id).context.event(Event::PointerUp);
        app.frame();
        key(&mut app.window(id).context, Key::ContextMenu);
        app.frame();
        key(&mut app.window(id).context, Key::Home);
        key(&mut app.window(id).context, Key::Enter);
        app.frame();
        let state = app.state();
        assert!(state.layout.windows.is_empty());
        assert_eq!(state.layout.place_of("editor"), Some(DockPlace::Main));
        assert!(app.window_ids().is_empty(), "the app closed the window");
        app.frame();
        assert!(shows(&app.context, "content:editor"));
    }

    #[test]
    fn closing_the_window_returns_its_panels_to_the_main_window() {
        let (mut app, id) = start();
        assert!(!app.state().layout.windows.is_empty());
        app.close_window_from_outside(id);
        app.frame();
        assert!(app.window_ids().is_empty());
        assert!(app.state().layout.windows.is_empty());
        assert_eq!(app.state().layout.place_of("editor"), Some(DockPlace::Main));
        assert!(
            shows(&app.context, "content:editor"),
            "the main dock shows it"
        );
    }

    #[test]
    fn the_app_closing_a_window_is_not_a_window_closed_call() {
        let (mut app, id) = start();
        app.effects(move |_, effects| effects.close_window(id));
        app.frame();
        assert!(app.window_ids().is_empty());
        // `window_closed` did not run, so the panel is still detached.
        assert_eq!(app.state().layout.windows.len(), 1);
    }
}
