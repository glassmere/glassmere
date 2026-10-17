# Milestone 4: application support

Implemented on 2026-09-20. The application workbench combines virtualized lists, trees and a data grid over application-owned identities with commands and shortcuts, in-process and OS drag/drop, a bounded worker mailbox and a second window sharing one graphics device. Core behavior is covered by headless tests; native multi-window presentation is covered by a smoke run. Real OS file-drop positioning and native Linux/macOS interaction remain manual validation work.

## Run

```sh
cargo run -p seaglass --example application --release
cargo run -p seaglass --example application -- --smoke
```

The workbench shows 50,000 tickets in a list and a sortable grid, a tree of areas, four commands, a drag source with two drop targets and a details window. A worker thread creates a ticket every two seconds through the mailbox. `--smoke` opens both windows, lets them present a few frames and exits when the last one closes.

```rust
struct Tickets { /* application-owned rows */ }
impl ListModel for Tickets { /* len, item_id, index_of, label */ }

// Inside the view closure:
let response = ui.virtual_list("tickets", &mut tickets, &mut list_state, VirtualListOptions {
    label: "Tickets".into(), height: 280.0, row_height: 30.0, ..VirtualListOptions::default()
});
if ui.command_button("save", &Command::new("Save", Some(Shortcut::primary('s')))).clicked() { save(); }
if let Some(DragPayload::Item { id, .. }) = ui.drop_target("archive", "Archive", DropKind::Item, style, |ui| { ui.label("Archive"); }) {
    archive(id);
}
```

## Contracts

| API | Behavior |
| --- | --- |
| `ItemId`, `ListModel` | Application identity, independent of realized `NodeId`s. `len`, `item_id`, `index_of` and `label` should be O(1); IDs must be unique and stable across reorder and filtering. Tree models add `depth`, `expanded`, `parent` and `set_expanded`. |
| `ItemOrder` | Indexed order over existing data, rebuilt only when the model changes. Duplicate IDs return `ModelError::Duplicate`. |
| `Ui::virtual_list` / `virtual_list_with` | Fixed-height rows; only the viewport plus bounded overscan is declared. `ListState` keeps the selected item, a reveal request and the scroll offset; the top logical item is re-anchored after a reorder unless the user scrolled independently. `ListResponse` reports `changed`, a one-shot `activated` item and the realized range. |
| `TreeModel`, `Ui::tree_view` | Validated forest (`MissingParent`, `Cycle`). Expansion rebuilds only the visible order. Right/Left expand, collapse or move to the parent; Space toggles; a click on the disclosure area toggles. |
| `GridModel`, `Ui::data_grid` | Column descriptors with finite positive widths; header buttons request `sort(column, direction)` and the model reports whether the order changed. Rows virtualize like lists and keep their selected identity across sorting. |
| `Command`, `Shortcut`, `Ui::command_button` | One button per command; the shortcut and button share the enabled check and a one-shot `clicked()`. `Shortcut::primary` is Ctrl on Windows/Linux and Command on macOS. Shortcuts are ignored while a text field is focused unless `while_editing` is set. Two enabled commands with the same shortcut panic at declaration. |
| `Ui::drag_source` / `drop_target`, `DragPayload`, `DropKind` | Pointer drags start after a 6 px threshold; Space picks up the focused source, Enter drops on the focused target, Escape or focus loss cancels. A target receives only payloads of its kind, once, on the next declaration. File payloads are paths only; the framework performs no filesystem operations. |
| `Event::DropAt` | Hosts deliver external data at a logical point. Several file drops for the same target before the next declaration merge into one `Files` payload. |
| `WakeHandle::channel`, `MessageSender`, `Mailbox` | Bounded non-blocking messages from workers or an application-owned async runtime. `try_send` wakes the UI; `drain(limit)` runs before declaration and schedules another turn if messages remain. Backpressure surfaces as `TrySendError::Full`. |
| `run_windows`, `DesktopHandle`, `WindowKey` | One event loop, graphics device, font system, glyph atlas and clipboard for several windows with independent contexts and focus. `open_window` transfers a `Send` view to the UI thread; share state with `Arc<Mutex<_>>` or mailboxes. `redraw(key)` refreshes another window after shared state changes. Closing the last window exits. |

Rows leaving the realized range are removed, so per-row editing state must live in the application model, not in widgets. The host declares again after consumed input, so labels before a collection reflect the new selection in the same presentation.

## Accessibility

Lists, trees and grids publish `ListBox`/`Option`, `Tree`/`TreeItem` and `Grid`/`Row`/`Cell`/`ColumnHeader` roles with set sizes, positions, levels, row/column indexes and sort direction. The selected row is the collection's active descendant. Drag sources and eligible drop targets expose "Pick up" and "Drop here" custom actions that route to `Action::StartDrag` and `Action::Drop`. Popup and collection semantics use the same core IDs as pointer, keyboard and paint.

## Validation

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p seaglass --no-default-features --locked
cargo run -p seaglass --example application -- --smoke
```

Locally verified with Rust 1.98.0 on Windows:

- 54 regular tests: 44 core behavior tests (6 collection, 6 drag/command), 2 system-font shaping tests and 8 host tests including the mailbox.
- Collections: one million items realize fewer than 35 nodes, End jumps to the last item, selection survives unrealized rows and reorders, disabled collections reject actions, tree validation and expansion, grid sorting keeps the selected identity.
- Drag/drop: threshold, kind matching, once-only delivery, Escape/focus-loss cancellation, keyboard and accessibility paths, merged external file drops.
- Commands: enabled shortcuts fire once, disabled and text-editing cases are ignored, duplicate shortcuts panic.
- Native: the two-window smoke run shared one device and exited after the last window closed.

## Remaining limits

- Rows have one fixed height per collection. Variable heights, horizontal virtualization, column resizing, multi-selection and in-place cell editing are later work.
- Drag/drop is in-process except for incoming OS file drops. winit 0.30 reports no drop position and Windows sends no pointer moves during an OLE drag, so the drop lands at the last known pointer position; the upgrade path is a platform cursor query. Outgoing OS drags and drag images are not implemented.
- Shortcuts are single chords without sequences, menus show no accelerator text, and there is no command palette.
- Windows share one graphics policy; per-window backends, modal dialogs and window-to-window drag are not supported. Native multi-window behavior on Linux/macOS is unverified here.
- Async integration is a mailbox contract; the framework does not run an executor.
