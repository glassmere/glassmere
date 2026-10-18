//! Overlays, dialogs, popovers and toasts sending messages through `Harness::app`.

use glassmere_core::{
    App, Button, Command, CommandPalette, DialogOptions, Effects, Event, Key, MenuItem,
    PopoverOptions, Style, Tab, Toast, Ui,
};
use glassmere_scene::{Point, Size};
use glassmere_testing::Harness;
use std::cell::Cell;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Popover(bool),
    Dialog(bool),
    OpenDialog,
    Menu(usize),
    Palette(bool),
    Query(String),
    Pick(usize),
    Raise,
    Undo(String),
    Tab(usize),
}

#[derive(Default)]
struct Overlays {
    popover: bool,
    dialog: bool,
    menu_picks: Vec<usize>,
    palette: bool,
    query: String,
    picks: Vec<usize>,
    undone: Vec<String>,
    views: Cell<usize>,
    tab: usize,
}

impl App for Overlays {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Popover(open) => self.popover = open,
            Msg::Dialog(open) => self.dialog = open,
            Msg::OpenDialog => self.dialog = true,
            Msg::Menu(index) => self.menu_picks.push(index),
            Msg::Palette(open) => self.palette = open,
            Msg::Query(query) => self.query = query,
            Msg::Pick(index) => {
                self.picks.push(index);
                self.palette = false;
                self.query.clear();
            }
            Msg::Raise => effects
                .toast_with(Toast::new("Saved").action("Undo").tag("doc"), |action| {
                    Msg::Undo(action.tag)
                }),
            Msg::Undo(tag) => self.undone.push(tag),
            Msg::Tab(index) => self.tab = index,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        ui.button(Button::new("Behind"));
        ui.button(Button::new("Share").on_click(Msg::Popover(true)));
        ui.popover(
            self.popover,
            PopoverOptions::default(),
            Msg::Popover,
            |ui| {
                ui.button("Inside");
            },
        );
        ui.button(Button::new("Open dialog").on_click(Msg::OpenDialog));
        ui.dialog(
            "confirm",
            self.dialog,
            DialogOptions::new("Delete?"),
            Msg::Dialog,
            |ui| {
                ui.button(Button::new("Cancel").on_click(Msg::Dialog(false)));
            },
        );
        ui.context_menu(
            &[MenuItem::new("Copy"), MenuItem::new("Paste")],
            Style::default(),
            Msg::Menu,
            |ui| ui.label("Target"),
        );
        ui.button(Button::new("Palette").on_click(Msg::Palette(true)));
        let commands = [
            Command::with_id("save", "Save", None),
            Command::with_id("open", "Open file", None),
        ];
        ui.command_palette(
            self.palette,
            &self.query,
            &commands,
            CommandPalette::new(Msg::Query)
                .on_open(Msg::Palette)
                .on_pick(Msg::Pick),
        );
        ui.button(Button::new("Raise").on_click(Msg::Raise));
        ui.tabs(
            self.tab,
            &[Tab::new("One"), Tab::new("Two")],
            Msg::Tab,
            |ui, index| {
                ui.label(format!("Panel {index}"));
            },
        );
    }
}

fn harness() -> Harness<Overlays> {
    Harness::app(Size::new(800.0, 600.0), Overlays::default())
}

fn center(harness: &Harness<Overlays>, label: &str) -> Point {
    let bounds = harness
        .context()
        .node(harness.by_label(label))
        .unwrap()
        .bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

#[test]
fn a_popover_opens_by_message_and_a_message_closes_it_on_escape_or_an_outside_press() {
    let mut harness = harness();
    let views = harness.state().views.get();
    let stats = harness.click(harness.by_label("Share"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().views.get(), views + 1, "one view per batch");
    assert!(harness.state().popover);
    harness.by_label("Inside");

    // Escape, with focus inside the popover, sends Popover(false).
    let stats = harness.press(Key::Escape);
    assert_eq!(stats.messages, 1);
    assert!(!harness.state().popover);
    assert!(harness.query_label("Inside").is_none());

    // A press outside the popover does too; the pressed control still gets its click.
    harness.click(harness.by_label("Share"));
    assert!(harness.state().popover);
    harness.click(harness.by_label("Behind"));
    assert!(!harness.state().popover);
    assert!(harness.query_label("Inside").is_none());
}

#[test]
fn a_dialog_opens_by_message_and_sends_false_on_escape_and_on_a_scrim_click() {
    let mut harness = harness();
    harness.click(harness.by_label("Open dialog"));
    assert!(harness.state().dialog);
    harness.by_label("Cancel");

    let stats = harness.press(Key::Escape);
    assert_eq!(stats.messages, 1);
    assert!(!harness.state().dialog);

    harness.click(harness.by_label("Open dialog"));
    assert!(harness.state().dialog);
    let stats = harness.click(harness.by_label("Behind"));
    assert_eq!(stats.messages, 1, "the scrim sends its dismissal");
    assert!(!harness.state().dialog);
    assert_eq!(harness.state().picks.len(), 0);
}

#[test]
fn a_dialog_button_sends_its_own_message_through_the_dialog() {
    let mut harness = harness();
    harness.click(harness.by_label("Open dialog"));
    harness.click(harness.by_label("Cancel"));
    assert!(!harness.state().dialog);
}

#[test]
fn a_context_menu_pick_sends_the_index_of_the_item_once() {
    let mut harness = harness();
    let point = center(&harness, "Target");
    harness.event(Event::ContextMenu(point));
    harness.run();
    let stats = harness.click(harness.by_label("Paste"));
    assert_eq!(stats.messages, 1);
    assert_eq!(harness.state().menu_picks, [1]);
}

#[test]
fn a_command_palette_filters_by_query_and_picks_by_click_or_enter() {
    let mut harness = harness();
    harness.click(harness.by_label("Palette"));
    assert!(harness.state().palette);

    harness.type_text("sav");
    assert_eq!(harness.state().query, "sav");
    harness.by_label("Save");
    assert!(harness.query_label("Open file").is_none(), "filtered out");

    harness.click(harness.by_label("Save"));
    assert_eq!(harness.state().picks, [0]);
    assert!(
        !harness.state().palette,
        "the application closes it in update"
    );
    assert!(harness.state().query.is_empty());

    harness.click(harness.by_label("Palette"));
    harness.type_text("open");
    let stats = harness.press(Key::Enter);
    assert_eq!(stats.messages, 1);
    assert_eq!(
        harness.state().picks,
        [0, 1],
        "Enter picks the first enabled match"
    );
}

#[test]
fn a_tab_click_sends_its_index_and_the_panel_follows_the_message() {
    let mut harness = harness();
    harness.by_label("Panel 0");
    let stats = harness.click(harness.by_label("Two"));
    assert_eq!(stats.messages, 1);
    assert_eq!(harness.state().tab, 1);
    harness.by_label("Panel 1");
    assert!(harness.query_label("Panel 0").is_none());
}

#[test]
fn a_toast_action_click_sends_its_message_in_the_next_batch() {
    let mut harness = harness();
    harness.click(harness.by_label("Raise"));
    let undone = harness.state().undone.len();
    harness.click(harness.by_label("Undo"));
    assert_eq!(
        harness.state().undone.len(),
        undone,
        "the click is delivered later"
    );
    harness.run();
    assert_eq!(harness.state().undone, ["doc"]);
    // The toast has left with its action, so nothing is sent again.
    harness.run();
    assert_eq!(harness.state().undone, ["doc"]);
}
