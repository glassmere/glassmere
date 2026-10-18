//! What a window asks the desktop for: a file dialog, a notification, a menu
//! bar and a tray icon.
//!
//! The framework says what is asked for and what comes back; a host does it
//! with the platform's own dialogs and menus. Nothing here draws anything,
//! and a host that cannot do one of them does not.

use crate::Command;
use seaglass_scene::Image;
use std::path::PathBuf;

/// What a file dialog is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileKind {
    #[default]
    Open,
    /// Several at once.
    OpenMany,
    Save,
    Directory,
}

/// A file dialog to put up after the batch. The answer arrives as the message
/// [`crate::Effects::choose_files`] was given, in a later batch, because a
/// dialog is modal and `update` cannot wait inside itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileRequest {
    pub kind: FileKind,
    pub title: String,
    /// A name and the extensions it covers, without dots, such as
    /// `("Images", ["png", "jpg"])`.
    pub filters: Vec<(String, Vec<String>)>,
    /// Where it opens. The platform decides when this is `None`.
    pub directory: Option<PathBuf>,
    /// The name a save dialog starts with.
    pub file_name: Option<String>,
}

impl FileRequest {
    pub fn open(title: impl Into<String>) -> Self {
        Self {
            kind: FileKind::Open,
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn open_many(title: impl Into<String>) -> Self {
        Self {
            kind: FileKind::OpenMany,
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn save(title: impl Into<String>) -> Self {
        Self {
            kind: FileKind::Save,
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn directory(title: impl Into<String>) -> Self {
        Self {
            kind: FileKind::Directory,
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn with_filter(
        mut self,
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.filters.push((
            name.into(),
            extensions.into_iter().map(Into::into).collect(),
        ));
        self
    }

    pub fn in_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }

    pub fn named(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }
}

/// A message for the desktop's own notification area. Clicking one is the
/// platform's business; nothing comes back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
    /// Asks the platform to show it more insistently, where it can.
    pub urgent: bool,
}

impl Notification {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            urgent: false,
        }
    }

    pub fn urgent(mut self) -> Self {
        self.urgent = true;
        self
    }
}

/// One line of a native menu. A command carries its own label, shortcut and
/// enabled state, and choosing it sends `Event::Command` with its id, so a
/// menu item and a button do the same thing by the same route.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    Command(Command),
    Separator,
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
}

impl MenuEntry {
    pub fn submenu(label: impl Into<String>, entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        Self::Submenu {
            label: label.into(),
            entries: entries.into_iter().collect(),
        }
    }
}

/// One level of a menu, flattened for a menu that cannot nest: each entry
/// with how deep it was, and `None` for a separator.
pub(crate) fn flatten<'a>(
    entries: &'a [MenuEntry],
    depth: usize,
    flat: &mut Vec<(usize, Option<&'a Command>)>,
) {
    for entry in entries {
        match entry {
            MenuEntry::Command(command) => flat.push((depth, Some(command))),
            MenuEntry::Separator => flat.push((depth, None)),
            MenuEntry::Submenu { label: _, entries } => flatten(entries, depth + 1, flat),
        }
    }
}

/// A native menu: the window's menu bar at the top level, or the menu a tray
/// icon opens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Menu {
    pub entries: Vec<MenuEntry>,
}

impl Menu {
    pub fn new(entries: impl IntoIterator<Item = MenuEntry>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An icon in the desktop's notification area, with the menu it opens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tray {
    pub tooltip: String,
    /// Shown at whatever size the platform asks for; small and square is the
    /// only thing that works everywhere.
    pub icon: Option<Image>,
    pub menu: Menu,
}

impl Tray {
    pub fn new(tooltip: impl Into<String>) -> Self {
        Self {
            tooltip: tooltip.into(),
            ..Self::default()
        }
    }

    pub fn with_icon(mut self, icon: Image) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_menu(mut self, menu: Menu) -> Self {
        self.menu = menu;
        self
    }
}
