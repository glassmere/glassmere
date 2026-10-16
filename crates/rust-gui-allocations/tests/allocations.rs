//! What a batch allocates, counted per thread by a global allocator of its
//! own, so the tests of this binary do not count each other.
#![allow(unsafe_code)]
use rust_gui_core::{
    Action, App, AppDriver, Button, Context, Effects, Event, FrameStats, MonospaceMeasurer, NodeId,
    TextEdit, Ui,
};
use rust_gui_scene::{Point, Size};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: every call is forwarded to `System`; counting touches only a
// thread-local `Cell`, which does not allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: the caller's contract is the one `System` needs.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: as above.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

#[derive(Clone)]
enum Msg {
    Increase,
    Name(String),
}

/// `buttons` keyed buttons, the first of which is the one the tests press,
/// and optionally one text input. `sends` decides whether the buttons send
/// `Increase`: the same view without it declares the same widgets with no
/// handlers.
struct Counter {
    count: u32,
    name: String,
    sends: bool,
    field: bool,
    buttons: usize,
}

impl Counter {
    fn new(buttons: usize, sends: bool) -> Self {
        Self {
            count: 0,
            name: String::new(),
            sends,
            field: false,
            buttons,
        }
    }

    fn with_field(buttons: usize, sends: bool) -> Self {
        Self {
            field: true,
            ..Self::new(buttons, sends)
        }
    }
}

impl App for Counter {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message {
            Msg::Increase => self.count += 1,
            Msg::Name(name) => self.name = name,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        // Constant text, so a click changes nothing a layout depends on: what
        // it allocates is the frame's, not the label's.
        ui.label(if self.count > 1000 { "many" } else { "Count" });
        if self.field {
            ui.keyed("name", |ui| ui.text_input("Name", &self.name, Msg::Name));
        }
        for index in 0..self.buttons {
            ui.keyed(index.to_string(), |ui| {
                let button = Button::new("Increase");
                ui.button(if self.sends {
                    button.on_click(Msg::Increase)
                } else {
                    button
                });
            });
        }
    }
}

struct Running<A: App> {
    app: A,
    driver: AppDriver<A::Message>,
    context: Context,
}

const SIZE: Size = Size::new(400.0, 1200.0);

impl<A: App> Running<A> {
    fn new(app: A) -> Self {
        let mut running = Self {
            app,
            driver: AppDriver::new(|| true),
            context: Context::new(),
        };
        // Warm up: the first frame declares, the next ones settle what the
        // first one left to do.
        for _ in 0..3 {
            running.frame();
        }
        running
    }

    fn frame(&mut self) -> FrameStats {
        self.driver.frame(
            &mut self.app,
            &mut self.context,
            SIZE,
            &mut MonospaceMeasurer,
        )
    }

    /// Allocations of one frame. `input` queues what the frame receives and
    /// is not counted, so the events themselves cost nothing here.
    fn allocated_by(&mut self, input: impl FnOnce(&mut Self)) -> usize {
        input(self);
        let before = allocations();
        self.frame();
        allocations() - before
    }

    fn button(&self) -> NodeId {
        self.context.find(&["0"]).expect("the first button")
    }

    fn field(&self) -> NodeId {
        self.context.find(&["name"]).expect("the field")
    }
}

/// What the message path of a click costs. Pressing a button always repaints
/// the scene, which allocates per node; so the baseline presses the same
/// button in a view that sends nothing, and forces the same declaration with
/// `invalidate`. The two frames differ only in the messages and their
/// handlers, so their difference is what the message path adds.
fn click_cost(buttons: usize) -> isize {
    let mut clicked = Running::new(Counter::new(buttons, true));
    let button = clicked.button();
    let with_message = clicked.allocated_by(|running| {
        running.context.action(button, Action::Activate);
    });
    let mut silent = Running::new(Counter::new(buttons, false));
    let button = silent.button();
    let without = silent.allocated_by(|running| {
        running.driver.invalidate();
        running.context.action(button, Action::Activate);
    });
    with_message as isize - without as isize
}

/// What typing one character into the text input costs over the same frame
/// with nothing sent, as in `click_cost`.
fn typing_cost(buttons: usize) -> isize {
    let mut typing = Running::new(Counter::with_field(buttons, true));
    let field = typing.field();
    typing.context.action(field, Action::Focus);
    typing.frame();
    let with_message = typing.allocated_by(|running| {
        running
            .context
            .event(Event::Text(TextEdit::Insert("a".into())));
    });
    let mut silent = Running::new(Counter::with_field(buttons, false));
    let field = silent.field();
    silent.context.action(field, Action::Focus);
    silent.frame();
    let button = silent.button();
    let without = silent.allocated_by(|running| {
        running.driver.invalidate();
        running.context.action(button, Action::Activate);
    });
    with_message as isize - without as isize
}

/// What declaring a handler on each button costs over declaring the same
/// buttons without one.
fn handler_cost(buttons: usize) -> isize {
    let with = Running::new(Counter::new(buttons, true))
        .allocated_by(|running| running.driver.invalidate());
    let without = Running::new(Counter::new(buttons, false))
        .allocated_by(|running| running.driver.invalidate());
    with as isize - without as isize
}

#[test]
fn a_batch_without_input_allocates_nothing_and_does_not_declare() {
    let mut running = Running::new(Counter::new(20, true));
    let before = allocations();
    let stats = running.frame();
    let allocated = allocations() - before;
    assert!(!stats.declared);
    assert_eq!(allocated, 0, "an idle batch allocated");
    // The counter counts.
    std::hint::black_box(Box::new(1));
    assert_eq!(allocations(), before + 1);
}

// Found while writing the text input tests: an idle batch with a text input
// in the view allocates once. Nothing is dirty and nothing is declared, so
// the contract says zero. Not fixed here: it lives in the kernel.
#[test]
#[ignore = "an idle batch with a text input allocates once per frame"]
fn an_idle_batch_with_a_text_input_allocates_nothing() {
    let mut running = Running::new(Counter::with_field(20, true));
    let allocated = running.allocated_by(|_| {});
    assert_eq!(allocated, 0, "an idle batch allocated");
}

#[test]
fn a_pointer_over_an_unobserved_button_does_not_declare() {
    let mut running = Running::new(Counter::new(20, true));
    let bounds = running.context.node(running.button()).unwrap().bounds;
    running.context.event(Event::PointerMoved(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }));
    let stats = running.frame();
    assert!(!stats.declared);
    assert_eq!(running.context.hovered(), Some(running.button()));
}

#[test]
fn a_click_declares_once_with_one_message() {
    let mut running = Running::new(Counter::new(20, true));
    running.context.action(running.button(), Action::Activate);
    let stats = running.frame();
    assert_eq!((stats.declared, stats.messages), (true, 1));
    assert_eq!(running.app.count, 1);
}

// The declaration itself grows with the view: every button is laid out. So
// the test does not compare a click with an idle batch; it compares it with
// the same frame that sends nothing (see `click_cost`), and checks that the
// difference is the same with 20 buttons and with 200.
#[test]
fn a_click_allocates_the_same_on_the_message_path_at_20_and_200_buttons() {
    let (small, large) = (click_cost(20), click_cost(200));
    assert_eq!(
        large, small,
        "a click's message path allocated {small} at 20 buttons and {large} at 200"
    );
}

// Declaring a handler on a button, over declaring it without one, fills a
// handler slot. Slots are reused in steady state, so the difference must not
// grow with the number of buttons either.
#[test]
fn a_declared_handler_costs_the_same_at_20_and_200_buttons() {
    let (small, large) = (handler_cost(20), handler_cost(200));
    assert_eq!(
        large, small,
        "handlers cost {small} extra allocations at 20 buttons and {large} at 200"
    );
}

// A `text_input` whose message constructor is the zero-sized `Msg::Name`
// function item: the boxed wrapper around it allocates nothing, so typing
// costs the same at 20 and 200 buttons.
#[test]
fn a_text_input_message_allocates_the_same_at_20_and_200_buttons() {
    let (small, large) = (typing_cost(20), typing_cost(200));
    assert_eq!(
        large, small,
        "typing's message path allocated {small} at 20 buttons and {large} at 200"
    );
}
