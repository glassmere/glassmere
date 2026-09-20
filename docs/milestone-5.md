# Milestone 5: tooling

Implemented on 2026-09-20. This milestone adds a headless testing crate with semantic finders and snapshots, deterministic pixel references, per-frame timings with `tracing` spans, and an F12 inspector window that shows the inspected window's tree, computed styles, accessibility view, events and frame timeline, with selection, damage and repaint overlays. Single- and multi-window hosts now share one desktop loop, so the inspector is an ordinary second window.

## Run

```sh
cargo run -p rust-gui --example application --release
```

Press F12 in any window to open its inspector, or start with `RUST_GUI_INSPECT=1` to open it immediately. Arrow keys move through the tree, Right/Left expand and collapse, and the selected node is outlined in the inspected window. The Damage and Repaints checkboxes overlay the latest damage rectangles and the nodes repainted by the latest declaration. The Frames tab exports the ring buffer to `rust-gui-frames-<window>.csv` in the working directory.

```rust
use rust_gui_testing::{Harness, assert_snapshot};

let mut harness = Harness::new(Size::new(320.0, 200.0), 0, |count: &mut i32, ui| {
    ui.label_keyed("count", format!("Count: {count}"));
    if ui.button_keyed("increment", "Increase").clicked() { *count += 1; }
});
let increase = harness.by_label("Increase");
harness.click(increase);
assert_eq!(*harness.state(), 1);
assert_snapshot(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"), "counter-tree", &harness.snapshot_tree());
```

## Crates and contracts

| API | Behavior |
| --- | --- |
| `rust_gui_testing::Harness<S>` | Owns application state and a view `FnMut(&mut S, &mut Ui)`. `run` declares once and again when activations were consumed, like the desktop host. `click`, `hover`, `press`, `press_shift`, `type_text`, `focus`, `action` and `event` go through the real input paths. |
| `Harness::find` / `by_label` / `query_label` / `path` | Key paths or exact accessible labels. Failures list the sibling keys or candidate paths. Ambiguous labels panic with both key paths. |
| `Harness::snapshot_tree/layout/accessibility/scene` | Deterministic text, one node or primitive per line, indented by depth, one-decimal numbers and key paths instead of `NodeId`s. |
| `rust_gui_testing::assert_snapshot(dir, name, actual)` | Compares with `<dir>/<name>.snap`; `UPDATE_SNAPSHOTS=1` rewrites, a missing file is created and reported, a mismatch names the first differing line. |
| `TextSystem::with_fonts`, `Renderer::with_text_system` | A font system containing only the supplied fonts, with the generic families bound to the first font, for cross-machine pixel references. The GPU test bundles Fira Mono (SIL OFL 1.1). |
| `FrameStats::frame/declare/layout/paint` | Declaration counter and CPU phase durations from the core. The host adds render and total time per presented frame. |
| `Context::details(id)` | Key, kind, text, style, the `Dirty` flags this node changed in the latest declaration and whether its paint chunk was regenerated. |
| `rust_gui_devtools::Recorder` | Bounded per-window ring of `FrameRecord`s and `EventRecord`s with CSV export. The host records every routed input event while `AppOptions::devtools` is on. |
| `InspectorModel::capture`, `Inspector::ui`, `Inspector::overlay` | Snapshot of an inspected context plus history; the inspector view is a plain declaration; the overlay is a list of logical primitives the host paints after the target's scene. |
| `AppOptions::devtools` | Default true. F12 requests an inspector window; `RUST_GUI_INSPECT` opens one at start. Inspector windows have it off. |
| `tracing` spans | `rust_gui.declare`, `rust_gui.layout`, `rust_gui.paint` in core; `rust_gui.frame` and `rust_gui.render` in the host. No subscriber is installed by the framework. |

`run`, `run_with_wakeup` and `run_windows` now share one desktop loop. `run_with_wakeup` views no longer need `Send`; windows opened through `DesktopHandle` still do because they cross threads.

## Validation

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p rust-gui --no-default-features --locked
cargo test -p rust-gui-render-wgpu --test gpu -- --ignored --nocapture
RUST_GUI_INSPECT=1 cargo run -p rust-gui --example application -- --smoke
```

Windows integration check, after `cargo build -p rust-gui --examples --release --locked`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-inspector.ps1
```

The script starts the application workbench with the inspector open, selects a node through UI Automation and posted keys, toggles the repaint overlay, switches to the Frames and Events tabs and saves `target/inspector-*.png`.

Locally verified with Rust 1.98.0 on Windows:

- 60 regular tests: 44 core, 4 testing harness, 2 devtools, 2 system-font shaping and 8 host tests.
- The explicit GPU test now also checks the bundled-font pixel reference (96×64 RGBA, per-channel tolerance 2 for 99.5% of channels).
- All four examples pass `--smoke`, and the counter and application examples pass with `RUST_GUI_INSPECT=1`.
- Screenshots of the inspector's tree, details, target overlay, Frames and Events tabs were reviewed at 125% scaling.

## Remaining limits

- Frame timings are CPU durations measured on the UI thread; GPU time, draw calls, text shaping and allocation counts are not recorded.
- The inspector captures the full target tree per inspector frame; it is a devtool cost, not an application one, but a 100,000-node target will feel it.
- Event recording formats every routed event while devtools are on. Disable `AppOptions::devtools` in release builds that do not want the cost or the F12 binding.
- The pixel reference was generated on one Direct3D 12 adapter; tolerance across drivers and operating systems is unverified until CI runs the ignored test.
- Framework diagnostics (relayout/repaint warnings, inaccessible controls, unvirtualized huge collections) and a memory panel are not implemented.
