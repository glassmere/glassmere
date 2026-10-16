mod support;

use rust_gui_core::{Button, Event, Length, Style, Ui};
use rust_gui_scene::{Point, Size};
use std::cell::Cell;
use support::{Driven, act, on};

const ROOT: Size = Size::new(400.0, 400.0);

#[derive(Default)]
struct State {
    /// What the memoized subtree is declared from.
    items: Vec<String>,
    /// Bumped by a button inside the subtree.
    clicks: usize,
    heading: String,
    /// How often the memoized content actually ran.
    runs: Cell<usize>,
}

fn list(state: &State, ui: &mut Ui<'_, rust_gui_core::app::Action<State>>) {
    ui.keyed("heading", |ui| ui.label(state.heading.clone()));
    ui.keyed("list", |ui| {
        ui.memo(
            &(&state.items, state.clicks),
            Style::default().width(Length::Percent(1.0)),
            |ui| {
                state.runs.set(state.runs.get() + 1);
                for (index, item) in state.items.iter().enumerate() {
                    ui.keyed(format!("item{index}"), |ui| ui.label(item.clone()));
                }
                ui.keyed("bump", |ui| {
                    ui.button(
                        Button::new("Bump")
                            .enabled(true)
                            .style(Style::button())
                            .on_click(act(|s: &mut State| s.clicks += 1)),
                    )
                });
            },
        )
    });
}

fn driven(state: State) -> Driven<State> {
    Driven::new(ROOT, state, list)
}

fn click(context: &mut rust_gui_core::Context, path: &[&str]) {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    let centre = Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    };
    context.event(Event::PointerMoved(centre));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

fn labels(context: &rust_gui_core::Context) -> Vec<String> {
    context
        .snapshots()
        .into_iter()
        .map(|node| node.semantics.label)
        .filter(|label| !label.is_empty())
        .collect()
}

#[test]
fn an_unchanged_state_keeps_the_subtree_without_declaring_it() {
    let mut driven = driven(State {
        items: vec!["one".into(), "two".into()],
        ..State::default()
    });
    assert_eq!(driven.state().runs.get(), 1);
    let shown = labels(&driven.context);
    assert!(shown.contains(&"one".to_owned()) && shown.contains(&"two".to_owned()));
    let scene = driven.context.scene().clone();

    // The same state again: the view declares, the memo keeps the subtree.
    driven.state_mut();
    let stats = driven.frame();
    assert!(stats.declared);
    assert_eq!((driven.state().runs.get(), stats.skipped), (1, 1));
    assert_eq!(labels(&driven.context), shown, "the subtree is still there");
    assert_eq!(driven.context.scene(), &scene, "and paints the same");
    assert_eq!(stats.removed, 0, "nothing was taken for undeclared");

    // A different state declares it again.
    driven.state_mut().items.push("three".into());
    let stats = driven.frame();
    assert_eq!((driven.state().runs.get(), stats.skipped), (2, 0));
    assert!(labels(&driven.context).contains(&"three".to_owned()));
}

#[test]
fn a_skipped_frame_still_repaints_and_lays_out_the_rest() {
    let mut driven = driven(State {
        items: vec!["one".into()],
        heading: "first".into(),
        ..State::default()
    });
    let inside = driven.context.find(&["list", "item0"]).unwrap();
    let before = driven.context.node(inside).unwrap().bounds;

    // Only the heading changes, and it is longer, so the subtree moves down.
    driven.state_mut().heading = "first\nsecond\nthird".into();
    let stats = driven.frame();
    assert_eq!((stats.skipped, driven.state().runs.get()), (1, 1));
    let after = driven.context.node(inside).unwrap().bounds;
    assert!(after.y > before.y, "{before:?} then {after:?}");
    assert_eq!(
        driven.context.find(&["list", "item0"]),
        Some(inside),
        "kept its id"
    );
}

#[test]
fn a_click_inside_a_skipped_subtree_still_fires_its_handler() {
    let mut driven = driven(State {
        items: vec!["one".into()],
        ..State::default()
    });
    // A frame with nothing to do declares nothing.
    let stats = driven.frame();
    assert!(!stats.declared);
    assert_eq!(driven.state().runs.get(), 1);

    // The click belongs to a button inside the memoized subtree. It is
    // resolved against the tree declared last time, so its action runs, and
    // the state it changes is what the subtree is declared from.
    click(&mut driven.context, &["list", "bump"]);
    let stats = driven.frame();
    assert_eq!(driven.state().clicks, 1);
    assert!(stats.declared);
    assert_eq!((driven.state().runs.get(), stats.skipped), (2, 0));

    // Nothing changed since: the subtree is kept again.
    let stats = {
        driven.state_mut();
        driven.frame()
    };
    assert_eq!(
        (
            driven.state().clicks,
            driven.state().runs.get(),
            stats.skipped
        ),
        (1, 2, 1)
    );
}

#[test]
fn memos_nest_and_hold_the_state_of_the_widgets_inside() {
    let mut driven = Driven::new(
        ROOT,
        NestState {
            typed: "start".into(),
            ..NestState::default()
        },
        nest,
    );
    assert_eq!(driven.state().runs.get(), (1, 1));
    let field = driven.context.find(&["outer", "field"]).unwrap();
    let deep = driven.context.find(&["outer", "inner", "deep"]).unwrap();

    // Typing reaches the field: the edit is an action that changes `typed`,
    // which the outer memo is keyed on, so the containers above it declare
    // again, but not the inner one, which holds nothing that changed.
    driven.context.action(field, rust_gui_core::Action::Focus);
    driven
        .context
        .action(field, rust_gui_core::Action::SetText("edited".into()));
    let stats = driven.frame();
    assert_eq!(driven.state().typed, "edited");
    assert_eq!((driven.state().runs.get(), stats.skipped), ((2, 1), 1));

    // Nothing changed: the outer one is kept, so the inner one is not reached.
    let stats = {
        driven.state_mut();
        driven.frame()
    };
    assert_eq!((driven.state().runs.get(), stats.skipped), ((2, 1), 1));
    assert_eq!(driven.context.find(&["outer", "inner", "deep"]), Some(deep));

    // Changing one state declares that container alone.
    driven.state_mut().outer += 1;
    driven.frame();
    assert_eq!(
        driven.state().runs.get(),
        (3, 1),
        "the inner subtree was kept"
    );

    driven.state_mut().inner += 1;
    driven.state_mut().outer += 1;
    let stats = driven.frame();
    assert_eq!((driven.state().runs.get(), stats.skipped), ((4, 2), 0));
    assert_eq!(
        driven.context.find(&["outer", "field"]),
        Some(field),
        "kept its id"
    );
    assert_eq!(driven.state().typed, "edited", "and what was typed into it");
}

#[derive(Default)]
struct NestState {
    outer: usize,
    inner: usize,
    typed: String,
    /// How often the outer and the inner content ran.
    runs: Cell<(usize, usize)>,
}

fn nest(state: &NestState, ui: &mut Ui<'_, rust_gui_core::app::Action<NestState>>) {
    ui.keyed("outer", |ui| {
        ui.memo(&(state.outer, &state.typed), Style::default(), |ui| {
            let (outer, inner) = state.runs.get();
            state.runs.set((outer + 1, inner));
            ui.keyed("field", |ui| {
                ui.text_input(
                    "Field",
                    &state.typed,
                    on(|s: &mut NestState, text: String| s.typed = text),
                )
            });
            ui.keyed("inner", |ui| {
                ui.memo(&state.inner, Style::default(), |ui| {
                    let (outer, inner) = state.runs.get();
                    state.runs.set((outer, inner + 1));
                    ui.keyed("deep", |ui| ui.label("deep"));
                })
            });
        })
    });
}
