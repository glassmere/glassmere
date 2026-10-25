# Known limitations

What does not work, is partial, or has not been tried. Grouped by area.

## Platforms

- Windows is the platform used day to day. macOS and Linux build and run the test suite in CI, but nobody has used applications on them by hand.
- Linux needs the GTK development packages for the `platform` feature.
- The clipboard in a pure Wayland session (`wayland-data-control`) has not been tried by hand.
- The `platform` feature (file dialogs, notifications, tray, menus) has not been exercised on macOS or Linux.
- Windows 10 lacks the snap layout flyout on the maximize button; only Windows 11 has it. There is no equivalent for other platforms.
- Fonts come from the system. The check for a missing family knows whether it is installed, not whether it has the glyphs a text needs.

## Accessibility

- The accessibility tree is checked by tests and by UI Automation scripts on Windows. Nobody has run NVDA, Narrator, VoiceOver or Orca against it, so everything below is what the tree contains, not what a screen reader was heard to say.
- Text fields, number fields, text areas, code editors and rich editors publish text runs with the length, position and width of each character, word starts, direction, and the caret or selection, and accept `SetTextSelection`. Multi-line editors have the multiline role. The tests check the tree and the round trip of a selection; reading by character, word and line, braille and the highlight cursor have not been tried. Word starts come from Unicode word boundaries, which may differ from where Ctrl+Arrow stops.
- A text area and a code editor expose only the lines they declare (the visible ones and a few around them). Text scrolled out of that window cannot be read or selected by the platform until it is scrolled in. A selection with an end outside the window is not reported, so the caret is unknown to the platform until it is scrolled in, and a selection that extends outside is refused. With several cursors only the primary one is reported.
- A line that mixes directions is one run with the direction of the line. The positions of its characters are measured from the edge the line starts at, in logical order, so across the change of direction they are not increasing, which AccessKit assumes when it turns a range of text into rectangles. Highlighting and the position of a caret inside such a line may be wrong on the platform side. Splitting the line into runs of one direction needs the level of each cluster, which `EditorLine` does not carry.
- A rich editor publishes every block as text runs on every accessibility update, wherever it is scrolled to, because it declares all its blocks. For a large document that is many nodes per update. There is no cache and no window for it.
- The rich editor's block kinds, marks, links and the code editor's line numbers, diagnostics and find matches are not exposed as text attributes. A custom widget has no way to publish text runs.
- A password field reads as bullets and has no runs, selection or word boundaries. The platform cannot read or move through it by character.
- Combo boxes, menu buttons, autocomplete fields and tree rows that have children report expanded or collapsed and take expand and collapse. The framework's menus do not nest, so there are no submenu items to report: `Ui::menu_bar` flattens a submenu inside a submenu.
- AccessKit's Windows adapter (0.35) does implement the UI Automation ExpandCollapse pattern and routes it to those actions. The Linux (AT-SPI, 0.21) and macOS (0.27) adapters have no handling of the expanded state at all, so there it is not reported. Both send `SetTextSelection`, but nobody has run them.
- IME routing and preedit geometry are covered by model-level tests. Real candidate windows for Japanese, Chinese and Korean, and dead-key input, have not been tried.
- Accessibility checks cover names, not contrast, focus order or target size.

## Rendering

- There are no conic gradients and no gradient or shadow animation. A path's gradient fill is linear or radial and a stroke is always one colour. A dash follows the length of the curve as each renderer measures it, so on a curved path the two can place the dashes a little apart.
- The GPU anti-aliases a path's edges with a fringe one pixel wide. Where outlines cross or pass within a pixel of each other, it can differ from the CPU renderer by a few pixels.
- The CPU renderer and the wgpu renderer differ at edges: a quad edge is covered by its exact area in one and feathered by the shader in the other. Resampled images and transformed layers are filtered in sRGB in one and in linear light in the other.
- `Primitive::Custom` is a wgpu callback. A window painted by the CPU renderer leaves it empty.
- Damage is the union of the changed rectangles, so two changes far apart redraw the box between them. A window with an exiting animation repaints entirely for its length.
- The glyph atlas size is not reported: glyphon does not expose it, so no memory total includes it.
- Images have no mipmaps. Animated GIF and WebP show the first frame; colour profiles are ignored.
- The window waits for the graphics stack before it shows anything. Startup is dominated by the driver's adapter enumeration.
- The pixel comparison between the two renderers was tuned on one Direct3D 12 adapter. It also passes on OpenGL on Windows; tolerances on other drivers are unverified.

## Layout and styling

- Grid: no named lines, only named areas; no nested `repeat`. A percentage width on a grid item resolves against the grid container.
- `Ui::responsive` reads the width of the last frame, so a view that changes structure at a threshold settles on the second frame.
- Stylesheets have no variables or nesting and reload by polling every half second.
- One transition covers all animated properties of a node, with no spring physics. Control internals (slider thumbs, caret) do not ease.
- Ancestors clip to their bounds; there is no unclipped container.
- Only direct children extend a scroll view's width, and popups scroll vertically only.
- The OS reduced-motion preference is read on Windows only (at startup and when a window gains focus, not the moment it changes). macOS and Linux use the application option alone. `AppOptions::reduced_motion = false` follows the system; an application cannot force animations on against it.

## Text

- A password field has no button that shows the text. Input methods stay on in it, and macOS secure input is not turned on.
- Visual-line and visual-caret moves in the document editors resolve against the last shaped frame, so several arrow presses between two frames move one step. Several cursors in the code editor move logically.
- Rich documents have one font size per block, no images, tables, nested lists, alignment or colours, are shaped in full on each change, and paste as plain text.
- Dragging a selection past the edge of a single-line text input only follows the pointer as it moves; it does not keep scrolling while the pointer rests. The code editor's horizontal range comes from the widest line declared around the view, so the scrollbar can change length as the view scrolls vertically through lines of different widths.
- Bracket matching ignores strings and comments. Find is line by line.
- The base direction of a paragraph is taken from its first strong character and cannot be forced in an editor. Left and Right in tree rows keep their left-to-right meanings (Left collapses, Right expands) in a right-to-left window.
- Fluent formats numbers plainly; use `Ui::format_number` for locale notation. Only the default catalog is checked by `tr!`.
- No text selection handles or magnifier for touch.

## Collections and grids

- Rows have one fixed height per collection.
- A grid selects one cell at a time, with no range selection or block copy. Number cells are edited as text.
- Filters are text only. There are no pinned data rows (totals).
- Property grid rows are declared, not virtualized; filter or split thousands of properties. There is no multi-object editing. Enums are edited through a `usize` index with `choices(..)`, and `Option<T>` and collection fields are not supported by `#[derive(Properties)]`.
- Menus are two levels deep; a deeper submenu is drawn as a heading with its items under it. Tooltips are pointer-only, with one delay and one placement.

## Windows and docking

- Detached dock windows open where the OS puts them, and resizing them is not written back to the layout. A tab cannot be dragged from one OS window into another.
- Retained widget state under a panel (focus, scroll, undo history) resets when a structural change re-parents its group.
- Floating dock groups are clipped to the dock and cannot split. Tab strips do not scroll.
- Windows share one graphics policy. Window-to-window drags of `Item` payloads are not possible.
- Outgoing OS drags work for files on Windows and macOS, always as a copy, with a fixed icon. Linux has none; the drag crate needs a GTK window. The system drag itself has not been exercised with a real pointer.
- A file dialog blocks all windows. Notifications report nothing back. A tray icon without an image can be invisible.
- The menu is the application's menu bar on macOS. On Windows it is the window's own menu bar only in a window with the system's frame whose view does not draw the menu itself; otherwise `Ui::app_bar` or `Ui::menu_bar` draws it, and that is all Linux has. An app bar has no icon and no system menu, and a window that is not resizable still gets its maximize button and edges. Linux has not been run with an app bar.
- Touch has been tested with injected input only, not on a touch screen or pen. There is no overscroll bounce, scroll chaining or inertia for a gesture-area pan.
- `AppOptions::decorations` is read when the window opens; a view that places an app bar turns the system's frame off after that.

## Tooling

- Hot reload is a rebuild and restart that keeps state; it is not in-process code swapping. It saves one window's state, polls every 500 ms, and does not carry focus or scroll unless the application saves them.
- The inspector captures the full target tree on each of its frames, which a 100,000-node target will feel. GPU timings come from timestamp queries and were checked on one adapter.
- The public API snapshot is read from the sources: it records `cfg` gates as written without evaluating them, expands only the workspace's own macros, and records signatures, not behaviour.
- The test suite's allocation counts are the framework's own declarations, not allocator calls.
