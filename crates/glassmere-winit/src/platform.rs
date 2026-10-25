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
    use glassmere_core::{
        Command, FileKind, FileRequest, Menu, MenuEntry, Notification, Theme, Tray,
    };
    use glassmere_scene::Image;
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
            other => {
                debug_assert!(false, "unhandled file dialog {other:?}");
                Vec::new()
            }
        }
    }

    pub(crate) fn notify(notification: &Notification) {
        let mut post = notify_rust::Notification::new();
        post.summary(&notification.title).body(&notification.body);
        // macOS notifications have no urgency.
        #[cfg(not(target_os = "macos"))]
        if notification.urgent {
            post.urgency(notify_rust::Urgency::Critical);
        }
        if let Err(error) = post.show() {
            tracing::warn!("notification not shown: {error}");
        }
    }

    /// The application's menu where the platform has one of its own. That is
    /// the menu bar on macOS and, for a window with the system's frame, the
    /// window's own menu bar on Windows. A window that draws its own title
    /// bar has no room for a Win32 menu bar, and on Linux a menu bar belongs
    /// to the window as well and is not put there, so `Ui::menu_bar` draws it
    /// instead.
    ///
    /// The Windows bar is attached to the window and detached when this is
    /// dropped. Windows opens it on Alt or F10, as it does for any window
    /// with a menu bar. The framework does not use a lone Alt or F10, but a
    /// shortcut with Alt in it is offered to the bar's mnemonics by Windows
    /// as well; none of this has been tried on a real desktop.
    pub(crate) struct NativeMenu(#[allow(dead_code)] Held);

    #[cfg(windows)]
    type Held = glassmere_win32::MenuBar;
    #[cfg(not(windows))]
    type Held = muda::Menu;

    impl NativeMenu {
        /// `framed` is whether the window has the system's frame, and
        /// `theme` the one the window follows, for a bar that is drawn dark
        /// or light with it.
        pub(crate) fn install(
            menu: &Menu,
            window: &Window,
            framed: bool,
            theme: &Theme,
        ) -> Option<Self> {
            #[cfg(not(windows))]
            let _ = (window, framed, theme);
            let native = build(&menu.entries)?;
            #[cfg(target_os = "macos")]
            native.init_for_nsapp();
            #[cfg(windows)]
            return framed
                .then(|| attach(window, native, theme))
                .flatten()
                .map(Self);
            #[cfg(not(windows))]
            Some(Self(native))
        }

        /// Whether the menu is in the window, as opposed to the application.
        pub(crate) fn in_window(&self) -> bool {
            cfg!(windows)
        }

        /// The bar follows the window's theme.
        pub(crate) fn set_theme(&mut self, theme: &Theme) {
            #[cfg(windows)]
            self.0.set_dark(is_dark(theme));
            #[cfg(not(windows))]
            let _ = theme;
        }
    }

    #[cfg(windows)]
    fn attach(window: &Window, menu: muda::Menu, theme: &Theme) -> Option<Held> {
        let bar =
            hwnd(window).and_then(|hwnd| glassmere_win32::attach_menu(hwnd, menu, is_dark(theme)));
        if bar.is_none() {
            tracing::warn!("menu bar not attached to the window");
        }
        bar
    }

    /// The window as Windows knows it.
    #[cfg(windows)]
    fn hwnd(window: &Window) -> Option<isize> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(isize::from(handle.hwnd)),
            _ => None,
        }
    }

    /// Whether a theme is a dark one, by how light its window is.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn is_dark(theme: &Theme) -> bool {
        let glassmere_scene::Color(r, g, b, _) = theme.window;
        0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b) < 128.0
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
                && let Ok(icon) =
                    tray_icon::Icon::from_rgba(crate::input::straight_rgba(pixels), width, height)
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
                    other => {
                        debug_assert!(false, "unhandled menu entry {other:?}");
                        return None;
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_bar_is_dark_with_the_dark_theme_and_light_with_the_light_one() {
            assert!(is_dark(&Theme::dark()));
            assert!(!is_dark(&Theme::light()));
        }
    }
}

#[cfg(not(feature = "platform"))]
mod disabled {
    use glassmere_core::{FileRequest, Menu, Notification, Theme, Tray};
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
        pub(crate) fn install(_: &Menu, _: &Window, _: bool, _: &Theme) -> Option<Self> {
            None
        }

        pub(crate) fn in_window(&self) -> bool {
            false
        }

        pub(crate) fn set_theme(&mut self, _: &Theme) {}
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
