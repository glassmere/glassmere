//! The caption buttons of a window the application draws the frame of, at the
//! system's size and at one of its own.
mod support;

use glassmere_core::{Context, Length, MonospaceMeasurer, Role, Style, Ui, WindowButtons};
use glassmere_scene::{Rect, Size};
use support::labelled;

fn buttons(sized: Option<WindowButtons>) -> Context {
    let mut context = Context::new();
    context.run(
        Size::new(500.0, 200.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_, ()>| {
            ui.keyed("bar", |ui| {
                ui.title_bar(
                    Style::default()
                        .width(Length::Percent(1.0))
                        .height(Length::Px(44.0))
                        .gap(0.0),
                    |ui| match &sized {
                        Some(sized) => ui.window_buttons_with(*sized, |_| ()),
                        None => ui.window_buttons(|_| ()),
                    },
                )
            });
        },
    );
    context
}

fn button(context: &Context, label: &str) -> Rect {
    labelled(context, Role::Button, label)
}

#[test]
fn the_buttons_are_the_systems_size_unless_told_otherwise() {
    let context = buttons(None);
    for label in ["Minimize", "Maximize", "Close"] {
        let rect = button(&context, label);
        assert_eq!((rect.width, rect.height), (46.0, 30.0), "{label}");
    }
}

#[test]
fn a_button_can_fill_the_title_bar_and_be_as_wide_as_asked() {
    let context = buttons(Some(
        WindowButtons::new()
            .width(52.0)
            .height(Length::Percent(1.0)),
    ));
    let bar = context
        .node(context.find(&["bar"]).unwrap())
        .unwrap()
        .bounds;
    for label in ["Minimize", "Maximize", "Close"] {
        let rect = button(&context, label);
        assert_eq!((rect.width, rect.height), (52.0, 44.0), "{label}");
        assert_eq!(rect.y, bar.y, "{label} reaches the top of the bar");
    }
    // Side by side, nothing between them, the close button last.
    let (minimize, maximize, close) = (
        button(&context, "Minimize"),
        button(&context, "Maximize"),
        button(&context, "Close"),
    );
    assert_eq!(maximize.x, minimize.x + minimize.width);
    assert_eq!(close.x, maximize.x + maximize.width);
}

#[test]
fn a_taller_fixed_height_is_taken_as_it_is() {
    let context = buttons(Some(WindowButtons::new().height(Length::Px(38.0))));
    assert_eq!(button(&context, "Close").height, 38.0);
    assert_eq!(button(&context, "Close").width, 46.0);
}
