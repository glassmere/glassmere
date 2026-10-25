# Running on a desktop

What the host (`glassmere-winit`, re-exported by `glassmere`) provides around the view: windows, graphics choice, touch, drag and drop, native services and the development runner.

## Starting

`run(options, app)` runs an `App`; `run_with(options, state, view)` is the shorter form whose message is an `Action<S>`. `AppOptions` carries `title`, `size`, `background`, `theme`, `stylesheet`, `direction`, `caret_movement`, `reduced_motion`, `decorations`, `resizable`, `placement`, `devtools`, `graphics`, `gpu` and `frame_limit`. The defaults are a 900 by 650 window that follows the system theme with the inspector available.

`App::save_state` and `glassmere::hot::saved_state` carry a string across a restart under the development runner. Outside it they do nothing.

## Windows

`Effects::open_window(WindowRequest { title, size, modal_owner })` returns a `WindowId`; `view_window(id, ui)` declares it and `window_closed(id, effects)` reports it closing. Every window shares one `update`, one device and one renderer, each with its own focus and IME target. `Effects::in_window` addresses an effect to a window, `close_window` closes one, and `window_command` sends a `WindowCommand` (`Close`, `Minimize`, `ToggleMaximize`, `Drag`, `Resize`) to the host.

`AppOptions::placement` is `Placement::System`, `Centered`, `WithOwner` or `Named(screen)`. Each window follows the scale factor of the screen it is on. A window with a `modal_owner` blocks input to its owner; only Windows gives it true owned-window behaviour. Windows share one graphics policy.

`Ui::dialog` is the in-window form: a layer over the content, modal with a scrim or floating, with `DialogOptions` for `title`, `width`, `modal` and `dismissible`. A modal dialog traps focus and hides what is behind it from assistive technology.

### Title bar

`AppOptions::decorations = false` removes the system frame. `Ui::title_bar` is a strip that moves the window and maximizes on a double press, `Ui::window_buttons` draws minimize, maximize and close, and `Ui::resize_frame(thickness)` places the resize grips. `Ui::window()` reports whether the window is maximized or focused, of the last frame. `decorations` is read when the window opens. Full screen is not a window command.

On Windows 11 the `glassmere-win32` crate answers the system's hit test for the maximize button so the snap layout flyout appears. It is the only crate with unsafe code and is a few lines of window procedure; on Windows 10 it does nothing. The system takes the press, so the maximize button shows hover but no pressed state.

## Graphics

The wgpu renderer tries one compiled backend at a time (Metal, Direct3D 12, Vulkan, OpenGL, filtered by platform), restricted by `WGPU_BACKEND` if set. `GraphicsOptions { backend, max_resource_bindings, maximum_frame_latency, memory_policy }` defaults to `GraphicsBackend::Auto`, 4,096 bindings, a latency of one frame and `GpuMemoryPolicy::Compact`. An explicit backend overrides `WGPU_BACKEND` and disables fallback to other APIs. `AppOptions::gpu` registers the application's own drawing for `Primitive::Custom` rectangles; see [renderers](renderers.md).

`GraphicsBackend::Cpu` or `GLASSMERE_RENDERER=cpu` paints with `CpuRenderer` through softbuffer. The host falls back to it on its own, after printing what failed, when no graphics backend can be started. `GraphicsOptions` other than the backend mean nothing to the CPU path, and a custom GPU draw leaves its rectangle empty.

## Environment variables

| Variable | Effect |
| --- | --- |
| `GLASSMERE_RENDERER=cpu` | Paint with the CPU renderer. |
| `WGPU_BACKEND` | Restrict the wgpu backends tried, as a comma list. |
| `GLASSMERE_INSPECT` | Open the inspector window at start. |
| `GLASSMERE_STARTUP` | Print how far startup had got at each step. |
| `GLASSMERE_HOT_STATE`, `GLASSMERE_HOT_STOP` | Set by `glassmere-dev`; not for hand use. |

With the `diagnostics` feature: `GLASSMERE_MEMORY_TRACE`, `GLASSMERE_MEMORY_PROFILE`, `GLASSMERE_FRAME_LATENCY` and `GLASSMERE_BINDING_LIMIT` print memory checkpoints (`GLASSMERE_MEMORY_TRACE`), pick a memory preset (`compact` or `memory`) and override the frame latency and resource binding limit, for measuring.

## Input

**Pointer and keyboard** arrive in logical pixels. Ctrl with the wheel is a zoom of a tenth per notch, and a macOS pinch or rotation arrives as `Event::Zoom` and `Event::Rotate`; both apply to the `gesture_area` under the pointer.

**Touch.** Every contact is a pointer of its own (`Event::Touch` with `TouchPhase`), so two fingers move two sliders at once and a finger and the mouse do not disturb each other. A tap within 10 px activates; a longer move pans the innermost scroll view, collection or editor under the finger, or drags a slider, split or dock handle. Content keeps the speed of the last 100 ms and decays (reduced motion disables it). A 500 ms press without travel picks up a drag source or opens the context menu. Tooltips are never shown for touch. `Context::touches()` lists live contacts with pressure.

`Ui::gesture_area(style, GestureBuilder::new().on_gesture(|g| ..), content)` reports `pan_x`, `pan_y`, `scale`, `rotation`, `center`, `tapped`, `double_tapped`, `swipe`, `pressure` and `active` since the last declaration. One finger pans after the slop and a release inside it is a tap; two fingers scale, turn and pan about the point between them. A third finger is ignored. The mouse does not pan a gesture area.

**Drag and drop** is retained in the context. `Ui::drag_source` (with `DragSourceBuilder`) and `Ui::drop_target` (with `DropTargetBuilder`) carry a `DragPayload` (`Text`, `Files` or `Item`) matched by `DropKind`. Space and Enter and the assistive `StartDrag` and `Drop` actions use the same path. Files dropped from the desktop arrive as `Event::DropAt`; successive drops for one target merge. winit reports no drop position, so the drop lands at the last known pointer position. An outgoing drag of `Files` to the desktop is supported on Windows and macOS only and is always a copy. Windows do not repaint while the system runs it.

**Files the system opened the application with** (file associations, command-line paths that exist) are `Ui::opened_files()`.

## Native services (`platform` feature)

Off by default; it adds four dependencies. `Effects::choose_files(FileRequest, |paths| Msg::Chosen(paths))` shows a native dialog (`open`, `open_many`, `save`, `directory`, `with_filter`, `in_directory`, `named`) and blocks every window of the application while it is up. `Effects::notify(Notification)` raises a system notification; nothing comes back from it. `Ui::set_tray(Some(Tray::new(tooltip).with_icon(image).with_menu(menu)))` adds a tray icon; give it an image, since some platforms show an empty space otherwise. `Ui::set_menu(Menu)` installs the application menu.

A `Menu` is `MenuEntry::Command`, `Separator` or `submenu`. A menu item is a `Command`: choosing it sends the same message as the button declared with `Ui::command_button`. On macOS the menu is the application's menu bar. `Ui::set_menu` is the application's menu; where it is shown on Windows and Linux follows how the view is composed, as in Flutter. `Ui::app_bar(AppBar::new().menu(menu))` is a title bar the view places itself: the window's title (or `AppBar::title`), the menu as flat headings, a gap that moves the window, the three window buttons and the resize edges, drawn in the app's theme with the title dimmed while the window has no focus. Its buttons act on the window without a message for the app to handle, and snap layouts work over its maximize button. Placing an app bar, or the low-level `Ui::title_bar`, turns the system's title bar off for a window that has it, so `decorations = false` is not needed; the window is kept hidden until it has drawn once, so the system's one is never seen. `Ui::menu_bar(&menu)` placed in the body is a bar of its own, under the app bar or under the system's title bar. With neither, and with the system's frame, the menu is the window's own menu bar on Windows, drawn dark or light with the window's theme and opened by Alt or F10; the host leaves it out when the view draws the same menu itself, so it is never shown twice. Linux has no native bar. The drawn bar shows two levels, with no checkable items, icons or Alt access, and `MenuItem::separator()` is a rule in a dropdown. Without the feature, the file dialog and notification requests do nothing and the menu and tray are not created.

`Command::with_id(id, label, Some(Shortcut::primary('s')))` attaches a shortcut. `Shortcut::primary` is Ctrl, or Cmd on macOS. Shortcuts are single chords; user remapping is `Effects::set_binding(id, shortcut)`, and the application stores the map itself. `Ui::command_palette` lists commands by label, twelve at most, without fuzzy matching.

## Development runner

```sh
cargo run -p glassmere-dev -- [--watch PATH]... [--poll MS] [cargo args] -- [app args]
```

`glassmere-dev` builds the app, runs it, and when a watched `.rs` or `.toml` file changes rebuilds it. After a successful build the running app is asked to stop, saves its state through `App::save_state`, and the new build starts where the old one was, with the window at the same position and size. A failed build leaves the running app alone. The watch defaults to the current directory and the poll to 500 ms. Only one window's state is saved, and focus and scroll positions are not part of it unless the application includes them.

Files the view reads are reloaded in place: `Ui::watched_text(path)` and `watched_bytes(path)` return the current contents and re-declare when the file's modification time changes. Stylesheets and image files reload the same way.

## Testing

`glassmere-testing` drives a `Context` with a synthetic text measurer through the same event and action paths as the host. `Harness::app(size, app)` runs an `App` without a window; `find` locates a node by key path, `by_label` by accessible name, and `press`, `action`, `snapshot_accessibility` and the other snapshot methods drive and describe it. See [accessibility](accessibility.md) and [widgets](widgets.md) for examples.
