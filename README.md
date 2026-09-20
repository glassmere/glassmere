# Rust GUI framework

A native Rust GUI framework prototype: immediate-style declarations over a retained tree, with stable widget identities, selective invalidation, text editing and standard controls.

Based on the [shared architecture discussion](https://chatgpt.com/share/6aaf8bcf-a0f8-83eb-9b44-21ab4373c0c8). The original feature-list attachment was not available from the shared page; the implemented milestones and remaining work are tracked in the [roadmap](docs/roadmap.md).

## Run

Install a current stable Rust toolchain and the native build prerequisites for your OS, then:

```sh
cargo run -p rust-gui --example counter
```

The playground includes a counter, disabled reset button, conditional content, and keyed reordering. Click buttons or use Tab / Shift+Tab followed by Enter or Space. The host sleeps between events.

The second milestone has its own workbench with 250 scrollable rows, independent components, a custom control and worker-thread updates:

```sh
cargo run -p rust-gui --example workbench --release
```

The third milestone adds a text and controls gallery:

```sh
cargo run -p rust-gui --example controls --release
```

It demonstrates single-line Unicode editing, selection, clipboard shortcuts, undo/redo, font styles, wrapping, checkboxes, radio buttons, sliders, combo boxes and menus. See [milestone 3](docs/milestone-3.md) for API contracts and current platform limits.

The fourth milestone adds an application workbench with two windows:

```sh
cargo run -p rust-gui --example application --release
```

It shows a virtualized list, tree and sortable grid over 50,000 application-owned rows, commands with shortcuts, drag and drop, a worker mailbox and a details window sharing the main window's GPU device. See [milestone 4](docs/milestone-4.md).

Press F12 in any window to open its inspector: node tree, computed style, accessibility view, event log, frame timings and selection/damage/repaint overlays. `RUST_GUI_INSPECT=1` opens it at start. Headless tests use the `rust-gui-testing` harness with semantic finders and text snapshots. See [milestone 5](docs/milestone-5.md) and the [feature coverage](docs/feature-coverage.md) table.

```rust
use rust_gui::{run, AppOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    run(AppOptions::default(), move |ui| {
        ui.label_keyed("count", format!("Count: {count}"));
        if ui.button_keyed("increment", "Increase").clicked() {
            count += 1;
        }
    })
}
```

The view may run more than once per input event. Put application actions behind responses such as `clicked()`. The host declares again after activation to make labels declared before a button reflect state changes in the same presentation.

## Implemented

- Parent-scoped explicit keys, positional convenience keys, monotonic `NodeId`s, reconciliation and removal.
- Separate tree/style/layout/paint/accessibility/text invalidation flags and frame counters.
- Cached Taffy flex layout, Row / Column / Container, labels and buttons.
- A renderer-independent scene with ordered rounded quads, text and rectangular clips.
- wgpu rendering with glyphon / cosmic-text shaping and system font fallback.
- Width-aware wrapping, generic/named font families, weights and italic styling.
- Single-line TextInput with shaped selection, grapheme/word editing, bounded undo/redo, lazy clipboard access and IME preedit/commit handling.
- Checkbox, Radio, Slider, ComboBox and one-level popup menus with keyboard, pointer and semantic actions.
- Pointer hit testing and capture, hover, focus, Tab navigation, Enter/Space activation, disabled states and focus-loss cancellation.
- Native winit host, logical coordinates, DPI scaling, zero-size handling and surface recovery.
- Vertical scrolling, nested wheel propagation, keyboard scrolling and focus revealing off-screen controls.
- AccessKit bridge sharing widget IDs, bounds, disabled state and activation with layout/input.
- Application-owned `Component` scopes and `CustomWidget` leaves with intrinsic size, local paint and semantics.
- Thread-safe, coalesced `WakeHandle` updates through `run_with_wakeup`.
- Subtree paint caches, conservative damage bounds and a reproducible large-tree benchmark.
- Stable `ItemId` models with virtualized lists, trees and a sortable data grid that realize only the viewport.
- Commands with keyboard shortcuts, in-process drag/drop with keyboard and accessibility paths, and incoming OS file drops.
- Bounded worker mailboxes and multiple windows on one shared graphics device.
- F12 inspector window with per-node change flags, frame timings, CSV export and `tracing` spans.
- Headless testing harness with `by_label` finders, real input paths and tree/layout/accessibility/scene snapshots.
- Headless kernel tests and an opt-in GPU readback test with a bundled-font pixel reference.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `rust-gui` | Public facade and runnable example; `desktop` is an optional default feature |
| `rust-gui-core` | UI builder, components/custom leaves, retained tree, scroll/layout/input, collections, commands, drag/drop, paint caches and semantics |
| `rust-gui-scene` | Geometry, colors, measurement interface and display list; no platform dependencies |
| `rust-gui-render-wgpu` | Ordered GPU rendering, text shaping and glyph atlas |
| `rust-gui-winit` | Native windows, input, worker wakeups and mailboxes, AccessKit mapping, inspector hosting and presentation |
| `rust-gui-devtools` | Frame/event recorder, inspector model, views and overlays; no platform dependencies |
| `rust-gui-testing` | Headless harness, semantic finders and text snapshot assertions for application tests |

## Identity and performance

Use `button("Save")` and `label("Hello")` for fixed child sequences. Their identity follows the position among siblings. Use `button_keyed`, `label_keyed`, and keyed groups for conditional or reordered content. Keys are unique **within a parent**; a duplicate panics with a diagnostic. Changing a widget's kind or moving it to a different parent creates a new identity. Removed IDs are not recycled within a context.

An unchanged declaration reuses matching nodes and skips measurement, layout and scene generation. Text changes request layout only when their measured intrinsic size changes. Paint changes invalidate the affected nodes and ancestor caches; clean subtrees reuse their paint chunks. The renderer retains GPU buffers, batches adjacent text and skips preparation/uploads on unchanged exposure redraws. Reconciliation still visits the whole declaration, and a changed scene still flattens all cached paint chunks and repaints the full surface. Damage bounds are exposed for future partial presentation. See [milestone 2 results](docs/milestone-2.md) for contracts and measured large-tree costs.

The headless core accepts any `TextMeasurer`. `MonospaceMeasurer` supplies deterministic synthetic metrics for logic tests; desktop rendering uses the actual shaping system for both measurement and painting.

The desktop host probes one graphics backend at a time: Direct3D 12 first on Windows, Metal on macOS, Vulkan on Linux. It checks for an adapter compatible with the window, creates that device, and stops; fallback APIs are tried only after failure. Merely finding a driver DLL would not establish device or presentation support. `WGPU_BACKEND=dx12`, `vulkan`, or `gl` restricts automatic selection for diagnosis.

`AppOptions::graphics` exposes `GraphicsOptions`. Defaults use compact allocation blocks, 4,096 live non-sampler resource bindings and a maximum frame latency of one. Binding count is not widget count; applications with many GPU resources can raise the limit. `GpuMemoryPolicy::Balanced` selects wgpu's general-purpose memory-saving preset. Explicit backend selection overrides `WGPU_BACKEND` and returns an error if unavailable:

```rust
use rust_gui::{AppOptions, GraphicsBackend, GraphicsOptions};

let options = AppOptions {
    graphics: GraphicsOptions {
        backend: GraphicsBackend::Vulkan,
        ..GraphicsOptions::default()
    },
    ..AppOptions::default()
};
```

On the local RTX 3070, the optimized counter now uses about 67 MiB of private resident RAM versus the original 115 MiB, with 122 MiB of private commit versus 385 MiB. This remains a substantial graphics/driver baseline. See [memory measurements](docs/memory.md) for startup attribution, the 6,000-frame stress run and reproduction steps.

## Validation

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p rust-gui --no-default-features --locked
cargo test -p rust-gui-render-wgpu --test gpu -- --ignored --nocapture
cargo run -p rust-gui --example counter -- --smoke
cargo run -p rust-gui --example workbench -- --smoke
cargo run -p rust-gui --example controls -- --smoke
cargo run -p rust-gui --example application -- --smoke
RUST_GUI_INSPECT=1 cargo run -p rust-gui --example application -- --smoke
cargo run -p rust-gui-core --example tree_bench --release
```

The GPU test checks actual pixels for clipping, paint order, text and 2x scaling, plus buffer reuse and DPI invalidation. `--smoke` opens a native window and exits after two presented frames. `--stress --frames 6000` continuously changes the counter, toggles content and reorders keyed labels. Neither requires the optional `diagnostics` feature; memory checkpoints and allocation-policy overrides do. Desktop execution requires a graphical session and system fonts. Linux builds also need the platform development packages required by winit (X11/Wayland and xkbcommon).

Verified locally on Windows with Rust 1.98.0: 60 regular tests, formatting, Clippy with warnings denied, the facade without desktop features, the two-window application smoke run with and without the inspector, and Direct3D 12 GPU pixel readback including wrapped text and the bundled-font reference. `scripts/test-inspector.ps1` drives the inspector through UI Automation and saves screenshots. Native checks cover worker wakeups/shutdown and UI Automation interactions. Earlier renderer validation also covered Vulkan and a 6,000-frame changing-scene stress run. After a release build, `scripts/test-desktop.ps1` checks the workbench and `scripts/test-controls.ps1` checks the controls gallery. The latter's `-PointerOnly` mode skips keyboard checks when the shared desktop is active. The CI matrix is configured for Windows, Linux and macOS; native Linux/macOS execution, real OS IME candidate review and manual screen-reader speech/navigation remain unverified here. Rust 1.90 is the declared dependency floor, not a separately tested toolchain.

## Current limits

This is a prototype with an evolving API. Text editing is single-line with logical grapheme navigation; multiline/rich editing and full accessible text selection/navigation remain future work. IME routing is implemented but real candidate windows need manual validation. The pinned AccessKit Windows adapter lacks ExpandCollapsePattern; popup opening works through keyboard/pointer input. Linux clipboard support currently uses X11/XWayland. Scrolling is vertical without draggable scrollbar thumbs; virtualization requires one fixed row height per collection. Drag/drop is in-process plus incoming OS file drops, whose position is the last known pointer on Windows. Custom leaves support drawing and button-style activation. Containers clip to rectangles, while popups overlay normal content. Windows share one graphics policy; partial GPU presentation, advanced styling and CPU rendering remain future work. Frame timings are CPU-side; GPU time, memory statistics and framework warnings are not recorded yet. AccessKit sends full tree updates only while accessibility is active; screen-reader behavior still needs manual review.

See [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md) for the next steps.
