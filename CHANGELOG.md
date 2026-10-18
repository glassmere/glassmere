# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0 a breaking change raises the minor version; see [API stability](docs/api-stability.md).

## [0.1.0] - Unreleased

First release. What does not work yet is listed in [docs/limitations.md](docs/limitations.md).

### Added

#### App model

- The `App` trait: a plain struct with `update(&mut self, message, &mut Effects)` and a pure `view(&self, &mut Ui)`. Widgets send messages, input is resolved in batches, and the view declares at most once per batch. `run` starts it on the desktop host; `run_with` is the closure form for small programs.
- `Effects` for everything `update` asks of the framework: background jobs that can be cancelled and superseded by key, channels from other threads with coalesced wake-ups, clipboard, focus, file choosers, toasts, window commands and extra windows.
- `Component` scopes the application owns, `Ui::widget` leaves of your own with input, state, style, semantics and tests (`examples/dial-widget` builds one from outside), and optional reactive `Store` signals and memos with a dependency graph in the inspector.

#### Widgets

- Controls: label, button, checkbox, radio, slider, text input, number input, autocomplete, combo box, menus, context menus, tooltips, popovers, tabs, split panes, dialogs, toasts and images.
- Virtualized lists, trees and a data grid with sorting, filtering, column resize, reorder, hide and freeze, and typed in-place cell editing, over models with stable `ItemId`s.
- Property grid with `#[derive(Properties)]`, a code editor, rich text blocks and a WYSIWYG rich text editor with Markdown persistence.
- Docking with split trees, tab groups, floating groups, detached windows and a persistable layout.

#### Layout and styling

- Retained tree with parent-scoped keys, call-site identity, keyed reconciliation, and cached flex and grid layout (tracks, areas, min/max, aspect ratio, alignment, opt-in shrinking).
- Typed `Style`, dark and light themes that follow the OS, inherited text properties, per-state overrides, stylesheets with classes and hot reload, and a compile-time checked `style_sheet!`.
- Transitions, enter and exit effects, layout animations and keyframes, composited as layers; gradients, shadows and background images with nine-slice frames.

#### Text and localization

- Shaped text with wrapping, font families, weights and italics; single-line editing with IME, selection, clipboard and undo; bidirectional text with caret affinity and right-to-left layout.
- Fluent catalogs with plurals, run-time language switching and a compile-time checked `tr!`, plus CLDR numbers and dates behind the `icu` feature.

#### Accessibility

- AccessKit tree from the same nodes the window draws, with labels, values and actions, keyboard navigation, focus scopes, and a debug diagnostic for controls with no accessible name.

#### Platform and windows

- Native winit host with DPI scaling, multiple windows on one shared graphics device, OS file drops and drags out, touch and gestures, and `Ui::title_bar` with Windows snap layouts through the `glassmere-win32` crate.
- Behind the `platform` feature: native file dialogs, notifications, an application menu, a tray icon and file associations.
- A development runner, `glassmere-dev`, that rebuilds and restarts with app state and window kept.

#### Testing and tooling

- `glassmere-testing` harness with semantic finders, real input paths and text snapshots of the tree, layout, accessibility and scene.
- F12 inspector with node details, frame timings, GPU pass times and memory, repaint and damage overlays, CSV export and `tracing` spans, and framework diagnostics.
- A public API snapshot guarded by a test, with the stability policy in [docs/api-stability.md](docs/api-stability.md).

#### Renderers

- Renderer-independent `Scene` display list, a wgpu renderer (Direct3D 12, Vulkan, Metal, OpenGL) with a glyph and image atlas, and `CpuRenderer`, which rasterizes the same scene without a GPU and is the host's fallback when no adapter works. Both redraw only damaged pixels.
- `AppOptions::gpu` and `Primitive::Custom` hand a rectangle to an application's own wgpu drawing.
