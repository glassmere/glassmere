# Seaglass

![MSRV](https://img.shields.io/badge/rust-1.90%2B-orange)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Status](https://img.shields.io/badge/status-0.1%2C%20API%20may%20change-yellow)

A native GUI framework for Rust. You write the view as plain function calls every frame; underneath, a retained tree keeps widget identity, layout and paint between frames and only redoes the parts that changed.

```rust
use seaglass::{run, App, AppOptions, Button, Effects, Ui};

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

Widgets send messages, `update` changes the state, and `view` only reads it. Input is collected into a batch, the batch goes through `update`, and the view runs once afterwards, so it always declares the new state. For small programs, `run_with` takes a closure per change instead of a message enum.

## Support

Green is used and tested by hand, yellow builds and passes tests but has gaps or has not been tried by hand, red is not there yet. The details are in [known limitations](docs/limitations.md).

**Platforms**

![Windows](https://img.shields.io/badge/Windows-tested-brightgreen)
![macOS](https://img.shields.io/badge/macOS-CI%20only-yellow)
![Linux](https://img.shields.io/badge/Linux-CI%20only-yellow)

**Rendering**

![wgpu tested](https://img.shields.io/badge/GPU-D3D12%20%7C%20Vulkan%20%7C%20OpenGL-brightgreen)
![wgpu untested](https://img.shields.io/badge/GPU-Metal%20untested-yellow)
![CPU](https://img.shields.io/badge/CPU%20fallback-supported-brightgreen)
![Vector paths](https://img.shields.io/badge/vector%20paths-not%20yet-red)

**Features**

![Accessibility](https://img.shields.io/badge/accessibility-no%20screen%20reader%20pass-yellow)
![IME](https://img.shields.io/badge/IME-no%20real%20CJK%20test-yellow)
![Touch](https://img.shields.io/badge/touch-injected%20input%20only-yellow)
![Native menu bar](https://img.shields.io/badge/native%20menu%20bar-macOS%20only-yellow)
![OS drag out](https://img.shields.io/badge/drag%20to%20OS-Windows%20%7C%20macOS-yellow)
![Snap layouts](https://img.shields.io/badge/snap%20layouts-Windows%2011-yellow)
![Password field](https://img.shields.io/badge/password%20field-not%20yet-red)
![Scrollbar drag](https://img.shields.io/badge/scrollbar%20drag-not%20yet-red)

## What is in it

- **Layout:** flex and grid through Taffy, with tracks, explicit placement and spans, min/max, aspect ratio and full alignment. Right-to-left windows mirror layout and widgets.
- **Controls:** buttons, checkboxes, radios, sliders, combo boxes, menus, number fields, autocomplete, tabs, split panes, tooltips, popovers, context menus, dialogs, toasts and a command palette.
- **Collections:** virtualized lists, trees and a data grid with sorting, filtering, frozen columns, a cell cursor and typed in-place editing. Also a property grid with `#[derive(Properties)]`.
- **Text:** a single-line field, a virtualized code editor over a rope with syntect highlighting, find and replace, multiple cursors and diagnostics, and a WYSIWYG rich text editor that saves Markdown. BiDi caret movement and IME composition.
- **Docking:** tab groups in a split tree, floating groups, detached OS windows and a layout the application owns and can save.
- **Styling:** theme tokens with dark and light themes that follow the OS, inherited text properties, per-state overrides and stylesheets that reload while the app runs. Transitions, enter and exit effects, layout animation and keyframes on GPU layers.
- **Images:** PNG, JPEG, WebP, GIF and SVG decoded off the UI thread, with nine-slice backgrounds.
- **Localization:** Fluent catalogs, a compile-time checked `tr!`, and CLDR number and date formatting through ICU4X behind the `icu` feature.
- **Input:** keyboard focus scopes, multi-touch with inertia and gestures, drag and drop within and between windows, and files dragged out to the OS.
- **Tooling:** an F12 inspector window (tree, styles, accessibility, events, frame and GPU timings, diagnostics), a headless test harness with semantic finders and snapshots, and `seaglass-dev`, which rebuilds and restarts the app and keeps its state.
- **Accessibility:** every widget publishes its role, name, value and actions through AccessKit.

## Examples

```sh
cargo run -p seaglass --example counter
```

| Example | Shows |
| --- | --- |
| `counter` | The smallest app. |
| `controls` | Text editing and the standard controls. |
| `application` | 50,000-row list, tree and grid, commands, drag and drop, two windows. |
| `workbench` | Scrolling, components, a custom control and updates from a worker thread. |
| `layout` | Grid tracks and placement, alignment, aspect ratio. |
| `theming` | Themes, OS appearance and a stylesheet edited while running (`examples/theming.style`). |
| `dialogs` | Modal and non-modal dialogs, focus scopes, a native modal window. |
| `panes` | Tabs, split panes, tooltips, context menu, popover. |
| `animations` | Transitions, enter and exit, layout animation, keyframes. |
| `docking` | Dockable panels, floating groups, detached windows. |
| `editor` | Rich text with links and the code editor. |
| `richtext` | The WYSIWYG editor. |
| `grid` | The data grid with 2,000 editable rows. |
| `properties` | A property grid editing a card. |
| `images` | Image formats, fit modes, tinted icons, dragging a file out. |
| `gestures` | Pan, pinch and rotate. |
| `localization` | Fluent catalogs and switching language at run time. Add `--features icu` for CLDR formatting. |
| `tasks` | Cancellable background jobs. |
| `reactive` | The optional signal store. |
| `titlebar` | A window that draws its own title bar. |
| `windows` | Several windows sharing one app. |
| `platform` | File dialogs, notifications, tray and menu. Needs `--features platform`. |

`examples/dial-widget` is a separate crate that builds a custom widget using only the public API.

To work on an app with hot restart:

```sh
cargo run -p seaglass-dev -- -p seaglass --example counter
```

## Guides

| Guide | |
| --- | --- |
| [Architecture](docs/architecture.md) | How a frame works and what invalidates what. |
| [State](docs/state.md) | Where state lives, threads, docking layouts and the reactive store. |
| [Widgets](docs/widgets.md) | The built-in controls and writing your own. |
| [Styling](docs/styling.md) | Styles, themes, stylesheets, motion and direction. |
| [Text](docs/text.md) | Fields, editors, IME, clipboard and localization. |
| [Platform](docs/platform.md) | Windows, graphics backends, input, native services and the dev runner. |
| [Accessibility](docs/accessibility.md) | What each node exposes and how to test it. |
| [Performance](docs/performance.md) | What a frame costs and how to see it. |
| [Renderers](docs/renderers.md) | The scene contract and writing a renderer. |
| [API stability](docs/api-stability.md) | What is public and how it changes. |
| [Known limitations](docs/limitations.md) | What does not work yet. |

## Crates

| Crate | |
| --- | --- |
| `seaglass` | The facade; most apps depend only on this. |
| `seaglass-core` | Retained tree, layout, input, widgets, collections and semantics. No platform code. |
| `seaglass-scene` | Geometry, colours and the display list. |
| `seaglass-text` | Font loading and shaping on cosmic-text, shared by both renderers. |
| `seaglass-render-wgpu` | The GPU renderer. |
| `seaglass-render-cpu` | A software renderer for headless use, screenshots and as a fallback. |
| `seaglass-winit` | Windows, input, AccessKit and presentation. |
| `seaglass-win32` | Windows 11 snap layouts. The only crate with `unsafe`. |
| `seaglass-image` | Image decoding and the background loader. |
| `seaglass-fluent`, `seaglass-icu` | Localization. |
| `seaglass-syntax` | Syntax highlighting for the code editor. |
| `seaglass-derive` | `#[derive(Properties)]`, `tr!` and `style_sheet!`. |
| `seaglass-devtools` | The inspector. |
| `seaglass-testing` | Headless test harness. |
| `seaglass-dev` | The hot-restart runner. |

## Building

Rust 1.90 or newer. Linux needs the usual winit packages (X11 or Wayland and xkbcommon), and GTK for the `platform` feature.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p seaglass-render-wgpu --test gpu -- --ignored   # needs a GPU
```

Every example takes `--smoke`, which opens the window, presents two frames and exits.

## License

[Apache License, Version 2.0](LICENSE).
