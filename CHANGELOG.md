# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0 a breaking change raises the minor version; see [API stability](docs/api-stability.md).

## [0.1.4] - 2026-11-14

### Added

- Screen readers can read and move through editor text. Text fields, text areas, the code editor and the rich editor publish one text run per visual line, with character extents and word starts, and they publish the caret and the selection.
  - The platform can move the caret or set the selection, as `Action::SetSelection` in core.
  - `Context::editor_text` returns what is published.
  - Password fields publish no text.
- `TextInput::password_with_reveal`, a password field with a button beside it that shows the text and hides it again. A press on the button leaves the keyboard with the field. The button is as wide for one word as for the other, so the field keeps its width. The button's texts are the catalog keys `glassmere-password-show`, `-hide`, `-show-name` and `-hide-name`, with German and Czech in `glassmere-fluent`.
- Grid layout features:
  - `Style::row_gap` and `column_gap`. `gap` still sets both.
  - `Track::AutoFill` and `Track::AutoFit`, written `repeat(auto-fill, ...)` in a stylesheet.
  - Dense auto-flow through `Style::auto_flow` and `AutoFlow`.
  - Named template areas: `Style::areas` on the grid, `Style::area_name` on a child.
  - `GridArea::auto_span` for a child that auto-flow places across several tracks.

  The stylesheet has the same properties: `row-gap`, `column-gap`, `auto-flow`, `areas`, and `area: <name>`.

### Changed

- A `GridArea` line of 0 leaves that axis to auto-flow. Lines are 1-based, and 0 used to be read as line 1.
- Text areas, the code editor and the rich editor report `Role::MultilineTextInput` instead of `Role::TextInput`.

### Fixed

- A command picked in a `Ui::menu_bar` menu takes effect at once. Before, the application heard of it only with the next input, such as a pointer move.
- A submenu inside a menu of `Ui::menu_bar` opens beside its row, to any depth, instead of being drawn as a heading with its items indented under it. It opens on hover, on a click, on Enter and on the arrow key that points the reading way; the other arrow key and Escape go back out one level. It opens on the other side of its row where the window ends.
- A tab strip with more tabs than fit scrolls instead of cutting them off, in `Ui::tabs` and in dock groups. The wheel (turned either way), a touch pan and the arrow, Home and End keys move it, and a tab that becomes selected is brought into view with its close button. A dock group's panel menu stays at the end of the strip.
- Dragging a selection past the edge of a single-line text input keeps scrolling it while the pointer rests there, faster the further out it is, as the multi-line editors do. Before, the text moved only as the pointer did.
- The code editor no longer marks a bracket inside a string or a comment as the partner of one in code. `Highlighter::strings_and_comments` reports where they are, `SyntectHighlighter` implements it, and `TextDocument::matching_bracket_in` is the search that takes them. A bracket inside a string or comment pairs only within it. Without a highlighter every bracket counts, as before.

## [0.1.3] - 2026-11-07

### Added

- `AppOptions::app_id`, the name the desktop knows the application by: the Wayland `app_id` and X11 `WM_CLASS`, matched against a `.desktop` file for the taskbar icon and grouping, and the name notifications are posted under. It defaults to the executable's file name. `AppOptions::resolved_app_id` returns the name in use.
- `AppOptions::icon`, the window's icon on Windows and X11.

### Fixed

- On Linux, screen readers can read text fields. A field exposes its text over AT-SPI as characters and words, with the caret and selection in it; before, Orca heard only the field's name.
- On Linux, an application with the `platform` feature no longer panics at start when the appindicator library is not installed. The tray icon is now a StatusNotifierItem served over D-Bus, so the feature needs no GTK, libappindicator or libxdo, at build time or at run time.
- A command chosen in a tray or application menu is handled at once. On Linux it waited for some other input to wake the window.
- On Linux, tray menu items no longer show a tab and the shortcut in their label.
- Windows on Wayland are no longer shown with the compositor's generic icon when a matching `.desktop` file is installed, and notifications no longer go out under the executable's name when an `app_id` is set.
- A menu button's accessible name no longer ends in the arrow drawn after its text.

## [0.1.2] - 2026-10-25

### Added

- `Context::minimize_button` and `close_button`, beside `maximize_button`; `glassmere_win32::Caption::set_minimize`, `set_close` and `take_input`, whose `CaptionInput` also carries presses, and `glassmere_win32::frame_shadow`.

### Fixed

- On Windows, a title bar the view draws behaves like the system's:
  - Its maximize button offers the snap layouts from the first hover. Before, Windows learnt where the button was only once the view had been declared a second time.
  - A click on its maximize button maximizes or restores the window.
  - Its minimize, maximize and close buttons show the system's tooltips and arrow cursor, without the cursor flickering to a hand.
  - The window keeps the system's shadow and rounded corners.
- `Ui::title_bar` is as tall as the system's caption unless its style sets a height, its window buttons sit side by side with no gap, and a `Ui::menu_bar` in it has flat headings that fit the bar.
- A press in a title bar takes focus without drawing a focus ring; moving focus by keyboard shows it again.
- A window closed from the view, as a title bar's close button does, closes at once rather than at the next event.
- The yanked `clipboard-win` 5.4.1 in `Cargo.lock` is replaced with 5.4.3.

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
