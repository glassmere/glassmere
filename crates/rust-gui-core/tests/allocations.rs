//! What a batch allocates, counted per thread by a global allocator of its
//! own, so the tests of this binary do not count each other.
#![allow(unsafe_code)]
use rust_gui_core::{
    Action, App, AppDriver, Button, Context, Effects, Event, FrameStats, MonospaceMeasurer, Ui,
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
}

struct Counter {
    count: u32,
}

impl App for Counter {
    type Message = Msg;

    fn update(&mut self, Msg::Increase: Msg, _: &mut Effects<'_, Msg>) {
        self.count += 1;
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.label(format!("Count {}", self.count));
        for index in 0..20 {
            ui.keyed(index.to_string(), |ui| {
                ui.button(Button::new("Increase").on_click(Msg::Increase));
            });
        }
    }
}

struct Running {
    app: Counter,
    driver: AppDriver<Msg>,
    context: Context,
}

const SIZE: Size = Size::new(400.0, 1200.0);

impl Running {
    fn new() -> Self {
        let mut running = Self {
            app: Counter { count: 0 },
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

    fn button(&self) -> rust_gui_core::NodeId {
        self.context.find(&["0"]).expect("the first button")
    }
}

#[test]
fn a_batch_without_input_allocates_nothing_and_does_not_declare() {
    let mut running = Running::new();
    let before = allocations();
    let stats = running.frame();
    let allocated = allocations() - before;
    assert!(!stats.declared);
    assert_eq!(allocated, 0, "an idle batch allocated");
    // The counter counts.
    std::hint::black_box(Box::new(1));
    assert_eq!(allocations(), before + 1);
}

#[test]
fn a_pointer_over_an_unobserved_button_does_not_declare() {
    let mut running = Running::new();
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
    let mut running = Running::new();
    running.context.action(running.button(), Action::Activate);
    let stats = running.frame();
    assert_eq!((stats.declared, stats.messages), (true, 1));
    assert_eq!(running.app.count, 1);
}
