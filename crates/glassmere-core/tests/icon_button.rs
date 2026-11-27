//! A button with an icon beside its label, built from a widget of the
//! application's own: the recipe in "A button with an icon" in the widgets
//! guide. It is a button to the keyboard and to a screen reader, its size is
//! what the layout measures its text to be, and it clicks like any other.
mod support;

use glassmere_core::app::Action;
use glassmere_core::{
    CustomWidget, Event, Key, Role, Semantics, Style, Ui, WidgetBuilder, WidgetInput,
};
use glassmere_scene::{Color, Point, Primitive, Rect, Size};
use std::{cell::Cell, rc::Rc};
use support::{Driven, act, center};

const ROOT: Size = Size::new(400.0, 300.0);
const ICON: f32 = 14.0;
const GAP: f32 = 8.0;
const PAD: (f32, f32) = (14.0, 8.0);

#[derive(Default)]
struct Page {
    saves: u32,
}

/// An icon, a gap and a label inside a rounded fill, sized to its text.
fn icon_button(
    ui: &mut Ui<'_, Action<Page>>,
    key: &str,
    text: &str,
    send: Action<Page>,
    measured: &Cell<f32>,
) {
    let style = ui.text_style();
    let label = ui.measure_text(text, &style);
    measured.set(label.width);
    let size = Size::new(
        PAD.0 * 2.0 + ICON + GAP + label.width,
        PAD.1 * 2.0 + label.height.max(ICON),
    );
    let theme = ui.theme();
    let (fill, ink, accent) = (theme.control, theme.text, theme.accent);
    let name = text.to_owned();
    ui.keyed(key, |ui| {
        ui.widget(
            Style::default(),
            WidgetBuilder::new(move |input: &WidgetInput| {
                let lift = |c: Color| Color(c.0 + 12, c.1 + 12, c.2 + 12, c.3);
                CustomWidget {
                    size,
                    semantics: Semantics {
                        role: Role::Button,
                        label: name.clone(),
                        focusable: true,
                        ..Semantics::default()
                    },
                    primitives: vec![
                        Primitive::Quad {
                            rect: Rect {
                                x: 0.0,
                                y: 0.0,
                                width: size.width,
                                height: size.height,
                            },
                            color: if input.hovered { lift(fill) } else { fill },
                            radius: 6.0,
                        },
                        // The icon: any primitive will do, or an `Image`.
                        Primitive::Quad {
                            rect: Rect {
                                x: PAD.0,
                                y: (size.height - ICON) / 2.0,
                                width: ICON,
                                height: ICON,
                            },
                            color: accent,
                            radius: 3.0,
                        },
                        Primitive::Text {
                            rect: Rect {
                                x: PAD.0 + ICON + GAP,
                                y: (size.height - label.height) / 2.0,
                                width: label.width,
                                height: label.height,
                            },
                            text: name.clone(),
                            color: ink,
                            font_size: style.font_size,
                        },
                    ],
                    ..CustomWidget::default()
                }
            })
            .on_click(send),
        );
    });
}

fn page() -> (Driven<Page>, Rc<Cell<f32>>, Rc<Cell<f32>>) {
    let (short, long) = (Rc::new(Cell::new(0.0)), Rc::new(Cell::new(0.0)));
    let (a, b) = (short.clone(), long.clone());
    let driven = Driven::new(ROOT, Page::default(), move |_, ui| {
        icon_button(ui, "save", "Save", act(|s: &mut Page| s.saves += 1), &a);
        icon_button(
            ui,
            "long",
            "Save every open document",
            act(|s: &mut Page| s.saves += 10),
            &b,
        );
    });
    (driven, short, long)
}

fn width(driven: &Driven<Page>, key: &str) -> f32 {
    driven
        .context
        .node(driven.context.find(&[key]).unwrap())
        .unwrap()
        .bounds
        .width
}

#[test]
fn it_is_as_wide_as_its_text_and_no_wider() {
    let (driven, short, long) = page();
    let room = PAD.0 * 2.0 + ICON + GAP;
    assert_eq!(width(&driven, "save"), room + short.get());
    assert_eq!(width(&driven, "long"), room + long.get());
    assert!(long.get() > short.get() + 50.0);
}

#[test]
fn it_is_a_button_to_a_screen_reader_and_to_the_keyboard() {
    let (mut driven, _, _) = page();
    let node = driven
        .context
        .node(driven.context.find(&["save"]).unwrap())
        .unwrap();
    assert_eq!(node.semantics.role, Role::Button);
    assert_eq!(node.semantics.label, "Save");
    assert!(node.semantics.focusable);
    let save = driven.context.find(&["save"]).unwrap();
    driven.context.action(save, glassmere_core::Action::Focus);
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
    assert_eq!(driven.state().saves, 2);
}

#[test]
fn a_click_reaches_it_and_only_it() {
    let (mut driven, _, _) = page();
    let at = center(&driven.context, &["long"]);
    driven.context.event(Event::PointerMoved(at));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert_eq!(driven.state().saves, 10);
    let outside = Point { x: 390.0, y: 290.0 };
    driven.context.event(Event::PointerMoved(outside));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert_eq!(driven.state().saves, 10);
}
