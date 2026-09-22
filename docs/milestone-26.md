# Milestone 26: the desktop around the window

Implemented on 2026-09-22. Everything so far happened inside a window. This is what lies around it: the file dialogs, the notifications, the menus, the tray icon and the files an association hands over.

It is all behind the `platform` feature, which is off by default. Each piece is a dependency an application may not want in its binary, and the framework asks for none of them to draw a window.

```rust
ui.set_menu(menu());                       // the platform's menu where there is one
ui.menu_bar("bar", &menu());               // and one the framework draws where there is not
ui.set_tray(Some(Tray::new("Notes")));
ui.choose_file(FileRequest::save("Save the notes").named("notes.txt"));
ui.notify(Notification::new("Notes", "Saved."));
```

| Piece | Contract |
| --- | --- |
| `FileRequest`, `Ui::choose_file`, `Ui::chosen_files` | A dialog to open one file, several, a directory, or to save. Asking and reading are separate, because a dialog is modal and a view cannot wait inside itself: the answer arrives on a later frame. An empty answer is a cancellation, which is different from no answer at all. |
| `Notification`, `Ui::notify` | A message for the desktop's notification area. Nothing comes back: what a click on one does is the platform's business. |
| `Menu`, `MenuEntry`, `Ui::set_menu` | The application's menu as data: commands, separators and submenus. The same menu declared again changes nothing, so a view may set it every frame. |
| `Ui::menu_bar(key, &menu)` | The same menu, drawn by the framework, for the platforms where a menu belongs to the window. Two levels; a submenu inside a submenu becomes a heading with its items under it, because the framework's own menus do not nest. |
| `Tray`, `Ui::set_tray` | An icon in the notification area, with a tooltip, an optional image and the menu it opens. `None` takes it away. |
| `Event::Command(id)` | What every one of those routes ends in. Choosing a menu item, clicking a tray entry or calling it directly activates the command the view declared with that id, exactly as its button would. A view that declares its commands once handles all three without knowing which it was. |
| `Ui::opened_files` | The paths the operating system started the application with, which is how an association or a file dropped on the executable arrives. Registering the association itself belongs to an installer. |
| `AppOptions::decorations`, `resizable` | The system's title bar and border, and whether the window can be resized. |
| System appearance | Already followed: `ThemeChoice::System` tracks the desktop's light and dark preference (milestone 6a). |

The platform work happens between frames, in `about_to_wait`. A file dialog runs a message loop of its own, so anything borrowed across it, the shared renderer above all, would still be borrowed when a window repaints inside it.

## What the platform gives, and what it does not

- **macOS** takes the menu as the application menu bar. **Windows** and **Linux** put a menu bar on the window itself, which needs unsafe platform calls this workspace does not make, so `Ui::menu_bar` draws one instead. The tray menu is the platform's own everywhere.
- Shortcuts are written into the menu labels rather than registered as platform accelerators, because the framework dispatches its own and a platform accelerator would fire them twice.

## Validation

Six tests in the core: a dialog asked for once and answered later, with cancelling distinguished from silence; notifications queued in order and taken once; a menu and a tray handed over only when they change; a command chosen from outside activating the command inside, and doing nothing for an unknown id or a disabled command; files the system opened with being there from the first frame; and the framework's menu bar opening, refusing its separator and activating the command its item names.

The `platform` example was run in a real window with a file name as its argument:

```
Started with: Cargo.toml
```

The screenshot is `target/platform-window.png`, with the menu bar the framework drew. The tray icon reported itself installed; on Windows 11 a new icon goes into the overflow area until somebody pins it, so it was not photographed.

The workspace suite is 318 tests; formatting and clippy are clean with all features and with `platform`; all 19 default examples pass their smoke run.

## Remaining limits

- A file dialog blocks the application while it is up, which is what a modal dialog does, but it also blocks the application's other windows.
- Nothing comes back from a notification: no click, no dismissal, no action buttons.
- A tray icon without an image is an empty space on some platforms; give it one.
- `Ui::menu_bar` is two levels deep and has no checkable items, no icons and no keyboard access through Alt.
- The `platform` feature was not built or run on macOS or Linux here.
