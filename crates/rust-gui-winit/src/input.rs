use crate::Application;
use rust_gui_core::{Action, Context, Event, Key, NodeId, TextEdit, TextMove, Ui};
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{Key as WinitKey, ModifiersState, NamedKey},
};

enum Command {
    Input(Event),
    Copy,
    Cut,
    Paste,
}

pub(super) fn window_focus(
    context: &mut Context,
    restore: &mut Option<NodeId>,
    focused: bool,
) -> bool {
    if focused {
        restore
            .take()
            .is_some_and(|id| context.focused().is_none() && context.action(id, Action::Focus))
    } else {
        // Save the owner, not an ephemeral popup item that is about to disappear.
        *restore = context
            .focused()
            .and_then(|id| context.node(id))
            .map(|node| {
                if matches!(
                    node.semantics.role,
                    rust_gui_core::Role::Option | rust_gui_core::Role::MenuItem
                ) {
                    node.parent
                        .and_then(|id| context.node(id))
                        .and_then(|panel| panel.parent)
                        .unwrap_or(node.id)
                } else {
                    node.id
                }
            });
        context.event(Event::FocusLost)
    }
}

fn named(key: &WinitKey) -> Option<Key> {
    Some(match key {
        WinitKey::Named(NamedKey::Tab) => Key::Tab,
        WinitKey::Named(NamedKey::Enter) => Key::Enter,
        WinitKey::Named(NamedKey::Space) => Key::Space,
        WinitKey::Named(NamedKey::Escape) => Key::Escape,
        WinitKey::Named(NamedKey::ArrowUp) => Key::ArrowUp,
        WinitKey::Named(NamedKey::ArrowDown) => Key::ArrowDown,
        WinitKey::Named(NamedKey::ArrowLeft) => Key::ArrowLeft,
        WinitKey::Named(NamedKey::ArrowRight) => Key::ArrowRight,
        WinitKey::Named(NamedKey::PageUp) => Key::PageUp,
        WinitKey::Named(NamedKey::PageDown) => Key::PageDown,
        WinitKey::Named(NamedKey::Home) => Key::Home,
        WinitKey::Named(NamedKey::End) => Key::End,
        WinitKey::Named(NamedKey::Backspace) => Key::Backspace,
        WinitKey::Named(NamedKey::Delete) => Key::Delete,
        WinitKey::Named(NamedKey::ContextMenu) => Key::ContextMenu,
        _ => return None,
    })
}

fn pressed(
    key: &WinitKey,
    text: Option<&str>,
    modifiers: ModifiersState,
    repeat: bool,
    editing: bool,
    composing: bool,
) -> Option<Command> {
    let command = if cfg!(target_os = "macos") {
        modifiers.super_key()
    } else {
        modifiers.control_key()
    };
    let shift = modifiers.shift_key();
    if editing {
        // The IME owns character and navigation keys until it commits/cancels.
        if composing && !matches!(key, WinitKey::Named(NamedKey::Escape | NamedKey::Tab)) {
            return None;
        }
        if command
            && !modifiers.alt_key()
            && let WinitKey::Character(character) = key
        {
            return Some(match character.to_lowercase().as_str() {
                "a" => Command::Input(Event::Text(TextEdit::SelectAll)),
                "c" => Command::Copy,
                "x" => Command::Cut,
                "v" => Command::Paste,
                "z" if shift => Command::Input(Event::Text(TextEdit::Redo)),
                "z" => Command::Input(Event::Text(TextEdit::Undo)),
                "y" => Command::Input(Event::Text(TextEdit::Redo)),
                _ => return None,
            });
        }
        let word = if cfg!(target_os = "macos") {
            modifiers.alt_key()
        } else {
            modifiers.control_key()
        };
        if word && let Some(key) = named(key) {
            let edit = match key {
                Key::ArrowLeft => TextEdit::Move {
                    direction: TextMove::Left,
                    select: shift,
                    word: true,
                },
                Key::ArrowRight => TextEdit::Move {
                    direction: TextMove::Right,
                    select: shift,
                    word: true,
                },
                Key::Backspace => TextEdit::Backspace { word: true },
                Key::Delete => TextEdit::Delete { word: true },
                Key::Home if !cfg!(target_os = "macos") => TextEdit::Move {
                    direction: TextMove::DocumentStart,
                    select: shift,
                    word: false,
                },
                Key::End if !cfg!(target_os = "macos") => TextEdit::Move {
                    direction: TextMove::DocumentEnd,
                    select: shift,
                    word: false,
                },
                _ => return Some(Command::Input(Event::KeyDown { key, shift, repeat })),
            };
            return Some(Command::Input(Event::Text(edit)));
        }
        // Ctrl+Alt is AltGr on Windows/Linux and may produce printable text.
        if (!command || modifiers.alt_key())
            && !modifiers.super_key()
            && let Some(text) =
                text.filter(|text| !text.is_empty() && text.chars().all(|ch| !ch.is_control()))
        {
            return Some(Command::Input(Event::Text(TextEdit::Insert(text.into()))));
        }
    }
    named(key).map(|key| Command::Input(Event::KeyDown { key, shift, repeat }))
}

// Clipboard access remains lazy and owned for the lifetime of the application.
// This also keeps X11 clipboard data alive after a copy operation.
trait ClipboardAccess {
    fn read(&mut self) -> Result<String, ()>;
    fn write(&mut self, text: &str) -> Result<(), ()>;
}

impl ClipboardAccess for Option<arboard::Clipboard> {
    fn read(&mut self) -> Result<String, ()> {
        if self.is_none() {
            *self = Some(arboard::Clipboard::new().map_err(|_| ())?);
        }
        self.as_mut().unwrap().get_text().map_err(|_| ())
    }
    fn write(&mut self, text: &str) -> Result<(), ()> {
        if self.is_none() {
            *self = Some(arboard::Clipboard::new().map_err(|_| ())?);
        }
        self.as_mut().unwrap().set_text(text).map_err(|_| ())
    }
}

/// Best effort: a cut already happened in the document and stays undoable.
pub(super) fn write_clipboard(clipboard: &mut Option<arboard::Clipboard>, text: &str) {
    let _ = clipboard.write(text);
}

fn execute(context: &mut Context, clipboard: &mut impl ClipboardAccess, command: Command) -> bool {
    match command {
        Command::Input(event) => context.event(event),
        Command::Copy | Command::Cut => {
            // Multi-line editors keep their text in an app-owned document: the
            // text arrives through `take_clipboard` after the next declaration.
            if context.request_clipboard(matches!(command, Command::Cut)) {
                return true;
            }
            let Some(selected) = context.selected_text().filter(|text| !text.is_empty()) else {
                return false;
            };
            if clipboard.write(selected).is_err() {
                return false;
            }
            matches!(command, Command::Cut)
                && context.event(Event::Text(TextEdit::Insert(String::new())))
        }
        Command::Paste => clipboard
            .read()
            .ok()
            .is_some_and(|text| context.event(Event::Text(TextEdit::Insert(text)))),
    }
}

impl<F: FnMut(&mut Ui<'_>)> Application<F> {
    pub(super) fn keyboard(&mut self, event: KeyEvent) {
        self.sync_clock();
        if event.state == ElementState::Pressed
            && !event.repeat
            && event.logical_key == WinitKey::Named(NamedKey::F10)
            && self.modifiers.shift_key()
        {
            self.input(Event::KeyDown {
                key: Key::ContextMenu,
                shift: true,
                repeat: false,
            });
            return;
        }
        if self.options.devtools
            && event.state == ElementState::Pressed
            && !event.repeat
            && event.logical_key == WinitKey::Named(NamedKey::F12)
        {
            self.inspector_requested = true;
            return;
        }
        if event.state == ElementState::Pressed && !event.repeat && !self.context.composing_text() {
            let key = match &event.logical_key {
                WinitKey::Character(text) if text.chars().count() == 1 => {
                    Some(rust_gui_core::ShortcutKey::Character(
                        text.chars().next().unwrap().to_ascii_lowercase(),
                    ))
                }
                key => named(key).map(rust_gui_core::ShortcutKey::Named),
            };
            if let Some(key) = key
                && self.context.event(Event::Shortcut(rust_gui_core::Shortcut {
                    key,
                    control: self.modifiers.control_key(),
                    alt: self.modifiers.alt_key(),
                    shift: self.modifiers.shift_key(),
                    super_key: self.modifiers.super_key(),
                }))
            {
                if let Some(state) = &self.state {
                    state.window.request_redraw();
                }
                return;
            }
        }
        let command = if event.state == ElementState::Pressed {
            pressed(
                &event.logical_key,
                event.text.as_deref(),
                self.modifiers,
                event.repeat,
                self.context.accepts_text(),
                self.context.composing_text(),
            )
        } else {
            named(&event.logical_key).map(|key| Command::Input(Event::KeyUp(key)))
        };
        if let Some(command) = command
            && execute(
                &mut self.context,
                &mut *self.clipboard.borrow_mut(),
                command,
            )
            && let Some(state) = &self.state
        {
            state.window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_gui_core::{Action, MonospaceMeasurer};
    use rust_gui_scene::Size;

    #[derive(Default)]
    struct Clipboard {
        text: String,
        fail: bool,
        writes: usize,
    }
    impl ClipboardAccess for Clipboard {
        fn read(&mut self) -> Result<String, ()> {
            if self.fail {
                Err(())
            } else {
                Ok(self.text.clone())
            }
        }
        fn write(&mut self, text: &str) -> Result<(), ()> {
            if self.fail {
                return Err(());
            }
            self.text = text.into();
            self.writes += 1;
            Ok(())
        }
    }
    fn frame(context: &mut Context, value: &mut String) {
        context.run(Size::new(400.0, 200.0), &mut MonospaceMeasurer, |ui| {
            ui.text_input("input", "Name", value);
        });
    }

    #[test]
    fn clipboard_cut_is_transactional_and_paste_is_single_undoable_edit() {
        let mut context = Context::new();
        let mut value = "日本語".into();
        frame(&mut context, &mut value);
        context.action(context.find(&["input"]).unwrap(), Action::Focus);
        context.event(Event::Text(TextEdit::SelectAll));
        let mut clipboard = Clipboard {
            fail: true,
            ..Clipboard::default()
        };
        assert!(!execute(&mut context, &mut clipboard, Command::Cut));
        assert_eq!(context.selected_text(), Some("日本語"));
        clipboard.fail = false;
        assert!(execute(&mut context, &mut clipboard, Command::Cut));
        frame(&mut context, &mut value);
        assert!(value.is_empty());
        assert_eq!(clipboard.text, "日本語");
        clipboard.text = "two\nlines".into();
        assert!(execute(&mut context, &mut clipboard, Command::Paste));
        frame(&mut context, &mut value);
        assert_eq!(value, "two lines");
        context.event(Event::Text(TextEdit::Undo));
        frame(&mut context, &mut value);
        assert!(value.is_empty());
        assert!(!execute(&mut context, &mut clipboard, Command::Copy));
        assert_eq!(clipboard.writes, 1);
    }

    #[test]
    fn keyboard_shortcuts_altgr_and_ime_ownership_are_distinct() {
        let command = if cfg!(target_os = "macos") {
            ModifiersState::SUPER
        } else {
            ModifiersState::CONTROL
        };
        let character = WinitKey::Character("z".into());
        assert!(matches!(
            pressed(&character, Some("z"), command, false, true, false),
            Some(Command::Input(Event::Text(TextEdit::Undo)))
        ));
        assert!(matches!(
            pressed(
                &character,
                Some("z"),
                command | ModifiersState::SHIFT,
                false,
                true,
                false
            ),
            Some(Command::Input(Event::Text(TextEdit::Redo)))
        ));
        assert!(
            pressed(
                &character,
                Some("z"),
                ModifiersState::empty(),
                false,
                true,
                true
            )
            .is_none()
        );
        assert!(
            pressed(
                &WinitKey::Named(NamedKey::ArrowLeft),
                None,
                ModifiersState::empty(),
                false,
                true,
                true
            )
            .is_none()
        );
        assert!(matches!(
            pressed(
                &WinitKey::Named(NamedKey::Tab),
                None,
                ModifiersState::empty(),
                false,
                true,
                true
            ),
            Some(Command::Input(Event::KeyDown { key: Key::Tab, .. }))
        ));
        assert!(
            matches!(pressed(&WinitKey::Named(NamedKey::Space), Some(" "), ModifiersState::empty(), false, true, false), Some(Command::Input(Event::Text(TextEdit::Insert(text)))) if text == " ")
        );
        assert!(
            matches!(pressed(&WinitKey::Character("@".into()), Some("@"), ModifiersState::CONTROL | ModifiersState::ALT, false, true, false), Some(Command::Input(Event::Text(TextEdit::Insert(text)))) if text == "@")
        );
        assert!(pressed(&character, Some("z"), command, false, false, false).is_none());
    }

    #[test]
    fn window_reactivation_restores_live_input_without_restarting_composition() {
        let mut context = Context::new();
        let mut value = "Hello".into();
        let mut restore = None;
        frame(&mut context, &mut value);
        let id = context.find(&["input"]).unwrap();
        context.action(id, Action::Focus);
        context.event(Event::Text(TextEdit::SelectAll));
        context.event(Event::Text(TextEdit::Preedit {
            text: "temp".into(),
            selection: None,
        }));
        assert!(window_focus(&mut context, &mut restore, false));
        assert!(!context.accepts_text());
        assert!(window_focus(&mut context, &mut restore, true));
        assert_eq!(context.focused(), Some(id));
        assert!(!context.composing_text());
        assert_eq!(context.selected_text(), Some("Hello"));
        window_focus(&mut context, &mut restore, false);
        context.run(Size::new(400.0, 200.0), &mut MonospaceMeasurer, |_| {});
        assert!(!window_focus(&mut context, &mut restore, true));
    }
}
