use rust_gui_core::{Action, Context, Dirty, Event, Key, MonospaceMeasurer, Store, Style};
use rust_gui_devtools::{Inspector, InspectorModel, InspectorResponse, Recorder, Tab};
use rust_gui_scene::Size;
use std::time::Duration;

const TARGET: Size = Size::new(400.0, 300.0);
const VIEWPORT: Size = Size::new(800.0, 700.0);

fn declare(context: &mut Context, recorder: &mut Recorder, title: &str) {
    let stats = context.run(TARGET, &mut MonospaceMeasurer, |ui| {
        ui.column("page", Style::default(), |ui| {
            ui.label_keyed("title", title);
            ui.button_keyed("save", "Save");
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
    let mut model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    model.viewport = VIEWPORT;
    let mut inspector = Inspector::default();
    let mut host = Context::new();
    let frame = |host: &mut Context, inspector: &mut Inspector| -> InspectorResponse {
        let mut response = InspectorResponse::default();
        host.run(VIEWPORT, &mut MonospaceMeasurer, |ui| {
            response = inspector.ui(ui, &model);
        });
        response
    };
    frame(&mut host, &mut inspector);
    let tree = host.find(&["inspector", "body", "tree"]).unwrap();
    let rows = |host: &Context| host.node(tree).unwrap().collection.unwrap().total;
    assert_eq!(rows(&host), 4);
    assert!(inspector.overlay(&model).is_empty());
    host.action(tree, Action::Focus);
    key(&mut host, Key::End);
    frame(&mut host, &mut inspector);
    let save = context.find(&["page", "save"]).unwrap();
    assert_eq!(inspector.selected(), Some(save));
    let overlay = inspector.overlay(&model);
    assert_eq!(overlay.len(), 4, "selection outline is four edge quads");
    inspector.show_repaints = true;
    assert!(inspector.overlay(&model).len() > 4);
    key(&mut host, Key::Home);
    key(&mut host, Key::ArrowLeft);
    frame(&mut host, &mut inspector);
    assert_eq!(rows(&host), 1, "collapsing the root hides its subtree");
    key(&mut host, Key::ArrowRight);
    frame(&mut host, &mut inspector);
    assert_eq!(rows(&host), 4);
    inspector.tab = Tab::Frames;
    frame(&mut host, &mut inspector);
    let export = host.find(&["inspector", "actions", "export"]).unwrap();
    assert!(host.action(export, Action::Activate));
    assert!(frame(&mut host, &mut inspector).export_csv);
    assert!(!frame(&mut host, &mut inspector).export_csv);
    inspector.tab = Tab::Accessibility;
    frame(&mut host, &mut inspector);
    assert!(
        host.node(tree).is_none(),
        "switching tabs recreates the tree"
    );
    let tree = host.find(&["inspector", "body", "tree"]).unwrap();
    assert_eq!(host.node(tree).unwrap().collection.unwrap().total, 4);
}

#[test]
fn the_diagnostics_tab_lists_entries_and_selecting_one_shows_its_node_in_the_tree() {
    let mut context = Context::new();
    let recorder = Recorder::new(8);
    context.run(TARGET, &mut MonospaceMeasurer, |ui| {
        ui.column("page", Style::default(), |ui| {
            ui.button_keyed("nameless", "");
        });
    });
    let mut model = InspectorModel::capture(&context, "Target", TARGET, &recorder);
    model.viewport = VIEWPORT;
    assert_eq!(model.diagnostics.len(), 1);
    assert_eq!(model.diagnostics[0].path, "page/nameless");

    let mut inspector = Inspector::default();
    inspector.tab = Tab::Diagnostics;
    let mut host = Context::new();
    let frame = |host: &mut Context, inspector: &mut Inspector| {
        host.run(VIEWPORT, &mut MonospaceMeasurer, |ui| {
            inspector.ui(ui, &model);
        });
    };
    frame(&mut host, &mut inspector);
    let entry = host
        .find(&["inspector", "diagnostics", "entry-0"])
        .expect("the entry is a button");
    assert!(host.action(entry, Action::Activate));
    frame(&mut host, &mut inspector);
    assert_eq!(inspector.tab, Tab::Tree);
    assert_eq!(inspector.selected(), context.find(&["page", "nameless"]));
    // The inspector's own UI raises no diagnostics.
    assert!(host.diagnostics().is_empty(), "{:?}", host.diagnostics());
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
            ui.label_keyed("reading", format!("{}", fahrenheit.get()));
        });
    };
    declare(&mut context);
    celsius.set(100);
    declare(&mut context);

    let mut model = InspectorModel::capture(&context, "Target", TARGET, &recorderless());
    model.viewport = VIEWPORT;
    assert_eq!(model.state.len(), 2);
    let mut inspector = Inspector::showing(Tab::State);
    let mut host = Context::new();
    host.run(VIEWPORT, &mut MonospaceMeasurer, |ui| {
        inspector.ui(ui, &model);
    });
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
