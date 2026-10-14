use rust_gui_core::{
    Action, Context, DialogOptions, Event, FocusScope, Key, Length, MonospaceMeasurer, Role, Style,
    Ui,
};
use rust_gui_scene::{Point, Primitive, Size};

const ROOT: Size = Size::new(600.0, 400.0);

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

fn dialog_view<'a>(
    open: &'a mut bool,
    dismissed: &'a mut bool,
    dismissible: bool,
    modal: bool,
) -> impl FnMut(&mut Ui<'_>) + 'a {
    move |ui: &mut Ui<'_>| {
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
        let response = ui.dialog(
            "confirm",
            open,
            DialogOptions::new("Delete file?")
                .dismissible(dismissible)
                .modal(modal),
            |ui, open| {
                ui.keyed("question", |ui| ui.label("This cannot be undone."));
                ui.keyed("actions", |ui| {
                    ui.row(Style::default(), |ui| {
                        if ui.keyed("cancel", |ui| ui.button("Cancel")).clicked() {
                            *open = false;
                        }
                        ui.keyed("delete", |ui| ui.button("Delete"));
                    })
                });
            },
        );
        *dismissed = response.dismissed;
    }
}

#[test]
fn modal_dialogs_take_focus_trap_tab_block_pointer_and_wheel_and_restore_on_close() {
    let mut context = Context::new();
    let mut open = false;
    let mut dismissed = false;
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    let behind = context.find(&["page", "behind"]).unwrap();
    context.action(behind, Action::Focus);
    open = true;
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    let dialog = context.find(&["$dialog:confirm"]).unwrap();
    let snapshot = context.node(dialog).unwrap();
    assert_eq!(snapshot.semantics.role, Role::Dialog);
    assert_eq!(snapshot.semantics.label, "Delete file?");
    assert!(snapshot.modal);
    assert_eq!(
        snapshot.bounds.width, ROOT.width,
        "the layer covers the window"
    );
    let panel = context
        .node(context.find(&["$dialog:confirm", "$panel"]).unwrap())
        .unwrap();
    assert!(
        (panel.bounds.x + panel.bounds.width / 2.0 - ROOT.width / 2.0).abs() < 1.0,
        "centered"
    );
    assert_eq!(
        focused_key(&context),
        "cancel",
        "initial focus lands on the first control"
    );
    key(&mut context, Key::Tab);
    assert_eq!(focused_key(&context), "delete");
    key(&mut context, Key::Tab);
    assert_eq!(
        focused_key(&context),
        "cancel",
        "Tab is trapped inside the dialog"
    );
    // The scrim wins hit testing over content behind it, and wheel input is blocked.
    let behind_point = center(&context, &["page", "behind"]);
    assert_eq!(context.hit_test(behind_point), Some(dialog));
    let scrim_quads = context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Quad { rect, .. } if rect.width == ROOT.width && rect.height == ROOT.height))
        .count();
    assert!(scrim_quads >= 1, "a scrim quad covers the window");
    let list = context.find(&["page", "list"]).unwrap();
    context.event(Event::PointerMoved(center(&context, &["page", "list"])));
    assert!(!context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 40.0
    }));
    assert_eq!(context.node(list).unwrap().scroll.unwrap().offset, 0.0);
    // Clicking the scrim dismisses; the flag clears and the response reports it once.
    click(&mut context, behind_point);
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    assert!(dismissed && !open);
    assert_eq!(
        focused_key(&context),
        "behind",
        "focus returns to the opener"
    );
    assert!(context.find(&["$dialog:confirm"]).is_none());
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    assert!(!dismissed, "dismissal is reported once");
    // Escape dismisses too, but not when the dialog is not dismissible.
    open = true;
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, false, true),
    );
    key(&mut context, Key::Escape);
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, false, true),
    );
    assert!(open, "non-dismissible dialogs ignore Escape");
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    key(&mut context, Key::Escape);
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    assert!(!open);
    // A button inside the dialog can close it through the flag it receives.
    open = true;
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    let cancel = context
        .find(&["$dialog:confirm", "$panel", "actions", "cancel"])
        .unwrap();
    assert!(context.action(cancel, Action::Activate));
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, true),
    );
    assert!(!open);
}

#[test]
fn non_modal_dialogs_float_without_blocking_and_dialogs_stack_in_order() {
    let mut context = Context::new();
    let mut open = true;
    let mut dismissed = false;
    frame(
        &mut context,
        dialog_view(&mut open, &mut dismissed, true, false),
    );
    let dialog = context.find(&["$dialog:confirm"]).unwrap();
    assert!(!context.node(dialog).unwrap().modal);
    let behind = context.find(&["page", "behind"]).unwrap();
    assert_eq!(
        context.hit_test(center(&context, &["page", "behind"])),
        Some(behind)
    );
    context.action(behind, Action::Focus);
    key(&mut context, Key::Tab);
    assert_ne!(focused_key(&context), "behind");
    // Tab from content walks into the floating dialog and out again.
    let keys: Vec<_> = (0..4)
        .map(|_| {
            key(&mut context, Key::Tab);
            focused_key(&context)
        })
        .collect();
    assert!(keys.contains(&"cancel".to_owned()) && keys.contains(&"behind".to_owned()));

    // Stacked modals: the inner one is on top, Escape closes only it.
    let mut context = Context::new();
    let mut outer = true;
    let mut inner = true;
    let stacked = |context: &mut Context, outer: &mut bool, inner: &mut bool| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            ui.keyed("behind", |ui| ui.button("Behind"));
            ui.dialog("outer", outer, DialogOptions::new("Outer"), |ui, _| {
                ui.keyed("outer-ok", |ui| ui.button("OK"));
                ui.dialog("inner", inner, DialogOptions::new("Inner"), |ui, _| {
                    ui.keyed("inner-ok", |ui| ui.button("OK"));
                });
            });
        });
    };
    stacked(&mut context, &mut outer, &mut inner);
    let root_children = context.node(context.root()).unwrap().children;
    let names: Vec<_> = root_children
        .iter()
        .map(|id| context.details(*id).unwrap().key)
        .collect();
    assert_eq!(
        names,
        ["behind", "$dialog:outer", "$dialog:inner"],
        "inner paints last"
    );
    assert_eq!(focused_key(&context), "inner-ok");
    key(&mut context, Key::Tab);
    assert_eq!(
        focused_key(&context),
        "inner-ok",
        "trapped in the top modal"
    );
    key(&mut context, Key::Escape);
    stacked(&mut context, &mut outer, &mut inner);
    assert!(outer && !inner);
    assert_eq!(
        focused_key(&context),
        "outer-ok",
        "focus falls back into the remaining modal"
    );
}

#[test]
fn arrow_navigation_moves_between_scope_children_by_geometry_but_leaves_arrow_controls_alone() {
    let mut context = Context::new();
    let mut volume = 50.0;
    let mut view = |ui: &mut Ui<'_>| {
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
                        ui.slider("Volume", &mut volume, 0.0..=100.0, 10.0)
                    });
                },
            )
        });
        ui.keyed("outside", |ui| ui.button("Outside"));
    };
    frame(&mut context, &mut view);
    context.action(context.find(&["pad", "top", "nw"]).unwrap(), Action::Focus);
    key(&mut context, Key::ArrowRight);
    assert_eq!(focused_key(&context), "ne");
    key(&mut context, Key::ArrowDown);
    assert_eq!(focused_key(&context), "se");
    key(&mut context, Key::ArrowLeft);
    assert_eq!(focused_key(&context), "sw");
    key(&mut context, Key::ArrowUp);
    assert_eq!(focused_key(&context), "nw");
    assert!(
        !context.event(Event::KeyDown {
            key: Key::ArrowLeft,
            shift: false,
            repeat: false
        }),
        "nothing to the left"
    );
    assert_eq!(focused_key(&context), "nw");
    // The slider keeps its arrows; focus stays and the value changes.
    let slider = context.find(&["pad", "volume"]).unwrap();
    context.action(slider, Action::Focus);
    key(&mut context, Key::ArrowRight);
    frame(&mut context, &mut view);
    assert_eq!(volume, 60.0);
    assert_eq!(focused_key(&context), "volume");
    // Outside any arrow scope, arrows do not move focus.
    context.action(context.find(&["outside"]).unwrap(), Action::Focus);
    key(&mut context, Key::ArrowUp);
    assert_eq!(focused_key(&context), "outside");
}

#[test]
#[should_panic(expected = "duplicate dialog key")]
fn duplicate_dialog_keys_are_rejected() {
    let mut context = Context::new();
    let mut a = true;
    let mut b = true;
    frame(&mut context, |ui| {
        ui.dialog("same", &mut a, DialogOptions::new("A"), |_, _| {});
        ui.dialog("same", &mut b, DialogOptions::new("B"), |_, _| {});
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
