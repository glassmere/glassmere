//! A menu in a window's own menu bar.
//!
//! muda builds the menu and answers it: attaching one to a window subclasses
//! the window, so that choosing an item sends `muda::MenuEvent`, and draws
//! the bar dark when asked. Attaching and detaching take a raw `HWND`, which
//! is why they are here and not in the host.

use muda::{Menu, MenuTheme};
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindow};

/// A menu shown in a window's menu bar. Dropping it takes the bar away.
pub struct MenuBar {
    window: isize,
    menu: Menu,
    dark: bool,
}

fn theme(dark: bool) -> MenuTheme {
    if dark {
        MenuTheme::Dark
    } else {
        MenuTheme::Light
    }
}

/// Puts `menu` in the menu bar of `window`, dark or light as asked. `window`
/// is a `HWND` as `raw-window-handle` reports it, of a window on the calling
/// thread. `None` when Windows or muda would not take it.
pub fn attach_menu(window: isize, menu: Menu, dark: bool) -> Option<MenuBar> {
    // SAFETY: `IsWindow` takes any value and says whether it is a window.
    if unsafe { IsWindow(window as HWND) } == 0 {
        return None;
    }
    // SAFETY: `window` was just seen to be a live window, and the host calls
    // from the thread that owns it and the menu. The menu stays attached
    // only as long as the `MenuBar` holds it: `Drop` detaches it first.
    let attached = unsafe { menu.init_for_hwnd_with_theme(window, theme(dark)) };
    match attached {
        Ok(()) => Some(MenuBar { window, menu, dark }),
        Err(_) => None,
    }
}

impl MenuBar {
    /// Draws the bar dark or light. Nothing happens when it already is.
    pub fn set_dark(&mut self, dark: bool) {
        if self.dark == dark {
            return;
        }
        self.dark = dark;
        // SAFETY: the window was attached to this menu in `attach_menu` and
        // is detached only in `drop`, so it is still the one `init_for_hwnd`
        // took; a window that has been destroyed since is refused by
        // `IsWindow`.
        if unsafe { IsWindow(self.window as HWND) } != 0 {
            let _ = unsafe { self.menu.set_theme_for_hwnd(self.window, theme(dark)) };
        }
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        // A window that is gone took its menu bar with it, and has nothing
        // left to detach from.
        // SAFETY: `IsWindow` takes any value and says whether it is a window.
        if unsafe { IsWindow(self.window as HWND) } != 0 {
            // SAFETY: still a live window, the one this menu was attached to
            // in `attach_menu`; the menu is dropped only after this.
            let _ = unsafe { self.menu.remove_for_hwnd(self.window) };
        }
    }
}
