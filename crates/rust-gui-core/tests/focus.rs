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
        ui.column("page", Style::default(), |ui| {
            ui.button_keyed("before", "Before");
            ui.focus_scope("group", FocusScope::default(), Style::default(), |ui| {
                ui.button_keyed("g1", "One");
                ui.button_keyed("g2", "Two");
                ui.button_keyed("g3", "Three");
            });
            ui.button_keyed("after", "After");
            ui.focus_scope(
                "trap",
                FocusScope {
                    trap: true,
                    ..FocusScope::default()
                },
                Style::default(),
                |ui| {
                    ui.button_keyed("t1", "T1");
                    ui.button_keyed("t2", "T2");
                },
            );
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
            ui.column("page", Style::default(), |ui| {
                ui.button_keyed("opener", "Open");
                if show {
                    ui.focus_scope(
                        "panel",
                        FocusScope {
                            restore: true,
                            ..FocusScope::default()
                        },
                        Style::default(),
                        |ui| {
                            ui.button_keyed("inside", "Inside");
                        },
                    );
                }
                ui.button_keyed("other", "Other");
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
            ui.button_keyed("opener", "Open");
            if show {
                ui.focus_scope("panel", FocusScope::default(), Style::default(), |ui| {
                    ui.button_keyed("inside", "Inside");
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
        ui.column("page", Style::default(), |ui| {
            ui.button_keyed("behind", "Behind");
            ui.scroll(
                "list",
                Style::default()
                    .width(Length::Px(200.0))
                    .height(Length::Px(60.0)),
                |ui| {
                    for index in 0..20 {
                        ui.label_keyed(format!("row-{index}"), format!("Row {index}"));
                    }
                },
            );
        });
        let response = ui.dialog(
            "confirm",
            open,
            DialogOptions::new("Delete file?")
                .dismissible(dismissible)
                .modal(modal),
            |ui, open| {
                ui.label_keyed("question", "This cannot be undone.");
                ui.row("actions", Style::default(), |ui| {
                    if ui.button_keyed("cancel", "Cancel").clicked() {
                        *open = false;
                    }
                    ui.button_keyed("delete", "Delete");
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
    assert!(!context.event(Event::Scroll { delta_y: 40.0 }));
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
            ui.button_keyed("behind", "Behind");
            ui.dialog("outer", outer, DialogOptions::new("Outer"), |ui, _| {
                ui.button_keyed("outer-ok", "OK");
                ui.dialog("inner", inner, DialogOptions::new("Inner"), |ui, _| {
                    ui.button_keyed("inner-ok", "OK");
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
        ui.focus_scope(
            "pad",
            FocusScope {
                arrows: true,
                ..FocusScope::default()
            },
            Style::default().gap(10.0),
            |ui| {
                ui.row("top", Style::default().gap(10.0), |ui| {
                    ui.button_keyed("nw", "NW");
                    ui.button_keyed("ne", "NE");
                });
                ui.row("bottom", Style::default().gap(10.0), |ui| {
                    ui.button_keyed("sw", "SW");
                    ui.button_keyed("se", "SE");
                });
                ui.slider("volume", "Volume", &mut volume, 0.0..=100.0, 10.0);
            },
        );
        ui.button_keyed("outside", "Outside");
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
