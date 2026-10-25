# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0 a breaking change raises the minor version; see [API stability](docs/api-stability.md).

## [Unreleased]

### Added

- Path styles, drawn the same way by the CPU and GPU renderers:
  - Stroke joins (`Join`: miter with `miter_limit`, round, bevel) and caps (`Cap`: butt, round, square).
  - Dashes with an offset (`Dash`). A pattern that would cut a path into more than `Dash::MAX_COUNT` dashes draws solid.
  - The even-odd fill rule (`FillRule`).
  - Linear and radial gradient fills (`Brush`), placed relative to the path's bounds.
  - See `examples/paths.rs`.

- Screen readers can read and move through editor text. Text fields, text areas, the code editor and the rich editor publish one text run per visual line, with character extents and word starts, and they publish the caret and the selection.
  - The platform can move the caret or set the selection, as `Action::SetSelection` in core.
  - `Context::editor_text` returns what is published.
  - Password fields publish no text.

- Grid layout features:
  - `Style::row_gap` and `column_gap`. `gap` still sets both.
  - `Track::AutoFill` and `Track::AutoFit`, written `repeat(auto-fill, ...)` in a stylesheet.
  - Dense auto-flow through `Style::auto_flow` and `AutoFlow`.
  - Named template areas: `Style::areas` on the grid, `Style::area_name` on a child.
  - `GridArea::auto_span` for a child that auto-flow places across several tracks.

  The stylesheet has the same properties: `row-gap`, `column-gap`, `auto-flow`, `areas`, and `area: <name>`.

### Changed

- `Stroke` is no longer `Copy`, because its dash pattern is a `Vec`. `Primitive::Path`'s `fill` is an `Option<Fill>`, which holds a brush and a fill rule; `Some(color.into())` gives a solid fill.
- A gradient whose start and end are the same point paints its last stop on the GPU as well as on the CPU. The GPU used to paint the first stop.
- A `GridArea` line of 0 leaves that axis to auto-flow. It used to mean line 1.
- Text areas, the code editor and the rich editor report `Role::MultilineTextInput` instead of `Role::TextInput`.
- wgpu 30. It is part of the public API through `AppOptions::gpu`, `CustomDraw` and the renderer, so an application that draws with wgpu itself needs wgpu 30 too.
- Most public enums are `#[non_exhaustive]`, so a later release can add a variant without breaking a `match`; a `match` on one needs a `_` arm. The closed sets that stay exhaustive are listed in [API stability](docs/api-stability.md#what-a-release-can-add).
- Structs the framework fills in and the application only reads are `#[non_exhaustive]` and cannot be built outside their crate. Among them are `FrameStats`, `MemoryReport`, `NodeSnapshot`, `NodeDetails`, `CollectionInfo`, `FrameDiagnostic`, `GraphNode`, `DragInfo`, `GestureResponse`, `RichEditorResponse`, `ResolvedText`, `Swipe`, `Pen`, `WidgetInput`, `RenderStats`, `CpuStats`, `CustomDraw`, the GPU profile and memory types, the devtools records, and the error types `DockLayoutError`, `StyleSheetError` and `FluentError`.
- Options structs are built with `..Default::default()`, so they can gain fields without breaking an application. `Stroke` and `DialogOptions` now implement `Default`.

### Removed

- `TextDocument::rope`. Use `TextDocument`'s own text methods.
- `Harness::state_mut`. Use `Harness::app_mut`.

## [0.1.1] - 2026-10-25

### Added

- `Debug` for `Store`, `Signal`, `Memo`, `TreeModel`, `CommandPalette`, `Autocomplete` and the area, widget, gesture, drag source and drop target builders; `PartialEq` for `DockAction` and `RichEditorOptions`, which also gains `Clone` and `Debug`.
- `glassmere_win32::animations_disabled()` reads the Windows "Animation effects" switch.

### Changed

- `AppOptions::reduced_motion = false` now follows the system on Windows: when animation effects are off in Windows settings, animations are skipped. `true` still forces them off.
- On Linux the clipboard also works in a pure Wayland session, through `arboard`'s `wayland-data-control` feature.

### Fixed

- Holding a press on a scrollbar track keeps paging until the thumb reaches the pointer.
- Dragging a selection past the edge of a text area, code editor or rich text editor scrolls it, and the code editor scrolls sideways for long lines.

### Deprecated

- `TextDocument::rope`, which exposes the `ropey` type; it goes in 0.2.
- `Harness::state_mut`; use `Harness::app_mut`.

## [0.1.0] - 2026-10-18

First release. What does not work yet is listed in [docs/limitations.md](docs/limitations.md).

### Added

#### App model

- The `App` trait: a plain struct with `update(&mut self, message, &mut Effects)` and a pure `view(&self, &mut Ui)`. Widgets send messages, input is resolved in batches, and the view declares at most once per batch. `run` starts it on the desktop host; `run_with` is the closure form for small programs.
- `Effects` for everything `update` asks of the framework: background jobs that can be cancelled and superseded by key, channels from other threads with coalesced wake-ups, clipboard, focus, file choosers, toasts, window commands and extra windows.
- `Component` scopes the application owns, `Ui::widget` leaves of your own with input, state, style, semantics and tests (`examples/dial-widget` builds one from outside), and optional reactive `Store` signals and memos with a dependency graph in the inspector.

#### Widgets

- Controls: label, button, checkbox, radio, slider, text input, password input, number input, autocomplete, combo box, menus, context menus, tooltips, popovers, tabs, split panes, dialogs, toasts and images.
- Virtualized lists, trees and a data grid with sorting, filtering, column resize, reorder, hide and freeze, and typed in-place cell editing, over models with stable `ItemId`s.
- Property grid with `#[derive(Properties)]`, a code editor, rich text blocks and a WYSIWYG rich text editor with Markdown persistence.
- Docking with split trees, tab groups, floating groups, detached windows and a persistable layout.

#### Layout and styling

- Retained tree with parent-scoped keys, call-site identity, keyed reconciliation, and cached flex and grid layout (tracks, areas, min/max, aspect ratio, alignment, opt-in shrinking).
- Typed `Style`, dark and light themes that follow the OS, inherited text properties, per-state overrides, stylesheets with classes and hot reload, and a compile-time checked `style_sheet!`.
- Transitions, enter and exit effects, layout animations and keyframes, composited as layers; gradients, shadows and background images with nine-slice frames.
- Scrollbars that can be dragged and whose tracks page when pressed, on both axes and in the data grid.

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
- `Primitive::Path`: filled and stroked shapes of lines and curves, tessellated by lyon on the GPU and rasterized by tiny-skia on the CPU.
