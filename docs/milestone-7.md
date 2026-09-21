# Milestone 7: advanced features

One slice so far: 7a docking (below).

# Milestone 7a: docking

Implemented on 2026-09-21. An app-owned `DockLayout` describes a tree of splits and tab groups, floating groups inside the window and detached windows. `Ui::dock` renders it, lets tabs be dragged onto drop indicators, other tab strips or empty space, and gives every group a panel menu with the same moves for keyboard and screen-reader users. Layouts persist through a built-in text format or, with the `serde` feature, any serde format.

## Run

```powershell
cargo run -p rust-gui --example docking --release
cargo run -p rust-gui --example docking --release -- --open    # starts with a floating panel, saves nothing
cargo run -p rust-gui --example docking --release -- --reset   # ignores the saved layout
```

The example saves its layout to the temp directory on every change and restores it at start, detached windows included.

```rust
let mut layout = DockLayout::new(DockNode::row(
    0.25,
    DockNode::tabs(["files", "search"]),
    DockNode::column(0.7, DockNode::tabs(["editor", "preview"]), DockNode::tabs(["output"])),
));
let panels = [DockPanel::pinned("editor", "Editor"), DockPanel::new("files", "Files") /* ... */];

let response = ui.dock("dock", &mut layout, &panels, DockOptions::default(), |ui, id| match id {
    "editor" => editor(ui, state),
    _ => ui.label(id),
});
if response.changed { std::fs::write(path, layout.to_text())?; }

layout.show("search");                                   // reopen or select a panel
layout.move_panel("output", DockTarget::Edge { area: DockArea::Main, zone: DockZone::Right });
let restored = DockLayout::parse(&text)?;                // line-numbered errors
```

```text
dock v1
main {
    split horizontal 0.25 {
        tabs 0 "files" "search"
        split vertical 0.7 {
            tabs 1 "editor" "preview"
            tabs 0 "output"
        }
    }
}
float 120 80 320 240 {
    tabs 0 "inspector"
}
window 1 640 480 {
    tabs 0 "console"
}
```

## Contracts

| API | Behavior |
| --- | --- |
| `DockLayout { root, floating, windows }`, `DockNode::{Tabs, Split}` | Plain public data owned by the app. Panels are app-chosen string ids. `DockNode::tabs`, `row` and `column` build trees. |
| `DockLayout::move_panel(panel, DockTarget) -> bool` | Targets: `Group { anchor, zone }` (beside or into the group holding `anchor`), `Tab { anchor, index }` (position in the visible strip), `Edge { area, zone }` (a quarter along a whole edge, `Center` joins the first group), `Float { .. }` and `Window { .. }`. Unknown panels or anchors and moves that change nothing return false and leave the layout untouched. Emptied groups, floats and windows disappear and one-sided splits collapse. Floating groups take tabs but never split. |
| `show`, `activate`, `close`, `contains`, `panels`, `group_of`, `groups_in`, `place_of`, `retain_panels`, `redock`, `remove_window`, `normalize` | Query and edit helpers. `normalize` drops duplicates and empty containers and repairs non-finite or out-of-range numbers; `Ui::dock` calls it, so hand-edited or deserialized layouts are safe. |
| `to_text`, `DockLayout::parse` | The `dock v1` format above with `//` comments, `\"` and `\\` escapes and `DockLayoutError { line, message }`. Parsing normalizes. |
| `serde` feature (`rust-gui/serde`, `rust-gui-core/serde`) | `Serialize` and `Deserialize` on the layout types and `Axis`. |
| `Ui::dock(key, &mut layout, &[DockPanel], DockOptions, content) -> DockResponse` | Declares the main tree with draggable dividers, tab groups and floating groups. `content(ui, id)` runs for each visible panel. Layout ids without a declared `DockPanel` are skipped, so old files survive app changes; declared panels missing from the layout stay hidden until `show`. `DockPanel::pinned` has no close button. The response reports `changed`, `closed` panel ids and newly `detached` window ids. |
| Tab dragging | After six pixels a tab drag shows a compass on the group under the pointer (left, right, top, bottom, centre), four whole-dock edge indicators, a translucent preview and the tab title. Tab strips accept insertion and reordering. A release over no indicator floats the panel there; outside the window it detaches when `native_windows` is set and otherwise changes nothing. Escape or focus loss cancels. |
| Panel menu | Each group has a `▾` menu button acting on its active panel: Float, Move to new window, Move to main window, Dock left/right/top/bottom, Move to next group, Close. Items that would change nothing are disabled. Right-clicking a tab (which also activates it) or Shift+F10 / the Menu key on a focused tab opens the same menu. |
| Floating groups | Drawn above the tree in `floating` order, back to front; their surface blocks clicks to content below. The grip moves and the corner resizes, by pointer or arrow keys (ten pixels), within the dock and down to 160 by 100. Moving, resizing or pressing inside a float raises it. |
| `Ui::dock_window(key, &mut layout, DockWindowId, ..)` | The dock of one detached window, with its own compass and edges. It declares nothing once the layout no longer has the window. |
| `DockWindows::sync(&desktop, &layout, open)`, `DockWindowView`, `DockWindows::redraw` | Host helper that keeps one OS window per `layout.windows` entry: it opens missing ones, closes windows whose entry left, and runs `on_close` when a window is gone for any reason, where apps usually call `layout.redock(id)`. Views share state through `Arc<Mutex<..>>` as for any `run_windows` app. |
| `Style::offset(x, y)` | New general style property: takes a node out of flow at an offset from its parent's top-left corner. Floating groups use it. |

## Validation

Ten docking tests. Four cover the model: moves between groups, edges, floats and windows with collapsing containers, no-op and unknown moves, tab positions, `show`/`redock`/`retain_panels`, and the text format's round trip, repair of damaged input and error lines. One asserts the serde trait bounds under `--all-features`; no serde format crate is in the lockfile, so there is no serialized round-trip test. Five drive `Ui::dock` headlessly: active panels and fill geometry, tab switching and closing, drags onto a compass indicator, another strip, empty space and a dock edge with Escape cancelling, floating groups moved and resized by pointer and keys with click blocking and raising, the panel menu from the keyboard including detach and return through `dock_window`, right-click menus, and releases outside the window with and without native windows. The full suite is 103 tests plus 9 GPU tests; clippy is clean with and without the `serde` feature. The docking example passes `--smoke`; screenshots confirmed the split tree, tab groups, a floating group with grip and resize corner, and the panel menu opened by right-clicking a tab.

Not verified natively: a real pointer drag with its indicators, and tearing a panel off into an OS window and closing it. A background script cannot drive the pointer into the window, so these paths are covered headlessly and by the `DockWindows` code being small. They deserve a manual pass.

## Remaining limits

- A tab cannot be dragged from one OS window into another; use "Move to main window" or close the detached window, which re-docks its panels in the example.
- Detached windows open at the OS-chosen position, and resizing them is not written back to the layout.
- Retained widget state under a panel (focus, scroll offset, text-edit history, enter animations) resets when its group is re-parented by a structural change, because node identity follows the tree path. Switching tabs already behaves this way.
- Floating groups are clipped to the dock and cannot split.
- Tab strips do not scroll; with many tabs the panel menu button can be pushed out of view, though right-click and Shift+F10 still reach the menu.
- The panel menu has nine items and the shared popup height cap makes it scroll.
- The wheel over a floating group can still scroll a scroll view underneath it.
- "Move to next group" and edge docking are the keyboard vocabulary; there is no keyboard equivalent of dropping onto a specific group's side.
