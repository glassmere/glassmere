//! A label bound to a signal or memo follows it, and a view that says what
//! it reads is not run again for a message that changes nothing it reads.
use glassmere_core::{
    Action, App, AppDriver, Button, Context, Effects, FrameStats, Memo, MonospaceMeasurer, Signal,
    Store, Ui,
};
use glassmere_scene::Size;
use std::cell::Cell;
use std::rc::Rc;

const SIZE: Size = Size::new(400.0, 300.0);

#[derive(Clone)]
enum Msg {
    Add,
    Rename,
}

/// How an app answers `view_state`.
#[derive(Clone, Copy, PartialEq)]
enum Declares {
    /// Not at all: every message runs the view.
    Never,
    /// A constant: the view reads nothing that a message changes.
    Constant,
    /// The number of items listed, which `Msg::Add` changes.
    Items,
}

struct Counter {
    count: Signal<u32>,
    caption: Memo<String>,
    name: Signal<String>,
    items: u32,
    declares: Declares,
    views: Rc<Cell<u32>>,
}

impl Counter {
    fn new(declares: Declares) -> Self {
        let store = Store::new();
        let count = store.signal("count", 0u32);
        let read = count.clone();
        let caption = store.memo("caption", move || format!("Count {}", read.get()));
        Self {
            count,
            caption,
            name: store.signal("name", "Ada".to_string()),
            items: 0,
            declares,
            views: Rc::new(Cell::new(0)),
        }
    }
}

impl App for Counter {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message {
            Msg::Add => {
                self.count.update(|count| *count += 1);
                self.items += 1;
            }
            Msg::Rename => self.name.set("Grace Hopper".to_string()),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        ui.keyed("caption", |ui| ui.bound_label(&self.caption));
        ui.keyed("name", |ui| ui.bound_label(&self.name));
        ui.keyed("add", |ui| ui.button(Button::new("Add").on_click(Msg::Add)));
        ui.keyed("rename", |ui| {
            ui.button(Button::new("Rename").on_click(Msg::Rename))
        });
    }

    fn view_state(&self) -> Option<u64> {
        match self.declares {
            Declares::Never => None,
            Declares::Constant => Some(0),
            Declares::Items => Some(u64::from(self.items)),
        }
    }
}

struct Rig {
    app: Counter,
    driver: AppDriver<Msg>,
    context: Context,
}

impl Rig {
    fn new(declares: Declares) -> Self {
        let mut rig = Self {
            app: Counter::new(declares),
            driver: AppDriver::new(|| true),
            context: Context::new(),
        };
        rig.frame();
        rig
    }

    fn frame(&mut self) -> FrameStats {
        self.driver.frame(
            &mut self.app,
            &mut self.context,
            SIZE,
            &mut MonospaceMeasurer,
        )
    }

    fn press(&mut self, key: &str) -> FrameStats {
        let target = self.context.find(&[key]).unwrap();
        self.context.action(target, Action::Activate);
        self.frame()
    }

    fn text(&self, key: &str) -> String {
        let id = self.context.find(&[key]).unwrap();
        self.context.node(id).unwrap().semantics.label.clone()
    }

    fn width(&self, key: &str) -> f32 {
        let id = self.context.find(&[key]).unwrap();
        self.context.node(id).unwrap().bounds.width
    }
}

#[test]
fn a_bound_label_shows_the_source_when_the_view_declares_it() {
    let rig = Rig::new(Declares::Never);
    assert_eq!(rig.text("caption"), "Count 0");
    assert_eq!(rig.text("name"), "Ada");
}

#[test]
fn without_a_view_state_every_message_runs_the_view() {
    let mut rig = Rig::new(Declares::Never);
    let before = rig.app.views.get();
    let stats = rig.press("add");
    assert!(stats.declared);
    assert_eq!(rig.app.views.get(), before + 1);
    assert_eq!(rig.text("caption"), "Count 1");
}

#[test]
fn a_message_that_leaves_the_view_state_alone_skips_the_view_and_still_updates_the_label() {
    let mut rig = Rig::new(Declares::Constant);
    let before = rig.app.views.get();
    let narrow = rig.width("caption");
    let stats = rig.press("add");
    assert_eq!(stats.messages, 1);
    assert!(!stats.declared, "the view was not run");
    assert_eq!(rig.app.views.get(), before);
    assert_eq!(rig.text("caption"), "Count 1");
    // Nine more, so that the text grows by a character and layout follows.
    for _ in 0..9 {
        rig.press("add");
    }
    assert_eq!(rig.app.views.get(), before);
    assert_eq!(rig.text("caption"), "Count 10");
    assert!(
        rig.width("caption") > narrow,
        "the label was measured again"
    );
}

#[test]
fn a_source_the_message_did_not_touch_is_not_shown_again() {
    let mut rig = Rig::new(Declares::Constant);
    rig.press("add");
    let stats = rig.press("rename");
    assert!(!stats.declared);
    assert_eq!(rig.text("name"), "Grace Hopper");
    assert_eq!(rig.text("caption"), "Count 1");
}

#[test]
fn a_changed_view_state_runs_the_view() {
    let mut rig = Rig::new(Declares::Items);
    let before = rig.app.views.get();
    let stats = rig.press("add");
    assert!(stats.declared);
    assert_eq!(rig.app.views.get(), before + 1);
    rig.press("add");
    assert_eq!(rig.app.views.get(), before + 2);
    assert_eq!(rig.text("caption"), "Count 2");
    // A message that leaves the state as it was does not run the view.
    rig.press("rename");
    assert_eq!(rig.app.views.get(), before + 2, "rename leaves the items");
    assert_eq!(rig.text("name"), "Grace Hopper");
}

#[test]
fn a_frame_with_nothing_to_show_changes_nothing() {
    let mut rig = Rig::new(Declares::Constant);
    rig.press("add");
    let stats = rig.frame();
    assert!(!stats.declared);
    assert_eq!(rig.text("caption"), "Count 1");
}
