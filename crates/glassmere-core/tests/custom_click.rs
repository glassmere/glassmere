//! A click on a widget of the application's own, and what it tells assistive
//! technology about its state.
mod support;

use glassmere_core::app::Action;
use glassmere_core::{
    CustomWidget, Event, EventArea, Key, Role, Semantics, Style, Ui, WidgetBuilder, WidgetInput,
};
use glassmere_scene::{Point, Size};
use support::{Driven, act, center};

const ROOT: Size = Size::new(400.0, 300.0);

#[derive(Default)]
struct Pills {
    clicks: u32,
    events: u32,
}

fn pill(role: Role, checked: Option<bool>) -> impl FnOnce(&WidgetInput) -> CustomWidget {
    move |_| CustomWidget {
        size: Size::new(80.0, 30.0),
        semantics: Semantics {
            role,
            label: "pill".into(),
            focusable: true,
            checked,
            ..Semantics::default()
        },
        primitives: Vec::new(),
        input: EventArea::default(),
    }
}

fn view(role: Role) -> impl Fn(&Pills, &mut Ui<'_, Action<Pills>>) {
    move |_, ui| {
        ui.keyed("pill", |ui| {
            ui.widget(
                Style::default(),
                WidgetBuilder::new(pill(role, Some(false)))
                    .on_click(act(|s: &mut Pills| s.clicks += 1)),
            );
        });
    }
}

fn press(driven: &mut Driven<Pills>, at: Point) {
    driven.context.event(Event::PointerMoved(at));
    driven.context.event(Event::PointerDown);
}

fn release(driven: &mut Driven<Pills>, at: Point) {
    driven.context.event(Event::PointerMoved(at));
    driven.context.event(Event::PointerUp);
    driven.frame();
}

#[test]
fn a_press_and_release_on_the_widget_is_one_click_whatever_its_pressed_role() {
    for role in [Role::Button, Role::Checkbox, Role::Radio, Role::Tab] {
        let mut driven = Driven::new(ROOT, Pills::default(), view(role));
        let at = center(&driven.context, &["pill"]);
        press(&mut driven, at);
        release(&mut driven, at);
        assert_eq!(driven.state().clicks, 1, "{role:?}");
    }
}

#[test]
fn dragging_off_the_widget_and_letting_go_is_no_click() {
    let mut driven = Driven::new(ROOT, Pills::default(), view(Role::Checkbox));
    let at = center(&driven.context, &["pill"]);
    press(&mut driven, at);
    release(&mut driven, Point { x: 300.0, y: 250.0 });
    assert_eq!(driven.state().clicks, 0);
    // And a press elsewhere that ends on it is not one either.
    press(&mut driven, Point { x: 300.0, y: 250.0 });
    release(&mut driven, at);
    assert_eq!(driven.state().clicks, 0);
    // The next honest one still works.
    press(&mut driven, at);
    release(&mut driven, at);
    assert_eq!(driven.state().clicks, 1);
}

#[test]
fn enter_and_space_on_a_focused_checkbox_click_it() {
    let mut driven = Driven::new(ROOT, Pills::default(), view(Role::Checkbox));
    let pill = driven.context.find(&["pill"]).unwrap();
    driven.context.action(pill, glassmere_core::Action::Focus);
    driven.frame();
    for key in [Key::Enter, Key::Space] {
        driven.context.event(Event::KeyDown {
            key,
            shift: false,
            repeat: false,
        });
        driven.context.event(Event::KeyUp(key));
        driven.frame();
    }
    assert_eq!(driven.state().clicks, 2);
}

#[test]
fn a_role_that_is_moved_rather_than_pressed_is_not_clicked_by_the_keyboard() {
    let mut driven = Driven::new(ROOT, Pills::default(), |_, ui| {
        ui.keyed("dial", |ui| {
            ui.custom(
                Style::default(),
                pill(Role::Slider, None)(&WidgetInput::default()),
                act(|s: &mut Pills| s.clicks += 1),
            );
        });
    });
    let dial = driven.context.find(&["dial"]).unwrap();
    driven.context.action(dial, glassmere_core::Action::Focus);
    driven.frame();
    driven.context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
    driven.frame();
    assert_eq!(driven.state().clicks, 0);
}

#[test]
fn ui_custom_with_a_checkbox_role_is_clicked_too() {
    let mut driven = Driven::new(ROOT, Pills::default(), |_, ui| {
        ui.keyed("box", |ui| {
            ui.custom(
                Style::default(),
                pill(Role::Checkbox, Some(true))(&WidgetInput::default()),
                act(|s: &mut Pills| s.clicks += 1),
            );
        });
    });
    let at = center(&driven.context, &["box"]);
    press(&mut driven, at);
    release(&mut driven, at);
    assert_eq!(driven.state().clicks, 1);
}

#[test]
fn a_custom_widget_reports_its_state_to_assistive_technology() {
    let mut driven = Driven::new(ROOT, Pills::default(), |_, ui| {
        ui.keyed("pill", |ui| {
            ui.custom(
                Style::default(),
                CustomWidget {
                    semantics: Semantics {
                        role: Role::Tab,
                        label: "Rules".into(),
                        focusable: true,
                        checked: Some(true),
                        selected: Some(true),
                        expanded: Some(false),
                        ..Semantics::default()
                    },
                    ..pill(Role::Tab, None)(&WidgetInput::default())
                },
                act(|_: &mut Pills| ()),
            );
        });
    });
    driven.frame();
    let id = driven.context.find(&["pill"]).unwrap();
    let snapshot = driven
        .context
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.id == id)
        .unwrap();
    assert_eq!(snapshot.checked, Some(true));
    assert_eq!(snapshot.selected, Some(true));
    assert_eq!(snapshot.expanded, Some(false));
    assert_eq!(snapshot.semantics.role, Role::Tab);
}

#[test]
#[should_panic(expected = "needs a role that is pressed")]
fn a_click_on_a_widget_that_cannot_be_pressed_says_so() {
    let _ = Driven::new(ROOT, Pills::default(), view(Role::Group));
}

#[test]
fn a_widget_with_events_and_a_click_gets_both() {
    let mut driven = Driven::new(ROOT, Pills::default(), |_, ui| {
        ui.keyed("pill", |ui| {
            ui.widget(
                Style::default(),
                WidgetBuilder::new(|_: &WidgetInput| CustomWidget {
                    size: Size::new(80.0, 30.0),
                    semantics: Semantics {
                        role: Role::Button,
                        label: "pill".into(),
                        focusable: true,
                        ..Semantics::default()
                    },
                    primitives: Vec::new(),
                    input: EventArea {
                        pointer: true,
                        ..EventArea::default()
                    },
                })
                .on_event(|event| match event {
                    glassmere_core::UiEvent::PointerDown(_) => {
                        Some(act(|s: &mut Pills| s.events += 1))
                    }
                    _ => None,
                })
                .on_click(act(|s: &mut Pills| s.clicks += 1)),
            );
        });
    });
    let at = center(&driven.context, &["pill"]);
    press(&mut driven, at);
    release(&mut driven, at);
    assert_eq!((driven.state().events, driven.state().clicks), (1, 1));
}
