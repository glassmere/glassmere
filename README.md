# Rust GUI framework

A native Rust GUI framework prototype: immediate-style declarations over a retained tree, with stable widget identities, selective invalidation, text editing and standard controls.

Based on the [shared architecture discussion](https://chatgpt.com/share/6aaf8bcf-a0f8-83eb-9b44-21ab4373c0c8) and the [original feature list](docs/feature-list.md). Implemented milestones and remaining work are tracked in the [roadmap](docs/roadmap.md) and the [feature coverage](docs/feature-coverage.md) table.

## The guides

| Guide | What it answers |
| --- | --- |
| [Architecture](docs/architecture.md) | How a frame works, and what invalidates what. |
| [State](docs/state.md) | Where state lives, how answers come back, threads, and the optional reactive store. |
| [Widgets](docs/widgets.md) | The three ways to add something, and what a widget of your own gets. |
| [Accessibility](docs/accessibility.md) | What a node says, how the keyboard works, and how to test it without a screen reader. |
| [Performance](docs/performance.md) | What makes a frame expensive, what it costs, and how to see it. |
| [Renderers](docs/renderers.md) | The scene contract, colour, damage, and writing one of your own. |
| [API stability](docs/api-stability.md) | What is public, what may change, and what guards it. |

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

Press F12 in any window to open its inspector: node tree, computed style, accessibility view, event log, frame timings, GPU pass times and memory, and selection/damage/repaint overlays. `RUST_GUI_INSPECT=1` opens it at start. Headless tests use the `rust-gui-testing` harness with semantic finders and text snapshots. See [milestone 5](docs/milestone-5.md) and the [feature coverage](docs/feature-coverage.md) table.

Theming and stylesheets have their own example:

```sh
cargo run -p rust-gui --example theming --release
```

It follows the OS light/dark preference, switches themes at runtime, inherits text properties, applies per-state overrides, and hot-reloads `examples/theming.style` while running. See [milestone 6a](docs/milestone-6.md).

Layout depth has its own example too:

```sh
cargo run -p rust-gui --example layout --release
```

It shows grid tracks with `repeat` and explicit areas, a form on `[auto, 1fr]` columns, alignment and distribution, aspect-ratio cards bounded by min/max widths and opt-in shrinking. See [milestone 6b](docs/milestone-6.md#milestone-6b-layout-depth).

Dialogs and focus scopes:

```sh
cargo run -p rust-gui --example dialogs --release
```

Modal dialogs with a scrim, nested dialogs, a non-modal panel, arrow-key and remembered focus scopes, and a native modal window that blocks its owner. See [milestone 6c](docs/milestone-6.md#milestone-6c-focus-scopes-and-dialogs).

Tabs, split panes and overlays:

```sh
cargo run -p rust-gui --example panes --release
```

Closable tabs, nested draggable split panes, tooltips after a hover dwell, a right-click context menu and an anchored popover. See [milestone 6d](docs/milestone-6.md#milestone-6d-tooltips-tabs-split-panes-context-menus-and-popovers).

Animations:

```powershell
cargo run -p rust-gui --example animations --release
```

Eased state transitions, enter and exit effects, layout animations and looping keyframes, composited as GPU layers with group opacity and transforms. See [milestone 6e](docs/milestone-6.md#milestone-6e-animations).

Rich text and code editing:

```powershell
cargo run -p rust-gui --example editor --release
```

Styled rich text with links, and a virtualized code editor over an app-owned rope document with syntect highlighting, find and replace, bracket features, multiple cursors and diagnostics. See milestones [7b](docs/milestone-7.md#milestone-7b-rich-text-and-the-code-editor-foundation) and [7c](docs/milestone-7.md#milestone-7c-code-intelligence).

WYSIWYG rich text:

```powershell
cargo run -p rust-gui --example richtext --release
```

An editable document of headings, lists, quotes, code blocks and marked text with a toolbar, saved as Markdown. See [milestone 7d](docs/milestone-7.md#milestone-7d-the-wysiwyg-rich-text-editor).

Hot reload while developing:

```powershell
cargo run -p rust-gui-dev -- -p rust-gui --example counter
```

Rebuilds and restarts the app when sources change, with its state and window kept; watched files and stylesheets update in place. See [milestone 7g](docs/milestone-7.md#milestone-7g-hot-reload).

Localization:

```powershell
cargo run -p rust-gui --example localization --release
```

Fluent catalogs, plurals, a language switch while running, framework controls that follow the language, and `tr!` checked at compile time. Add `--features icu` for CLDR numbers and dates. See [milestone 11](docs/milestone-11.md).

Data grid:

```powershell
cargo run -p rust-gui --example grid --release
```

Two thousand rows with typed in-place editing, a cell cursor, a filter row, columns the user resizes, reorders, hides and freezes, and sideways scrolling that only creates the columns in view. See [milestone 10](docs/milestone-10.md).

Property grid:

```powershell
cargo run -p rust-gui --example properties --release
```

A card edited by the grid beside it. See [milestone 9c](docs/milestone-9.md#milestone-9c-property-grid).

Images:

```powershell
cargo run -p rust-gui --example images --release
```

PNG, JPEG, WebP, GIF and SVG files decoded in the background and reloaded when they change, fit modes, icons tinted with the text colour, and one SVG sharp at every size. See [milestone 8a](docs/milestone-8.md#milestone-8a-images). The file card at the bottom can be dragged out of the window, see [milestone 13a](docs/milestone-13.md).

Gestures:

```powershell
cargo run -p rust-gui --example gestures --release
```

A photo that one finger pans and two fingers pinch and turn; Ctrl with the wheel zooms about the pointer; the two sliders can be dragged at once. `scripts/test-touch.ps1` drives it with injected touch input. See [milestone 13](docs/milestone-13.md).

Docking:

```powershell
cargo run -p rust-gui --example docking --release
```

Dockable tab groups in a split tree, drop indicators, floating groups, detached OS windows, a keyboard panel menu and a saved workspace layout. See [milestone 7a](docs/milestone-7.md#milestone-7a-docking).

```rust
use rust_gui::{run, App, AppOptions, Button, Effects, Ui};

#[derive(Clone)]
enum Msg {
    Increase,
}

struct Counter {
    count: u32,
}

impl App for Counter {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message {
            Msg::Increase => self.count += 1,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.label(format!("Count: {}", self.count));
        ui.button(Button::new("Increase").on_click(Msg::Increase));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(AppOptions::default(), Counter { count: 0 })
}
```

Widgets send messages; `update` changes the state and `view` only reads it. Input is resolved into messages in batches, the batch is delivered to `update`, and the view runs once after it, so everything it declares already shows the new state. `run_with` takes a closure per change instead of a message enum, for small programs. See [where state lives](docs/state.md).

## Implemented

- Parent-scoped explicit keys, call-site keys for everything else, monotonic `NodeId`s, reconciliation and removal.
- Separate tree/style/layout/paint/accessibility/text invalidation flags and frame counters.
- Cached Taffy flex and grid layout with tracks, areas, min/max, aspect ratio, alignment and opt-in shrinking; Row / Column / Container / Grid.
- A renderer-independent scene with ordered rounded quads, text and rectangular clips.
- wgpu rendering with glyphon / cosmic-text shaping and system font fallback.
- Width-aware wrapping, generic/named font families, weights and italic styling.
- Single-line TextInput with shaped selection, grapheme/word editing, bounded undo/redo, lazy clipboard access and IME preedit/commit handling.
- Checkbox, Radio, Slider, ComboBox and one-level popup menus with keyboard, pointer and semantic actions.
- Pointer hit testing and capture, hover, focus, Tab navigation, focus scopes with trap/restore/memory and arrow navigation, Enter/Space activation, disabled states and focus-loss cancellation.
- In-window modal and non-modal dialogs with scrim, stacking, Escape/scrim dismissal and focus restoration; native modal windows that block their owner.
- Tooltips driven by a core clock and host wake requests, closable tabs, split panes with draggable dividers, context menus and anchored popovers.
- Transitions, enter/exit effects, layout animations and keyframes with easing curves and a reduced-motion switch, rendered through offscreen GPU layers.
- Bidirectional text editing with visual caret order and affinity, and right-to-left windows or subtrees with mirrored layout and widgets, detected from the OS locale.
- Framework diagnostics in debug builds: unstable ids, needless relayout, excessive repainting, unnamed controls, oversized layers and huge containers, in the inspector, the log and `Context::diagnostics`.
- Scroll views and collections scroll on both axes, with Shift+wheel, touchpads, arrow keys, focus reveal and right-to-left support.
- `#[derive(Properties)]` turns a struct into property grid rows.
- A property grid that reads app values and sends their edits as messages: groups, filter, resizable name column, reset buttons, numbers with units, colours and custom rows.
- Toasts owned by the framework: timed, held while hovered, announced to screen readers, with an action button and a sender for other threads.
- Number input with commit-on-Enter drafts, ranges, steps and units, and autocomplete over app-supplied matches.
- Images: PNG, JPEG, WebP, GIF and SVG, decoded in the background with placeholders and live reload, fit modes, theme-following icon tints, background images with nine-slice frames from code or stylesheets, a shared GPU atlas and SVGs rasterized sharp at every size.
- Docking with an app-owned, persistable layout: split tree, tab groups, floating groups, detached windows, drop indicators and a keyboard panel menu.
- Native winit host, logical coordinates, DPI scaling, zero-size handling and surface recovery.
- Vertical scrolling, nested wheel propagation, keyboard scrolling and focus revealing off-screen controls.
- AccessKit bridge sharing widget IDs, bounds, disabled state and activation with layout/input.
- Application-owned `Component` scopes and `CustomWidget` leaves with intrinsic size, local paint and semantics.
- Thread-safe, coalesced wake-ups for messages from `Effects::spawn` and `Effects::channel`.
- Subtree paint caches, conservative damage bounds and a reproducible large-tree benchmark.
- Stable `ItemId` models with virtualized lists, trees and a sortable data grid that realize only the viewport.
- Commands with keyboard shortcuts, in-process drag/drop with keyboard and accessibility paths, and incoming OS file drops.
- Bounded worker mailboxes and multiple windows on one shared graphics device.
- Touch: tap, pan with inertia, long press for the context menu or to pick up a drag, and two controls under two fingers at once. `Ui::gesture_area` reports pan, pinch and rotation from fingers, a trackpad or Ctrl with the wheel.
- Files dragged out of a window are handed to the operating system on Windows and macOS; the source hears whether they were dropped.
- `Ui::memo` keeps a subtree without declaring it again, and both renderers redraw only the pixels that changed.
- [The API stability note](docs/api-stability.md) says what is public and how it changes, guarded by a snapshot of the whole surface.
- `Ui::title_bar`, `Ui::window_buttons` and `Ui::resize_frame` let a window with no system frame move, maximize and resize itself, with the shell's own caption glyphs, and Windows offers its snap layouts over the maximize button.
- Behind the `platform` feature: native file dialogs, notifications, an application menu, a tray icon and the files an association opens.
- `Effects::copy` and `Effects::paste` carry text, images and the application's own formats, and a drag can cross to another of its windows.
- [The performance guide](docs/performance.md) says what makes a frame expensive, what it costs and how to see it.
- `Ui::widget` gives a control of your own the input, state, style, semantics and tests a built-in one has; `examples/dial-widget` is one, built from outside.
- Optional reactive state: `Store` signals and memos that record what they read, and an inspector tab that shows the graph.
- `Effects::spawn` runs background jobs that can be cancelled and superseded, and a superseded job's results never reach `update`.
- Gradients and shadows on both renderers, and `AppOptions::gpu` hands a rectangle to an application's own wgpu drawing.
- `Ui::event_area` watches or intercepts the events of a subtree and turns them into messages.
- Commands carry their scope and binding, and feed buttons, shortcuts, menu items and `Ui::command_palette`.
- F12 inspector window with per-node change flags, frame timings, CSV export and `tracing` spans.
- GPU tab: pass times from timestamp queries where the adapter has them, draw calls, renderer memory by category and the driver's report; measured only while an inspector is open.
- Headless testing harness with `by_label` finders, real input paths and tree/layout/accessibility/scene snapshots.
- Headless kernel tests and an opt-in GPU readback test with a bundled-font pixel reference.
- Theme tokens for every control with dark/light themes, OS preference and runtime switching; inherited text properties; hover/pressed/focused/disabled overrides; Rust and stylesheet classes with hot reload.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `rust-gui` | Public facade and runnable example; `desktop` is an optional default feature |
| `rust-gui-core` | UI builder, components/custom leaves, retained tree, scroll/layout/input, collections, commands, drag/drop, paint caches and semantics |
| `rust-gui-scene` | Geometry, colors, measurement interface and display list; no platform dependencies |
| `rust-gui-text` | Fonts, shaping, measuring and text geometry on cosmic-text, shared by the renderers |
| `rust-gui-render-wgpu` | Ordered GPU rendering, glyph atlas and image atlas |
| `rust-gui-render-cpu` | The same display list rasterized without a GPU, for screenshots, export and as the host's fallback |
| `rust-gui-winit` | Native windows, input, worker wakeups and mailboxes, AccessKit mapping, inspector hosting and presentation |
| `rust-gui-image` | PNG, JPEG, WebP, GIF and SVG decoding and the background loader; `rust_gui::image` behind the default `images` feature |
| `rust-gui-fluent` | Fluent catalogs for `Ui::tr`, with Czech and German for the framework's own texts; `rust_gui::fluent` behind the default `fluent` feature |
| `rust-gui-icu` | CLDR numbers, dates and times through ICU4X; `rust_gui::icu` behind the opt-in `icu` feature |
| `rust-gui-derive` | `#[derive(Properties)]`; `rust_gui::Properties` behind the default `derive` feature |
| `rust-gui-devtools` | Frame/event recorder, inspector model, views and overlays; no platform dependencies |
| `rust-gui-testing` | Headless harness, semantic finders and text snapshot assertions for application tests |

## Identity and performance

A widget is identified by the line that declares it, so a widget declared conditionally does not disturb the ones after it. Repeats of one line, such as a loop body, are told apart by order; wrap them in `ui.keyed(id, |ui| ..)` when items can be inserted, removed or reordered. Keys are unique **within a parent**; a duplicate panics in a debug build, and in a release build is left out and reported as a diagnostic. Changing a widget's kind or moving it to a different parent creates a new identity. Removed IDs are not recycled within a context.

Keys that change every frame throw away focus, scroll offset, selection and undo history for everything under them. The `UnstableIds` diagnostic catches that, and in debug builds says so once on stderr as well as in the inspector.

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

When no adapter can be used at all, the host says so and paints the window with the CPU renderer instead, so the application still runs. `GraphicsBackend::Cpu` or `RUST_GUI_RENDERER=cpu` asks for that path on a working machine. See [milestone 14](docs/milestone-14.md).

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
cargo run -p rust-gui --example theming -- --smoke
cargo run -p rust-gui --example layout -- --smoke
cargo run -p rust-gui --example dialogs -- --smoke
cargo run -p rust-gui --example panes -- --smoke
cargo run -p rust-gui --example animations -- --smoke
cargo run -p rust-gui --example docking -- --smoke
cargo run -p rust-gui --example editor -- --smoke
cargo run -p rust-gui --example richtext -- --smoke
cargo run -p rust-gui --example images -- --smoke
cargo run -p rust-gui --example properties -- --smoke
cargo run -p rust-gui --example grid -- --smoke
cargo run -p rust-gui --example localization -- --smoke
cargo run -p rust-gui-core --example tree_bench --release
```

The GPU test checks actual pixels for clipping, paint order, text and 2x scaling, plus buffer reuse and DPI invalidation. `--smoke` opens a native window and exits after two presented frames. `--stress --frames 6000` continuously changes the counter, toggles content and reorders keyed labels. Neither requires the optional `diagnostics` feature; memory checkpoints and allocation-policy overrides do. Desktop execution requires a graphical session and system fonts. Linux builds also need the platform development packages required by winit (X11/Wayland and xkbcommon).

Verified locally on Windows with Rust 1.98.0: 84 regular tests, formatting, Clippy with warnings denied, the facade without desktop features, the two-window application smoke run with and without the inspector, and Direct3D 12 GPU pixel readback including wrapped text and the bundled-font reference. `scripts/test-inspector.ps1` drives the inspector through UI Automation and saves screenshots; `scripts/test-theming.ps1` switches themes, hot-reloads the stylesheet and captures the result. Native checks cover worker wakeups/shutdown and UI Automation interactions. Earlier renderer validation also covered Vulkan and a 6,000-frame changing-scene stress run. After a release build, `scripts/test-desktop.ps1` checks the workbench and `scripts/test-controls.ps1` checks the controls gallery. The latter's `-PointerOnly` mode skips keyboard checks when the shared desktop is active. The CI matrix is configured for Windows, Linux and macOS; native Linux/macOS execution, real OS IME candidate review and manual screen-reader speech/navigation remain unverified here. Rust 1.90 is the declared dependency floor, not a separately tested toolchain.

## Current limits

This is a prototype with an evolving API. Text editing is single-line with logical grapheme navigation; multiline/rich editing and full accessible text selection/navigation remain future work. IME routing is implemented but real candidate windows need manual validation. The pinned AccessKit Windows adapter lacks ExpandCollapsePattern; popup opening works through keyboard/pointer input. Linux clipboard support currently uses X11/XWayland. Scrolling is vertical without draggable scrollbar thumbs; virtualization requires one fixed row height per collection. Drag/drop is in-process plus incoming OS file drops, whose position is the last known pointer on Windows. Custom leaves support drawing and button-style activation. Containers clip to rectangles, while popups overlay normal content. Windows share one graphics policy; partial GPU presentation and CPU rendering remain future work. Themes cover colors and the base font, not spacing or control sizes; stylesheets reload by polling. Grid item percentages resolve against the container and baseline alignment uses text bottoms. Dialogs are centered only, native modals get owned-window semantics on Windows alone, tooltips are pointer-only, and context menus stay one level deep. Frame timings are CPU-side; GPU time, memory statistics and framework warnings are not recorded yet. AccessKit sends full tree updates only while accessibility is active; screen-reader behavior still needs manual review.

See [architecture](docs/architecture.md) and [roadmap](docs/roadmap.md) for the next steps.
