mod support;

use rust_gui_core::{Action, Context, Dirty, Event, Key, Length, MonospaceMeasurer, Store, Style};
use rust_gui_devtools::{Inspector, InspectorModel, Recorder, Tab};
use rust_gui_scene::Size;
use std::time::Duration;
use support::Shown;

const TARGET: Size = Size::new(400.0, 300.0);

fn declare(context: &mut Context, recorder: &mut Recorder, title: &str) {
    let stats = context.run(TARGET, &mut MonospaceMeasurer, |ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("title", |ui| ui.label(title));
                ui.keyed("save", |ui| ui.button("Save"));
            })
        });
    });
    recorder.frame(
        stats,
        Duration::from_micros(500),
        Duration::from_micros(900),
    );
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}

#[test]
fn model_captures_paths_depths_last_changes_and_history() {
    let mut context = Context::new();
    let mut recorder = Recorder::new(8);
    declare(&mut context, &mut recorder, "Hello");
    recorder.event(1, "PointerMoved".into(), true);
    declare(&mut context, &mut recorder, "Hello!");
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    assert_eq!(model.nodes.len(), 4);
    let save = context.find(&["page", "save"]).unwrap();
    let title = context.find(&["page", "title"]).unwrap();
    assert_eq!(model.path(save), "page/save");
    assert_eq!(model.node(save).unwrap().depth, 2);
    let changed = &model.node(title).unwrap().details;
    assert!(changed.changed.contains(Dirty::TEXT) && changed.painted);
    assert!(model.node(save).unwrap().details.changed.is_empty());
    assert_eq!(model.node(save).unwrap().details.key, "save");
    assert_eq!((model.frames.len(), model.events.len()), (2, 1));
    assert_eq!(recorder.csv().lines().count(), 3);
    assert!(recorder.csv().starts_with("frame,declare_us"));
}

#[test]
fn inspector_rows_follow_collapse_selection_drives_overlay_and_export_is_requested() {
    let mut context = Context::new();
    let mut recorder = Recorder::new(8);
    declare(&mut context, &mut recorder, "Hello");
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    let mut shown = Shown::new(Inspector::default(), model);
    shown.frame();
    let tree = shown.context.find(&["inspector", "body", "tree"]).unwrap();
    let rows = |shown: &Shown| shown.context.node(tree).unwrap().collection.unwrap().total;
    assert_eq!(rows(&shown), 4);
    assert!(shown.host.inspector.overlay(&shown.host.model).is_empty());
    shown.context.action(tree, Action::Focus);
    key(&mut shown.context, Key::End);
    shown.frame();
    let save = context.find(&["page", "save"]).unwrap();
    assert_eq!(shown.host.inspector.selected(), Some(save));
    let overlay = shown.host.inspector.overlay(&shown.host.model);
    assert_eq!(overlay.len(), 4, "selection outline is four edge quads");
    shown.host.inspector.show_repaints = true;
    assert!(shown.host.inspector.overlay(&shown.host.model).len() > 4);
    key(&mut shown.context, Key::Home);
    key(&mut shown.context, Key::ArrowLeft);
    shown.frame();
    assert_eq!(rows(&shown), 1, "collapsing the root hides its subtree");
    key(&mut shown.context, Key::ArrowRight);
    shown.frame();
    assert_eq!(rows(&shown), 4);
    shown.host.inspector.tab = Tab::Frames;
    shown.frame();
    let export = shown
        .context
        .find(&["inspector", "actions", "export"])
        .unwrap();
    assert!(shown.context.action(export, Action::Activate));
    assert!(shown.frame().export_csv);
    assert!(!shown.frame().export_csv);
    shown.host.inspector.tab = Tab::Accessibility;
    shown.frame();
    assert!(
        shown.context.node(tree).is_none(),
        "switching tabs recreates the tree"
    );
    let tree = shown.context.find(&["inspector", "body", "tree"]).unwrap();
    assert_eq!(
        shown.context.node(tree).unwrap().collection.unwrap().total,
        4
    );
}

#[test]
fn the_diagnostics_tab_lists_entries_and_selecting_one_shows_its_node_in_the_tree() {
    let mut context = Context::new();
    let recorder = Recorder::new(8);
    context.run(TARGET, &mut MonospaceMeasurer, |ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("nameless", |ui| ui.button(""));
            })
        });
    });
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    assert_eq!(model.diagnostics.len(), 1);
    assert_eq!(model.diagnostics[0].path, "page/nameless");

    let mut shown = Shown::new(Inspector::showing(Tab::Diagnostics), model);
    shown.frame();
    let entry = shown
        .context
        .find(&["inspector", "diagnostics", "entry-0"])
        .expect("the entry is a button");
    assert!(shown.context.action(entry, Action::Activate));
    shown.frame();
    assert_eq!(shown.host.inspector.tab, Tab::Tree);
    assert_eq!(
        shown.host.inspector.selected(),
        context.find(&["page", "nameless"])
    );
    // The inspector's own UI raises no diagnostics.
    assert!(
        shown.context.diagnostics().is_empty(),
        "{:?}",
        shown.context.diagnostics()
    );
}

#[test]
fn the_state_tab_lists_a_stores_values_their_edges_and_what_moved_last() {
    let store = Store::new();
    let celsius = store.signal("celsius", 20i32);
    let read = celsius.clone();
    let fahrenheit = store.memo("fahrenheit", move || read.get() * 9 / 5 + 32);
    let mut context = Context::new();
    let declare = |context: &mut Context| {
        context.run(TARGET, &mut MonospaceMeasurer, |ui| {
            ui.observe(&store);
            ui.keyed("reading", |ui| ui.label(format!("{}", fahrenheit.get())));
        });
    };
    declare(&mut context);
    celsius.set(100);
    declare(&mut context);

    let model = InspectorModel::capture(&context, "Target", TARGET, &recorderless());
    assert_eq!(model.state.len(), 2);
    let mut shown = Shown::new(Inspector::showing(Tab::State), model);
    shown.frame();
    let host = &shown.context;
    let row = |index: usize| {
        let id = host
            .find(&["inspector", "state", &format!("state-{index}")])
            .expect("a row for every value");
        host.details(id).expect("the row").text
    };
    assert_eq!(row(0), "signal  celsius     v1");
    assert_eq!(row(1), "memo    fahrenheit  v2  <- celsius");
    // The signal moved last, so it is the one picked out.
    assert!(host.diagnostics().is_empty(), "{:?}", host.diagnostics());
}

/// The state tab needs no frame history.
fn recorderless() -> Recorder {
    Recorder::new(1)
}

#[test]
fn the_scene_tab_counts_what_is_held_and_lists_the_layers() {
    let mut context = Context::new();
    context.run(TARGET, &mut MonospaceMeasurer, |ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("title", |ui| ui.label("Hello"));
                // An opacity makes a layer, which is what the tab is for.
                ui.keyed("faded", |ui| {
                    ui.container(
                        Style::default()
                            .width(Length::Px(120.0))
                            .height(Length::Px(40.0))
                            .opacity(0.5),
                        |ui| {
                            ui.keyed("inside", |ui| ui.label("Faded"));
                        },
                    )
                });
            })
        });
    });
    let model = InspectorModel::capture(&context, "Target", TARGET, &Recorder::new(1));
    assert!(model.memory.nodes >= 5, "{:?}", model.memory);
    assert!(model.memory.scene_primitives > 0);
    assert!(model.memory.text_bytes > 0);
    assert_eq!(model.layers.len(), 1);
    assert_eq!(model.layers[0].opacity, 0.5);
    assert!(model.layers[0].primitives > 0);

    let nodes_in_model = model.memory.nodes;
    let mut shown = Shown::new(Inspector::showing(Tab::Scene), model);
    shown.frame();
    let host = &shown.context;
    let nodes = host
        .find(&["inspector", "memory-0"])
        .and_then(|id| host.details(id))
        .expect("the first memory line");
    assert!(
        nodes.text.contains(&format!("{nodes_in_model} nodes")),
        "{}",
        nodes.text
    );
    let layer = host
        .find(&["inspector", "layers", "layer-0"])
        .and_then(|id| host.details(id))
        .expect("the layer row");
    assert!(layer.text.contains("opacity 0.50"), "{}", layer.text);
    assert!(host.diagnostics().is_empty(), "{:?}", host.diagnostics());
}

#[test]
fn the_events_tab_follows_the_newest_event_in_its_own_row_style() {
    let mut context = Context::new();
    let mut recorder = Recorder::new(200);
    declare(&mut context, &mut recorder, "Hello");
    for number in 0..150 {
        recorder.event(1, format!("Event {number}"), number % 2 == 0);
    }
    let model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    let newest = model.events.last().unwrap().sequence;
    let mut shown = Shown::new(Inspector::showing(Tab::Events), model);
    // The list is revealed once its node exists, which is the second frame.
    shown.frame();
    shown.frame();
    let row = shown
        .context
        .find(&["inspector", "events", &format!("item-{newest}"), "label"])
        .expect("the newest event is in view");
    let text = shown.context.details(row).unwrap().text;
    assert!(text.contains("Event 149"), "{text}");
    assert!(
        shown
            .context
            .find(&["inspector", "events", "item-0"])
            .is_none(),
        "the oldest is scrolled away"
    );
}
