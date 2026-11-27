//! Choosing the item that is already chosen in a drop-down sends nothing.
mod support;

use glassmere_core::{ComboBox, Event, Key};
use glassmere_scene::Size;
use support::{Driven, act, center, click};

#[derive(Default)]
struct Choice {
    chosen: usize,
    sent: Vec<usize>,
}

fn drop_down() -> Driven<Choice> {
    Driven::new(Size::new(300.0, 300.0), Choice::default(), |state, ui| {
        ui.keyed("theme", |ui| {
            ui.combo_box(
                ComboBox::new("Theme"),
                state.chosen,
                &["Dark", "Light", "System"],
                |index| {
                    act(move |s: &mut Choice| {
                        s.chosen = index;
                        s.sent.push(index);
                    })
                },
            );
        });
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
fn enter_on_the_chosen_item_sends_nothing_and_on_another_sends_it() {
    let mut driven = drop_down();
    // Focus it, open it, and take the highlighted item, which is the chosen one.
    let at = center(&driven.context, &["theme"]);
    click(&mut driven.context, at);
    driven.frame();
    press(&mut driven, Key::Enter);
    assert!(driven.state().sent.is_empty(), "{:?}", driven.state().sent);
    // Open it again and move down one: that one is sent.
    press(&mut driven, Key::ArrowDown);
    press(&mut driven, Key::ArrowDown);
    press(&mut driven, Key::Enter);
    assert_eq!(driven.state().sent, [1]);
    assert_eq!(driven.state().chosen, 1);
}
