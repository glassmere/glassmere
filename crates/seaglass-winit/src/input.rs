use crate::Application;
use seaglass_core::{Action, Clip, Context, EditorCommand, Event, Key, NodeId, TextEdit, TextMove};
#[cfg(feature = "images")]
use seaglass_scene::Image;
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
                    seaglass_core::Role::Option | seaglass_core::Role::MenuItem
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
        WinitKey::Named(NamedKey::F2) => Key::F2,
        _ => return None,
    })
}

/// Keys of a focused code editor that are not text editing.
fn editor_command(key: &WinitKey, modifiers: ModifiersState) -> Option<EditorCommand> {
    let command = if cfg!(target_os = "macos") {
        modifiers.super_key()
    } else {
        modifiers.control_key()
    };
    match key {
        WinitKey::Named(NamedKey::F3) if modifiers.shift_key() => Some(EditorCommand::FindPrevious),
        WinitKey::Named(NamedKey::F3) => Some(EditorCommand::FindNext),
        WinitKey::Named(NamedKey::ArrowUp) if command && modifiers.alt_key() => {
            Some(EditorCommand::AddCursorAbove)
        }
        WinitKey::Named(NamedKey::ArrowDown) if command && modifiers.alt_key() => {
            Some(EditorCommand::AddCursorBelow)
        }
        WinitKey::Character(character) if command && !modifiers.alt_key() => {
            match character.to_lowercase().as_str() {
                "f" => Some(EditorCommand::Find),
                "h" => Some(EditorCommand::Replace),
                "d" => Some(EditorCommand::SelectNextOccurrence),
                "b" => Some(EditorCommand::ToggleBold),
                "i" => Some(EditorCommand::ToggleItalic),
                "u" => Some(EditorCommand::ToggleUnderline),
                "e" => Some(EditorCommand::ToggleCode),
                "x" if modifiers.shift_key() => Some(EditorCommand::ToggleStrikethrough),
                _ => None,
            }
        }
        _ => None,
    }
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

/// The system clipboard, and what this application last put on it. The text
/// that went with our private bytes is kept so that we can tell whether the
/// clipboard is still ours: if something else has copied since, the text has
/// changed and the bytes are stale.
#[derive(Default)]
pub(crate) struct HostClipboard {
    system: Option<arboard::Clipboard>,
    owned: Option<(String, String, Vec<u8>)>,
}

impl HostClipboard {
    fn system(&mut self) -> Result<&mut arboard::Clipboard, ()> {
        if self.system.is_none() {
            self.system = Some(arboard::Clipboard::new().map_err(|_| ())?);
        }
        self.system.as_mut().ok_or(())
    }

    /// Puts a copy on the clipboard. An image and text cannot go on together
    /// through this platform layer, so an image wins; anything private stays
    /// here, beside the text that identifies it.
    pub(crate) fn write_clip(&mut self, clip: Clip) {
        self.owned = clip
            .custom
            .as_ref()
            .map(|(format, bytes)| (clip.text.clone(), format.clone(), bytes.clone()));
        #[cfg(feature = "images")]
        if let Some((width, height, pixels)) = clip.image.as_ref().and_then(Image::pixels) {
            let straight = straight_rgba(pixels);
            if let Ok(system) = self.system() {
                let _ = system.set_image(arboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: straight.into(),
                });
            }
            return;
        }
        if !clip.text.is_empty() {
            let _ = self.write(&clip.text);
        }
    }

    /// What is on the clipboard now. Private bytes come back only while the
    /// text beside them is still the one they were copied with.
    pub(crate) fn read_clip(&mut self) -> Clip {
        let text = self.read().unwrap_or_default();
        let mut clip = Clip::text(text.clone());
        #[cfg(feature = "images")]
        if let Ok(system) = self.system()
            && let Ok(image) = system.get_image()
            && let Some(image) = Image::from_rgba8(
                image.width as u32,
                image.height as u32,
                image.bytes.into_owned(),
            )
        {
            clip.image = Some(image);
        }
        clip.custom = self
            .owned
            .as_ref()
            .filter(|(copied, _, _)| *copied == text)
            .map(|(_, format, bytes)| (format.clone(), bytes.clone()));
        clip
    }
}

/// Undoes the premultiplication scenes carry, which is what the platform
/// clipboards want.
#[cfg(feature = "images")]
fn straight_rgba(premultiplied: &[u8]) -> Vec<u8> {
    let mut straight = Vec::with_capacity(premultiplied.len());
    for pixel in premultiplied.as_chunks::<4>().0 {
        let alpha = pixel[3];
        for channel in &pixel[..3] {
            straight.push(if alpha == 0 {
                0
            } else {
                ((u32::from(*channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255)
                    as u8
            });
        }
        straight.push(alpha);
    }
    straight
}

impl ClipboardAccess for HostClipboard {
    fn read(&mut self) -> Result<String, ()> {
        self.system()?.get_text().map_err(|_| ())
    }
    fn write(&mut self, text: &str) -> Result<(), ()> {
        self.system()?.set_text(text).map_err(|_| ())
    }
}

fn execute(context: &mut Context, clipboard: &mut impl ClipboardAccess, command: Command) -> bool {
    match command {
        Command::Input(event) => context.event(event),
        Command::Copy | Command::Cut => {
            // Multi-line editors keep their text in an app-owned document: the
            // copy is an operation the application applies, and puts on the
            // clipboard through `Effects::copy`.
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

impl Application {
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
                    Some(seaglass_core::ShortcutKey::Character(
                        text.chars().next().unwrap().to_ascii_lowercase(),
                    ))
                }
                key => named(key).map(seaglass_core::ShortcutKey::Named),
            };
            if let Some(key) = key
                && self.context.event(Event::Shortcut(seaglass_core::Shortcut {
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
        let editor = (event.state == ElementState::Pressed
            && self.context.editor_focused()
            && !self.context.composing_text())
        .then(|| editor_command(&event.logical_key, self.modifiers))
        .flatten();
        let command = if let Some(editor) = editor {
            Some(Command::Input(Event::Editor(editor)))
        } else if event.state == ElementState::Pressed {
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
    use seaglass_core::{Action, App, AppDriver, Effects, MonospaceMeasurer, Ui};
    use seaglass_scene::Size;

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
    /// The one text field of the tests; its value is what `update` stored.
    struct Field {
        value: String,
    }

    impl App for Field {
        type Message = String;

        fn update(&mut self, value: String, _effects: &mut Effects<'_, String>) {
            self.value = value;
        }

        fn view(&self, ui: &mut Ui<'_, String>) {
            ui.keyed("input", |ui| {
                ui.text_input("Name", &self.value, |value| value)
            });
        }
    }

    fn frame(context: &mut Context, field: &mut Field) {
        AppDriver::new(|| true).frame(
            field,
            context,
            Size::new(400.0, 200.0),
            &mut MonospaceMeasurer,
        );
    }

    #[test]
    fn clipboard_cut_is_transactional_and_paste_is_single_undoable_edit() {
        let mut context = Context::new();
        let mut field = Field {
            value: "日本語".into(),
        };
        frame(&mut context, &mut field);
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
        frame(&mut context, &mut field);
        assert!(field.value.is_empty());
        assert_eq!(clipboard.text, "日本語");
        clipboard.text = "two\nlines".into();
        assert!(execute(&mut context, &mut clipboard, Command::Paste));
        frame(&mut context, &mut field);
        assert_eq!(field.value, "two lines");
        context.event(Event::Text(TextEdit::Undo));
        frame(&mut context, &mut field);
        assert!(field.value.is_empty());
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
        let mut field = Field {
            value: "Hello".into(),
        };
        let mut restore = None;
        frame(&mut context, &mut field);
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

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    /// Takes over the system clipboard, so it is opt-in:
    /// `cargo test -p seaglass-winit --features images -- --ignored`.
    #[test]
    #[ignore = "writes to the system clipboard"]
    #[cfg(feature = "images")]
    fn an_image_and_private_bytes_survive_a_round_trip_through_the_platform() {
        let mut clipboard = HostClipboard::default();
        // Half-transparent blue, premultiplied as scenes hold it.
        let image = Image::from_premultiplied(2, 1, vec![255, 0, 0, 255, 0, 0, 64, 128])
            .expect("two pixels");
        clipboard.write_clip(Clip::text("Shape").with_image(image.clone()));
        let read = clipboard.read_clip();
        let (width, height, pixels) = read.image.as_ref().and_then(Image::pixels).expect("pixels");
        assert_eq!((width, height), (2, 1));
        assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
        // Straight blue 128 out of alpha 128 comes back premultiplied to 64.
        assert_eq!(pixels[7], 128);
        assert!(pixels[6].abs_diff(64) <= 1, "{:?}", &pixels[4..]);

        // Private bytes come back while the text beside them is still ours.
        clipboard.write_clip(Clip::custom("app/shape", vec![7, 8]).with_text("Shape"));
        assert_eq!(
            clipboard.read_clip().bytes("app/shape"),
            Some(&[7, 8][..]),
            "the clipboard is still ours"
        );
        clipboard.write_clip(Clip::text("something else"));
        assert_eq!(clipboard.read_clip().bytes("app/shape"), None);
    }
}
