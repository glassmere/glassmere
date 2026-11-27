//! Choosing the radio button that is already chosen sends nothing.
mod support;

use glassmere_core::{Action, Event, Key};
use glassmere_scene::Size;
use support::{Driven, act, center, click};

#[derive(Default)]
struct Choice {
    chosen: usize,
    sent: Vec<usize>,
}

fn group() -> Driven<Choice> {
    Driven::new(Size::new(300.0, 200.0), Choice::default(), |state, ui| {
        for (index, name) in ["small", "medium", "large"].into_iter().enumerate() {
            ui.keyed(name, |ui| {
                ui.radio(name, state.chosen, index, move |option| {
                    act(move |s: &mut Choice| {
                        s.chosen = option;
                        s.sent.push(option);
                    })
                });
            });
        }
    })
}

fn press(driven: &mut Driven<Choice>, key: Key) {
    driven.context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    driven.context.event(Event::KeyUp(key));
    driven.frame();
}

#[test]
fn a_click_on_the_chosen_radio_sends_nothing_and_on_another_sends_its_option() {
    let mut driven = group();
    let at = center(&driven.context, &["small"]);
    click(&mut driven.context, at);
    driven.frame();
    assert!(driven.state().sent.is_empty(), "{:?}", driven.state().sent);
    let at = center(&driven.context, &["large"]);
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(driven.state().sent, [2]);
    assert_eq!(driven.state().chosen, 2);
}

#[test]
fn space_and_activate_on_the_chosen_radio_send_nothing() {
    let mut driven = group();
    let small = driven.context.find(&["small"]).unwrap();
    driven.context.action(small, Action::Focus);
    driven.frame();
    press(&mut driven, Key::Space);
    driven.context.action(small, Action::Activate);
    driven.frame();
    assert!(driven.state().sent.is_empty(), "{:?}", driven.state().sent);
    // And Space on one that is not chosen still chooses it.
    let medium = driven.context.find(&["medium"]).unwrap();
    driven.context.action(medium, Action::Focus);
    driven.frame();
    press(&mut driven, Key::Space);
    assert_eq!(driven.state().sent, [1]);
}
