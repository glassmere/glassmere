//! What a batch allocates, counted per thread by a global allocator of its
//! own, so the tests of this binary do not count each other.
#![allow(unsafe_code)]
use glassmere_core::app::{Action as Change, bind};
use glassmere_core::{
    Action, App, AppDriver, Block, BlockKind, Button, Context, Effects, Event, FrameStats, Label,
    Length, MonospaceMeasurer, NodeId, RichDocument, RichEditorOptions, RichOp, RichPosition, Role,
    Shadow, Style, TextEdit, Transition, Ui,
};
use glassmere_scene::{Color, Point, Size};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Duration;

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

/// Two buttons, keyed "0" and "1", and `boxes` check boxes under them. Every
/// check box draws a path, which is allocated again each time it is written
/// into the scene.
struct Ticks {
    boxes: usize,
}

impl App for Ticks {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        for key in ["0", "1"] {
            ui.keyed(key, |ui| {
                ui.button("Increase");
            });
        }
        for index in 0..self.boxes {
            ui.keyed(format!("box{index}"), |ui| {
                ui.checkbox("Tick", index % 2 == 0, |_| Msg::Increase);
            });
        }
    }
}

/// A plain button, keyed "0", and a button with a shadow, keyed "1".
struct Raised;

impl App for Raised {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("0", |ui| {
            ui.button("Plain");
        });
        ui.keyed("1", |ui| {
            let shadow = Shadow::lift(Color(0, 0, 0, 90), 6.0, 2.0);
            ui.button(Button::new("Raised").style(Style::button().shadow(shadow)));
        });
    }
}

/// `cells` labels in a row that ease to a colour, which `lit` chooses. While
/// they do, every one of them is painted again in every frame.
struct Wall {
    cells: usize,
    lit: bool,
}

impl App for Wall {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.row(Style::default(), |ui| {
            for index in 0..self.cells {
                ui.keyed(index.to_string(), |ui| {
                    ui.label(
                        Label::new("*").style(
                            Style::default()
                                .width(Length::Px(20.0))
                                .background(if self.lit {
                                    Color::rgb(220, 40, 60)
                                } else {
                                    Color::rgb(40, 60, 220)
                                })
                                .transition(Transition::new(300)),
                        ),
                    );
                });
            }
        });
    }
}

/// `count` keyed text inputs, none of them focused, all showing `text`.
struct Fields {
    count: usize,
    text: &'static str,
}

impl App for Fields {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        for index in 0..self.count {
            ui.keyed(format!("field{index}"), |ui| {
                ui.text_input("Name", self.text, Msg::Name);
            });
        }
    }
}

/// What a bound field's message is.
#[derive(Clone)]
enum Bound {
    Name(String),
    Tick(bool),
    Set(Change<Binding>),
}

impl From<Change<Binding>> for Bound {
    fn from(change: Change<Binding>) -> Self {
        Self::Set(change)
    }
}

/// How the fields of `Binding` send what they report.
#[derive(Clone, Copy)]
enum Sends {
    /// A message with a name: a variant or a function.
    Named,
    /// `bind` with a setter that captures nothing.
    Bound,
    /// `bind` with a setter that captures the index of its field.
    Captured,
}

/// `count` keyed text inputs, or check boxes when `boxes`, each sending as
/// `sends` says.
struct Binding {
    count: usize,
    boxes: bool,
    sends: Sends,
    names: Vec<String>,
    ticks: Vec<bool>,
}

impl Binding {
    fn new(count: usize, boxes: bool, sends: Sends) -> Self {
        Self {
            count,
            boxes,
            sends,
            names: vec![String::new(); count],
            ticks: vec![false; count],
        }
    }
}

impl App for Binding {
    type Message = Bound;

    fn update(&mut self, message: Bound, _: &mut Effects<'_, Bound>) {
        match message {
            Bound::Name(name) => self.names[0] = name,
            Bound::Tick(on) => self.ticks[0] = on,
            Bound::Set(change) => change(self),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Bound>) {
        for index in 0..self.count {
            ui.keyed(format!("field{index}"), |ui| {
                match (self.boxes, self.sends) {
                    (false, Sends::Named) => {
                        ui.text_input("Name", &self.names[index], Bound::Name);
                    }
                    (false, Sends::Bound) => {
                        ui.text_input(
                            "Name",
                            &self.names[index],
                            bind(|binding: &mut Self, name| binding.names[0] = name),
                        );
                    }
                    (false, Sends::Captured) => {
                        ui.text_input(
                            "Name",
                            &self.names[index],
                            bind(move |binding: &mut Self, name| binding.names[index] = name),
                        );
                    }
                    (true, Sends::Named) => {
                        ui.checkbox("Tick", self.ticks[index], Bound::Tick);
                    }
                    (true, Sends::Bound) => {
                        ui.checkbox(
                            "Tick",
                            self.ticks[index],
                            bind(|binding: &mut Self, on| binding.ticks[0] = on),
                        );
                    }
                    (true, Sends::Captured) => {
                        ui.checkbox(
                            "Tick",
                            self.ticks[index],
                            bind(move |binding: &mut Self, on| binding.ticks[index] = on),
                        );
                    }
                }
            });
        }
    }
}

/// `count` keyed rows of one label, or of four labels, each declared from a
/// line of its own, when `wide`.
struct Rows {
    count: usize,
    wide: bool,
}

impl App for Rows {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        for index in 0..self.count {
            ui.keyed(format!("row{index}"), |ui| {
                ui.row(Style::default(), |ui| {
                    ui.label("name");
                    if self.wide {
                        ui.label("kind");
                        ui.label("size");
                        ui.label("date");
                    }
                });
            });
        }
    }
}

/// `count` keyed labels in one scope.
struct Leaves {
    count: usize,
}

impl App for Leaves {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        for index in 0..self.count {
            ui.keyed(format!("leaf{index}"), |ui| ui.label("leaf"));
        }
    }
}

/// A scroll container, keyed "list", of `count` keyed rows, each a label and a
/// button, so that every row has children.
struct Scrolling {
    count: usize,
}

impl App for Scrolling {
    type Message = Msg;

    fn update(&mut self, _: Msg, _: &mut Effects<'_, Msg>) {}

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("list", |ui| {
            ui.scroll(Style::default().height(Length::Px(300.0)), |ui| {
                for index in 0..self.count {
                    ui.keyed(format!("row{index}"), |ui| {
                        ui.row(Style::default(), |ui| {
                            ui.label("name");
                            ui.button("Open");
                        });
                    });
                }
            });
        });
    }
}

/// Longer than eight bytes, so that a text built a character at a time
/// grows more than once.
const SHOWN: &str = "hello world, 12";

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

/// What repainting two buttons, keyed "0" and "1", costs when the pointer
/// moves from one to the other. Their text stays what it was, and the rest of
/// the scene is not touched, so what it costs is the repainted buttons and not
/// the others.
fn hover_cost<A: App>(app: A) -> usize {
    let mut running = Running::new(app);
    let middle = |running: &Running<A>, key: &str| {
        let bounds = running
            .context
            .node(running.context.find(&[key]).unwrap())
            .unwrap()
            .bounds;
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    };
    let first = middle(&running, "0");
    let second = middle(&running, "1");
    running.allocated_by(|running| {
        running.context.event(Event::PointerMoved(first));
    });
    running.allocated_by(|running| {
        running.context.event(Event::PointerMoved(second));
    })
}

/// What it costs to paint the button of `Raised` keyed `key` again, when the
/// pointer comes onto it. With `focus` it has the keyboard focus first, and so
/// a ring around it.
fn repaint_of(key: &str, focus: bool) -> usize {
    let mut running = Running::new(Raised);
    let button = running.context.find(&[key]).unwrap();
    if focus {
        running.context.action(button, Action::Focus);
        running.frame();
    }
    let bounds = running.context.node(button).unwrap().bounds;
    running.allocated_by(|running| {
        running.context.event(Event::PointerMoved(Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }));
    })
}

/// What a frame allocates in which each of `cells` cells is painted again, in
/// the middle of the time they take to ease to the colour that the view gave
/// them. The frames before it are the ones that make the room that this one
/// goes on using.
fn easing_cost(cells: usize) -> usize {
    let mut running = Running::new(Wall { cells, lit: false });
    let at = |frame: u64| Duration::from_millis(frame * 16);
    running.app.lit = true;
    running.driver.invalidate();
    running.context.set_now(at(1));
    running.frame();
    for frame in 2..5 {
        running.context.set_now(at(frame));
        running.frame();
    }
    running.allocated_by(|running| running.context.set_now(at(5)))
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

// A field's hint is measured when it changes, not on every frame.
#[test]
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

// The scene of the last frame is written over, so a button's text is not
// allocated again and the scene is not grown from nothing: repainting two
// buttons costs the same with 20 buttons as with 200. A button the pointer did
// not touch is not written at all, and one that is written has its text put
// over the old text; either alone keeps the cost down, and the test after this
// one is the one that tells them apart. The vectors that hold a node for each
// button grow by doubling, which is a few allocations more for ten times the
// buttons; a text allocated again would be 180 more.
#[test]
fn a_repaint_that_changes_no_text_allocates_the_same_at_20_and_200_buttons() {
    let (small, large) = (
        hover_cost(Counter::new(20, true)),
        hover_cost(Counter::new(200, true)),
    );
    assert!(
        large <= small + 8,
        "moving the pointer allocated {small} times at 20 buttons and {large} at 200"
    );
}

// What the pointer did not touch is left where it is in the scene. A check box
// that was written again would allocate its path again, 180 more at 200 check
// boxes than at 20.
#[test]
fn a_repaint_leaves_the_check_boxes_alone_and_allocates_the_same_at_20_and_200_of_them() {
    let (small, large) = (
        hover_cost(Ticks { boxes: 20 }),
        hover_cost(Ticks { boxes: 200 }),
    );
    assert!(
        large <= small + 8,
        "moving the pointer allocated {small} times with 20 check boxes and {large} with 200"
    );
}

// A node's primitives are held in one vector with room for eight. A plain
// button draws four: a clip, its fill, its text and the pop. One with a shadow
// and the focus draws six: the shadow too, and the ring, a stroked path. The
// primitives themselves do not allocate, so painting either button again costs
// the same as long as the vector holds what is drawn, except for the ring: a
// path owns its segments, and painting a focused node again measures two
// allocations for it.
#[test]
fn a_repaint_allocates_the_same_for_a_node_of_four_primitives_and_one_of_six() {
    let (plain, raised) = (repaint_of("0", false), repaint_of("1", true));
    assert_eq!(
        raised,
        plain + 2,
        "painting a button of six primitives allocated {raised} times, one of four {plain}"
    );
}

// A node painted again is written into the chunk it had: the chunk, the vector
// of its primitives and the buffer of its text are used again. Without that,
// every cell would allocate three times in every frame it eases in, 540 more
// for 200 cells than for 20. What is left is the lists the cells are in, which
// grow by doubling.
#[test]
fn a_frame_that_paints_every_cell_again_allocates_the_same_at_20_and_200_cells() {
    let (small, large) = (easing_cost(20), easing_cost(200));
    assert!(
        large <= small + 8,
        "painting 20 cells again allocated {small} times and 200 cells {large}"
    );
}

/// What a declaration of `count` fields showing `text` allocates.
fn declaration_cost(count: usize, text: &'static str) -> usize {
    Running::new(Fields { count, text }).allocated_by(|running| running.driver.invalidate())
}

/// What a declaration of `count` text fields, or check boxes when `boxes`,
/// allocates when they send as `sends` says.
fn binding_cost(count: usize, boxes: bool, sends: Sends) -> usize {
    Running::new(Binding::new(count, boxes, sends))
        .allocated_by(|running| running.driver.invalidate())
}

/// What the sweep that cancels compositions allocates over `count` fields
/// showing `text`.
fn sweep_cost(count: usize, text: &'static str) -> usize {
    let mut running = Running::new(Fields { count, text });
    let before = allocations();
    running.context.event(Event::FocusLost);
    allocations() - before
}

// A field's text is copied once in a declaration: into the element, which
// owns it. Comparing it with the editor's takes no copy of it, however long
// it is, and neither does cancelling a composition.
#[test]
fn a_declared_field_text_costs_one_allocation_per_field() {
    for count in [20, 50, 200] {
        let (with_text, without) = (declaration_cost(count, SHOWN), declaration_cost(count, ""));
        assert!(
            with_text <= without + count,
            "{count} fields with text allocated {with_text} times in a declaration, \
             {without} without"
        );
    }
}

// A setter that captures nothing makes a zero-sized handler, so a bound field
// is declared with the allocations of a named message.
#[test]
fn a_bound_text_field_is_declared_with_the_allocations_of_a_named_message() {
    for count in [20, 200] {
        let named = binding_cost(count, false, Sends::Named);
        let bound = binding_cost(count, false, Sends::Bound);
        assert_eq!(
            bound, named,
            "{count} bound fields allocated {bound} times in a declaration, \
             {count} named ones {named}"
        );
    }
}

// A check box makes the message for its next state while it is declared, which
// for a bound one is the change: one allocation.
#[test]
fn a_bound_check_box_costs_one_allocation_a_declaration_more_than_a_named_one() {
    for count in [20, 200] {
        let named = binding_cost(count, true, Sends::Named);
        let bound = binding_cost(count, true, Sends::Bound);
        assert_eq!(
            bound,
            named + count,
            "{count} bound check boxes allocated {bound} times in a declaration, \
             {count} named ones {named}"
        );
    }
}

// A setter that captures is stored with the handler: one allocation.
#[test]
fn a_bound_field_that_captures_an_index_costs_one_allocation_a_declaration() {
    for count in [20, 200] {
        let named = binding_cost(count, false, Sends::Named);
        let captured = binding_cost(count, false, Sends::Captured);
        assert_eq!(
            captured,
            named + count,
            "{count} capturing fields allocated {captured} times in a declaration, \
             {count} named ones {named}"
        );
    }
}

// The sweep visits every field and copies nothing.
#[test]
fn a_sweep_that_cancels_compositions_does_not_copy_field_texts() {
    for count in [20, 200] {
        let (with_text, without) = (sweep_cost(count, SHOWN), sweep_cost(count, ""));
        assert_eq!(
            with_text, without,
            "a sweep over {count} fields with text allocated {with_text} times, {without} without"
        );
    }
}

/// What a declaration of `count` rows allocates.
fn rows_cost(count: usize, wide: bool) -> usize {
    Running::new(Rows { count, wide }).allocated_by(|running| running.driver.invalidate())
}

/// What a declaration of `count` labels in one scope allocates.
fn leaves_cost(count: usize) -> usize {
    Running::new(Leaves { count }).allocated_by(|running| running.driver.invalidate())
}

// A container compares the key of a new widget with its siblings' and counts
// the lines that declare them in an array, so a row of four labels builds no
// table for either. Whatever else a row allocates, the three labels it has
// beyond the first cost their texts and nothing more.
#[test]
fn the_siblings_of_a_small_scope_are_told_apart_without_a_table() {
    const ROWS: usize = 100;
    const TEXTS: usize = 3;
    let (one, four) = (rows_cost(ROWS, false), rows_cost(ROWS, true));
    assert!(
        four - one <= ROWS * TEXTS,
        "{ROWS} rows of one label allocated {one} times in a declaration and of four labels {four}"
    );
}

// A scope with many widgets keeps a table of hashes, and no key is copied
// into it, so a label costs its name and its text however many come before.
#[test]
fn the_siblings_of_a_big_scope_are_told_apart_without_copying_their_keys() {
    const SMALL: usize = 100;
    const LARGE: usize = 400;
    const PER_LABEL: usize = 2;
    // The table of hashes grows by a few steps for the extra labels.
    const GROWTH: usize = 8;
    let (small, large) = (leaves_cost(SMALL), leaves_cost(LARGE));
    assert!(
        large - small <= (LARGE - SMALL) * PER_LABEL + GROWTH,
        "{SMALL} labels allocated {small} times in a declaration and {LARGE} labels {large}"
    );
}

// Reconciling a container copies the ids of its children once, and builds no
// list of its new children while they are the ones it had. A row of four
// labels, declared again unchanged, costs its name, the texts of its labels
// and that copy.
#[test]
fn a_row_declared_again_allocates_its_texts_and_one_list_of_its_children() {
    const FEW: usize = 20;
    const MANY: usize = 100;
    // The name of a row, the text of each of its four labels, and the copy.
    const PER_ROW: usize = 6;
    // What the lists of the outer container grow by.
    const GROWTH: usize = 8;
    let (few, many) = (rows_cost(FEW, true), rows_cost(MANY, true));
    assert!(
        many - few <= (MANY - FEW) * PER_ROW + GROWTH,
        "{FEW} rows allocated {few} times in a declaration and {MANY} rows {many}"
    );
}

/// What one notch of the wheel over a container of `count` rows allocates. The
/// notch before it, and the frame after that, make the room that this one goes
/// on using. The bounds of every node are worked out inside the event.
fn notch_cost(count: usize) -> usize {
    let mut running = Running::new(Scrolling { count });
    let bounds = running
        .context
        .node(running.context.find(&["list"]).unwrap())
        .unwrap()
        .bounds;
    running.context.event(Event::PointerMoved(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }));
    let notch = Event::Scroll {
        delta_x: 0.0,
        delta_y: 120.0,
    };
    running.context.event(notch.clone());
    running.frame();
    let before = allocations();
    running.context.event(notch);
    allocations() - before
}

// Working out the bounds again after a notch moves every row, and none of them
// is copied or listed for it: a notch allocates no more for 200 rows than for
// 20, however many of them have children.
#[test]
fn a_wheel_notch_allocates_the_same_at_20_and_200_rows() {
    let (small, large) = (notch_cost(20), notch_cost(200));
    assert!(
        large <= small + 8,
        "a notch over 20 rows allocated {small} times and over 200 rows {large}"
    );
}

/// A rich editor with nothing else around it.
struct Notes {
    document: RichDocument,
}

impl App for Notes {
    type Message = RichOp;

    fn update(&mut self, op: RichOp, _: &mut Effects<'_, RichOp>) {
        self.document.apply(op);
    }

    fn view(&self, ui: &mut Ui<'_, RichOp>) {
        ui.keyed("editor", |ui| {
            ui.rich_editor(
                &self.document,
                RichEditorOptions {
                    label: "Notes".into(),
                    toolbar: false,
                    ..RichEditorOptions::default()
                },
                |op| op,
            );
        });
    }
}

/// The editor itself, which a rich editor declares inside a group.
fn editor_named(context: &Context, label: &str) -> NodeId {
    let mut stack = vec![context.root()];
    while let Some(id) = stack.pop() {
        let node = context.node(id).unwrap();
        if node.semantics.role == Role::MultilineTextInput && node.semantics.label == label {
            return id;
        }
        stack.extend(node.children.iter().rev());
    }
    panic!("no editor labelled {label}");
}

// What assistive technology reads of a block is made once and copied after
// that: reading 200 blocks again that have not changed allocates a few times
// for each, where making it allocates a dozen. A letter typed in one block
// makes the reading of that block again, and no more.
#[test]
fn rereading_an_unchanged_rich_editor_allocates_a_fraction_of_the_first_read() {
    let blocks = (0..200)
        .map(|n| Block::new(BlockKind::Paragraph, &format!("paragraph {n}")))
        .collect();
    let mut running = Running::new(Notes {
        document: RichDocument::from_blocks(blocks),
    });
    let editor = editor_named(&running.context, "Notes");
    let read = |running: &Running<Notes>| {
        let before = allocations();
        running.context.editor_text(editor);
        allocations() - before
    };
    let first = read(&running);
    let second = read(&running);
    assert!(
        second * 2 <= first,
        "the first read allocated {first} times and the second {second}"
    );

    running.app.document.set_cursor(
        RichPosition {
            block: 100,
            offset: 3,
        },
        false,
    );
    running.app.document.apply(RichOp::Insert("x".into()));
    running.driver.invalidate();
    running.frame();
    let third = read(&running);
    assert!(
        third * 2 < first,
        "the first read allocated {first} times and the one after a letter {third}"
    );
}
