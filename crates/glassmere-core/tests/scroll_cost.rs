//! What a wheel step over a long list costs: the view is not declared again
//! when only a plain scroll area moved, and what the area clips away is not
//! painted until it comes into view.
mod support;

use glassmere_core::{Button, Context, Event, Length, MonospaceMeasurer, Style, Ui};
use glassmere_scene::{Point, Primitive, Size};
use std::cell::Cell;
use std::rc::Rc;
use support::{Driven, act};

const WINDOW: Size = Size::new(300.0, 300.0);

struct State {
    views: Rc<Cell<u32>>,
    clicked: u32,
}

fn list(state: &State, ui: &mut Ui<'_, glassmere_core::app::Action<State>>) {
    state.views.set(state.views.get() + 1);
    ui.keyed("view", |ui| {
        ui.scroll(
            Style::default()
                .width(Length::Px(200.0))
                .height(Length::Px(100.0))
                .gap(0.0),
            |ui| {
                for row in 0..200 {
                    ui.keyed(format!("row{row}"), |ui| {
                        ui.button(
                            Button::new(format!("Row {row}"))
                                .on_click(act(|s: &mut State| s.clicked += 1)),
                        )
                    });
                }
            },
        )
    });
}

fn shows(context: &Context, text: &str) -> bool {
    context
        .scene()
        .primitives
        .iter()
        .any(|primitive| matches!(primitive, Primitive::Text { text: t, .. } if t == text))
}

fn driven() -> (Driven<State>, Rc<Cell<u32>>) {
    let views = Rc::new(Cell::new(0));
    let state = State {
        views: views.clone(),
        clicked: 0,
    };
    (Driven::new(WINDOW, state, list), views)
}

fn wheel(driven: &mut Driven<State>, delta: f32) -> glassmere_core::FrameStats {
    driven
        .context
        .event(Event::PointerMoved(Point { x: 100.0, y: 50.0 }));
    driven.frame();
    driven.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: delta,
    });
    driven.frame()
}

#[test]
fn a_wheel_step_over_a_plain_scroll_area_does_not_declare_the_view() {
    let (mut driven, views) = driven();
    let before = views.get();
    let view = driven.context.find(&["view"]).unwrap();
    let start = driven.context.node(view).unwrap().scroll.unwrap().offset;
    let stats = wheel(&mut driven, 120.0);
    assert!(
        !stats.declared,
        "the view ran for a step that changed nothing it reads"
    );
    assert_eq!(views.get(), before);
    let moved = driven.context.node(view).unwrap().scroll.unwrap().offset;
    assert!(moved > start, "the area scrolled: {start} to {moved}");
}

#[test]
fn what_the_area_clips_away_is_not_painted_until_it_scrolls_into_view() {
    let (mut driven, _) = driven();
    assert!(shows(&driven.context, "Row 0"));
    assert!(
        !shows(&driven.context, "Row 150"),
        "a row 150 rows down is clipped away"
    );
    let stats = driven.frame();
    // Every row is a node with two or more of its own, and almost none is painted.
    assert!(
        stats.paint_nodes < 40,
        "{} nodes painted of the 200 rows' worth",
        stats.paint_nodes
    );

    // Far enough that row 150 is under the pointer.
    let view = driven.context.find(&["view"]).unwrap();
    let row = driven.context.find(&["view", "row150"]).unwrap();
    let wanted =
        driven.context.node(row).unwrap().bounds.y - driven.context.node(view).unwrap().bounds.y;
    for _ in 0..10 {
        wheel(&mut driven, wanted / 10.0);
    }
    assert!(
        shows(&driven.context, "Row 150"),
        "it is drawn once it is in view"
    );
    assert!(!shows(&driven.context, "Row 0"), "and the first is not");
}

#[test]
fn a_row_scrolled_into_view_can_be_clicked() {
    let (mut driven, _) = driven();
    for _ in 0..30 {
        wheel(&mut driven, 100.0);
    }
    let rows: Vec<_> = (0..200)
        .filter(|row| {
            driven
                .context
                .find(&["view", &format!("row{row}")])
                .and_then(|id| driven.context.node(id))
                .is_some_and(|node| node.clip.height > 0.0)
        })
        .collect();
    assert!(!rows.is_empty() && rows[0] > 20, "{rows:?}");
    let target = rows[rows.len() / 2];
    let id = driven
        .context
        .find(&["view", &format!("row{target}")])
        .unwrap();
    let bounds = driven.context.node(id).unwrap().bounds;
    driven.context.event(Event::PointerMoved(Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }));
    driven.context.event(Event::PointerDown);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert_eq!(driven.state().clicked, 1, "row {target}");
}

#[test]
fn a_virtual_list_is_still_declared_for_a_wheel_step() {
    use glassmere_core::{ItemId, ListModel, ListState, VirtualListOptions};

    struct Rows;
    impl ListModel for Rows {
        fn len(&self) -> usize {
            500
        }
        fn item_id(&self, index: usize) -> ItemId {
            ItemId(index as u64)
        }
        fn index_of(&self, id: ItemId) -> Option<usize> {
            Some(id.0 as usize)
        }
        fn label(&self, index: usize) -> String {
            format!("Item {index}")
        }
    }
    let views = Rc::new(Cell::new(0));
    let counter = views.clone();
    let state = ListState::default();
    let mut driven = Driven::new(
        WINDOW,
        (),
        move |_, ui: &mut Ui<'_, glassmere_core::app::Action<()>>| {
            counter.set(counter.get() + 1);
            let options = VirtualListOptions {
                height: 200.0,
                row_height: 30.0,
                ..VirtualListOptions::default()
            };
            ui.virtual_list_with(
                &Rows,
                &state,
                options,
                |_| act(|_: &mut ()| {}),
                |ui, _, i, _| {
                    ui.label(format!("Item {i}"));
                },
            );
        },
    );
    assert!(shows(&driven.context, "Item 0"));
    let before = views.get();
    driven
        .context
        .event(Event::PointerMoved(Point { x: 100.0, y: 80.0 }));
    driven.frame();
    for _ in 0..10 {
        driven.context.event(Event::Scroll {
            delta_x: 0.0,
            delta_y: 120.0,
        });
        driven.frame();
    }
    assert!(
        views.get() > before,
        "a list realizes its rows by its offset, so the view runs"
    );
    assert!(!shows(&driven.context, "Item 0"));
    let _ = MonospaceMeasurer;
}
