mod support;

use glassmere_core::{
    Action, Button, Context, DialogOptions, Event, FocusRing, FocusScope, Key, Length,
    MonospaceMeasurer, Role, Style, Ui,
};
use glassmere_scene::{Color, Point, Primitive, Size};
use support::{Driven, act, center, click, on};

const ROOT: Size = Size::new(600.0, 400.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = glassmere_core::app::Action<S>;

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> glassmere_core::FrameStats {
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
    // Without `restore`, a vanished focus is cleared.
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
        DialogOptions::new("Delete file?")
            .dismissible(state.dismissible)
            .modal(state.modal),
        on(|s: &mut Dialogs, open: bool| {
            if !open {
                s.dismissed += 1;
            }
            s.open = open;
        }),
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
        DialogOptions::new("Outer"),
        on(|s: &mut Stack, open: bool| s.outer = open),
        |ui| {
            ui.keyed("outer-ok", |ui| ui.button("OK"));
            ui.dialog(
                "inner",
                state.inner,
                DialogOptions::new("Inner"),
                on(|s: &mut Stack, open: bool| s.inner = open),
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
        ui.dialog("same", true, DialogOptions::new("A"), |_: bool| (), |_| {});
        ui.dialog("same", true, DialogOptions::new("B"), |_: bool| (), |_| {});
    });
}

/// What the scene draws in the theme's focus colour: bars and stroked paths.
fn rings(context: &Context) -> Vec<Primitive> {
    let focus = context.theme().focus;
    ring_primitives(context, focus)
}

fn ring_primitives(context: &Context, focus: Color) -> Vec<Primitive> {
    context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| match primitive {
            Primitive::Quad { color, .. } => *color == focus,
            Primitive::Path {
                stroke: Some(stroke),
                ..
            } => stroke.color == focus,
            _ => false,
        })
        .cloned()
        .collect()
}

fn page_and_button(ui: &mut Ui<'_>) {
    let page = Style {
        width: Length::Percent(1.0),
        height: Length::Percent(1.0),
        padding: 40.0,
        ..Style::default()
    };
    ui.keyed("page", |ui| {
        ui.scroll(page, |ui| {
            ui.keyed("save", |ui| ui.button("Save"));
            // Taller than the window, so the page scrolls and can be focused.
            ui.keyed("tall", |ui| {
                ui.container(Style::default().height(Length::Px(900.0)), |_| {})
            });
        })
    });
}

#[test]
fn a_focus_ring_never_floods_a_node_that_has_no_opaque_background() {
    let mut context = Context::new();
    frame(&mut context, page_and_button);
    let page = context.find(&["page"]).unwrap();
    context.action(page, Action::Focus);
    frame(&mut context, page_and_button);
    assert_eq!(focused_key(&context), "page");
    let ring = rings(&context);
    assert_eq!(ring.len(), 4, "four edges");
    let middle = Point { x: 300.0, y: 300.0 };
    for edge in &ring {
        let Primitive::Quad { rect, .. } = edge else {
            panic!("a square node is ringed by bars, got {edge:?}");
        };
        assert!(!rect.contains(middle), "the ring must not cover the page");
        assert!(rect.width.min(rect.height) <= 2.0);
    }
}

#[test]
fn a_rounded_node_is_ringed_by_a_stroked_path_over_its_own_fill() {
    let mut context = Context::new();
    frame(&mut context, page_and_button);
    let save = context.find(&["page", "save"]).unwrap();
    context.action(save, Action::Focus);
    frame(&mut context, page_and_button);
    assert_eq!(focused_key(&context), "save");
    let ring = rings(&context);
    let [Primitive::Path { fill, stroke, path }] = ring.as_slice() else {
        panic!("one stroked path, got {ring:?}");
    };
    assert_eq!(*fill, None);
    assert_eq!(stroke.unwrap().width, 2.0);
    // The stroke lies inside the node's box, which is what is repainted.
    let bounds = context.node(save).unwrap().bounds;
    let outline = path.bounds();
    assert!(outline.x >= bounds.x && outline.y >= bounds.y);
    assert!(outline.x + outline.width <= bounds.x + bounds.width);
    assert!(outline.y + outline.height <= bounds.y + bounds.height);
}

#[test]
fn a_click_shows_no_ring_on_a_button_and_the_keyboard_does() {
    let mut context = Context::new();
    frame(&mut context, page_and_button);
    frame(&mut context, page_and_button);
    let save = center(&context, &["page", "save"]);
    click(&mut context, save);
    frame(&mut context, page_and_button);
    assert_eq!(focused_key(&context), "save");
    assert!(rings(&context).is_empty(), "focused by the mouse");

    // The next key that could move focus brings the ring, on the same node.
    key(&mut context, Key::Tab);
    frame(&mut context, page_and_button);
    assert!(!rings(&context).is_empty(), "the keyboard took over");

    // And a press takes it away again.
    let save = center(&context, &["page", "save"]);
    click(&mut context, save);
    frame(&mut context, page_and_button);
    assert!(rings(&context).is_empty());
}

#[derive(Default)]
struct Field {
    text: String,
}

#[test]
fn a_text_input_keeps_its_ring_after_a_click() {
    let mut driven = Driven::new(ROOT, Field::default(), |state, ui| {
        ui.keyed("name", |ui| {
            ui.text_input(
                "Name",
                &state.text,
                on(|s: &mut Field, text: String| s.text = text),
            );
        });
    });
    let name = center(&driven.context, &["name"]);
    click(&mut driven.context, name);
    driven.frame();
    assert_eq!(focused_key(&driven.context), "name");
    assert!(
        !rings(&driven.context).is_empty(),
        "typing shows where it will go, however focus got there"
    );
}

#[test]
fn a_node_can_set_its_ring_or_have_none() {
    let view = |ring: Option<FocusRing>| {
        move |ui: &mut Ui<'_>| {
            let style = Style {
                focus_ring: ring,
                radius: 8.0,
                ..Style::default()
            };
            ui.keyed("save", |ui| ui.button(Button::new("Save").style(style)));
        }
    };
    let pink = Color::rgb(255, 0, 128);
    let mut context = Context::new();
    frame(
        &mut context,
        view(Some(FocusRing::new(3.0).color(pink).inset(1.0))),
    );
    let save = context.find(&["save"]).unwrap();
    context.action(save, Action::Focus);
    frame(
        &mut context,
        view(Some(FocusRing::new(3.0).color(pink).inset(1.0))),
    );
    let own = ring_primitives(&context, pink);
    let [Primitive::Path { stroke, path, .. }] = own.as_slice() else {
        panic!("one ring in the node's own colour, got {own:?}");
    };
    assert_eq!(stroke.unwrap().width, 3.0);
    let bounds = context.node(save).unwrap().bounds;
    assert!(path.bounds().x >= bounds.x + 1.0, "inset from the edge");
    assert!(rings(&context).is_empty(), "the theme's colour is not used");

    frame(&mut context, view(Some(FocusRing::NONE)));
    assert!(ring_primitives(&context, pink).is_empty());
    assert!(rings(&context).is_empty());
    assert_eq!(focused_key(&context), "save", "it still has focus");
}

#[test]
fn a_focused_node_keeps_its_gradient() {
    let view = |ui: &mut Ui<'_>| {
        let style = Style::default()
            .background(Color::rgb(30, 30, 30))
            .gradient(glassmere_core::Gradient::linear(
                (0.0, 0.0),
                (0.0, 40.0),
                Color::rgb(255, 0, 0),
                Color::rgb(0, 0, 255),
            ));
        ui.keyed("save", |ui| ui.button(Button::new("Save").style(style)));
    };
    let mut context = Context::new();
    frame(&mut context, view);
    let save = context.find(&["save"]).unwrap();
    context.action(save, Action::Focus);
    frame(&mut context, view);
    let primitives = &context.scene().primitives;
    assert!(
        primitives
            .iter()
            .any(|primitive| matches!(primitive, Primitive::Gradient { .. })),
        "the ring is drawn over the gradient, not instead of it"
    );
    assert!(!rings(&context).is_empty());
}

/// A scroll container with a spacer and a button, and a plain pane beside it.
fn layout(tall: bool) -> impl Fn(&mut Ui<'_>) {
    move |ui| {
        ui.row(Style::default().gap(0.0), |ui| {
            ui.keyed("left", |ui| {
                let style = Style {
                    width: Length::Px(300.0),
                    height: Length::Px(300.0),
                    ..Style::default()
                };
                ui.scroll(style, |ui| {
                    ui.keyed("spacer", |ui| {
                        ui.container(Style::default().height(Length::Px(40.0)), |_| {})
                    });
                    ui.keyed("go", |ui| ui.button("Go"));
                    if tall {
                        ui.keyed("tall", |ui| {
                            ui.container(Style::default().height(Length::Px(900.0)), |_| {})
                        });
                    }
                })
            });
            ui.keyed("right", |ui| {
                let style = Style {
                    width: Length::Px(200.0),
                    height: Length::Px(300.0),
                    ..Style::default()
                };
                ui.container(style, |ui| ui.label("pane"))
            });
        });
    }
}

#[test]
fn layout_that_scrolls_nothing_takes_no_focus_from_a_click_or_from_tab() {
    let mut context = Context::new();
    let view = layout(false);
    frame(&mut context, &view);
    frame(&mut context, &view);
    // The empty part of the scroll container, its spacer and the pane.
    for at in [
        Point { x: 100.0, y: 250.0 },
        Point { x: 100.0, y: 10.0 },
        Point { x: 400.0, y: 100.0 },
    ] {
        click(&mut context, at);
        frame(&mut context, &view);
        assert_eq!(focused_key(&context), "none", "a click at {at:?}");
    }
    // Tab goes to the one thing there that is for the keyboard.
    key(&mut context, Key::Tab);
    frame(&mut context, &view);
    assert_eq!(focused_key(&context), "go");
    key(&mut context, Key::Tab);
    frame(&mut context, &view);
    assert_ne!(focused_key(&context), "left", "Tab does not stop on layout");
}

#[test]
fn a_container_that_scrolls_still_takes_focus_for_the_keyboard() {
    let mut context = Context::new();
    let view = layout(true);
    frame(&mut context, &view);
    frame(&mut context, &view);
    click(&mut context, Point { x: 100.0, y: 250.0 });
    frame(&mut context, &view);
    assert_eq!(focused_key(&context), "left", "arrow keys now scroll it");
    assert!(rings(&context).is_empty(), "and a click shows no ring");
}

#[derive(Default)]
struct Clicks {
    count: u32,
}

#[test]
fn a_node_that_opts_out_of_focus_still_reacts_to_the_pointer() {
    let mut driven = Driven::new(ROOT, Clicks::default(), |_, ui| {
        ui.keyed("before", |ui| ui.button("Before"));
        ui.keyed("quiet", |ui| {
            ui.button(
                Button::new("Quiet")
                    .style(Style::button().focusable(false))
                    .on_click(act(|s: &mut Clicks| s.count += 1)),
            )
        });
        ui.keyed("after", |ui| ui.button("After"));
    });
    let quiet = center(&driven.context, &["quiet"]);
    click(&mut driven.context, quiet);
    driven.frame();
    assert_eq!(driven.state().count, 1, "it was clicked");
    assert_eq!(focused_key(&driven.context), "none", "and took no focus");

    let before = driven.context.find(&["before"]).unwrap();
    driven.context.action(before, Action::Focus);
    driven.frame();
    key(&mut driven.context, Key::Tab);
    driven.frame();
    assert_eq!(focused_key(&driven.context), "after", "Tab skips it");
}
