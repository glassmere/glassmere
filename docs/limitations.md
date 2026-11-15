# Known limitations

What does not work, is partial, or has not been tried. Grouped by area.

## Platforms

- Windows is the platform used day to day. Linux has been used by hand on KDE Plasma, on Wayland and X11, at scale factors of 1, 1.25, 1.5 and 2 and through a scale change while a window was open. macOS builds and runs the test suite in CI, but nobody has used applications on it by hand.
- On Linux the tray icon is a StatusNotifierItem. KDE Plasma, XFCE and most panels show one; GNOME needs the AppIndicator extension. Its menu shows no shortcuts.
- A Wayland window takes its icon from the `.desktop` file named by `AppOptions::app_id`; `AppOptions::icon` is used on Windows and X11 only.
- On Linux the clipboard has been tried by hand on KDE Plasma, in a pure Wayland session (`ext-data-control`) and on X11, for text only. Compositors without a data-control protocol, such as GNOME's Mutter, have not been tried.
- The `platform` feature (file dialogs, notifications, tray, menus) has not been exercised on macOS. On Linux it has been tried on KDE Plasma (Wayland and X11) only.
- Windows 10 lacks the snap layout flyout on the maximize button; only Windows 11 has it. There is no equivalent for other platforms.
- Fonts come from the system. The check for a missing family knows whether it is installed, not whether it has the glyphs a text needs.

## Accessibility

- The accessibility tree is checked by tests, by UI Automation scripts on Windows, and over AT-SPI on Linux (KDE Plasma). Nobody has run NVDA, Narrator, VoiceOver or Orca against it, so everything below is what the tree contains, not what a screen reader was heard to say.
- Text fields, number fields, text areas, code editors and rich editors publish text runs with the length, position and width of each character, word starts, direction, and the caret or selection, and accept `SetTextSelection`. Multi-line editors have the multiline role. The tests check the tree and the round trip of a selection; reading by character, word and line, braille and the highlight cursor have not been tried. Word starts come from Unicode word boundaries, which may differ from where Ctrl+Arrow stops.
- A text area and a code editor expose only the lines they declare (the visible ones and a few around them). Text scrolled out of that window cannot be read or selected by the platform until it is scrolled in. A selection with an end outside the window is not reported, so the caret is unknown to the platform until it is scrolled in, and a selection that extends outside is refused. With several cursors only the primary one is reported.
- A line that mixes directions is one run with the direction of the line. The positions of its characters are measured from the edge the line starts at, in logical order, so across the change of direction they are not increasing, which AccessKit assumes when it turns a range of text into rectangles. Highlighting and the position of a caret inside such a line may be wrong on the platform side. Splitting the line into runs of one direction needs the level of each cluster, which `EditorLine` does not carry.
- A rich editor publishes every block as text runs on every accessibility update, wherever it is scrolled to, because it declares all its blocks. For a large document that is many nodes per update. There is no cache and no window for it.
- The rich editor's block kinds, marks, links and the code editor's line numbers, diagnostics and find matches are not exposed as text attributes. A custom widget has no way to publish text runs.
- A password field reads as bullets and has no runs, selection or word boundaries. The platform cannot read or move through it by character.
- AccessKit's Windows adapter has no UI Automation ExpandCollapse pattern. The tree exposes expand and collapse actions, but combo boxes and menus open from automation only through focus and keyboard or pointer input.
- IME routing and preedit geometry are covered by model-level tests. Real candidate windows for Japanese, Chinese and Korean, and dead-key input, have not been tried.
- Accessibility checks cover names, not contrast, focus order or target size.

## Rendering

- Paths fill by the non-zero rule only. Strokes have mitred corners (bevelled past four widths) and square-cut ends: no round joins or caps, no dashes, no gradient fills. There are no conic gradients and no gradient or shadow animation.
- The GPU anti-aliases a path's edges with a fringe one pixel wide. Where outlines cross or pass within a pixel of each other, it can differ from the CPU renderer by a few pixels.
- The CPU renderer and the wgpu renderer differ at edges: a quad edge is covered by its exact area in one and feathered by the shader in the other. Resampled images and transformed layers are filtered in sRGB in one and in linear light in the other.
- `Primitive::Custom` is a wgpu callback. A window painted by the CPU renderer leaves it empty.
- Damage is at most eight rectangles: ones that overlap or lie within 8 logical pixels join, and past eight the closest pair do, so crowded changes still redraw the gaps between them. A node animating a transform (a slide or zoom in, a layout transition, keyframes) still repaints the whole window while it runs; an exit animation does not.
- The glyph atlas size is not reported: glyphon does not expose it, so no memory total includes it.
- Images have no mipmap chain. A photo drawn at under half its size is box-filtered down in halving steps, once per step, on the render thread the first time it is drawn there, so a large photo shown small can cost one slow frame; between steps it is sampled bilinearly, without blending two steps. An embedded ICC profile for RGB or grey pixels is converted to sRGB (PNG, JPEG and WebP); CMYK profiles and PNG `gAMA`, `cHRM` and `cICP` information are ignored.
- Animated GIF, WebP and APNG play in image widgets only; a background image shows the first frame. An animation is kept only while its decoded frames add up to 32 MiB, and past that it shows its first frame. Playback follows the clock, so an animation off screen is not paused but skips ahead, and one that finished its loops while hidden shows its last frame. A GIF without a loop extension repeats forever (browsers play it once), because the `image` crate does not tell it from an infinite one. Animated frames skip EXIF orientation and ICC conversion.
- The window waits for the graphics stack before it shows anything. Startup is dominated by the driver's adapter enumeration.
- The pixel comparison between the two renderers was tuned on one Direct3D 12 adapter. It also passes on OpenGL on Windows; tolerances on other drivers are unverified.

## Layout and styling

- Grid: no named lines, only named areas; no nested `repeat`. A percentage width on a grid item resolves against the grid container.
- `Ui::responsive` reads the width of the last frame, so a view that changes structure at a threshold settles on the second frame.
- Stylesheets reload by polling every half second. Variables are plain text resolved when the sheet is parsed, in one scope for the whole sheet: they cannot read a theme token, so a colour taken from one does not follow a theme switch, and a variable must be declared above its first use. Nesting is `&:state` and `&.name` only, because a class is a name and not a selector; there are no descendant selectors.
- One transition covers all animated properties of a node, with no spring physics. Control internals (slider thumbs, caret) do not ease; a switch's thumb is the exception, and follows the style's transition.
- Ancestors clip to their bounds; there is no unclipped container.
- Only direct children extend a scroll view's width, and popups scroll vertically only.
- The OS reduced-motion preference is read on Windows only (at startup and when a window gains focus, not the moment it changes). macOS and Linux use the application option alone. `AppOptions::reduced_motion = false` follows the system; an application cannot force animations on against it.

## Text

- Input methods stay on in a password field, and macOS secure input is not turned on. The button that shows its text is an opt-in (`TextInput::password_with_reveal`) and is labelled with a word, not an icon.
- A visual-line or visual-caret move in the document editors starts from where the visual moves and clicks queued before it in the same frame lead. After any other queued operation (a logical move such as Left in left-to-right text, typing, a command) it starts from the last shaped frame, so it can step from a stale cursor. Several cursors in the code editor move logically.
- Rich documents have one font size per block, no images, tables, nested lists, alignment or colours, and paste as plain text. An edit reshapes only the blocks it changed, but the first frame and a change of width, font size or direction shape every block, not only the visible ones.
- The code editor's horizontal range comes from the line with the most columns in the document (a tab counts as its tab width, every other character as one), shaped once per edit. A line that shapes wider than that one, such as wide or fallback-font characters, still stretches the range while it is declared, so the scrollbar can then change length as the view scrolls past it.
- Bracket matching tells strings and comments from code only when the highlighter reports them (`Highlighter::strings_and_comments`, which `SyntectHighlighter` implements); without one, or on lines its parsing has not reached yet, every bracket counts. Auto-close does not look at them.
- A base direction forced with `Style::base_direction` orders each line of an editor, but text areas and code editors do not right-align a line that reads right to left (a rich editor and a text field do). Left and Right in tree rows keep their left-to-right meanings (Left collapses, Right expands) in a right-to-left window.
- Fluent formats numbers plainly; use `Ui::format_number` for locale notation. Only the default catalog is checked by `tr!`.
- No text selection handles or magnifier for touch.
- A selectable label (`Label::selectable`) is selected by dragging, Shift and click, the arrow keys, Home and End (by line) and Select All. It has no word or line selection by double and triple click, no Up and Down, and no scrolling while a drag is held past it. Assistive technology reads the label's text but gets no selection or text runs from it, and a press that lands on one does not reach a clickable container it is in.

## Collections and grids

- Row heights come from the model (`ListModel::row_heights`), not from content: a row is never measured, so wrapping text or an expanding row needs its height set by the application. A grid's rows all have the one height.
- A grid selects one cell at a time, with no range selection or block copy.
- Filters are text only. There are no pinned data rows (totals).
- A property grid with a `height` declares only the rows in view and a few beyond, but the view still names every property on each frame, so keep that closure cheap. Its rows and group headers share one fixed height, a grid without a `height` declares every row, and the editor that has focus goes when its row scrolls out of the declared window. Editing several objects works through `PropertyGrid::mixed`, which the view drives: a custom `with` editor cannot show a mixed value, and the derived `properties_many` compares fields with `PartialEq`. Collection fields such as `Vec<T>` are not supported by `#[derive(Properties)]`: adding, removing and reordering items needs rows whose identity follows the item, so use `with` or `custom` rows for them.
- Submenus nest to any depth in `Ui::menu_bar` only: `Ui::menu`, context menus and combo boxes take a flat list. A submenu opens as soon as the pointer is on its row, with no delay, and closes when the pointer crosses another row on the way to it. Tooltips are pointer-only, with one delay and one placement.

## Windows and docking

- Detached dock windows open where the OS puts them, and resizing them is not written back to the layout. A tab cannot be dragged from one OS window into another.
- A dock panel keeps its retained widget state (focus, scroll, undo history) through any change of the layout inside one window. While its tab is not the selected one its content is not declared, drawn or reachable, but its scroll offsets, undo history and the like are kept for the next selection; the keyboard focus is not. The state resets when the panel is closed or moves to another OS window, which has its own tree. `Ui::tabs` has no panel ids to keep state by and drops the content of a tab it leaves.
- Floating dock groups are clipped to the dock and cannot split.
- Windows share one graphics policy. A drag carries its payload, `Item` payloads included, to another window of the same application only where the platform reports where windows are on the screen, which Wayland does not. That path is covered by tests that drive two windows; nobody has dragged between two real windows by hand. A dock tab does not cross windows.
- Outgoing OS drags work for files on Windows and macOS, always as a copy, with a fixed icon. Linux has none; the drag crate needs a GTK window. The system drag itself has not been exercised with a real pointer.
- A file dialog blocks all windows. Notifications report nothing back. A tray icon without an image can be invisible.
- The native menu bar exists on macOS only; elsewhere `Ui::menu_bar` draws one in the window.
- Touch has been tested with injected input only, not on a touch screen or pen. There is no overscroll bounce, scroll chaining or inertia for a gesture-area pan.
- `AppOptions::decorations` is read when the window opens.

## Tooling

- Hot reload is a rebuild and restart that keeps state; it is not in-process code swapping. It saves one window's state, polls every 500 ms, and does not carry focus or scroll unless the application saves them.
- The inspector captures the full target tree on each of its frames, which a 100,000-node target will feel. GPU timings come from timestamp queries and were checked on one adapter.
- The public API snapshot is read from the sources: it records `cfg` gates as written without evaluating them, expands only the workspace's own macros, and records signatures, not behaviour.
- The test suite's allocation counts are the framework's own declarations, not allocator calls.
