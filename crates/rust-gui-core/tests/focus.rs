mod support;

use rust_gui_core::{
    Action, Button, Context, DialogOptions, Event, FocusScope, Key, Length, MonospaceMeasurer,
    Role, Style, Ui,
};
use rust_gui_scene::{Point, Primitive, Size};
use support::{Driven, act, on};

const ROOT: Size = Size::new(600.0, 400.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = rust_gui_core::app::Action<S>;

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> rust_gui_core::FrameStats {
    context.run(ROOT, &mut MonospaceMeasurer, view)
}

fn key(context: &mut Context, key: Key) {
    context.event(Event::KeyDown {
        key,
        shift: false,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

fn shift_tab(context: &mut Context) {
    context.event(Event::KeyDown {
        key: Key::Tab,
        shift: true,
        repeat: false,
    });
}

fn focused_key(context: &Context) -> String {
    context
        .focused()
        .and_then(|id| context.details(id))
        .map_or_else(|| "none".into(), |details| details.key)
}

fn center(context: &Context, path: &[&str]) -> Point {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    }
}

fn click(context: &mut Context, point: Point) {
    context.event(Event::PointerMoved(point));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

#[test]
fn trapping_scopes_cycle_tab_and_remembered_scopes_reenter_on_their_last_child() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.keyed("before", |ui| ui.button("Before"));
                ui.keyed("group", |ui| {
                    ui.focus_scope(FocusScope::default(), Style::default(), |ui| {
                        ui.keyed("g1", |ui| ui.button("One"));
                        ui.keyed("g2", |ui| ui.button("Two"));
                        ui.keyed("g3", |ui| ui.button("Three"));
                    })
                });
                ui.keyed("after", |ui| ui.button("After"));
                ui.keyed("trap", |ui| {
                    ui.focus_scope(
                        FocusScope {
                            trap: true,
                            ..FocusScope::default()
                        },
                        Style::default(),
                        |ui| {
                            ui.keyed("t1", |ui| ui.button("T1"));
                            ui.keyed("t2", |ui| ui.button("T2"));
                        },
                    )
                });
            })
        });
    };
    frame(&mut context, view);
    // Remember: focus the middle child, leave, and re-enter from either side.
    let g2 = context.find(&["page", "group", "g2"]).unwrap();
    context.action(g2, Action::Focus);
    key(&mut context, Key::Tab);
    assert_eq!(
        focused_key(&context),
        "g3",
        "Tab inside a plain scope moves normally"
    );
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "after");
    shift_tab(&mut context);
    assert_eq!(
        focused_key(&context),
        "g3",
        "the scope remembers its last child"
    );
    context.action(g2, Action::Focus);
    let before = context.find(&["page", "before"]).unwrap();
    context.action(before, Action::Focus);
    key(&mut context, Key::Tab);
    assert_eq!(
        focused_key(&context),
        "g2",
        "entering from the front lands on the memory"
    );
    // Trap: once inside, Tab and Shift+Tab never leave.
    let t1 = context.find(&["page", "trap", "t1"]).unwrap();
    context.action(t1, Action::Focus);
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "t2");
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "t1");
    shift_tab(&mut context);
    assert_eq!(focused_key(&context), "t2");
    // Outside the trap, traversal still reaches it and passes through.
    let after = context.find(&["page", "after"]).unwrap();
    context.action(after, Action::Focus);
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "t2", "the trap remembers T2");
}

#[test]
fn restoring_scopes_return_focus_when_they_disappear() {
    let mut context = Context::new();
    let view = |show: bool| {
        move |ui: &mut Ui<'_>| {
            ui.keyed("page", |ui| {
                ui.column(Style::default(), |ui| {
                    ui.keyed("opener", |ui| ui.button("Open"));
                    if show {
                        ui.keyed("panel", |ui| {
                            ui.focus_scope(
                                FocusScope {
                                    restore: true,
                                    ..FocusScope::default()
                                },
                                Style::default(),
                                |ui| {
                                    ui.keyed("inside", |ui| ui.button("Inside"));
                                },
                            )
                        });
                    }
                    ui.keyed("other", |ui| ui.button("Other"));
                })
            });
        }
    };
    frame(&mut context, view(true));
    let opener = context.find(&["page", "opener"]).unwrap();
    context.action(opener, Action::Focus);
    let inside = context.find(&["page", "panel", "inside"]).unwrap();
    context.action(inside, Action::Focus);
    frame(&mut context, view(true));
    frame(&mut context, view(false));
    assert_eq!(
        focused_key(&context),
        "opener",
        "focus returns to the opener"
    );
    // Without `restore`, a vanished focus is simply cleared.
    let plain = |show: bool| {
        move |ui: &mut Ui<'_>| {
            ui.keyed("opener", |ui| ui.button("Open"));
            if show {
                ui.keyed("panel", |ui| {
                    ui.focus_scope(FocusScope::default(), Style::default(), |ui| {
                        ui.keyed("inside", |ui| ui.button("Inside"));
                    })
                });
            }
        }
    };
    let mut context = Context::new();
    frame(&mut context, plain(true));
    context.action(context.find(&["opener"]).unwrap(), Action::Focus);
    context.action(context.find(&["panel", "inside"]).unwrap(), Action::Focus);
    frame(&mut context, plain(false));
    assert_eq!(focused_key(&context), "none");
}

#[derive(Default)]
struct Dialogs {
    open: bool,
    dismissed: usize,
    dismissible: bool,
    modal: bool,
}

/// A page behind one dialog. A dismissal (scrim or Escape) is `on_open(false)`
/// and is counted; the Cancel button is an ordinary click that closes it.
fn dialog_page(state: &Dialogs, ui: &mut Ui<'_, Handler<Dialogs>>) {
    ui.keyed("page", |ui| {
        ui.column(Style::default(), |ui| {
            ui.keyed("behind", |ui| ui.button("Behind"));
            ui.keyed("list", |ui| {
                ui.scroll(
                    Style::default()
                        .width(Length::Px(200.0))
                        .height(Length::Px(60.0)),
                    |ui| {
                        for index in 0..20 {
                            ui.keyed(format!("row-{index}"), |ui| {
                                ui.label(format!("Row {index}"))
                            });
                        }
                    },
                )
            });
        })
    });
    ui.dialog(
        "confirm",
        state.open,
        on(|s: &mut Dialogs, open: bool| {
            if !open {
                s.dismissed += 1;
            }
            s.open = open;
        }),
        DialogOptions::new("Delete file?")
            .dismissible(state.dismissible)
            .modal(state.modal),
        |ui| {
            ui.keyed("question", |ui| ui.label("This cannot be undone."));
            ui.keyed("actions", |ui| {
                ui.row(Style::default(), |ui| {
                    ui.keyed("cancel", |ui| {
                        ui.button(
                            Button::new("Cancel").on_click(act(|s: &mut Dialogs| s.open = false)),
                        )
                    });
                    ui.keyed("delete", |ui| ui.button("Delete"));
                })
            });
        },
    );
}

#[test]
fn modal_dialogs_take_focus_trap_tab_block_pointer_and_wheel_and_restore_on_close() {
    let mut driven = Driven::new(
        ROOT,
        Dialogs {
            dismissible: true,
            modal: true,
            ..Dialogs::default()
        },
        dialog_page,
    );
    let behind = driven.context.find(&["page", "behind"]).unwrap();
    driven.context.action(behind, Action::Focus);
    driven.state_mut().open = true;
    driven.frame();
    let dialog = driven.context.find(&["$dialog:confirm"]).unwrap();
    let snapshot = driven.context.node(dialog).unwrap();
    assert_eq!(snapshot.semantics.role, Role::Dialog);
    assert_eq!(snapshot.semantics.label, "Delete file?");
    assert!(snapshot.modal);
    assert_eq!(
        snapshot.bounds.width, ROOT.width,
        "the layer covers the window"
    );
    let panel = driven
        .context
        .node(driven.context.find(&["$dialog:confirm", "$panel"]).unwrap())
        .unwrap();
    assert!(
        (panel.bounds.x + panel.bounds.width / 2.0 - ROOT.width / 2.0).abs() < 1.0,
        "centered"
    );
    assert_eq!(
        focused_key(&driven.context),
        "cancel",
        "initial focus lands on the first control"
    );
    key(&mut driven.context, Key::Tab);
    assert_eq!(focused_key(&driven.context), "delete");
    key(&mut driven.context, Key::Tab);
    assert_eq!(
        focused_key(&driven.context),
        "cancel",
        "Tab is trapped inside the dialog"
    );
    // The scrim wins hit testing over content behind it, and wheel input is blocked.
    let behind_point = center(&driven.context, &["page", "behind"]);
    assert_eq!(driven.context.hit_test(behind_point), Some(dialog));
    let scrim_quads = driven
        .context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Quad { rect, .. } if rect.width == ROOT.width && rect.height == ROOT.height))
        .count();
    assert!(scrim_quads >= 1, "a scrim quad covers the window");
    let list = driven.context.find(&["page", "list"]).unwrap();
    driven.context.event(Event::PointerMoved(center(
        &driven.context,
        &["page", "list"],
    )));
    assert!(!driven.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 40.0
    }));
    assert_eq!(
        driven.context.node(list).unwrap().scroll.unwrap().offset,
        0.0
    );
    // Clicking the scrim dismisses: one message, the flag clears, the dialog goes.
    click(&mut driven.context, behind_point);
    driven.frame();
    assert_eq!(driven.state().dismissed, 1);
    assert!(!driven.state().open);
    assert_eq!(
        focused_key(&driven.context),
        "behind",
        "focus returns to the opener"
    );
    assert!(driven.context.find(&["$dialog:confirm"]).is_none());
    driven.frame();
    assert_eq!(driven.state().dismissed, 1, "dismissal is reported once");
    // Escape dismisses too, but not when the dialog is not dismissible.
    driven.state_mut().open = true;
    driven.state_mut().dismissible = false;
    driven.frame();
    key(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(driven.state().open, "non-dismissible dialogs ignore Escape");
    assert_eq!(driven.state().dismissed, 1);
    driven.state_mut().dismissible = true;
    driven.frame();
    key(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(!driven.state().open);
    assert_eq!(driven.state().dismissed, 2);
    // A button inside the dialog closes it through its own message, which is
    // not a dismissal.
    driven.state_mut().open = true;
    driven.frame();
    let cancel = driven
        .context
        .find(&["$dialog:confirm", "$panel", "actions", "cancel"])
        .unwrap();
    assert!(driven.context.action(cancel, Action::Activate));
    driven.frame();
    assert!(!driven.state().open);
    assert_eq!(driven.state().dismissed, 2, "Cancel is not a dismissal");
}

#[test]
fn non_modal_dialogs_float_without_blocking_and_dialogs_stack_in_order() {
    let mut driven = Driven::new(
        ROOT,
        Dialogs {
            open: true,
            dismissible: true,
            ..Dialogs::default()
        },
        dialog_page,
    );
    let dialog = driven.context.find(&["$dialog:confirm"]).unwrap();
    assert!(!driven.context.node(dialog).unwrap().modal);
    let behind = driven.context.find(&["page", "behind"]).unwrap();
    assert_eq!(
        driven
            .context
            .hit_test(center(&driven.context, &["page", "behind"])),
        Some(behind)
    );
    driven.context.action(behind, Action::Focus);
    key(&mut driven.context, Key::Tab);
    assert_ne!(focused_key(&driven.context), "behind");
    // Tab from content walks into the floating dialog and out again.
    let keys: Vec<_> = (0..4)
        .map(|_| {
            key(&mut driven.context, Key::Tab);
            focused_key(&driven.context)
        })
        .collect();
    assert!(keys.contains(&"cancel".to_owned()) && keys.contains(&"behind".to_owned()));
}

#[derive(Default)]
struct Stack {
    outer: bool,
    inner: bool,
}

fn stacked(state: &Stack, ui: &mut Ui<'_, Handler<Stack>>) {
    ui.keyed("behind", |ui| ui.button("Behind"));
    ui.dialog(
        "outer",
        state.outer,
        on(|s: &mut Stack, open: bool| s.outer = open),
        DialogOptions::new("Outer"),
        |ui| {
            ui.keyed("outer-ok", |ui| ui.button("OK"));
            ui.dialog(
                "inner",
                state.inner,
                on(|s: &mut Stack, open: bool| s.inner = open),
                DialogOptions::new("Inner"),
                |ui| {
                    ui.keyed("inner-ok", |ui| ui.button("OK"));
                },
            );
        },
    );
}

#[test]
fn stacked_modals_keep_the_top_one_in_front_and_escape_closes_only_it() {
    let mut driven = Driven::new(
        ROOT,
        Stack {
            outer: true,
            inner: true,
        },
        stacked,
    );
    let root_children = driven.context.node(driven.context.root()).unwrap().children;
    let names: Vec<_> = root_children
        .iter()
        .map(|id| driven.context.details(*id).unwrap().key)
        .collect();
    assert_eq!(
        names,
        ["behind", "$dialog:outer", "$dialog:inner"],
        "inner paints last"
    );
    assert_eq!(focused_key(&driven.context), "inner-ok");
    key(&mut driven.context, Key::Tab);
    assert_eq!(
        focused_key(&driven.context),
        "inner-ok",
        "trapped in the top modal"
    );
    key(&mut driven.context, Key::Escape);
    driven.frame();
    assert!(driven.state().outer && !driven.state().inner);
    assert_eq!(
        focused_key(&driven.context),
        "outer-ok",
        "focus falls back into the remaining modal"
    );
}

#[derive(Default)]
struct Pad {
    volume: f64,
}

fn pad(state: &Pad, ui: &mut Ui<'_, Handler<Pad>>) {
    ui.keyed("pad", |ui| {
        ui.focus_scope(
            FocusScope {
                arrows: true,
                ..FocusScope::default()
            },
            Style::default().gap(10.0),
            |ui| {
                ui.keyed("top", |ui| {
                    ui.row(Style::default().gap(10.0), |ui| {
                        ui.keyed("nw", |ui| ui.button("NW"));
                        ui.keyed("ne", |ui| ui.button("NE"));
                    })
                });
                ui.keyed("bottom", |ui| {
                    ui.row(Style::default().gap(10.0), |ui| {
                        ui.keyed("sw", |ui| ui.button("SW"));
                        ui.keyed("se", |ui| ui.button("SE"));
                    })
                });
                ui.keyed("volume", |ui| {
                    ui.slider(
                        "Volume",
                        state.volume,
                        0.0..=100.0,
                        10.0,
                        on(|s: &mut Pad, value: f64| s.volume = value),
                    );
                });
            },
        )
    });
    ui.keyed("outside", |ui| ui.button("Outside"));
}

#[test]
fn arrow_navigation_moves_between_scope_children_by_geometry_but_leaves_arrow_controls_alone() {
    let mut driven = Driven::new(ROOT, Pad { volume: 50.0 }, pad);
    driven.context.action(
        driven.context.find(&["pad", "top", "nw"]).unwrap(),
        Action::Focus,
    );
    key(&mut driven.context, Key::ArrowRight);
    assert_eq!(focused_key(&driven.context), "ne");
    key(&mut driven.context, Key::ArrowDown);
    assert_eq!(focused_key(&driven.context), "se");
    key(&mut driven.context, Key::ArrowLeft);
    assert_eq!(focused_key(&driven.context), "sw");
    key(&mut driven.context, Key::ArrowUp);
    assert_eq!(focused_key(&driven.context), "nw");
    assert!(
        !driven.context.event(Event::KeyDown {
            key: Key::ArrowLeft,
            shift: false,
            repeat: false
        }),
        "nothing to the left"
    );
    assert_eq!(focused_key(&driven.context), "nw");
    // The slider keeps its arrows; focus stays and the value changes.
    let slider = driven.context.find(&["pad", "volume"]).unwrap();
    driven.context.action(slider, Action::Focus);
    key(&mut driven.context, Key::ArrowRight);
    driven.frame();
    assert_eq!(driven.state().volume, 60.0);
    assert_eq!(focused_key(&driven.context), "volume");
    // Outside any arrow scope, arrows do not move focus.
    driven
        .context
        .action(driven.context.find(&["outside"]).unwrap(), Action::Focus);
    key(&mut driven.context, Key::ArrowUp);
    assert_eq!(focused_key(&driven.context), "outside");
}

#[test]
#[should_panic(expected = "duplicate dialog key")]
fn duplicate_dialog_keys_are_rejected() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.dialog("same", true, |_: bool| (), DialogOptions::new("A"), |_| {});
        ui.dialog("same", true, |_: bool| (), DialogOptions::new("B"), |_| {});
    });
}

#[test]
fn a_focus_ring_never_floods_a_node_that_has_no_opaque_background() {
    use rust_gui_core::{Length, Style};
    use rust_gui_scene::{Color, Primitive, Rect};
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        let page = Style {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            padding: 40.0,
            ..Style::default()
        };
        ui.keyed("page", |ui| {
            ui.scroll(page, |ui| {
                ui.keyed("save", |ui| ui.button("Save"));
            })
        });
    };
    frame(&mut context, view);
    let focus = context.theme().focus;
    let focus_quads = |context: &Context| -> Vec<Rect> {
        context
            .scene()
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                Primitive::Quad { rect, color, .. } if *color == focus => Some(*rect),
                _ => None,
            })
            .collect()
    };

    // Clicking the empty page focuses the scroll view, which has no background.
    click(&mut context, Point { x: 300.0, y: 300.0 });
    frame(&mut context, view);
    assert_eq!(focused_key(&context), "page");
    let ring = focus_quads(&context);
    assert_eq!(ring.len(), 4, "four edges");
    let middle = Point { x: 300.0, y: 300.0 };
    assert!(
        ring.iter().all(|edge| !edge.contains(middle)),
        "the focus colour must not cover the page: {ring:?}"
    );
    assert!(ring.iter().all(|edge| edge.width.min(edge.height) <= 2.0));

    // An opaque control keeps the rounded ring: focus colour, background inside.
    let save = center(&context, &["page", "save"]);
    click(&mut context, save);
    frame(&mut context, view);
    assert_eq!(focused_key(&context), "save");
    let primitives = &context.scene().primitives;
    let outer = primitives
        .iter()
        .position(|primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == focus))
        .unwrap();
    let Primitive::Quad { rect: ring, .. } = primitives[outer] else {
        unreachable!()
    };
    let Primitive::Quad {
        rect: inside,
        color,
        ..
    } = primitives[outer + 1]
    else {
        panic!("the background follows the ring");
    };
    assert_eq!(color.3, 255);
    assert_ne!(color, Color::TRANSPARENT);
    assert_eq!(
        (inside.x, inside.width),
        (ring.x + 2.0, ring.width - 4.0),
        "inset by the ring's width"
    );
    assert_eq!(focus_quads(&context).len(), 1);
}
