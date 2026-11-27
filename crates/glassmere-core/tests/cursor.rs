//! The pointer a host shows over a node.
use glassmere_core::{
    Axis, Button, Context, Cursor, Length, MonospaceMeasurer, SplitOptions, Style, Ui,
};
use glassmere_scene::Size;

const ROOT: Size = Size::new(600.0, 400.0);

/// The cursor over the widget at `path` inside the page of `view`.
fn cursor(context: &Context, path: &[&str]) -> Cursor {
    let full: Vec<&str> = core::iter::once("page")
        .chain(path.iter().copied())
        .collect();
    context.cursor(context.find(&full).unwrap_or_else(|| panic!("no {full:?}")))
}

fn view(ui: &mut Ui<'_>) {
    ui.keyed("page", |ui| {
        ui.column(Style::default().gap(4.0), |ui| {
            ui.keyed("button", |ui| ui.button("Go"));
            ui.keyed("disabled", |ui| {
                ui.button(Button::new("Off").enabled(false))
            });
            // Only for the mouse: out of the focus order, and still a button.
            ui.keyed("mouse", |ui| {
                ui.button(Button::new("Mouse").style(Style::button().focusable(false)))
            });
            ui.keyed("input", |ui| ui.text_input("Name", "", |_: String| ()));
            ui.keyed("plain", |ui| {
                ui.container(Style::default().height(Length::Px(20.0)), |ui| {
                    ui.keyed("note", |ui| ui.label("note"))
                })
            });
            ui.keyed("fits", |ui| {
                ui.scroll(Style::default().height(Length::Px(60.0)), |ui| {
                    ui.keyed("inside", |ui| ui.label("short"))
                })
            });
            ui.keyed("scrolls", |ui| {
                ui.scroll(Style::default().height(Length::Px(60.0)), |ui| {
                    ui.keyed("tall", |ui| {
                        ui.container(Style::default().height(Length::Px(500.0)), |_| {})
                    })
                })
            });
            ui.keyed("pill", |ui| {
                ui.container(Style::default().cursor(Cursor::Grab), |ui| {
                    ui.keyed("label", |ui| ui.label("drag me"));
                    ui.keyed("button", |ui| {
                        ui.button(
                            Button::new("x").style(Style::button().cursor(Cursor::NotAllowed)),
                        )
                    });
                })
            });
        })
    });
}

#[test]
fn a_widget_shows_the_pointer_for_what_it_is() {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, view);
    assert_eq!(cursor(&context, &["button"]), Cursor::Pointer);
    assert_eq!(cursor(&context, &["input"]), Cursor::Text);
    assert_eq!(cursor(&context, &["plain"]), Cursor::Default);
    assert_eq!(cursor(&context, &["plain", "note"]), Cursor::Default);
}

#[test]
fn a_disabled_text_field_does_not_ask_for_the_i_beam() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("on", |ui| ui.text_input("On", "", |_: String| ()));
                ui.enabled(false, |ui| {
                    ui.keyed("off", |ui| ui.text_input("Off", "", |_: String| ()));
                });
            })
        });
    };
    context.run(ROOT, &mut MonospaceMeasurer, view);
    assert_eq!(cursor(&context, &["on"]), Cursor::Text);
    assert_eq!(cursor(&context, &["off"]), Cursor::Default);
}

#[test]
fn a_disabled_widget_keeps_the_arrow() {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, view);
    assert_eq!(cursor(&context, &["disabled"]), Cursor::Default);
}

#[test]
fn a_button_that_is_not_for_the_keyboard_is_still_for_the_mouse() {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, view);
    assert_eq!(cursor(&context, &["mouse"]), Cursor::Pointer);
}

#[test]
fn a_scroll_area_is_no_hand_whether_or_not_it_scrolls() {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, view);
    context.run(ROOT, &mut MonospaceMeasurer, view);
    assert_eq!(cursor(&context, &["fits"]), Cursor::Default);
    assert_eq!(cursor(&context, &["fits", "inside"]), Cursor::Default);
    assert_eq!(cursor(&context, &["scrolls"]), Cursor::Default);
}

#[test]
fn a_style_sets_the_pointer_for_what_is_inside_it_unless_that_sets_its_own() {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, view);
    assert_eq!(cursor(&context, &["pill"]), Cursor::Grab);
    assert_eq!(cursor(&context, &["pill", "label"]), Cursor::Grab);
    assert_eq!(cursor(&context, &["pill", "button"]), Cursor::NotAllowed);
}

#[test]
fn a_divider_shows_the_resize_pointer_of_its_direction() {
    let mut context = Context::new();
    let panes = |axis| {
        move |ui: &mut Ui<'_>| {
            ui.keyed("page", |ui| {
                ui.column(Style::default(), |ui| {
                    ui.keyed("split", |ui| {
                        ui.split(
                            0.5,
                            axis,
                            SplitOptions {
                                style: Style::default()
                                    .width(Length::Px(400.0))
                                    .height(Length::Px(300.0)),
                                ..SplitOptions::default()
                            },
                            |_| (),
                            |ui| ui.label("a"),
                            |ui| ui.label("b"),
                        )
                    })
                })
            });
        }
    };
    context.run(ROOT, &mut MonospaceMeasurer, panes(Axis::Horizontal));
    assert_eq!(cursor(&context, &["split", "$divider"]), Cursor::ColResize);
    context.run(ROOT, &mut MonospaceMeasurer, panes(Axis::Vertical));
    assert_eq!(cursor(&context, &["split", "$divider"]), Cursor::RowResize);
}
