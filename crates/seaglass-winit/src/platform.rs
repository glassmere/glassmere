//! The desktop's own dialogs, notifications, menus and tray icon.
//!
//! The framework asks (`Context::take_file_request`, `take_notifications`,
//! `take_menu`, `take_tray`) and this answers with the platform's versions.
//! Everything here is behind the `platform` feature, because each one is a
//! dependency an application may not want in its binary. Without the
//! feature the requests are not fulfilled and nothing else changes.

#[cfg(not(feature = "platform"))]
pub(crate) use disabled::*;
#[cfg(feature = "platform")]
pub(crate) use enabled::*;

#[cfg(feature = "platform")]
mod enabled {
    use seaglass_core::{Command, FileKind, FileRequest, Menu, MenuEntry, Notification, Tray};
    use seaglass_scene::Image;
    use std::path::PathBuf;
    use winit::window::Window;

    /// Puts up a file dialog and waits for it, which is what a modal dialog
    /// is. Call it between frames: it runs a message loop of its own, so
    /// anything borrowed across it would still be borrowed when a window
    /// repaints inside it.
    pub(crate) fn choose_files(window: &Window, request: FileRequest) -> Vec<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .set_title(&request.title)
            .set_parent(window);
        for (name, extensions) in &request.filters {
            let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
            dialog = dialog.add_filter(name, &extensions);
        }
        if let Some(directory) = &request.directory {
            dialog = dialog.set_directory(directory);
        }
        if let Some(name) = &request.file_name {
            dialog = dialog.set_file_name(name);
        }
        match request.kind {
            FileKind::Open => dialog.pick_file().into_iter().collect(),
            FileKind::OpenMany => dialog.pick_files().unwrap_or_default(),
            FileKind::Save => dialog.save_file().into_iter().collect(),
            FileKind::Directory => dialog.pick_folder().into_iter().collect(),
        }
    }

    pub(crate) fn notify(notification: &Notification) {
        let mut post = notify_rust::Notification::new();
        post.summary(&notification.title).body(&notification.body);
        if notification.urgent {
            post.urgency(notify_rust::Urgency::Critical);
        }
        if let Err(error) = post.show() {
            tracing::warn!("notification not shown: {error}");
        }
    }

    /// The application's menu where the platform has one of its own. That is
    /// the menu bar on macOS; on Windows and Linux a menu bar belongs to the
    /// window, and putting one there needs unsafe platform calls this
    /// workspace does not make, so `Ui::menu_bar` draws it instead.
    pub(crate) struct NativeMenu(#[allow(dead_code)] muda::Menu);

    impl NativeMenu {
        pub(crate) fn install(menu: &Menu) -> Option<Self> {
            let native = build(&menu.entries)?;
            #[cfg(target_os = "macos")]
            native.init_for_nsapp();
            #[cfg(not(target_os = "macos"))]
            return Some(Self(native));
            #[cfg(target_os = "macos")]
            Some(Self(native))
        }
    }

    /// An icon in the notification area, with the menu it opens. Dropping it
    /// takes the icon away.
    pub(crate) struct NativeTray(#[allow(dead_code)] tray_icon::TrayIcon);

    impl NativeTray {
        pub(crate) fn install(tray: &Tray) -> Option<Self> {
            let mut builder = tray_icon::TrayIconBuilder::new().with_tooltip(&tray.tooltip);
            if !tray.menu.is_empty()
                && let Some(menu) = build(&tray.menu.entries)
            {
                builder = builder.with_menu(Box::new(menu));
            }
            if let Some((width, height, pixels)) = tray.icon.as_ref().and_then(Image::pixels)
                && let Ok(icon) = tray_icon::Icon::from_rgba(straight(pixels), width, height)
            {
                builder = builder.with_icon(icon);
            }
            builder
                .build()
                .inspect_err(|error| tracing::warn!("tray icon not shown: {error}"))
                .ok()
                .map(Self)
        }
    }

    /// Icons are straight RGBA; scenes hold premultiplied pixels.
    fn straight(premultiplied: &[u8]) -> Vec<u8> {
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

    /// Builds a platform menu. Each command keeps its own id, so the event
    /// that comes back names the command the view declared.
    fn build(entries: &[MenuEntry]) -> Option<muda::Menu> {
        let items = items(entries);
        let borrowed: Vec<&dyn muda::IsMenuItem> = items.iter().map(AsRef::as_ref).collect();
        muda::Menu::with_items(&borrowed)
            .inspect_err(|error| tracing::warn!("menu not built: {error}"))
            .ok()
    }

    fn items(entries: &[MenuEntry]) -> Vec<Box<dyn muda::IsMenuItem>> {
        entries
            .iter()
            .filter_map(|entry| -> Option<Box<dyn muda::IsMenuItem>> {
                Some(match entry {
                    MenuEntry::Command(command) => Box::new(muda::MenuItem::with_id(
                        command.id.clone(),
                        label(command),
                        command.enabled,
                        None,
                    )),
                    MenuEntry::Separator => Box::new(muda::PredefinedMenuItem::separator()),
                    MenuEntry::Submenu { label, entries } => {
                        let inside = items(entries);
                        let borrowed: Vec<&dyn muda::IsMenuItem> =
                            inside.iter().map(AsRef::as_ref).collect();
                        Box::new(muda::Submenu::with_items(label, true, &borrowed).ok()?)
                    }
                })
            })
            .collect()
    }

    /// The shortcut goes in the label, because the framework dispatches its
    /// own shortcuts and a platform accelerator would fire them twice.
    fn label(command: &Command) -> String {
        match &command.shortcut {
            Some(shortcut) => format!("{}\t{}", command.label, shortcut.describe()),
            None => command.label.clone(),
        }
    }

    /// Commands chosen in a platform menu since the last call.
    pub(crate) fn chosen_commands() -> Vec<String> {
        let mut chosen = Vec::new();
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            chosen.push(event.id.0);
        }
        chosen
    }
}

#[cfg(not(feature = "platform"))]
mod disabled {
    use seaglass_core::{FileRequest, Menu, Notification, Tray};
    use std::path::PathBuf;
    use winit::window::Window;

    pub(crate) fn choose_files(_: &Window, _: FileRequest) -> Vec<PathBuf> {
        tracing::warn!("a file dialog was asked for without the `platform` feature");
        Vec::new()
    }

    pub(crate) fn notify(_: &Notification) {
        tracing::warn!("a notification was asked for without the `platform` feature");
    }

    pub(crate) struct NativeMenu;

    impl NativeMenu {
        pub(crate) fn install(_: &Menu) -> Option<Self> {
            None
        }
    }

    pub(crate) struct NativeTray;

    impl NativeTray {
        pub(crate) fn install(_: &Tray) -> Option<Self> {
            None
        }
    }

    pub(crate) fn chosen_commands() -> Vec<String> {
        Vec::new()
    }
}
