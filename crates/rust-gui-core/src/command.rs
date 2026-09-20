use crate::Key;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    Character(char),
    Named(Key),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub key: ShortcutKey,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}
impl Shortcut {
    /// Ctrl on Windows/Linux, Command on macOS.
    pub fn primary(character: char) -> Self {
        Self {
            key: ShortcutKey::Character(character.to_ascii_lowercase()),
            control: !cfg!(target_os = "macos"),
            alt: false,
            shift: false,
            super_key: cfg!(target_os = "macos"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    pub label: String,
    pub shortcut: Option<Shortcut>,
    pub enabled: bool,
    /// Explicit opt-in to intercept shortcuts while a text field is focused.
    pub while_editing: bool,
}
impl Command {
    pub fn new(label: impl Into<String>, shortcut: Option<Shortcut>) -> Self {
        Self {
            label: label.into(),
            shortcut,
            enabled: true,
            while_editing: false,
        }
    }
}
