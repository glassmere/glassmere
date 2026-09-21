use rust_gui_core::{
    Axis, Context, DockArea, DockLayout, DockNode, DockOptions, DockPanel, DockPlace, DockResponse,
    DockTarget, DockWindowId, DockZone, Event, Key, MonospaceMeasurer, Role,
};
use rust_gui_scene::{Point, Primitive, Rect, Size};

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
        Some(DockNode::Tabs(rust_gui_core::DockTabs {
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

struct App {
    layout: DockLayout,
    native: bool,
    last: DockResponse,
}

fn panels() -> Vec<DockPanel> {
    vec![
        DockPanel::pinned("files", "Files"),
        DockPanel::new("editor", "Editor"),
        DockPanel::new("preview", "Preview"),
        DockPanel::new("output", "Output"),
    ]
}

fn frame(context: &mut Context, app: &mut App) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        app.last = ui.dock(
            "dock",
            &mut app.layout,
            &panels(),
            DockOptions {
                native_windows: app.native,
                ..DockOptions::default()
            },
            |ui, id| ui.label(format!("content:{id}")),
        );
    });
}

fn start(native: bool) -> (Context, App) {
    let mut context = Context::new();
    let mut app = App {
        layout: workspace(),
        native,
        last: DockResponse::default(),
    };
    frame(&mut context, &mut app);
    (context, app)
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

#[test]
fn the_dock_declares_active_panels_switches_tabs_and_closes_them() {
    let (mut context, mut app) = start(false);
    for (text, shown) in [
        ("content:files", true),
        ("content:editor", true),
        ("content:output", true),
        ("content:preview", false),
    ] {
        assert_eq!(shows(&context, text), shown, "{text}");
    }
    // Groups fill the dock: files on the left quarter, output at the bottom.
    let files = group_bounds(&context, "files");
    let output = group_bounds(&context, "output");
    assert_eq!((files.x, files.y, files.height), (0.0, 0.0, 600.0));
    assert!((files.width - 200.0).abs() < 8.0);
    assert!((output.y + output.height - 600.0).abs() < 0.5 && output.x > files.width);

    let preview = center(labelled(&context, Role::Tab, "Preview"));
    press(&mut context, preview);
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    assert!(app.last.changed);
    assert!(shows(&context, "content:preview") && !shows(&context, "content:editor"));

    // Delete closes the focused closable tab and reports it.
    context.event(Event::KeyDown {
        key: Key::Delete,
        shift: false,
        repeat: false,
    });
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    assert!(!app.layout.contains("preview"));
    assert!(shows(&context, "content:editor"));
}

#[test]
fn dragging_a_tab_onto_indicators_strips_and_empty_space_docks_reorders_and_floats() {
    let (mut context, mut app) = start(false);
    let preview = center(labelled(&context, Role::Tab, "Preview"));
    let output = group_bounds(&context, "output");
    // Left compass indicator of the output group.
    let left = Point {
        x: output.x + output.width / 2.0 - 36.0,
        y: output.y + output.height / 2.0,
    };
    press(&mut context, preview);
    context.event(Event::PointerMoved(left));
    assert!(context.dock_dragging());
    frame(&mut context, &mut app);
    // The overlay shows the dragged title next to the pointer.
    assert!(shows(&context, "Preview"));
    context.event(Event::PointerUp);
    assert!(!context.dock_dragging());
    frame(&mut context, &mut app);
    assert!(app.last.changed);
    let moved = group_bounds(&context, "preview");
    let output = group_bounds(&context, "output");
    assert!(moved.x < output.x && (moved.y - output.y).abs() < 0.5);

    // Escape cancels a drag.
    let editor = center(labelled(&context, Role::Tab, "Editor"));
    press(&mut context, editor);
    context.event(Event::PointerMoved(center(output)));
    context.event(Event::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    });
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    assert!(!app.last.changed);

    // Onto another strip, before its first tab.
    let output_tab = labelled(&context, Role::Tab, "Output");
    drag(
        &mut context,
        editor,
        Point {
            x: output_tab.x + 2.0,
            y: output_tab.y + output_tab.height / 2.0,
        },
    );
    frame(&mut context, &mut app);
    assert_eq!(group(&app.layout, "output"), ["editor", "output"]);

    // Over a group but on no indicator: the panel floats under the pointer.
    let editor = center(labelled(&context, Role::Tab, "Editor"));
    let files = group_bounds(&context, "files");
    drag(
        &mut context,
        editor,
        Point {
            x: files.x + 30.0,
            y: files.y + files.height - 40.0,
        },
    );
    frame(&mut context, &mut app);
    assert_eq!(app.layout.place_of("editor"), Some(DockPlace::Float(0)));
    frame(&mut context, &mut app);
    assert!(shows(&context, "content:editor"));

    // The whole dock's right edge indicator.
    let editor = center(labelled(&context, Role::Tab, "Editor"));
    drag(
        &mut context,
        editor,
        Point {
            x: 800.0 - 24.0,
            y: 300.0,
        },
    );
    frame(&mut context, &mut app);
    assert!(app.layout.floating.is_empty());
    frame(&mut context, &mut app);
    let editor = group_bounds(&context, "editor");
    assert_eq!((editor.y, editor.height), (0.0, 600.0));
    assert!((editor.x + editor.width - 800.0).abs() < 0.5 && (editor.width - 200.0).abs() < 8.0);
}

#[test]
fn floating_groups_move_resize_by_pointer_and_keys_block_clicks_and_rise_to_the_front() {
    let (mut context, mut app) = start(false);
    for (panel, x) in [("preview", 100.0), ("output", 160.0)] {
        app.layout.move_panel(
            panel,
            DockTarget::Float {
                x,
                y: 100.0,
                width: 300.0,
                height: 200.0,
            },
        );
    }
    frame(&mut context, &mut app);
    let float = group_bounds(&context, "preview");
    assert!(float.x >= 100.0 && float.y >= 100.0 && float.width <= 300.0);

    // Pressing inside the back float raises it.
    press(&mut context, Point { x: 110.0, y: 250.0 });
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    assert_eq!(app.layout.floating[1].tabs.panels, ["preview"]);
    // Its surface swallows the press instead of reaching the editor tab below.
    assert_eq!(app.layout.place_of("editor"), Some(DockPlace::Main));

    frame(&mut context, &mut app);
    let grip = center(labelled(&context, Role::Group, "Move floating panel"));
    press(&mut context, grip);
    context.event(Event::PointerMoved(Point {
        x: grip.x + 50.0,
        y: grip.y + 30.0,
    }));
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    let moved = app.layout.floating.last().unwrap();
    assert_eq!((moved.x, moved.y), (150.0, 130.0));

    // The grip keeps focus: arrows nudge by ten pixels.
    context.event(Event::KeyDown {
        key: Key::ArrowLeft,
        shift: false,
        repeat: false,
    });
    frame(&mut context, &mut app);
    assert_eq!(app.layout.floating.last().unwrap().x, 140.0);

    // Resizing the back float (the first in tree order) stops at the minimum
    // size and brings it to the front.
    frame(&mut context, &mut app);
    assert_eq!(app.layout.floating[0].tabs.panels, ["output"]);
    let resize = center(labelled(&context, Role::Group, "Resize floating panel"));
    press(&mut context, resize);
    context.event(Event::PointerMoved(Point { x: 0.0, y: 0.0 }));
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    let float = app.layout.floating.last().unwrap();
    assert_eq!(float.tabs.panels, ["output"]);
    assert_eq!((float.width, float.height), (160.0, 100.0));
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

/// Opens the focused tab's panel menu and activates the item `downs` enabled
/// entries below the first; arrow keys skip disabled items.
fn choose(context: &mut Context, app: &mut App, downs: usize) {
    key(context, Key::ContextMenu);
    frame(context, app);
    key(context, Key::Home);
    for _ in 0..downs {
        key(context, Key::ArrowDown);
    }
    key(context, Key::Enter);
    frame(context, app);
    frame(context, app);
}

#[test]
fn the_panel_menu_docks_floats_detaches_and_returns_panels_from_the_keyboard() {
    let (mut context, mut app) = start(true);
    let editor = center(labelled(&context, Role::Tab, "Editor"));
    press(&mut context, editor);
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);

    // Float, new window, (main window: disabled), dock left, dock right.
    choose(&mut context, &mut app, 3);
    let bounds = group_bounds(&context, "editor");
    assert!((bounds.x + bounds.width - 800.0).abs() < 0.5 && bounds.height == 600.0);

    // The moved tab is a new node, so select it again before the next menu.
    let refocus = |context: &mut Context, app: &mut App| {
        let tab = center(labelled(context, Role::Tab, "Editor"));
        press(context, tab);
        context.event(Event::PointerUp);
        frame(context, app);
    };
    refocus(&mut context, &mut app);
    choose(&mut context, &mut app, 0);
    assert_eq!(app.layout.place_of("editor"), Some(DockPlace::Float(0)));

    // Item 1: a new window is reported once, and its dock renders separately.
    // "Float" is disabled for a lone floating panel, so the first enabled item
    // is "Move to new window".
    refocus(&mut context, &mut app);
    key(&mut context, Key::ContextMenu);
    frame(&mut context, &mut app);
    key(&mut context, Key::Home);
    key(&mut context, Key::Enter);
    frame(&mut context, &mut app);
    assert_eq!(app.last.detached, [DockWindowId(1)]);
    frame(&mut context, &mut app);
    assert!(app.last.detached.is_empty());
    assert!(!shows(&context, "content:editor"));

    let mut window = Context::new();
    let mut response = DockResponse::default();
    let mut window_frame = |window: &mut Context, layout: &mut DockLayout| {
        window.run(ROOT, &mut MonospaceMeasurer, |ui| {
            response = ui.dock_window(
                "dock",
                layout,
                DockWindowId(1),
                &panels(),
                DockOptions {
                    native_windows: true,
                    ..DockOptions::default()
                },
                |ui, id| ui.label(format!("content:{id}")),
            );
        });
    };
    window_frame(&mut window, &mut app.layout);
    assert!(shows(&window, "content:editor"));

    // Alone in its window, the panel's first enabled item is "Move to main window".
    let tab = center(labelled(&window, Role::Tab, "Editor"));
    press(&mut window, tab);
    window.event(Event::PointerUp);
    window_frame(&mut window, &mut app.layout);
    key(&mut window, Key::ContextMenu);
    window_frame(&mut window, &mut app.layout);
    key(&mut window, Key::Home);
    key(&mut window, Key::Enter);
    window_frame(&mut window, &mut app.layout);
    assert!(app.layout.windows.is_empty());
    assert_eq!(app.layout.place_of("editor"), Some(DockPlace::Main));
    window_frame(&mut window, &mut app.layout);
    assert!(!shows(&window, "content:editor"));
}

#[test]
fn right_click_on_a_tab_opens_its_menu_and_a_release_outside_the_window_detaches() {
    let (mut context, mut app) = start(true);
    let preview = center(labelled(&context, Role::Tab, "Preview"));
    context.event(Event::ContextMenu(preview));
    frame(&mut context, &mut app);
    // The right-clicked tab became active and the menu is open.
    assert!(shows(&context, "content:preview"));
    assert!(shows(&context, "Move to new window"));
    key(&mut context, Key::Escape);
    frame(&mut context, &mut app);

    let preview = center(labelled(&context, Role::Tab, "Preview"));
    drag(&mut context, preview, Point { x: 900.0, y: 300.0 });
    frame(&mut context, &mut app);
    assert_eq!(app.last.detached, [DockWindowId(1)]);

    // Without native windows the same release changes nothing.
    let (mut context, mut app) = start(false);
    let preview = center(labelled(&context, Role::Tab, "Preview"));
    drag(&mut context, preview, Point { x: 900.0, y: 300.0 });
    frame(&mut context, &mut app);
    assert!(!app.last.changed);
    assert_eq!(app.layout, workspace());
}
