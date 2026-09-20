use rust_gui_core::{
    Action, Command, Context, DragPayload, DropKind, Event, Key, Length, MonospaceMeasurer,
    Shortcut, Style, Ui,
};
use rust_gui_scene::{Point, Size};

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, view);
}
fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
}
fn center(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}
fn boxed() -> Style {
    Style {
        width: Length::Px(200.0),
        height: Length::Px(60.0),
        ..Style::default()
    }
}
#[derive(Default)]
struct Drops {
    text: Option<DragPayload>,
    item: Option<DragPayload>,
}
fn board(context: &mut Context, target_enabled: bool) -> Drops {
    let mut drops = Drops::default();
    frame(context, |ui| {
        ui.drag_source(
            "source",
            "Card",
            DragPayload::Text("hello".into()),
            boxed(),
            |ui| ui.label("Card"),
        );
        ui.enabled(target_enabled, |ui| {
            drops.text = ui.drop_target("text", "Text bin", DropKind::Text, boxed(), |_| {});
        });
        drops.item = ui.drop_target("item", "Item bin", DropKind::Item, boxed(), |_| {});
    });
    drops
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

#[test]
fn pointer_drag_needs_a_threshold_matches_payload_kind_and_delivers_once() {
    let mut context = Context::new();
    board(&mut context, true);
    let start = center(&context, &["source"]);
    context.event(Event::PointerMoved(start));
    context.event(Event::PointerDown);
    context.event(Event::PointerMoved(Point {
        x: start.x + 3.0,
        y: start.y + 3.0,
    }));
    assert!(!dragging(&context, &["source"]));
    assert!(!can_drop(&context, &["text"]));
    context.event(Event::PointerMoved(center(&context, &["text"])));
    assert!(dragging(&context, &["source"]));
    assert!(can_drop(&context, &["text"]));
    assert!(!can_drop(&context, &["item"]));
    assert_eq!(context.captured(), None);
    assert!(board(&mut context, true).text.is_none());
    context.event(Event::PointerUp);
    assert_eq!(
        board(&mut context, true).text,
        Some(DragPayload::Text("hello".into()))
    );
    assert!(board(&mut context, true).text.is_none());
    assert!(!dragging(&context, &["source"]));

    drag_to(&mut context, "item");
    context.event(Event::PointerUp);
    let drops = board(&mut context, true);
    assert!(drops.item.is_none() && drops.text.is_none());

    board(&mut context, false);
    drag_to(&mut context, "text");
    assert!(!can_drop(&context, &["text"]));
    context.event(Event::PointerUp);
    assert!(board(&mut context, false).text.is_none());
}

#[test]
fn escape_and_focus_loss_cancel_an_active_drag() {
    let mut context = Context::new();
    board(&mut context, true);
    drag_to(&mut context, "text");
    assert!(dragging(&context, &["source"]));
    assert!(context.event(Event::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    }));
    assert!(!dragging(&context, &["source"]));
    assert!(!can_drop(&context, &["text"]));
    context.event(Event::PointerUp);
    assert!(board(&mut context, true).text.is_none());

    drag_to(&mut context, "text");
    assert!(dragging(&context, &["source"]));
    context.event(Event::FocusLost);
    assert!(!dragging(&context, &["source"]));
    context.event(Event::PointerUp);
    assert!(board(&mut context, true).text.is_none());
}

#[test]
fn keyboard_and_accessibility_drags_share_the_pointer_delivery_path() {
    let mut context = Context::new();
    board(&mut context, true);
    let source = context.find(&["source"]).unwrap();
    let text = context.find(&["text"]).unwrap();
    assert!(context.action(source, Action::Focus));
    assert!(context.event(Event::KeyDown {
        key: Key::Space,
        shift: false,
        repeat: false,
    }));
    assert!(dragging(&context, &["source"]));
    assert!(can_drop(&context, &["text"]));
    assert!(context.action(text, Action::Focus));
    key(&mut context, Key::Enter);
    assert_eq!(
        board(&mut context, true).text,
        Some(DragPayload::Text("hello".into()))
    );
    assert!(board(&mut context, true).text.is_none());
    assert!(!dragging(&context, &["source"]));

    assert!(!context.action(text, Action::Drop));
    assert!(context.action(source, Action::StartDrag));
    assert!(context.action(text, Action::Drop));
    assert_eq!(
        board(&mut context, true).text,
        Some(DragPayload::Text("hello".into()))
    );
    assert!(!dragging(&context, &["source"]));
}

#[test]
fn external_file_drops_merge_per_target_and_miss_empty_space() {
    let mut context = Context::new();
    let files = |context: &mut Context| {
        let mut dropped = None;
        frame(context, |ui| {
            dropped = ui.drop_target("files", "Files", DropKind::Files, boxed(), |_| {});
        });
        dropped
    };
    files(&mut context);
    let position = center(&context, &["files"]);
    for name in ["a.txt", "b.txt"] {
        assert!(context.event(Event::DropAt {
            position,
            payload: DragPayload::Files(vec![name.into()]),
        }));
    }
    assert_eq!(
        files(&mut context),
        Some(DragPayload::Files(vec!["a.txt".into(), "b.txt".into()]))
    );
    assert!(files(&mut context).is_none());
    assert!(!context.event(Event::DropAt {
        position: Point { x: 450.0, y: 450.0 },
        payload: DragPayload::Files(vec!["c.txt".into()]),
    }));
    assert!(!context.event(Event::DropAt {
        position,
        payload: DragPayload::Text("not a file".into()),
    }));
    assert!(files(&mut context).is_none());
}

#[test]
fn shortcuts_fire_enabled_commands_once_and_respect_text_editing() {
    let mut context = Context::new();
    let save = Command::new("Save", Some(Shortcut::primary('s')));
    let mut new = Command::new("New", Some(Shortcut::primary('n')));
    new.enabled = false;
    let mut find = Command::new("Find", Some(Shortcut::primary('f')));
    find.while_editing = true;
    let mut value = String::from("draft");
    #[derive(Default)]
    struct Clicked {
        save: bool,
        new: bool,
        find: bool,
    }
    let mut declare = |context: &mut Context| {
        let mut clicked = Clicked::default();
        frame(context, |ui| {
            clicked.save = ui.command_button("save", &save).clicked();
            clicked.new = ui.command_button("new", &new).clicked();
            clicked.find = ui.command_button("find", &find).clicked();
            ui.text_input("input", "Name", &mut value);
        });
        clicked
    };
    declare(&mut context);
    assert!(context.event(Event::Shortcut(Shortcut::primary('s'))));
    let clicked = declare(&mut context);
    assert!(clicked.save && !clicked.new && !clicked.find);
    assert!(!declare(&mut context).save);
    assert!(!context.event(Event::Shortcut(Shortcut::primary('n'))));
    assert!(!declare(&mut context).new);

    let input = context.find(&["input"]).unwrap();
    assert!(context.action(input, Action::Focus));
    assert!(context.accepts_text());
    assert!(!context.event(Event::Shortcut(Shortcut::primary('s'))));
    assert!(!declare(&mut context).save);
    assert!(context.event(Event::Shortcut(Shortcut::primary('f'))));
    assert!(declare(&mut context).find);
}

#[test]
#[should_panic(expected = "duplicate enabled command shortcut")]
fn duplicate_enabled_shortcuts_are_rejected_at_declaration() {
    let mut context = Context::new();
    let command = Command::new("Save", Some(Shortcut::primary('s')));
    frame(&mut context, |ui| {
        ui.command_button("a", &command);
        ui.command_button("b", &command);
    });
}
