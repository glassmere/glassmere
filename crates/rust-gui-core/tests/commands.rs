use rust_gui_core::{
    Action, Command, Context, DialogOptions, Event, FocusScope, Key, Length, MenuItem,
    MonospaceMeasurer, Shortcut, ShortcutKey, Style, Ui,
};
use rust_gui_scene::Size;

const ROOT: Size = Size::new(600.0, 500.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

/// Focuses a field and presses Enter in it.
fn submit(context: &mut Context, field: rust_gui_core::NodeId) {
    assert!(context.action(field, Action::Focus));
    context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
}

/// The node a screen reader would call `label`.
fn named(context: &Context, label: &str) -> rust_gui_core::NodeId {
    context
        .snapshots()
        .into_iter()
        .find(|node| node.semantics.label == label)
        .expect("a node with that name")
        .id
}

fn labels(context: &Context) -> Vec<String> {
    context
        .snapshots()
        .into_iter()
        .map(|node| node.semantics.label)
        .filter(|label| !label.is_empty())
        .collect()
}

#[test]
fn a_shortcut_is_written_the_way_a_menu_shows_it() {
    assert_eq!(
        Shortcut {
            key: ShortcutKey::Character('s'),
            control: true,
            alt: false,
            shift: true,
            super_key: false,
        }
        .describe(),
        "Ctrl+Shift+S"
    );
    assert_eq!(
        Shortcut {
            key: ShortcutKey::Named(Key::Delete),
            control: false,
            alt: true,
            shift: false,
            super_key: false,
        }
        .describe(),
        "Alt+Delete"
    );
    let item = MenuItem::command(&Command::new("Save", Some(Shortcut::primary('s'))));
    assert_eq!((item.label.as_str(), item.enabled), ("Save", true));
    assert!(item.hint.ends_with('S') && !item.hint.is_empty());
    assert!(MenuItem::new("Plain").hint.is_empty());
}

#[test]
fn a_scoped_command_fires_only_where_it_belongs_and_the_inner_one_wins() {
    let mut context = Context::new();
    // The same keys in the window and in a panel, and a third that belongs to
    // the panel alone.
    let window = Command::with_id("window.save", "Save", Some(Shortcut::primary('s')));
    let panel = Command::with_id("panel.save", "Save here", Some(Shortcut::primary('s')));
    let only = Command::with_id("panel.only", "Panel only", Some(Shortcut::primary('k'))).scoped();
    let mut fired = Vec::new();
    let declare = |context: &mut Context, fired: &mut Vec<&'static str>| {
        frame(context, |ui| {
            if ui.command_button("window", &window).clicked() {
                fired.push("window");
            }
            ui.focus_scope("panel", FocusScope::default(), Style::default(), |ui| {
                ui.button_keyed("inside", "Inside");
                if ui.command_button("panel", &panel).clicked() {
                    fired.push("panel");
                }
                if ui.command_button("only", &only).clicked() {
                    fired.push("only");
                }
            });
        });
    };
    declare(&mut context, &mut fired);

    // Focus outside the panel: the window's command takes it, and the panel's
    // own one is not reachable at all.
    assert!(context.event(Event::Shortcut(Shortcut::primary('s'))));
    declare(&mut context, &mut fired);
    assert_eq!(fired, ["window"]);
    assert!(!context.event(Event::Shortcut(Shortcut::primary('k'))));
    declare(&mut context, &mut fired);
    assert_eq!(fired, ["window"]);

    // Focus inside it: the panel's command wins the shared keys.
    let inside = context.find(&["panel", "inside"]).unwrap();
    assert!(context.action(inside, Action::Focus));
    assert!(context.event(Event::Shortcut(Shortcut::primary('s'))));
    declare(&mut context, &mut fired);
    assert_eq!(fired, ["window", "panel"]);
    assert!(context.event(Event::Shortcut(Shortcut::primary('k'))));
    declare(&mut context, &mut fired);
    assert_eq!(fired, ["window", "panel", "only"]);
}

#[test]
fn the_user_can_rebind_and_unbind_a_command() {
    let mut context = Context::new();
    let save = Command::with_id("save", "Save", Some(Shortcut::primary('s')));
    let mut fired = 0;
    let mut shown = String::new();
    let declare = |context: &mut Context, fired: &mut usize, shown: &mut String| {
        frame(context, |ui| {
            *shown = MenuItem::command(&ui.command(&save)).hint;
            if ui.command_button("save", &save).clicked() {
                *fired += 1;
            }
        });
    };
    declare(&mut context, &mut fired, &mut shown);
    assert!(shown.ends_with('S'));

    context.set_binding("save", Some(Shortcut::primary('e')));
    declare(&mut context, &mut fired, &mut shown);
    assert!(shown.ends_with('E'), "the menu shows the new keys: {shown}");
    assert!(!context.event(Event::Shortcut(Shortcut::primary('s'))));
    assert!(context.event(Event::Shortcut(Shortcut::primary('e'))));
    declare(&mut context, &mut fired, &mut shown);
    assert_eq!(fired, 1);

    // Unbound: no keys reach it, and the menu says nothing.
    context.set_binding("save", None);
    declare(&mut context, &mut fired, &mut shown);
    assert_eq!(shown, "");
    assert!(!context.event(Event::Shortcut(Shortcut::primary('e'))));
    context.clear_bindings();
    declare(&mut context, &mut fired, &mut shown);
    assert!(context.event(Event::Shortcut(Shortcut::primary('s'))));
    declare(&mut context, &mut fired, &mut shown);
    assert_eq!(fired, 2);
}

#[test]
fn the_palette_filters_commands_and_gives_back_the_one_that_was_chosen() {
    let mut context = Context::new();
    let mut disabled = Command::with_id("close", "Close window", None);
    disabled.enabled = false;
    let commands = [
        Command::with_id("save", "Save file", Some(Shortcut::primary('s'))),
        Command::with_id("open", "Open file", Some(Shortcut::primary('o'))),
        disabled,
    ];
    let (mut open, mut query, mut chosen) = (true, String::new(), None);
    let declare = |context: &mut Context,
                   open: &mut bool,
                   query: &mut String,
                   chosen: &mut Option<String>| {
        frame(context, |ui| {
            if let Some(id) = ui.command_palette("palette", open, query, &commands) {
                *chosen = Some(id);
            }
        });
    };
    declare(&mut context, &mut open, &mut query, &mut chosen);
    let shown = labels(&context);
    // Every command, each with the keys that would run it.
    assert!(shown.iter().any(|label| label == "Save file, Ctrl+S"));
    assert!(shown.iter().any(|label| label == "Close window"));
    // The typing field has the focus the moment it opens.
    let field = named(&context, "Search commands");
    declare(&mut context, &mut open, &mut query, &mut chosen);
    assert_eq!(context.focused(), Some(field));

    // What is typed filters the list.
    query.push_str("ope");
    declare(&mut context, &mut open, &mut query, &mut chosen);
    let shown = labels(&context);
    assert!(shown.iter().any(|label| label.starts_with("Open file")));
    assert!(!shown.iter().any(|label| label.starts_with("Save file")));

    // Enter takes the first one that can run, closes the palette and clears
    // what was typed, and the choice is reported once.
    submit(&mut context, field);
    declare(&mut context, &mut open, &mut query, &mut chosen);
    assert_eq!(chosen.as_deref(), Some("open"));
    assert!(!open && query.is_empty());
    chosen = None;
    declare(&mut context, &mut open, &mut query, &mut chosen);
    assert_eq!(chosen, None);
    assert!(!labels(&context).iter().any(|label| label == "Commands"));

    // A command that cannot run cannot be chosen.
    open = true;
    query.push_str("close");
    declare(&mut context, &mut open, &mut query, &mut chosen);
    let field = named(&context, "Search commands");
    submit(&mut context, field);
    declare(&mut context, &mut open, &mut query, &mut chosen);
    assert_eq!(chosen, None);
    assert!(open);
}

#[test]
fn a_menu_item_writes_its_shortcut_along_the_far_edge() {
    let mut context = Context::new();
    let items = [
        MenuItem::command(&Command::new("Save", Some(Shortcut::primary('s')))),
        MenuItem::new("Plain"),
    ];
    let declare = |context: &mut Context| {
        frame(context, |ui| {
            ui.context_menu(
                "area",
                &items,
                Style::default()
                    .width(Length::Px(300.0))
                    .height(Length::Px(120.0)),
                |ui| ui.label_keyed("text", "Right-click me"),
            );
        });
    };
    declare(&mut context);
    context.event(Event::ContextMenu(rust_gui_scene::Point {
        x: 40.0,
        y: 40.0,
    }));
    declare(&mut context);
    let item = context.find(&["area", "$popup", "0"]).unwrap();
    let bounds = context.node(item).unwrap().bounds;
    // The hint is its own text, drawn inside the item and along its end edge.
    let hint = context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            rust_gui_scene::Primitive::StyledText { rect, text, .. } if text.ends_with('S') => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the shortcut is painted");
    assert!(hint.x > bounds.x + bounds.width / 2.0, "{hint:?}");
    assert!(hint.x + hint.width <= bounds.x + bounds.width + 0.5);
    assert!(hint.y >= bounds.y && hint.y + hint.height <= bounds.y + bounds.height + 0.5);
    // The screen reader hears it as part of the item.
    assert!(
        labels(&context)
            .iter()
            .any(|label| label.contains("Save") && label.contains("Ctrl")),
        "{:?}",
        labels(&context)
    );
}

#[test]
fn a_dialog_and_the_window_may_bind_the_same_keys() {
    let mut context = Context::new();
    let window = Command::with_id("window", "Save", Some(Shortcut::primary('s')));
    let inner = Command::with_id("dialog", "Save here", Some(Shortcut::primary('s')));
    let (mut open, mut fired) = (true, Vec::new());
    let declare = |context: &mut Context, open: &mut bool, fired: &mut Vec<&'static str>| {
        frame(context, |ui| {
            if ui.command_button("window", &window).clicked() {
                fired.push("window");
            }
            ui.dialog("editor", open, DialogOptions::new("Editor"), |ui, _| {
                if ui.command_button("inner", &inner).clicked() {
                    fired.push("dialog");
                }
            });
        });
    };
    // Two commands share the keys without a duplicate, because a dialog is a
    // scope of its own, and the modal dialog holds the focus.
    declare(&mut context, &mut open, &mut fired);
    declare(&mut context, &mut open, &mut fired);
    assert!(context.event(Event::Shortcut(Shortcut::primary('s'))));
    declare(&mut context, &mut open, &mut fired);
    assert_eq!(fired, ["dialog"]);
}
