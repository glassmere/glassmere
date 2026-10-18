//! Number fields and the property grid send messages. Each test sends one
//! input through the harness, which is one batch, and checks the messages
//! that arrive and the view that follows them.

use glassmere_core::{
    App, Effects, Key, NodeId, NumberOptions, PropertyGridOptions, PropertyGridState, PropertyMsg,
    Ui,
};
use glassmere_scene::Size;
use glassmere_testing::Harness;

mod support;
use std::cell::Cell;
use support::find_keyed;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Width(f64),
}

/// A width from 0 to 100 in steps of 5, shown back as text.
#[derive(Default)]
struct Widths {
    width: f64,
    views: Cell<usize>,
    messages: Vec<Msg>,
}

impl App for Widths {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match &message {
            Msg::Width(width) => self.width = *width,
        }
        self.messages.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        self.views.set(self.views.get() + 1);
        ui.label(format!("Shown {}", self.width));
        ui.number_input(
            "Width",
            self.width,
            NumberOptions::default().range(0.0, 100.0).step(5.0),
            Msg::Width,
        );
    }
}

fn harness() -> Harness<Widths> {
    Harness::app(Size::new(400.0, 400.0), Widths::default())
}

#[test]
fn typing_a_number_and_pressing_enter_sends_one_parsed_clamped_snapped_value() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    // The field shows 0 with the caret after it; 12.5 snaps to 15 in steps of 5.
    let stats = harness.type_text("12.5");
    assert_eq!((stats.messages, stats.declared), (0, false));
    assert!(harness.state().messages.is_empty());

    let stats = harness.press(Key::Enter);
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Width(15.0)]);
    harness.by_label("Shown 15");
}

#[test]
fn a_number_above_the_range_is_clamped_before_it_is_sent() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    harness.type_text("250");
    let stats = harness.press(Key::Enter);
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Width(100.0)]);
}

#[test]
fn a_draft_that_does_not_parse_sends_nothing_and_reverts() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    harness.type_text("abc");
    let stats = harness.press(Key::Enter);
    assert_eq!(stats.messages, 0);
    assert!(harness.state().messages.is_empty());

    // Reverted: the next Enter has no draft to commit and sends nothing either.
    let stats = harness.press(Key::Enter);
    assert_eq!(stats.messages, 0);
    assert!(harness.state().messages.is_empty());
}

#[test]
fn a_draft_equal_to_the_shown_number_sends_nothing() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    harness.type_text("0");
    let stats = harness.press(Key::Enter);
    assert_eq!(stats.messages, 0);
    assert!(harness.state().messages.is_empty());
}

#[test]
fn the_stepper_sends_the_stepped_number_and_the_view_shows_it() {
    let mut harness = harness();
    let stats = harness.click(find_keyed(&harness, "$more"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Width(5.0)]);
    harness.by_label("Shown 5");

    let stats = harness.click(find_keyed(&harness, "$less"));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Width(5.0), Msg::Width(0.0)]);
}

#[test]
fn up_and_page_down_step_through_the_same_commit() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    let stats = harness.press(Key::ArrowUp);
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().messages, [Msg::Width(5.0)]);
}

#[test]
fn a_typed_draft_is_committed_by_the_step_that_follows_it() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    harness.type_text("20");
    // Up parses the draft first: 020 is 20, and one step of 5 makes 25.
    let stats = harness.press(Key::ArrowUp);
    assert_eq!(stats.messages, 1);
    assert_eq!(harness.state().messages, [Msg::Width(25.0)]);
}

#[test]
fn one_batch_runs_the_view_at_most_once() {
    let mut harness = harness();
    let before = harness.state().views.get();
    harness.click(find_keyed(&harness, "$more"));
    assert_eq!(harness.state().views.get(), before + 1);
}

#[test]
fn leaving_a_number_field_commits_its_draft_before_the_next_click_steps() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    harness.type_text("12.5");
    // The click moves focus away, which commits 12.5 as 15; the stepper then adds 5.
    let stats = harness.click(find_keyed(&harness, "$more"));
    assert_eq!(stats.messages, 2);
    assert_eq!(
        harness.state().messages,
        [Msg::Width(15.0), Msg::Width(20.0)]
    );
}

#[test]
fn leaving_a_number_field_with_a_bad_draft_reverts_and_the_stepper_steps_the_old_value() {
    let mut harness = harness();
    harness.focus(harness.by_label("Width"));
    harness.type_text("abc");
    let stats = harness.click(find_keyed(&harness, "$more"));
    assert_eq!(stats.messages, 1);
    assert_eq!(harness.state().messages, [Msg::Width(5.0)]);
}

/// A grid of one number row and one group, sending `PropertyMsg`s.
#[derive(Default)]
struct Grid {
    state: PropertyGridState,
    width: f64,
    messages: Vec<PropertyMsg>,
}

impl App for Grid {
    type Message = PropertyMsg;

    fn update(&mut self, message: PropertyMsg, _effects: &mut Effects<'_, PropertyMsg>) {
        self.state.apply(&message);
        if let PropertyMsg::Number { value, .. } = &message {
            self.width = *value;
        }
        self.messages.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, PropertyMsg>) {
        ui.property_grid(
            &self.state,
            PropertyGridOptions::new("Card"),
            |message| message,
            |grid| {
                grid.number(
                    "Width",
                    self.width,
                    NumberOptions::default().range(0.0, 100.0).step(5.0),
                );
                grid.group("Light", |grid| {
                    grid.number("Radius", 2.0, NumberOptions::default());
                });
            },
        );
    }
}

#[test]
fn a_grid_number_edit_sends_its_path_and_the_parsed_value() {
    let mut harness = Harness::app(Size::new(400.0, 400.0), Grid::default());
    harness.focus(find_keyed_in(&harness, "$field", "row:Width"));
    harness.type_text("12.5");
    let stats = harness.press(Key::Enter);
    assert_eq!(stats.messages, 1);
    assert_eq!(
        harness.state().messages,
        [PropertyMsg::Number {
            path: "Width".into(),
            value: 15.0,
        }]
    );
}

#[test]
fn a_group_header_sends_the_toggled_state_and_apply_stores_it() {
    let mut harness = Harness::app(Size::new(400.0, 400.0), Grid::default());
    harness.click(find_keyed_in(&harness, "$header", "group:Light"));
    assert_eq!(
        harness.state().messages,
        [PropertyMsg::Group {
            path: "Light".into(),
            collapsed: true,
        }]
    );
    harness.click(find_keyed_in(&harness, "$header", "group:Light"));
    assert_eq!(
        harness.state().messages.last(),
        Some(&PropertyMsg::Group {
            path: "Light".into(),
            collapsed: false,
        })
    );
}

/// The first node with this key whose path contains `within`, for any app.
fn find_keyed_in<A: App>(harness: &Harness<A>, key: &str, within: &str) -> NodeId {
    let context = harness.context();
    let mut pending = vec![context.root()];
    while let Some(id) = pending.pop() {
        if context
            .details(id)
            .is_some_and(|details| details.key == key)
            && harness.path(id).contains(within)
        {
            return id;
        }
        if let Some(node) = context.node(id) {
            pending.extend(node.children);
        }
    }
    panic!("no node keyed {key:?}");
}
