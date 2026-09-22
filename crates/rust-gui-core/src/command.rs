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

    /// How a menu writes it: `Ctrl+Shift+S`.
    pub fn describe(&self) -> String {
        let mut text = String::new();
        for (held, name) in [
            (self.control, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (
                self.super_key,
                if cfg!(target_os = "macos") {
                    "Cmd"
                } else {
                    "Super"
                },
            ),
        ] {
            if held {
                text.push_str(name);
                text.push('+');
            }
        }
        match &self.key {
            ShortcutKey::Character(character) => {
                text.extend(character.to_uppercase());
            }
            // The variant names read the way keys are written.
            ShortcutKey::Named(key) => text.push_str(&format!("{key:?}")),
        }
        text
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    /// Identity for binding overrides, and what a palette returns. Labels are
    /// translated; this is not.
    pub id: String,
    pub label: String,
    pub shortcut: Option<Shortcut>,
    pub enabled: bool,
    /// Explicit opt-in to intercept shortcuts while a text field is focused.
    pub while_editing: bool,
    /// Fires only while the focus is inside the focus scope, dialog or popover
    /// that declares it. A command without one is the window's.
    pub scoped: bool,
}
impl Command {
    /// The label is the identity as well; use `with_id` where labels are
    /// translated or where two commands share a label.
    pub fn new(label: impl Into<String>, shortcut: Option<Shortcut>) -> Self {
        let label = label.into();
        Self::with_id(label.clone(), label, shortcut)
    }

    pub fn with_id(
        id: impl Into<String>,
        label: impl Into<String>,
        shortcut: Option<Shortcut>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            shortcut,
            enabled: true,
            while_editing: false,
            scoped: false,
        }
    }

    /// Only while the focus is inside the scope that declares it.
    pub fn scoped(mut self) -> Self {
        self.scoped = true;
        self
    }
}
