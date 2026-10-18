mod support;

use glassmere_core::{
    Action, Command, CommandPalette, Context, DialogOptions, Event, FocusScope, Key, Length,
    MenuItem, MonospaceMeasurer, Shortcut, ShortcutKey, Style, Ui,
};
use glassmere_scene::Size;
use support::{Driven, act, on};

const ROOT: Size = Size::new(600.0, 500.0);

/// The message type of a `Driven` view over state `S`.
type Handler<S> = glassmere_core::app::Action<S>;

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

/// Focuses a field and presses Enter in it.
fn submit(context: &mut Context, field: glassmere_core::NodeId) {
    assert!(context.action(field, Action::Focus));
    context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
}

/// The node a screen reader would call `label`.
fn named(context: &Context, label: &str) -> glassmere_core::NodeId {
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

#[derive(Default)]
struct Scopes {
    fired: Vec<&'static str>,
}

/// The same keys in the window and in a panel, and a third that belongs to
/// the panel alone.
fn scopes_page(_: &Scopes, ui: &mut Ui<'_, Handler<Scopes>>) {
    let window = Command::with_id("window.save", "Save", Some(Shortcut::primary('s')));
    let panel = Command::with_id("panel.save", "Save here", Some(Shortcut::primary('s')));
    let only = Command::with_id("panel.only", "Panel only", Some(Shortcut::primary('k'))).scoped();
    ui.keyed("window", |ui| {
        ui.command_button(&window, act(|s: &mut Scopes| s.fired.push("window")));
    });
    ui.keyed("panel", |ui| {
        ui.focus_scope(FocusScope::default(), Style::default(), |ui| {
            ui.keyed("inside", |ui| ui.button("Inside"));
            ui.keyed("panel", |ui| {
                ui.command_button(&panel, act(|s: &mut Scopes| s.fired.push("panel")));
            });
            ui.keyed("only", |ui| {
                ui.command_button(&only, act(|s: &mut Scopes| s.fired.push("only")));
            });
        })
    });
}

#[test]
fn a_scoped_command_fires_only_where_it_belongs_and_the_inner_one_wins() {
    let mut driven = Driven::new(ROOT, Scopes::default(), scopes_page);

    // Focus outside the panel: the window's command takes it, and the panel's
    // own one is not reachable at all.
    assert!(
        driven
            .context
            .event(Event::Shortcut(Shortcut::primary('s')))
    );
    driven.frame();
    assert_eq!(driven.state().fired, ["window"]);
    assert!(
        !driven
            .context
            .event(Event::Shortcut(Shortcut::primary('k')))
    );
    driven.frame();
    assert_eq!(driven.state().fired, ["window"]);

    // Focus inside it: the panel's command wins the shared keys.
    let inside = driven.context.find(&["panel", "inside"]).unwrap();
    assert!(driven.context.action(inside, Action::Focus));
    assert!(
        driven
            .context
            .event(Event::Shortcut(Shortcut::primary('s')))
    );
    driven.frame();
    assert_eq!(driven.state().fired, ["window", "panel"]);
    assert!(
        driven
            .context
            .event(Event::Shortcut(Shortcut::primary('k')))
    );
    driven.frame();
    assert_eq!(driven.state().fired, ["window", "panel", "only"]);
}

#[derive(Default)]
struct Rebind {
    fired: usize,
}

fn rebind_page(_: &Rebind, ui: &mut Ui<'_, Handler<Rebind>>) {
    let save = Command::with_id("save", "Save", Some(Shortcut::primary('s')));
    ui.keyed("save", |ui| {
        ui.command_button(&save, act(|s: &mut Rebind| s.fired += 1));
    });
}

#[test]
fn the_user_can_rebind_and_unbind_a_command() {
    let mut driven = Driven::new(ROOT, Rebind::default(), rebind_page);
    assert_eq!(
        driven.context.binding("save"),
        None,
        "the default keys come from the command itself"
    );

    driven
        .context
        .set_binding("save", Some(Shortcut::primary('e')));
    driven.frame();
    assert_eq!(
        driven.context.binding("save"),
        Some(&Some(Shortcut::primary('e')))
    );
    assert!(
        !driven
            .context
            .event(Event::Shortcut(Shortcut::primary('s')))
    );
    assert!(
        driven
            .context
            .event(Event::Shortcut(Shortcut::primary('e')))
    );
    driven.frame();
    assert_eq!(driven.state().fired, 1);

    // Unbound: no keys reach it.
    driven.context.set_binding("save", None);
    driven.frame();
    assert_eq!(driven.context.binding("save"), Some(&None));
    assert!(
        !driven
            .context
            .event(Event::Shortcut(Shortcut::primary('e')))
    );
    driven.context.clear_bindings();
    driven.frame();
    assert!(
        driven
            .context
            .event(Event::Shortcut(Shortcut::primary('s')))
    );
    driven.frame();
    assert_eq!(driven.state().fired, 2);
}

#[derive(Default)]
struct Palette {
    open: bool,
    query: String,
    picks: Vec<usize>,
}

/// The palette's commands are `save`, `open` and a disabled `close`; a pick
/// is the index of the command, and it closes the palette and clears the query.
fn palette_page(state: &Palette, ui: &mut Ui<'_, Handler<Palette>>) {
    let mut disabled = Command::with_id("close", "Close window", None);
    disabled.enabled = false;
    let commands = [
        Command::with_id("save", "Save file", Some(Shortcut::primary('s'))),
        Command::with_id("open", "Open file", Some(Shortcut::primary('o'))),
        disabled,
    ];
    ui.keyed("palette", |ui| {
        ui.command_palette(
            state.open,
            &state.query,
            &commands,
            CommandPalette::new(on(|s: &mut Palette, query: String| s.query = query))
                .on_open(on(|s: &mut Palette, open: bool| s.open = open))
                .on_pick(on(|s: &mut Palette, index: usize| {
                    s.picks.push(index);
                    s.open = false;
                    s.query.clear();
                })),
        );
    });
}

#[test]
fn the_palette_filters_commands_and_gives_back_the_one_that_was_chosen() {
    let mut driven = Driven::new(
        ROOT,
        Palette {
            open: true,
            ..Palette::default()
        },
        palette_page,
    );
    let shown = labels(&driven.context);
    // Every command, each with the keys that would run it.
    let save = format!("Save file, {}", Shortcut::primary('s').describe());
    assert!(shown.contains(&save));
    assert!(shown.iter().any(|label| label == "Close window"));
    // The typing field has the focus the moment it opens.
    let field = named(&driven.context, "Search commands");
    driven.frame();
    assert_eq!(driven.context.focused(), Some(field));

    // What is typed filters the list.
    driven.state_mut().query.push_str("ope");
    driven.frame();
    let shown = labels(&driven.context);
    assert!(shown.iter().any(|label| label.starts_with("Open file")));
    assert!(!shown.iter().any(|label| label.starts_with("Save file")));

    // Enter takes the first one that can run, closes the palette and clears
    // what was typed, and the choice is reported once.
    submit(&mut driven.context, field);
    driven.frame();
    assert_eq!(driven.state().picks, [1]);
    assert!(!driven.state().open && driven.state().query.is_empty());
    driven.frame();
    assert_eq!(driven.state().picks, [1], "reported once");
    assert!(
        !labels(&driven.context)
            .iter()
            .any(|label| label == "Commands")
    );

    // A command that cannot run cannot be chosen.
    driven.state_mut().open = true;
    driven.state_mut().query.push_str("close");
    driven.frame();
    let field = named(&driven.context, "Search commands");
    submit(&mut driven.context, field);
    driven.frame();
    assert_eq!(driven.state().picks, [1]);
    assert!(driven.state().open);
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
            ui.keyed("area", |ui| {
                ui.context_menu(
                    &items,
                    Style::default()
                        .width(Length::Px(300.0))
                        .height(Length::Px(120.0)),
                    |_: usize| (),
                    |ui| {
                        ui.keyed("text", |ui| ui.label("Right-click me"));
                    },
                );
            });
        });
    };
    declare(&mut context);
    context.event(Event::ContextMenu(glassmere_scene::Point {
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
            glassmere_scene::Primitive::StyledText { rect, text, .. } if text.ends_with('S') => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the shortcut is painted");
    assert!(hint.x > bounds.x + bounds.width / 2.0, "{hint:?}");
    assert!(hint.x + hint.width <= bounds.x + bounds.width + 0.5);
    assert!(hint.y >= bounds.y && hint.y + hint.height <= bounds.y + bounds.height + 0.5);
    // The screen reader hears it as part of the item.
    let save = format!("Save, {}", Shortcut::primary('s').describe());
    assert!(labels(&context).contains(&save), "{:?}", labels(&context));
}

#[derive(Default)]
struct Dialogs {
    open: bool,
    fired: Vec<&'static str>,
}

fn dialog_window(state: &Dialogs, ui: &mut Ui<'_, Handler<Dialogs>>) {
    let window = Command::with_id("window", "Save", Some(Shortcut::primary('s')));
    let inner = Command::with_id("dialog", "Save here", Some(Shortcut::primary('s')));
    ui.keyed("window", |ui| {
        ui.command_button(&window, act(|s: &mut Dialogs| s.fired.push("window")));
    });
    ui.dialog(
        "editor",
        state.open,
        DialogOptions::new("Editor"),
        on(|s: &mut Dialogs, open: bool| s.open = open),
        |ui| {
            ui.keyed("inner", |ui| {
                ui.command_button(&inner, act(|s: &mut Dialogs| s.fired.push("dialog")));
            });
        },
    );
}

#[test]
fn a_dialog_and_the_window_may_bind_the_same_keys() {
    let mut driven = Driven::new(
        ROOT,
        Dialogs {
            open: true,
            ..Dialogs::default()
        },
        dialog_window,
    );
    // Two commands share the keys without a duplicate, because a dialog is a
    // scope of its own, and the modal dialog holds the focus.
    driven.frame();
    driven.frame();
    assert!(
        driven
            .context
            .event(Event::Shortcut(Shortcut::primary('s')))
    );
    driven.frame();
    assert_eq!(driven.state().fired, ["dialog"]);
}
