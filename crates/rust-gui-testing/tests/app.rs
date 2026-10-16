use rust_gui_core::{
    Action, App, Button, Component, Effects, Event, Key, MessageSender, Style, TextEdit, TextInput,
    Ui,
};
use rust_gui_scene::Size;
use rust_gui_testing::Harness;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Increase,
    Named(&'static str),
    Inner(Inner),
    Text(String),
    Submit,
    Toggle(bool),
    Value(f64),
}

#[derive(Clone, Debug, PartialEq)]
enum Inner {
    Hit,
}

/// Records every message and counts its declarations.
#[derive(Default)]
struct Log {
    count: i32,
    messages: Vec<Msg>,
    views: Cell<usize>,
    text: String,
    checked: bool,
    value: f64,
}

impl App for Log {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match &message {
            Msg::Increase => self.count += 1,
            Msg::Text(text) => self.text = text.clone(),
            Msg::Toggle(checked) => self.checked = *checked,
            Msg::Value(value) => self.value = *value,
            _ => {}
        }
        self.messages.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        ui.label(format!("Count {}", self.count));
        ui.button(Button::new("Increase").on_click(Msg::Increase));
        ui.button(Button::new("First").on_click(Msg::Named("first")));
        ui.button(Button::new("Second").on_click(Msg::Named("second")));
        ui.memo(&(), Style::default(), |ui| {
            ui.button(Button::new("Remembered").on_click(Msg::Named("remembered")));
        });
        ui.map(Msg::Inner, |ui| {
            ui.button(Button::new("Inner").on_click(Inner::Hit));
        });
        ui.text_input(
            TextInput::new("Name").on_submit(Msg::Submit),
            &self.text,
            Msg::Text,
        );
        ui.checkbox("Agree", self.checked, Msg::Toggle);
        ui.slider("Level", self.value, 0.0..=10.0, 0.5, Msg::Value);
    }
}

fn harness() -> Harness<Log> {
    Harness::app(Size::new(400.0, 900.0), Log::default())
}

#[test]
fn a_label_above_a_button_shows_the_click_after_one_declaration() {
    let mut harness = harness();
    let views = harness.state().views.get();
    let stats = harness.click(harness.by_label("Increase"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().views.get(), views + 1);
    harness.by_label("Count 1");
}

#[test]
fn messages_keep_the_order_of_their_input_and_the_batch_declares_once() {
    let mut harness = harness();
    let (first, second) = (harness.by_label("First"), harness.by_label("Second"));
    let views = harness.state().views.get();
    harness.send(Msg::Named("sent"));
    let views = views + 1;
    assert_eq!(harness.state().views.get(), views);
    harness.action(second, Action::Activate);
    harness.action(first, Action::Activate);
    let stats = harness.run();
    assert_eq!((stats.messages, stats.declared), (2, true));
    assert_eq!(harness.state().views.get(), views + 1);
    assert_eq!(
        harness.state().messages,
        [
            Msg::Named("sent"),
            Msg::Named("second"),
            Msg::Named("first")
        ]
    );
}

#[test]
fn a_batch_without_input_or_messages_does_not_declare() {
    let mut harness = harness();
    let views = harness.state().views.get();
    let stats = harness.run();
    assert_eq!((stats.messages, stats.declared), (0, false));
    // A hover over a button nobody asked about only repaints.
    let stats = harness.hover(harness.by_label("First"));
    assert!(!stats.declared);
    assert_eq!(harness.state().views.get(), views);
}

#[test]
fn a_skipped_memo_still_sends_from_its_widgets() {
    let mut harness = harness();
    let stats = harness.click(harness.by_label("Remembered"));
    assert_eq!(stats.messages, 1);
    assert_eq!(stats.skipped, 1);
    assert_eq!(harness.state().messages, [Msg::Named("remembered")]);
    harness.click(harness.by_label("Remembered"));
    assert_eq!(harness.state().messages.len(), 2);
}

#[test]
fn a_mapped_view_sends_through_its_function() {
    let mut harness = harness();
    harness.click(harness.by_label("Inner"));
    assert_eq!(harness.state().messages, [Msg::Inner(Inner::Hit)]);
}

#[test]
fn a_text_input_sends_its_edits_and_enter() {
    let mut harness = harness();
    harness.focus(harness.by_label("Name"));
    harness.type_text("Ada");
    assert_eq!(harness.state().text, "Ada");
    harness.press(Key::Enter);
    assert_eq!(harness.state().messages.last(), Some(&Msg::Submit));
    harness.event(Event::Text(TextEdit::Insert("!".into())));
    harness.run();
    assert_eq!(harness.state().text, "Ada!");
}

#[test]
fn a_checkbox_sends_the_state_it_toggles_to() {
    let mut harness = harness();
    harness.click(harness.by_label("Agree"));
    harness.click(harness.by_label("Agree"));
    assert_eq!(
        harness.state().messages,
        [Msg::Toggle(true), Msg::Toggle(false)]
    );
}

#[test]
fn a_slider_sends_a_snapped_value() {
    let mut harness = harness();
    let slider = harness.by_label("Level");
    harness.action(slider, Action::SetNumber(3.3));
    harness.run();
    assert_eq!(harness.state().value, 3.5);
    harness.action(slider, Action::Increment);
    harness.run();
    assert_eq!(harness.state().value, 4.0);
}

#[derive(Clone, Debug, PartialEq)]
enum Pipe {
    Start,
    Target,
    Spawned(String),
    Sent(String),
}

/// Starts a job in `update` and opens a channel in `init`, then records what
/// arrives.
#[derive(Default)]
struct Pipes {
    sender: Option<MessageSender<String>>,
    finished: Arc<AtomicBool>,
    messages: Vec<Pipe>,
}

impl App for Pipes {
    type Message = Pipe;

    fn init(&mut self, effects: &mut Effects<'_, Pipe>) {
        self.sender = Some(effects.channel(4, Pipe::Sent));
    }

    fn update(&mut self, message: Pipe, effects: &mut Effects<'_, Pipe>) {
        if message == Pipe::Start {
            let finished = self.finished.clone();
            let _ = effects.spawn(
                "job",
                move |sender| {
                    let _ = sender.send("spawned".to_owned());
                    finished.store(true, Ordering::Release);
                },
                Pipe::Spawned,
            );
        }
        self.messages.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, Pipe>) {
        ui.button(Button::new("Start").on_click(Pipe::Start));
        ui.button(Button::new("Target").on_click(Pipe::Target));
    }
}

#[test]
fn spawned_and_channel_messages_arrive_after_the_input_of_their_batch() {
    let mut harness = Harness::app(Size::new(400.0, 300.0), Pipes::default());
    harness.click(harness.by_label("Start"));
    // The job's result is only drained by the next batch, never the one that
    // started it, so this is the same whether or not the job has finished.
    assert_eq!(harness.state().messages, [Pipe::Start]);
    let started = Instant::now();
    while !harness.state().finished.load(Ordering::Acquire) {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the job never ran"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let sender = harness.state().sender.as_ref().expect("opened in init");
    sender.try_send("channel".to_owned()).expect("room");
    harness.click(harness.by_label("Target"));
    // One batch: the input first, then the spawned job's result, then the
    // channel's value. `AppDriver::batch` drains workers before channels.
    assert_eq!(
        harness.state().messages,
        [
            Pipe::Start,
            Pipe::Target,
            Pipe::Spawned("spawned".to_owned()),
            Pipe::Sent("channel".to_owned()),
        ]
    );
}

#[derive(Clone, Debug, PartialEq)]
enum Loader {
    Load,
    Loaded(String),
}

/// A component whose `update` asks for a channel, and whose result comes
/// back through `Effects::map` as the parent's message.
#[derive(Default)]
struct Fetch {
    loaded: Option<String>,
}

impl Component for Fetch {
    type Message = Loader;

    fn update(&mut self, message: Loader, effects: &mut Effects<'_, Loader>) {
        match message {
            Loader::Load => {
                let sender = effects.channel(1, Loader::Loaded);
                sender.try_send("data".to_owned()).expect("room");
            }
            Loader::Loaded(text) => self.loaded = Some(text),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Loader>) {
        ui.button(Button::new("Load").on_click(Loader::Load));
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Page {
    Fetch(Loader),
}

#[derive(Default)]
struct Parent {
    fetch: Fetch,
    received: Vec<Page>,
}

impl App for Parent {
    type Message = Page;

    fn update(&mut self, message: Page, effects: &mut Effects<'_, Page>) {
        self.received.push(message.clone());
        match message {
            Page::Fetch(message) => self.fetch.update(message, &mut effects.map(Page::Fetch)),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Page>) {
        ui.component(Style::default(), &self.fetch, Page::Fetch);
    }
}

#[test]
fn a_component_effect_result_reaches_the_parent_as_the_parent_message() {
    let mut harness = Harness::app(Size::new(400.0, 300.0), Parent::default());
    harness.click(harness.by_label("Load"));
    assert_eq!(harness.state().received, [Page::Fetch(Loader::Load)]);
    harness.run();
    assert_eq!(
        harness.state().received,
        [
            Page::Fetch(Loader::Load),
            Page::Fetch(Loader::Loaded("data".to_owned())),
        ]
    );
    assert_eq!(harness.state().fetch.loaded.as_deref(), Some("data"));
}
