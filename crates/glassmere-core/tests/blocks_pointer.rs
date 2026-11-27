//! A box that takes the pointer where it is, so that it shields what is under it.
mod support;

use glassmere_core::app::Action;
use glassmere_core::{Button, Length, Style, StyleSheet, Ui};
use glassmere_scene::{Color, Point, Size};
use support::{Driven, act, click};

const ROOT: Size = Size::new(400.0, 300.0);

#[derive(Default)]
struct Page {
    under: u32,
    inside: u32,
}

/// A button, and over its left part a panel that may block the pointer. The
/// panel holds a button of its own when `inner` is set.
fn page(blocks: bool, inner: bool) -> Driven<Page> {
    Driven::new(
        ROOT,
        Page::default(),
        move |_, ui: &mut Ui<'_, Action<Page>>| {
            ui.keyed("stage", |ui| {
                ui.container(
                    Style::default()
                        .width(Length::Px(300.0))
                        .height(Length::Px(120.0)),
                    |ui| {
                        ui.keyed("under", |ui| {
                            ui.button(
                                Button::new("Under")
                                    .style(
                                        Style::button()
                                            .width(Length::Px(160.0))
                                            .height(Length::Px(40.0))
                                            .offset(10.0, 10.0),
                                    )
                                    .on_click(act(|s: &mut Page| s.under += 1)),
                            )
                        });
                        ui.keyed("cover", |ui| {
                            ui.container(
                                Style::default()
                                    .width(Length::Px(80.0))
                                    .height(Length::Px(40.0))
                                    .offset(10.0, 10.0)
                                    .background(Color(70, 90, 130, 255))
                                    .blocks_pointer(blocks),
                                |ui| {
                                    if inner {
                                        ui.keyed("inner", |ui| {
                                            ui.button(
                                                Button::new("In")
                                                    .style(
                                                        Style::button()
                                                            .width(Length::Px(30.0))
                                                            .height(Length::Px(20.0)),
                                                    )
                                                    .on_click(act(|s: &mut Page| s.inside += 1)),
                                            )
                                        });
                                    }
                                },
                            )
                        });
                    },
                )
            });
        },
    )
}

const COVERED: Point = Point { x: 60.0, y: 30.0 };
const FREE: Point = Point { x: 150.0, y: 30.0 };

#[test]
fn a_container_is_transparent_to_the_pointer_unless_it_says_otherwise() {
    let mut driven = page(false, false);
    click(&mut driven.context, COVERED);
    driven.frame();
    assert_eq!(driven.state().under, 1, "the click goes through the panel");
}

#[test]
fn a_box_that_blocks_the_pointer_shields_what_is_under_it() {
    let mut driven = page(true, false);
    click(&mut driven.context, COVERED);
    driven.frame();
    assert_eq!(driven.state().under, 0, "the panel took the click");
    // What the panel leaves free is still the button's.
    click(&mut driven.context, FREE);
    driven.frame();
    assert_eq!(driven.state().under, 1);
    // And it is what the pointer is over, not the button.
    driven
        .context
        .event(glassmere_core::Event::PointerMoved(COVERED));
    let cover = driven.context.find(&["stage", "cover"]).unwrap();
    assert_eq!(driven.context.hovered(), Some(cover));
}

#[test]
fn what_is_inside_a_blocking_box_still_takes_the_pointer_first() {
    let mut driven = page(true, true);
    // The inner button is at the panel's top left: (10, 10) to (40, 30).
    click(&mut driven.context, Point { x: 20.0, y: 20.0 });
    driven.frame();
    assert_eq!((driven.state().inside, driven.state().under), (1, 0));
}

#[test]
fn a_stylesheet_has_blocks_pointer() {
    let sheet = StyleSheet::parse(
        "class card {\nblocks-pointer: true\n}\nclass plain {\nblocks-pointer: false\n}",
    )
    .unwrap();
    assert!(sheet.class("card").unwrap().blocks_pointer);
    assert!(!sheet.class("plain").unwrap().blocks_pointer);
    assert!(StyleSheet::parse("class bad {\nblocks-pointer: yes\n}").is_err());
}
