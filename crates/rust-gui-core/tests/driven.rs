//! The test driver the other kernel tests migrate to.
mod support;

use rust_gui_core::{Action as Accessibility, Button};
use rust_gui_scene::Size;
use support::{Driven, act, on};

#[derive(Default)]
struct State {
    clicks: u32,
    name: String,
}

#[test]
fn a_click_and_typed_text_reach_the_state_through_actions() {
    let mut driven = Driven::new(Size::new(300.0, 200.0), State::default(), |state, ui| {
        ui.label(format!("{} {}", state.name, state.clicks));
        ui.keyed("add", |ui| {
            ui.button(Button::new("Add").on_click(act(|s: &mut State| s.clicks += 1)));
        });
        ui.keyed("name", |ui| {
            ui.text_input_msg(
                "Name",
                &state.name,
                on(|s: &mut State, text: String| s.name = text),
            );
        });
    });
    let add = driven.context.find(&["add"]).unwrap();
    driven.context.action(add, Accessibility::Activate);
    let stats = driven.frame();
    assert_eq!(driven.state().clicks, 1);
    assert!(stats.declared);

    let name = driven.context.find(&["name"]).unwrap();
    driven.context.action(name, Accessibility::Focus);
    driven
        .context
        .event(rust_gui_core::Event::Text(rust_gui_core::TextEdit::Insert(
            "Ada".into(),
        )));
    driven.frame();
    assert_eq!(driven.state().name, "Ada");
}
