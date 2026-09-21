use rust_gui_core::{
    Context, DialogOptions, Event, Live, MonospaceMeasurer, NodeId, Role, Toast, ToastAction,
    ToastKind,
};
use rust_gui_scene::{Point, Size};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const ROOT: Size = Size::new(800.0, 600.0);

#[derive(Default)]
struct App {
    raise: Vec<Toast>,
    actions: Vec<ToastAction>,
    dialog: bool,
}

fn frame(context: &mut Context, app: &mut App) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        for toast in app.raise.drain(..) {
            ui.toast(toast);
        }
        while let Some(action) = ui.toast_action() {
            app.actions.push(action);
        }
        ui.button("page");
        if app.dialog {
            let mut open = true;
            ui.dialog("modal", &mut open, DialogOptions::new("Busy"), |ui, _| {
                ui.button("inside");
            });
        }
    });
}

fn at(context: &mut Context, app: &mut App, milliseconds: u64) {
    context.set_now(Duration::from_millis(milliseconds));
    frame(context, app);
}

fn texts(context: &Context) -> Vec<String> {
    context
        .toasts()
        .map(|(_, toast)| toast.text.clone())
        .collect()
}

fn click(context: &mut Context, id: NodeId) {
    let bounds = context.node(id).unwrap().bounds;
    context.event(Event::PointerMoved(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

#[test]
fn toasts_stack_in_the_corner_announce_themselves_and_leave_on_time() {
    let mut context = Context::new();
    let mut app = App {
        raise: vec![Toast::success("Saved"), Toast::error("Upload failed")],
        ..App::default()
    };
    at(&mut context, &mut app, 0);
    // Raised during a declaration, shown by the next one, which the context asks for.
    assert!(context.animating());
    at(&mut context, &mut app, 10);
    assert_eq!(texts(&context), ["Saved", "Upload failed"]);
    assert_eq!(context.next_wake(), Some(Duration::from_millis(5010)));

    let stack = context.node(context.find(&["$toasts"]).unwrap()).unwrap();
    assert!(stack.bounds.x + stack.bounds.width <= ROOT.width - 15.0);
    assert!(stack.bounds.y + stack.bounds.height <= ROOT.height - 15.0);
    assert!(stack.bounds.x > ROOT.width / 2.0 && stack.bounds.y > ROOT.height / 2.0);
    let saved = context.node(stack.children[0]).unwrap();
    assert_eq!(saved.semantics.role, Role::Status);
    assert_eq!(saved.semantics.label, "Saved");
    assert_eq!(saved.live, Some(Live::Polite));
    let failed = context.node(stack.children[1]).unwrap();
    assert_eq!(failed.semantics.role, Role::Alert);
    assert_eq!(failed.live, Some(Live::Assertive));

    // Five seconds for the first, eight for the error.
    at(&mut context, &mut app, 5009);
    assert_eq!(texts(&context).len(), 2);
    at(&mut context, &mut app, 5010);
    assert_eq!(texts(&context), ["Upload failed"]);
    at(&mut context, &mut app, 8010);
    assert!(texts(&context).is_empty());
    assert!(context.find(&["$toasts"]).is_none());
    assert_eq!(context.next_wake(), None);
}

#[test]
fn the_pointer_holds_toasts_and_gives_them_a_moment_more_afterwards() {
    let mut context = Context::new();
    let mut app = App::default();
    context.push_toast(Toast::new("Reading this").duration(Duration::from_secs(2)));
    at(&mut context, &mut app, 0);
    let stack = context.node(context.find(&["$toasts"]).unwrap()).unwrap();
    let panel = context.node(stack.children[0]).unwrap().bounds;
    context.event(Event::PointerMoved(Point {
        x: panel.x + 40.0,
        y: panel.y + panel.height / 2.0,
    }));
    at(&mut context, &mut app, 1000);
    assert_eq!(context.next_wake(), None, "held: nothing to wake for");
    at(&mut context, &mut app, 60_000);
    assert_eq!(texts(&context), ["Reading this"]);

    context.event(Event::PointerMoved(Point { x: 5.0, y: 5.0 }));
    at(&mut context, &mut app, 61_000);
    assert_eq!(texts(&context), ["Reading this"], "a moment more");
    assert_eq!(context.next_wake(), Some(Duration::from_millis(62_500)));
    at(&mut context, &mut app, 62_500);
    assert!(texts(&context).is_empty());
}

#[test]
fn actions_are_reported_once_with_their_tag_and_close_dismisses_silently() {
    let mut context = Context::new();
    let mut app = App {
        raise: vec![
            Toast::new("Message deleted")
                .action("Undo")
                .tag("message 42"),
            Toast::new("Draft kept").sticky(),
        ],
        ..App::default()
    };
    at(&mut context, &mut app, 0);
    at(&mut context, &mut app, 10);
    let deleted = context.toasts().next().unwrap();
    assert_eq!(deleted.1.duration, Some(Duration::from_secs(8)));
    let id = deleted.0;
    let key = context
        .node(context.find(&["$toasts"]).unwrap())
        .unwrap()
        .children[0];
    let action = context.node(key).unwrap().children[2];
    assert_eq!(context.node(action).unwrap().semantics.label, "Undo");
    click(&mut context, action);
    // The click is consumed by one declaration and handed over by the next.
    at(&mut context, &mut app, 20);
    at(&mut context, &mut app, 30);
    assert_eq!(app.actions.len(), 1);
    assert_eq!(
        (app.actions[0].id, app.actions[0].tag.as_str()),
        (id, "message 42")
    );
    assert_eq!(texts(&context), ["Draft kept"]);
    at(&mut context, &mut app, 40);
    assert_eq!(app.actions.len(), 1, "once");

    // Sticky toasts stay until closed; the close button has a name.
    at(&mut context, &mut app, 3_600_000);
    assert_eq!(texts(&context), ["Draft kept"]);
    let panel = context
        .node(context.find(&["$toasts"]).unwrap())
        .unwrap()
        .children[0];
    let close = *context.node(panel).unwrap().children.last().unwrap();
    assert_eq!(
        context.node(close).unwrap().description.as_deref(),
        Some("Dismiss")
    );
    click(&mut context, close);
    at(&mut context, &mut app, 3_600_010);
    assert!(texts(&context).is_empty() && app.actions.len() == 1);
}

#[test]
fn equal_toasts_restart_instead_of_stacking_and_only_four_show_at_once() {
    let mut context = Context::new();
    let mut app = App::default();
    context.push_toast(Toast::new("Saved"));
    at(&mut context, &mut app, 0);
    at(&mut context, &mut app, 4000);
    let again = context.push_toast(Toast::new("Saved"));
    at(&mut context, &mut app, 4001);
    assert_eq!(texts(&context), ["Saved"]);
    assert_eq!(
        context.toasts().next().unwrap().0,
        again,
        "the newest id stays valid"
    );
    at(&mut context, &mut app, 8000);
    assert_eq!(texts(&context), ["Saved"], "restarted at 4001");
    at(&mut context, &mut app, 9001);
    assert!(texts(&context).is_empty());

    for index in 0..6 {
        context.push_toast(Toast::new(format!("Job {index}")).duration(Duration::from_secs(1)));
    }
    at(&mut context, &mut app, 10_000);
    assert_eq!(texts(&context), ["Job 0", "Job 1", "Job 2", "Job 3"]);
    // The waiting ones get their full time once they show.
    at(&mut context, &mut app, 11_000);
    assert_eq!(texts(&context), ["Job 4", "Job 5"]);
    at(&mut context, &mut app, 11_999);
    assert_eq!(texts(&context).len(), 2);
    at(&mut context, &mut app, 12_000);
    assert!(texts(&context).is_empty());
}

#[test]
fn other_threads_raise_toasts_through_a_sender_that_wakes_the_window() {
    let mut context = Context::new();
    let mut app = App::default();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    context.set_toast_wake(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let mut sender = None;
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        sender = Some(ui.toast_sender());
    });
    let sender = sender.unwrap();
    let worker = sender.clone();
    std::thread::spawn(move || {
        worker.send(Toast::warning("Disk almost full").tag("disk"));
        worker.send(Toast::new("Sync finished"));
    })
    .join()
    .unwrap();
    assert_eq!(wakes.load(Ordering::SeqCst), 2);
    // Visible in the very next declaration, no extra frame needed.
    at(&mut context, &mut app, 0);
    assert_eq!(texts(&context), ["Disk almost full", "Sync finished"]);
    assert_eq!(context.toasts().next().unwrap().1.kind, ToastKind::Warning);
}

#[test]
fn a_toast_shows_above_a_modal_dialog_and_its_buttons_take_clicks() {
    let mut context = Context::new();
    let mut app = App {
        dialog: true,
        ..App::default()
    };
    context.push_toast(Toast::new("Still here").sticky());
    at(&mut context, &mut app, 0);
    at(&mut context, &mut app, 10);
    let root = context.node(context.root()).unwrap();
    let stack = context.find(&["$toasts"]).unwrap();
    assert_eq!(
        root.children.last(),
        Some(&stack),
        "the last layer paints on top"
    );
    let panel = context.node(stack).unwrap().children[0];
    let close = *context.node(panel).unwrap().children.last().unwrap();
    let bounds = context.node(close).unwrap().bounds;
    let centre = Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    };
    assert_eq!(context.hit_test(centre), Some(close));
    click(&mut context, close);
    at(&mut context, &mut app, 20);
    assert!(texts(&context).is_empty());
}
