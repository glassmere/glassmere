//! Milestone 35, phase 2A: the controls and fields that send messages. Each
//! test sends one input through the harness, which is one batch, and checks
//! that the message arrives and that the view shows its new value after it.

use rust_gui_core::{App, Command, Effects, Event, MenuItem, NodeId, Ui, WindowCommand};
use rust_gui_scene::Size;
use rust_gui_testing::Harness;

const THEMES: [&str; 3] = ["Light", "Dark", "Auto"];
const SUGGESTIONS: [&str; 3] = ["apple", "apricot", "banana"];

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Theme(usize),
    Pick(usize),
    Menu(usize),
    Save,
    Query(String),
    Window(WindowCommand),
}

#[derive(Default)]
struct Controls {
    theme: usize,
    choice: usize,
    menu: usize,
    saved: bool,
    query: String,
    window: Option<WindowCommand>,
    messages: Vec<Msg>,
}

impl App for Controls {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match &message {
            Msg::Theme(index) => self.theme = *index,
            Msg::Pick(index) => self.choice = *index,
            Msg::Menu(index) => self.menu = *index,
            Msg::Save => self.saved = true,
            Msg::Query(text) => self.query = text.clone(),
            Msg::Window(command) => {
                self.window = Some(*command);
                effects.window_command(*command);
            }
        }
        self.messages.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.label(format!("Theme {}", self.theme));
        ui.combo_box_msg("Theme", self.theme, &THEMES, Msg::Theme);
        ui.label(format!("Choice {}", self.choice));
        ui.radio_msg("Pick", self.choice, 1, Msg::Pick);
        ui.label(format!("Menu {}", self.menu));
        ui.menu_msg(
            "Actions",
            &[MenuItem::new("Alpha"), MenuItem::new("Beta")],
            Msg::Menu,
        );
        ui.label(format!("Saved {}", self.saved));
        ui.command_button_msg(&Command::new("Save", None), Msg::Save);
        ui.label(format!("Query {}", self.query));
        ui.autocomplete_msg("Search", &self.query, &SUGGESTIONS, Msg::Query);
        ui.window_buttons_msg(Msg::Window);
    }
}

fn harness() -> Harness<Controls> {
    Harness::app(Size::new(400.0, 1400.0), Controls::default())
}

/// The node with this key anywhere in the tree; the caption buttons are
/// declared under sites the test does not name.
fn find_keyed(harness: &Harness<Controls>, key: &str) -> NodeId {
    let context = harness.context();
    let mut pending = vec![context.root()];
    while let Some(id) = pending.pop() {
        if context
            .details(id)
            .is_some_and(|details| details.key == key)
        {
            return id;
        }
        if let Some(node) = context.node(id) {
            pending.extend(node.children);
        }
    }
    panic!("no node keyed {key:?}");
}

#[test]
fn a_combo_box_sends_the_index_picked_and_shows_it() {
    let mut harness = harness();
    harness.click(harness.by_label("Theme"));
    let stats = harness.click(harness.by_label("Dark"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Theme(1)]);
    harness.by_label("Theme 1");
}

#[test]
fn a_radio_sends_its_option_and_shows_it_selected() {
    let mut harness = harness();
    let stats = harness.click(harness.by_label("Pick"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Pick(1)]);
    harness.by_label("Choice 1");
}

#[test]
fn a_menu_sends_the_index_picked_and_shows_it() {
    let mut harness = harness();
    harness.click(harness.by_label("Actions  ▾"));
    let stats = harness.click(harness.by_label("Beta"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Menu(1)]);
    harness.by_label("Menu 1");
}

#[test]
fn a_command_button_sends_its_message_on_click_and_on_its_command() {
    let mut harness = harness();
    let stats = harness.click(harness.by_label("Save"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    harness.by_label("Saved true");

    // A menu pick or shortcut activates the same button, so the same message.
    harness.event(Event::Command("Save".into()));
    let stats = harness.run();
    assert_eq!(stats.messages, 1);
    assert_eq!(harness.state().messages, [Msg::Save, Msg::Save]);
}

#[test]
fn an_autocomplete_sends_its_typed_text_and_the_suggestion_picked() {
    let mut harness = harness();
    harness.focus(harness.by_label("Search"));
    let stats = harness.type_text("ap");
    assert_eq!((stats.messages, stats.declared), (1, true));
    harness.by_label("Query ap");

    harness.click(harness.by_label("banana"));
    assert_eq!(
        harness.state().messages,
        [Msg::Query("ap".into()), Msg::Query("banana".into())]
    );
    harness.by_label("Query banana");
}

#[test]
fn a_caption_button_sends_its_window_command_on_click() {
    let mut harness = harness();
    let close = find_keyed(&harness, "close");
    let stats = harness.click(close);
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(
        harness.state().messages,
        [Msg::Window(WindowCommand::Close)]
    );
    assert_eq!(harness.state().window, Some(WindowCommand::Close));
}
