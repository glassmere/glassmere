//! A dock and a split send what the user asked for as messages, `update`
//! applies it, and the view declares once per batch.

use seaglass_core::{
    Action, App, Axis, Context, DockAction, DockArea, DockLayout, DockNode, DockOptions, DockPanel,
    DockTarget, DockZone, Effects, Event, Key, Length, NodeId, SplitOptions, Style, Ui,
};
use seaglass_scene::{Point, Size};
use seaglass_testing::Harness;
use std::cell::Cell;

const SIZE: Size = Size::new(800.0, 1000.0);

#[derive(Clone, Debug)]
enum Msg {
    Dock(DockAction),
    Ratio(f32),
}

struct Docked {
    layout: DockLayout,
    ratio: f32,
    sent: Vec<Msg>,
    views: Cell<usize>,
}

fn panels() -> Vec<DockPanel> {
    vec![
        DockPanel::new("files", "Files"),
        DockPanel::new("editor", "Editor"),
        DockPanel::new("preview", "Preview"),
        DockPanel::new("output", "Output"),
    ]
}

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

impl App for Docked {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match &message {
            Msg::Dock(action) => {
                self.layout.apply(action.clone());
            }
            Msg::Ratio(ratio) => self.ratio = *ratio,
        }
        self.sent.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        let fixed = |height: f32| {
            Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Px(height))
        };
        ui.column(Style::default().width(Length::Percent(1.0)), |ui| {
            ui.dock(
                &self.layout,
                &panels(),
                DockOptions {
                    style: fixed(600.0),
                    ..DockOptions::default()
                },
                Msg::Dock,
                |ui, panel| {
                    ui.label(format!("Content of {panel}"));
                },
            );
            ui.split(
                self.ratio,
                Axis::Horizontal,
                SplitOptions {
                    style: fixed(200.0),
                    min_first: 0.0,
                    min_second: 0.0,
                    ..SplitOptions::default()
                },
                Msg::Ratio,
                |ui| {
                    ui.label("Left pane");
                },
                |ui| {
                    ui.label("Right pane");
                },
            );
        });
    }
}

fn harness() -> Harness<Docked> {
    Harness::app(
        SIZE,
        Docked {
            layout: workspace(),
            ratio: 0.5,
            sent: Vec::new(),
            views: Cell::new(0),
        },
    )
}

fn last(harness: &Harness<Docked>) -> Option<&Msg> {
    harness.state().sent.last()
}

/// Every node with `key`, in declaration order.
fn keyed(context: &Context, key: &str) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![context.root()];
    while let Some(id) = stack.pop() {
        let node = context.node(id).unwrap();
        if context
            .details(id)
            .is_some_and(|details| details.key == key)
        {
            found.push(id);
        }
        stack.extend(node.children.iter().rev());
    }
    found
}

fn center(context: &Context, id: NodeId) -> Point {
    let bounds = context.node(id).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

#[test]
fn a_tab_click_sends_the_panel_it_picks_and_declares_once() {
    let mut harness = harness();
    let views = harness.state().views.get();
    let preview = harness.by_label("Preview");
    let stats = harness.click(preview);
    assert!(stats.declared);
    assert_eq!(stats.messages, 1);
    assert_eq!(harness.state().views.get(), views + 1);
    assert!(matches!(
        last(&harness),
        Some(Msg::Dock(DockAction::Select(panel))) if panel == "preview"
    ));
    assert!(harness.query_label("Content of preview").is_some());
    assert!(harness.query_label("Content of editor").is_none());
}

#[test]
fn delete_on_a_tab_sends_close_and_the_update_removes_the_panel() {
    let mut harness = harness();
    let editor = harness.by_label("Editor");
    harness.focus(editor);
    let views = harness.state().views.get();
    let stats = harness.press(Key::Delete);
    assert!(stats.declared);
    assert_eq!(harness.state().views.get(), views + 1);
    assert!(matches!(
        last(&harness),
        Some(Msg::Dock(DockAction::Close(panel))) if panel == "editor"
    ));
    assert!(!harness.state().layout.contains("editor"));
    assert!(harness.query_label("Content of editor").is_none());
}

#[test]
fn a_divider_drag_sends_the_new_share_of_the_split() {
    let mut harness = harness();
    // The dock is declared before the split, so its divider comes first.
    // Dock root, the dock's column, then the split_msg below the dock.
    let dividers = keyed(harness.context(), "$divider");
    assert_eq!(dividers.len(), 3);
    assert!(harness.action(dividers[0], Action::SetNumber(30.0)));
    let stats = harness.run();
    assert!(stats.declared);
    match last(&harness) {
        Some(Msg::Dock(DockAction::Ratio { path, fraction, .. })) => {
            assert!(path.is_empty());
            assert!((fraction - 0.3).abs() < 1e-4, "fraction {fraction}");
        }
        other => panic!("expected a ratio, got {other:?}"),
    }
    let DockNode::Split { fraction, .. } = harness.state().layout.root.as_ref().unwrap() else {
        panic!("the root is a split");
    };
    assert!((fraction - 0.3).abs() < 1e-4);
}

#[test]
fn a_split_msg_divider_sends_its_f32_and_update_applies_it() {
    let mut harness = harness();
    let dividers = keyed(harness.context(), "$divider");
    assert!(harness.action(dividers[2], Action::SetNumber(70.0)));
    harness.run();
    assert!(matches!(last(&harness), Some(Msg::Ratio(ratio)) if (ratio - 0.7).abs() < 1e-4));
    assert!((harness.state().ratio - 0.7).abs() < 1e-4);
}

#[test]
fn dragging_a_tab_onto_another_group_sends_a_move() {
    let mut harness = harness();
    let from = center(harness.context(), harness.by_label("Files"));
    let to = center(harness.context(), harness.by_label("Output"));
    harness.event(Event::PointerMoved(from));
    harness.event(Event::PointerDown);
    harness.event(Event::PointerMoved(Point {
        x: from.x + 20.0,
        y: from.y + 20.0,
    }));
    harness.event(Event::PointerMoved(to));
    harness.event(Event::PointerUp);
    let stats = harness.run();
    assert!(stats.declared);
    assert!(matches!(
        last(&harness),
        Some(Msg::Dock(DockAction::Move { panel, .. })) if panel == "files"
    ));
    assert_ne!(
        harness.state().layout.group_of("files"),
        Some(&["files".to_owned()][..])
    );
}

#[test]
fn the_panel_menu_sends_its_pick_as_an_action() {
    let mut harness = harness();
    // The first group's menu, in declaration order.
    let menu = keyed(harness.context(), "$menu")[0];
    harness.click(menu);
    // Activation rather than a pointer click: the pick's popup item is clipped
    // to a few pixels in this layout, which the pointer cannot reach.
    let close = harness.by_label("Close");
    harness.action(close, Action::Activate);
    harness.run();
    assert!(matches!(
        last(&harness),
        Some(Msg::Dock(DockAction::Close(panel))) if panel == "files"
    ));
    assert!(!harness.state().layout.contains("files"));
}

#[test]
fn a_batch_without_events_declares_nothing() {
    let mut harness = harness();
    let views = harness.state().views.get();
    let stats = harness.run();
    assert!(!stats.declared);
    assert_eq!(harness.state().views.get(), views);
    assert!(harness.state().sent.is_empty());
}

#[test]
fn apply_changes_only_what_it_names() {
    let mut layout = workspace();
    assert!(layout.apply(DockAction::Select("preview".into())));
    assert!(!layout.apply(DockAction::Select("preview".into())));
    assert!(!layout.apply(DockAction::Ratio {
        area: DockArea::Main,
        path: vec![0],
        fraction: 0.9,
    }));
    assert!(layout.apply(DockAction::Ratio {
        area: DockArea::Main,
        path: vec![],
        fraction: 2.0,
    }));
    assert!(layout.apply(DockAction::Move {
        panel: "output".into(),
        target: DockTarget::Edge {
            area: DockArea::Main,
            zone: DockZone::Left,
        },
    }));
    assert!(layout.contains("output"));
}
