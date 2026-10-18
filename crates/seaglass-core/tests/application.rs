mod support;

use seaglass_core::app::Action as Message;
use seaglass_core::{
    Action, Command, Context, DragOutcome, DragPayload, DragSourceBuilder, DropKind,
    DropTargetBuilder, Event, Key, Length, Shortcut, Style, Ui, WindowId, WindowRequest,
};
use seaglass_scene::{Point, Size};
use support::{Driven, act, center};

const ROOT: Size = Size::new(500.0, 500.0);

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}
fn boxed() -> Style {
    Style {
        width: Length::Px(200.0),
        height: Length::Px(60.0),
        ..Style::default()
    }
}
fn hello() -> DragPayload {
    DragPayload::Text("hello".into())
}
fn dragging(context: &Context, path: &[&str]) -> bool {
    context
        .node(context.find(path).unwrap())
        .unwrap()
        .drag
        .unwrap()
        .dragging
}
fn can_drop(context: &Context, path: &[&str]) -> bool {
    context
        .node(context.find(path).unwrap())
        .unwrap()
        .drag
        .unwrap()
        .can_drop
}
fn drag_to(context: &mut Context, target: &str) {
    context.event(Event::PointerMoved(center(context, &["source"])));
    context.event(Event::PointerDown);
    context.event(Event::PointerMoved(center(context, &[target])));
}

/// What the board's drag source and its drop targets sent.
struct Board {
    target_enabled: bool,
    payload: DragPayload,
    text: Vec<DragPayload>,
    item: Vec<DragPayload>,
    outcomes: Vec<DragOutcome>,
}

fn board(payload: DragPayload) -> Driven<Board> {
    Driven::new(
        ROOT,
        Board {
            target_enabled: true,
            payload,
            text: Vec::new(),
            item: Vec::new(),
            outcomes: Vec::new(),
        },
        board_view,
    )
}

fn board_view(state: &Board, ui: &mut Ui<'_, Message<Board>>) {
    let payload = state.payload.clone();
    ui.keyed("source", |ui| {
        ui.drag_source(
            boxed(),
            DragSourceBuilder::new("Card", payload)
                .on_drag_end(|outcome| act(move |s: &mut Board| s.outcomes.push(outcome))),
            |ui| {
                ui.label("Card");
            },
        )
    });
    ui.enabled(state.target_enabled, |ui| {
        ui.keyed("text", |ui| {
            ui.drop_target(
                boxed(),
                DropTargetBuilder::new("Text bin", DropKind::Text)
                    .on_drop(|payload| act(move |s: &mut Board| s.text.push(payload.clone()))),
                |_| {},
            )
        });
    });
    ui.keyed("item", |ui| {
        ui.drop_target(
            boxed(),
            DropTargetBuilder::new("Item bin", DropKind::Item)
                .on_drop(|payload| act(move |s: &mut Board| s.item.push(payload.clone()))),
            |_| {},
        )
    });
}

#[test]
fn pointer_drag_needs_a_threshold_matches_payload_kind_and_delivers_once() {
    let mut app = board(hello());
    let start = center(&app.context, &["source"]);
    app.context.event(Event::PointerMoved(start));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerMoved(Point {
        x: start.x + 3.0,
        y: start.y + 3.0,
    }));
    assert!(!dragging(&app.context, &["source"]));
    assert!(!can_drop(&app.context, &["text"]));
    app.context
        .event(Event::PointerMoved(center(&app.context, &["text"])));
    assert!(dragging(&app.context, &["source"]));
    assert!(can_drop(&app.context, &["text"]));
    assert!(!can_drop(&app.context, &["item"]));
    assert_eq!(app.context.captured(), None);
    app.frame();
    assert!(app.state().text.is_empty());
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().text, [hello()]);
    app.frame();
    assert_eq!(app.state().text.len(), 1);
    assert!(!dragging(&app.context, &["source"]));

    drag_to(&mut app.context, "item");
    app.context.event(Event::PointerUp);
    app.frame();
    assert!(app.state().item.is_empty() && app.state().text.len() == 1);

    app.state_mut().target_enabled = false;
    app.frame();
    drag_to(&mut app.context, "text");
    assert!(!can_drop(&app.context, &["text"]));
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().text.len(), 1);
}

#[test]
fn escape_and_focus_loss_cancel_an_active_drag() {
    let mut app = board(hello());
    drag_to(&mut app.context, "text");
    assert!(dragging(&app.context, &["source"]));
    assert!(app.context.event(Event::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    }));
    assert!(!dragging(&app.context, &["source"]));
    assert!(!can_drop(&app.context, &["text"]));
    app.context.event(Event::PointerUp);
    app.frame();
    assert!(app.state().text.is_empty());

    drag_to(&mut app.context, "text");
    assert!(dragging(&app.context, &["source"]));
    app.context.event(Event::FocusLost);
    assert!(!dragging(&app.context, &["source"]));
    app.context.event(Event::PointerUp);
    app.frame();
    assert!(app.state().text.is_empty());
}

#[test]
fn keyboard_and_accessibility_drags_share_the_pointer_delivery_path() {
    let mut app = board(hello());
    let source = app.context.find(&["source"]).unwrap();
    let text = app.context.find(&["text"]).unwrap();
    assert!(app.context.action(source, Action::Focus));
    assert!(app.context.event(Event::KeyDown {
        key: Key::Space,
        shift: false,
        repeat: false,
    }));
    assert!(dragging(&app.context, &["source"]));
    assert!(can_drop(&app.context, &["text"]));
    assert!(app.context.action(text, Action::Focus));
    key(&mut app.context, Key::Enter);
    app.frame();
    assert_eq!(app.state().text, [hello()]);
    app.frame();
    assert_eq!(app.state().text.len(), 1);
    assert!(!dragging(&app.context, &["source"]));

    assert!(!app.context.action(text, Action::Drop));
    assert!(app.context.action(source, Action::StartDrag));
    assert!(app.context.action(text, Action::Drop));
    app.frame();
    assert_eq!(app.state().text, [hello(), hello()]);
    assert!(!dragging(&app.context, &["source"]));
}

#[test]
fn a_host_exports_an_active_drag_and_the_source_hears_how_it_ended_once() {
    let files = DragPayload::Files(vec!["report.pdf".into()]);
    let mut app = board(files.clone());
    // Nothing to export before the drag threshold is passed.
    app.context
        .event(Event::PointerMoved(center(&app.context, &["source"])));
    app.context.event(Event::PointerDown);
    assert_eq!(app.context.dragging(), None);
    assert_eq!(app.context.export_drag(), None);
    // The pointer leaves the window; the host takes the drag over.
    app.context
        .event(Event::PointerMoved(Point { x: -40.0, y: 20.0 }));
    assert_eq!(app.context.dragging(), Some(&files));
    assert_eq!(app.context.export_drag(), Some(files.clone()));
    assert_eq!(app.context.dragging(), None);
    assert!(!dragging(&app.context, &["source"]));
    assert_eq!(app.context.captured(), None);
    app.frame();
    assert!(app.state().outcomes.is_empty());
    // The system swallowed the release, so none is needed to end the drag.
    app.context.finish_exported_drag(DragOutcome::Dropped);
    app.frame();
    assert_eq!(app.state().outcomes, [DragOutcome::Dropped]);
    app.frame();
    assert_eq!(app.state().outcomes.len(), 1);
    // An outcome without an export is nobody's.
    app.context.finish_exported_drag(DragOutcome::Cancelled);
    app.frame();
    assert_eq!(app.state().outcomes.len(), 1);
}

/// What the file and bin drop targets received.
#[derive(Default)]
struct Dropped {
    dropped: Vec<DragPayload>,
}

fn files_view(_: &Dropped, ui: &mut Ui<'_, Message<Dropped>>) {
    ui.keyed("files", |ui| {
        ui.drop_target(
            boxed(),
            DropTargetBuilder::new("Files", DropKind::Files)
                .on_drop(|payload| act(move |s: &mut Dropped| s.dropped.push(payload.clone()))),
            |_| {},
        )
    });
}

#[test]
fn external_file_drops_merge_per_target_and_miss_empty_space() {
    let mut app = Driven::new(ROOT, Dropped::default(), files_view);
    let position = center(&app.context, &["files"]);
    for name in ["a.txt", "b.txt"] {
        assert!(app.context.event(Event::DropAt {
            position,
            payload: DragPayload::Files(vec![name.into()]),
        }));
    }
    app.frame();
    assert_eq!(
        app.state().dropped,
        [DragPayload::Files(vec!["a.txt".into(), "b.txt".into()])]
    );
    app.frame();
    assert_eq!(app.state().dropped.len(), 1);
    assert!(!app.context.event(Event::DropAt {
        position: Point { x: 450.0, y: 450.0 },
        payload: DragPayload::Files(vec!["c.txt".into()]),
    }));
    assert!(!app.context.event(Event::DropAt {
        position,
        payload: DragPayload::Text("not a file".into()),
    }));
    app.frame();
    assert_eq!(app.state().dropped.len(), 1);
}

/// The counts of what the command buttons and the field sent, and the field.
#[derive(Default)]
struct Shortcuts {
    save_count: usize,
    new_count: usize,
    find_count: usize,
    value: String,
}

fn shortcuts_view(state: &Shortcuts, ui: &mut Ui<'_, Message<Shortcuts>>) {
    let save = Command::new("Save", Some(Shortcut::primary('s')));
    let mut new = Command::new("New", Some(Shortcut::primary('n')));
    new.enabled = false;
    let mut find = Command::new("Find", Some(Shortcut::primary('f')));
    find.while_editing = true;
    ui.keyed("save", |ui| {
        ui.command_button(&save, act(|s: &mut Shortcuts| s.save_count += 1))
    });
    ui.keyed("new", |ui| {
        ui.command_button(&new, act(|s: &mut Shortcuts| s.new_count += 1))
    });
    ui.keyed("find", |ui| {
        ui.command_button(&find, act(|s: &mut Shortcuts| s.find_count += 1))
    });
    ui.keyed("input", |ui| {
        ui.text_input("Name", &state.value, |text| {
            act(move |s: &mut Shortcuts| s.value = text.clone())
        })
    });
}

#[test]
fn shortcuts_fire_enabled_commands_once_and_respect_text_editing() {
    let mut app = Driven::new(ROOT, Shortcuts::default(), shortcuts_view);
    assert!(app.context.event(Event::Shortcut(Shortcut::primary('s'))));
    app.frame();
    assert_eq!(app.state().save_count, 1);
    app.frame();
    assert_eq!(app.state().save_count, 1);
    assert!(!app.context.event(Event::Shortcut(Shortcut::primary('n'))));
    app.frame();
    assert_eq!(app.state().new_count, 0);

    let input = app.context.find(&["input"]).unwrap();
    assert!(app.context.action(input, Action::Focus));
    assert!(app.context.accepts_text());
    assert!(!app.context.event(Event::Shortcut(Shortcut::primary('s'))));
    app.frame();
    assert_eq!(app.state().save_count, 1);
    assert!(app.context.event(Event::Shortcut(Shortcut::primary('f'))));
    app.frame();
    assert_eq!(app.state().find_count, 1);
}

fn duplicate_view(_: &(), ui: &mut Ui<'_, Message<()>>) {
    let command = Command::new("Save", Some(Shortcut::primary('s')));
    ui.keyed("a", |ui| ui.command_button(&command, act(|_: &mut ()| {})));
    ui.keyed("b", |ui| ui.command_button(&command, act(|_: &mut ()| {})));
}

#[test]
#[should_panic(expected = "duplicate enabled command shortcut")]
fn duplicate_enabled_shortcuts_are_rejected_at_declaration() {
    let _ = Driven::new(ROOT, (), duplicate_view);
}

/// A bin that takes text, for a drag the host carries in and out of it.
fn bin_view(_: &Dropped, ui: &mut Ui<'_, Message<Dropped>>) {
    ui.keyed("bin", |ui| {
        ui.drop_target(
            boxed(),
            DropTargetBuilder::new("Text bin", DropKind::Text)
                .on_drop(|payload| act(move |s: &mut Dropped| s.dropped.push(payload.clone()))),
            |_| {},
        )
    });
}

/// Carrying it on to a third window, or back, takes it away again without
/// telling the window it started in anything.
#[test]
fn an_adopted_drag_can_be_taken_back_without_ending_it() {
    let text = hello();
    let mut app = Driven::new(ROOT, Dropped::default(), bin_view);
    let over = center(&app.context, &["bin"]);
    assert!(app.context.adopt_drag(text.clone(), over));
    // A second window cannot adopt what it already carries.
    assert!(!app.context.adopt_drag(text.clone(), over));
    assert_eq!(app.context.withdraw_drag(), Some(text.clone()));
    assert_eq!(app.context.withdraw_drag(), None);
    app.frame();
    assert!(!can_drop(&app.context, &["bin"]));
    // Nothing was dropped and nothing was exported, so no outcome is owed.
    app.context.event(Event::PointerUp);
    app.frame();
    assert!(app.state().dropped.is_empty());
}

/// The state of an app with a source in its main window and a bin in a second.
#[derive(Default)]
struct Cross {
    window: Option<WindowId>,
    dropped: Vec<DragPayload>,
    outcomes: Vec<DragOutcome>,
}

fn card() -> DragPayload {
    DragPayload::Item {
        domain: "card".into(),
        id: seaglass_core::ItemId(7),
    }
}

fn cross() -> (Driven<Cross>, WindowId) {
    let mut app = Driven::new(ROOT, Cross::default(), |_, ui| {
        ui.keyed("source", |ui| {
            ui.drag_source(
                boxed(),
                DragSourceBuilder::new("Card", card())
                    .on_drag_end(|outcome| act(move |s: &mut Cross| s.outcomes.push(outcome))),
                |ui| {
                    ui.label("Card");
                },
            )
        });
    });
    app.set_window_view(|_, _, ui| {
        ui.keyed("bin", |ui| {
            ui.drop_target(
                boxed(),
                DropTargetBuilder::new("Item bin", DropKind::Item)
                    .on_drop(|payload| act(move |s: &mut Cross| s.dropped.push(payload.clone()))),
                |_| {},
            )
        });
    });
    app.effects(|state, effects| {
        state.window = Some(effects.open_window(WindowRequest {
            title: "Bin".into(),
            size: Some(ROOT),
            modal_owner: None,
        }));
    });
    app.frame();
    let id = app.state().window.expect("the app opened a window");
    assert_eq!(app.window_ids(), [id]);
    (app, id)
}

#[test]
fn a_drag_crosses_to_another_window_and_the_source_hears_how_it_ended() {
    let (mut app, id) = cross();

    // The drag starts, then the pointer leaves the first window.
    let source = center(&app.context, &["source"]);
    app.context.event(Event::PointerMoved(source));
    app.context.event(Event::PointerDown);
    app.context
        .event(Event::PointerMoved(Point { x: 600.0, y: 20.0 }));
    let carried = app.context.export_drag().expect("the host takes it over");
    assert_eq!(carried, card());
    app.frame();
    assert!(!dragging(&app.context, &["source"]));

    // The host finds the second window under the pointer and hands it over.
    let over = center(&app.window(id).context, &["bin"]);
    assert!(app.window(id).context.adopt_drag(carried.clone(), over));
    app.frame();
    assert!(
        can_drop(&app.window(id).context, &["bin"]),
        "the target lights up"
    );
    assert!(
        app.window(id).context.accepts_drop(over, &carried),
        "and would take it"
    );
    app.window(id).context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().dropped, [card()]);

    // The window it came from is told, once.
    app.context.finish_exported_drag(DragOutcome::Dropped);
    app.frame();
    assert_eq!(app.state().outcomes, [DragOutcome::Dropped]);
    app.frame();
    assert_eq!(app.state().outcomes.len(), 1);
}
