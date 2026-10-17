use seaglass_core::{
    App, AppDriver, Context, DialogOptions, Effects, Event, Live, MonospaceMeasurer, NodeId, Role,
    Toast, ToastAction, ToastKind, Ui,
};
use seaglass_scene::{Point, Size};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const ROOT: Size = Size::new(800.0, 600.0);

#[derive(Clone)]
enum Msg {
    /// Raise a toast; an action on it comes back as `Acted`.
    Raise(Toast),
    Acted(ToastAction),
    Dialog(bool),
}

#[derive(Default)]
struct Toasts {
    actions: Vec<ToastAction>,
    dialog: bool,
}

impl App for Toasts {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Raise(toast) if toast.action.is_some() => {
                effects.toast_with(toast, Msg::Acted);
            }
            Msg::Raise(toast) => effects.toast(toast),
            Msg::Acted(action) => self.actions.push(action),
            Msg::Dialog(open) => self.dialog = open,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.button("page");
        if self.dialog {
            ui.dialog(
                "modal",
                true,
                DialogOptions::new("Busy"),
                Msg::Dialog,
                |ui| {
                    ui.button("inside");
                },
            );
        }
    }
}

/// The app, its driver and the context, run together as a window would.
struct Harness {
    context: Context,
    driver: AppDriver<Msg>,
    app: Toasts,
}

impl Harness {
    fn new(app: Toasts) -> Self {
        Self {
            context: Context::new(),
            driver: AppDriver::new(|| true),
            app,
        }
    }

    fn send(&mut self, message: Msg) {
        self.driver.send(message);
    }

    fn frame(&mut self) {
        self.driver.frame(
            &mut self.app,
            &mut self.context,
            ROOT,
            &mut MonospaceMeasurer,
        );
    }

    fn at(&mut self, milliseconds: u64) {
        self.context.set_now(Duration::from_millis(milliseconds));
        self.frame();
    }
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
    let mut h = Harness::new(Toasts::default());
    h.send(Msg::Raise(Toast::success("Saved")));
    h.send(Msg::Raise(Toast::error("Upload failed")));
    h.at(0);
    // Raised during a declaration, shown by the next one, which the context asks for.
    assert!(h.context.animating());
    h.at(10);
    assert_eq!(texts(&h.context), ["Saved", "Upload failed"]);
    // Effects stamp a toast when it is raised (t = 0), not at the first declaration.
    assert_eq!(h.context.next_wake(), Some(Duration::from_millis(5000)));

    let stack = h
        .context
        .node(h.context.find(&["$toasts"]).unwrap())
        .unwrap();
    assert!(stack.bounds.x + stack.bounds.width <= ROOT.width - 15.0);
    assert!(stack.bounds.y + stack.bounds.height <= ROOT.height - 15.0);
    assert!(stack.bounds.x > ROOT.width / 2.0 && stack.bounds.y > ROOT.height / 2.0);
    let saved = h.context.node(stack.children[0]).unwrap();
    assert_eq!(saved.semantics.role, Role::Status);
    assert_eq!(saved.semantics.label, "Saved");
    assert_eq!(saved.live, Some(Live::Polite));
    let failed = h.context.node(stack.children[1]).unwrap();
    assert_eq!(failed.semantics.role, Role::Alert);
    assert_eq!(failed.live, Some(Live::Assertive));

    // Five seconds for the first, eight for the error, both from the raise.
    h.at(4999);
    assert_eq!(texts(&h.context).len(), 2);
    h.at(5000);
    assert_eq!(texts(&h.context), ["Upload failed"]);
    h.at(8010);
    assert!(texts(&h.context).is_empty());
    assert!(h.context.find(&["$toasts"]).is_none());
    assert_eq!(h.context.next_wake(), None);
}

#[test]
fn the_pointer_holds_toasts_and_gives_them_a_moment_more_afterwards() {
    let mut h = Harness::new(Toasts::default());
    h.context
        .push_toast(Toast::new("Reading this").duration(Duration::from_secs(2)));
    h.at(0);
    let stack = h
        .context
        .node(h.context.find(&["$toasts"]).unwrap())
        .unwrap();
    let panel = h.context.node(stack.children[0]).unwrap().bounds;
    h.context.event(Event::PointerMoved(Point {
        x: panel.x + 40.0,
        y: panel.y + panel.height / 2.0,
    }));
    h.at(1000);
    assert_eq!(h.context.next_wake(), None, "held: nothing to wake for");
    h.at(60_000);
    assert_eq!(texts(&h.context), ["Reading this"]);

    h.context
        .event(Event::PointerMoved(Point { x: 5.0, y: 5.0 }));
    h.at(61_000);
    assert_eq!(texts(&h.context), ["Reading this"], "a moment more");
    assert_eq!(h.context.next_wake(), Some(Duration::from_millis(62_500)));
    h.at(62_500);
    assert!(texts(&h.context).is_empty());
}

#[test]
fn actions_are_reported_once_with_their_tag_and_close_dismisses_silently() {
    let mut h = Harness::new(Toasts::default());
    h.send(Msg::Raise(
        Toast::new("Message deleted")
            .action("Undo")
            .tag("message 42"),
    ));
    h.send(Msg::Raise(Toast::new("Draft kept").sticky()));
    h.at(0);
    h.at(10);
    let deleted = h.context.toasts().next().unwrap();
    assert_eq!(deleted.1.duration, Some(Duration::from_secs(8)));
    let id = deleted.0;
    let key = h
        .context
        .node(h.context.find(&["$toasts"]).unwrap())
        .unwrap()
        .children[0];
    let action = h.context.node(key).unwrap().children[2];
    assert_eq!(h.context.node(action).unwrap().semantics.label, "Undo");
    click(&mut h.context, action);
    // The click is consumed by one declaration and handed over by the next.
    h.at(20);
    h.at(30);
    assert_eq!(h.app.actions.len(), 1);
    assert_eq!(
        (h.app.actions[0].id, h.app.actions[0].tag.as_str()),
        (id, "message 42")
    );
    assert_eq!(texts(&h.context), ["Draft kept"]);
    h.at(40);
    assert_eq!(h.app.actions.len(), 1, "once");

    // Sticky toasts stay until closed; the close button has a name.
    h.at(3_600_000);
    assert_eq!(texts(&h.context), ["Draft kept"]);
    let panel = h
        .context
        .node(h.context.find(&["$toasts"]).unwrap())
        .unwrap()
        .children[0];
    let close = *h.context.node(panel).unwrap().children.last().unwrap();
    assert_eq!(
        h.context.node(close).unwrap().description.as_deref(),
        Some("Dismiss")
    );
    click(&mut h.context, close);
    h.at(3_600_010);
    assert!(texts(&h.context).is_empty() && h.app.actions.len() == 1);
}

#[test]
fn equal_toasts_restart_instead_of_stacking_and_only_four_show_at_once() {
    let mut h = Harness::new(Toasts::default());
    h.context.push_toast(Toast::new("Saved"));
    h.at(0);
    h.at(4000);
    let again = h.context.push_toast(Toast::new("Saved"));
    h.at(4001);
    assert_eq!(texts(&h.context), ["Saved"]);
    assert_eq!(
        h.context.toasts().next().unwrap().0,
        again,
        "the newest id stays valid"
    );
    h.at(8000);
    assert_eq!(texts(&h.context), ["Saved"], "restarted at 4001");
    h.at(9001);
    assert!(texts(&h.context).is_empty());

    for index in 0..6 {
        h.context
            .push_toast(Toast::new(format!("Job {index}")).duration(Duration::from_secs(1)));
    }
    h.at(10_000);
    assert_eq!(texts(&h.context), ["Job 0", "Job 1", "Job 2", "Job 3"]);
    // The waiting ones get their full time once they show.
    h.at(11_000);
    assert_eq!(texts(&h.context), ["Job 4", "Job 5"]);
    h.at(11_999);
    assert_eq!(texts(&h.context).len(), 2);
    h.at(12_000);
    assert!(texts(&h.context).is_empty());
}

#[test]
fn other_threads_raise_toasts_through_a_sender_that_wakes_the_window() {
    let mut h = Harness::new(Toasts::default());
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = wakes.clone();
    h.context.set_toast_wake(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let mut sender = None;
    h.context.run(ROOT, &mut MonospaceMeasurer, |ui| {
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
    h.at(0);
    assert_eq!(texts(&h.context), ["Disk almost full", "Sync finished"]);
    assert_eq!(
        h.context.toasts().next().unwrap().1.kind,
        ToastKind::Warning
    );
}

#[test]
fn a_toast_shows_above_a_modal_dialog_and_its_buttons_take_clicks() {
    let mut h = Harness::new(Toasts {
        dialog: true,
        ..Toasts::default()
    });
    h.context.push_toast(Toast::new("Still here").sticky());
    h.at(0);
    h.at(10);
    let root = h.context.node(h.context.root()).unwrap();
    let stack = h.context.find(&["$toasts"]).unwrap();
    assert_eq!(
        root.children.last(),
        Some(&stack),
        "the last layer paints on top"
    );
    let panel = h.context.node(stack).unwrap().children[0];
    let close = *h.context.node(panel).unwrap().children.last().unwrap();
    let bounds = h.context.node(close).unwrap().bounds;
    let centre = Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    };
    assert_eq!(h.context.hit_test(centre), Some(close));
    click(&mut h.context, close);
    h.at(20);
    assert!(texts(&h.context).is_empty());
}
