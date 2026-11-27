# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0 a breaking change raises the minor version; see [API stability](docs/api-stability.md).

## [Unreleased]

### Added

- `Style::wrap_items(true)`: a row or a column whose children go on to a new line when the next does not fit, as `flex-wrap: wrap` does. The gap is between the lines too. In a stylesheet: `wrap-items: true`. See "Wrapping rows" in [styling](docs/styling.md).
- `Style::cursor(Cursor)` sets the pointer over a node and what is inside it, and `Context::cursor(id)` is what a host shows. The winit host now takes its pointer from it. Without a style the pointer is as it was: a hand over a widget that takes a press, an I-beam over text, resize pointers over dividers. See "The pointer" in [styling](docs/styling.md).
- A widget of your own can be clicked: `WidgetBuilder::on_click(message)` sends it for a press and release on the same widget, and for Enter, Space and an assistive Activate, and `Ui::custom` does the same. Dragging off and letting go sends nothing. Before, only `Role::Button` was pressed, and a painted control had to track its own hover to tell a click from a drag.
- `Semantics::checked`, `selected` and `expanded` carry a widget's state to assistive technology, as for the built-in checkbox, tab and disclosure. `Semantics` is built with `..Semantics::default()` everywhere in the documentation, but a `Semantics { .. }` written out in full, as `examples/dial-widget` did, no longer compiles until it ends with `..Semantics::default()`. The same goes for `Style`, which gained `cursor`, `focusable` and `focus_ring` in this release.
- `Style::focusable(false)` takes a node out of the focus order and stops a click from focusing it, for a widget that is focusable by kind, such as a divider or a scroll container, and not wanted as a stop. The node still reacts to the pointer. See "What takes focus" in [styling](docs/styling.md).
- `Style::margin`, `Style::margin_edges` and the `Edges` type: room kept clear outside a box, per side. `Style::border` and `Style::border_color`: a border drawn along the edge of the box, inside it, that follows its radius and takes room like padding. `Style::inset` is the padding and the border together, which is where the content starts. In a stylesheet: `margin`, `margin-top`, `margin-right`, `margin-bottom`, `margin-left`, `border`, `border-width` and `border-color`. See "Margin and border" in [styling](docs/styling.md).
- `FocusRing`, with `Style::focus_ring` and `Style::no_focus_ring`: one widget's focus ring in its own width, colour and inset, or none. `Path::rounded_rect` in `glassmere-scene` builds the stroked outline the ring uses. See "The focus ring" in [styling](docs/styling.md).

### Changed

- A widget of your own with the role `Checkbox`, `Radio`, `Tab`, `MenuButton`, `MenuItem` or `Option` is pressed like one with `Button`: a click, Enter and Space activate it. It was announced as what it said but reached by neither. A widget with one of these roles that handles Enter and Space itself through `on_event` now hears `UiEvent::Key` and then `UiEvent::Activate` for each press, where it heard only the key; one that toggles on both toggles twice, so handle one of them, or move to `on_click`, which is sent once. Its `PointerDown` and `PointerUp` are unchanged.
- Over a scroll area the winit host showed a hand, because a scroll container was focusable by kind; it shows the arrow. A button with `Style::focusable(false)` keeps its hand.

- A widget that takes no typed text shows its focus ring after the keyboard and not after a click or a touch, as a browser does. A field you type into still always shows it, and so does a window before the first press. A press hides the ring and the next key that does something shows it again. Code that checked for the ring in a scene after a click, as a snapshot test might, sees none: focus the widget with `Tab` or `Action::Focus` first.

- A scroll container is focusable only while it scrolls. One that fits its content, as a pane that is only there for layout, no longer takes focus from a click on it or on empty space or a spacer inside it, and Tab no longer stops on it. A click there leaves focus where it was, or clears it, as a click on a plain container does. A container that overflows still takes focus, so that the arrow keys, Page Up and Page Down scroll the one you clicked. It was focusable whatever it held; code that focused an empty scroll view with `Action::Focus` now finds the action refused.

### Fixed

- The focus ring of a node with rounded corners and no opaque background was four square bars. It is now a stroked rounded outline for every node with a radius, drawn inside its box over its fill. A node with a gradient or a translucent background no longer loses it while focused: the old ring replaced the fill of an opaque node with the focus colour and drew the background back inside it, and skipped the gradient.

## [0.1.8] - 2026-11-23

### Added

- Fields bound to the state. `app::bind` writes the value a widget reports into the state, and `app::change` is a message that changes the state and needs no name. Give the message one variant that holds an `app::Action` and a `From` for it; a field then needs no variant and no arm in `update` of its own. A bound field whose setter captures nothing allocates nothing when it is declared. See `examples/bindings.rs` and "Fields without messages" in [state](docs/state.md).
- A selectable label selects the word under the pointer on a double click and its paragraph on a triple click, and Shift with Up or Down extends the selection by a line, keeping the column. Up and Down alone still scroll. The word or paragraph stays selected while the pointer is dragged, and the drag extends from it by character in either direction; a press with Shift always extends the selection, whatever its place in a quick row.
- For contributors: benchmarks and a profiling runner in the new `glassmere-bench` crate (not published). `cargo bench -p glassmere-bench` times 24 headless scenarios and compares them against a saved baseline, and its `profile` binary writes a flamegraph, runs a fixed number of steps for a tool that counts instructions or allocations, and with `--fingerprint` hashes what every step left. See [measuring and profiling](docs/profiling.md).

### Changed

- On Linux and the BSDs `AppOptions::reduced_motion = false` now follows the system: animations are skipped when the `enable-animations` setting of GNOME, and of the desktops that follow it, is off, as they already are on Windows. To read it the host runs `gsettings get org.gnome.desktop.interface enable-animations` as a child process on a background thread, at startup and at most once a second when a window gains focus, and only where a session bus exists. It is not run when the option is already true. An application that must not start processes calls `follow_system_reduced_motion(false)` (in `glassmere-winit`, re-exported by `glassmere`), or the user sets `GLASSMERE_SYSTEM_MOTION=off` in the environment, and then the option alone decides, on every system. KDE Plasma's own animation setting and macOS are not read. Checked against the output of `gsettings`, not in a GNOME session.
- Two presses on one spot of a selectable label within 400 ms and 4 px are a double click, as in a text area. A test with a fixed clock that presses the same spot twice now gets a word selected: call `Context::set_now` between the presses to keep two single clicks.
- A stylesheet, and a file read through `Ui::watched_text`, `Ui::watched_bytes`, `LiveMarkup` or the designer, shows a change in about a tenth of a second; it could take half a second, and up to a second in a window that kept receiving events. The host looks at these files every 100 ms for two minutes after the window opens or one of them changes, and every 500 ms after that, so an idle window that follows a file wakes ten times a second during those two minutes. A changed file is read once it has been unchanged for 40 ms, so the first change after two quiet minutes takes up to 540 ms. Image files are still looked at every 500 ms, and a window that follows no file is not woken.
- The CPU renderer keeps a photo it has filtered down for 120 frames after it was last drawn, up to 64 MiB of them; it dropped one after two frames. Hovering over a thumbnail or repainting a gallery no longer filters the photo again, and the renderer can hold up to 64 MiB more.
- Children that leave a parent in the same frame are removed in the order they had among its children, so exit effects and damage rectangles come in the same order on every run. In 0.1.7 that order could differ between runs.
- Frames cost less. Measured with the new benchmarks on a form of 1,200 widgets: a frame with no input takes half the time, declaring the form again unchanged 40% less, repainting or laying out every row 40 to 50% less, and scrolling a million-row table or a 100,000-node tree 20 to 30% less. Two further passes, counted in instructions on the same scenarios, took 91% off a frame in which the pointer moves to another widget and then 21 to 29% off declaring, repainting, laying out and scrolling the form. The CPU renderer repaints a damaged row in a quarter of the time and paints the whole form in under a third of the instructions. A frame that moves the pointer over the form makes about 20 allocations where it made about 1,600. A custom `TextMeasurer` is asked less often: once per width for a wrapped text within a layout pass. The CPU renderer can hold shaped text that has left the screen until its next whole frame, which it paints once the cache holds more than twice the blocks of the last whole frame plus 256, or more than 64 KiB of text beyond what that frame held.

### Fixed

- Typing in the code editor or a text area no longer reads the whole document on every keystroke to find its widest line. In a 40,000-line file a keystroke went from 8.5 ms to 37 µs.
- A stylesheet or watched file is no longer read while it is being written. A save that truncates the file and then writes it, or removes it and creates it again, within 40 ms no longer empties the sheet, shows half of it or reports a missing file. A file that is empty or missing is read only 500 ms after the change was first seen, and only if it is still so, so a save that leaves it empty for a moment is not shown; a file still changing 500 ms after the change was first seen is read as it is.
- A change to a stylesheet, watched file or image file that kept the modification time but changed the size is now seen, as when two writes land within one tick of the file system's clock.
- With a screen reader running, a rich editor no longer works out what is read of every block on every accessibility update, only of the blocks that changed. In a document of 2,000 paragraphs an update went from about 11 ms to about 1 ms. The tree it publishes is the same, and still grows with the document.
- A large photo drawn at under half its size is filtered faster, in both renderers, and the result is byte for byte the same. On one machine (four cores at 2.1 GHz) the CPU renderer first draws a 3 MP photo at 150 by 100 in 9 to 10 ms where it took 17 ms. The first draw of a very large photo is still one slow frame.
- Painting a line of text at least as tall as the box it is in could panic, at positions where rounding left no room to place it, as in a dense grid cell or a tightly packed column. It no longer does, and the line is drawn from the top of the box.
- A number input whose `min` is above its `max`, or whose bound is not a number, no longer panics; the lower bound wins, as it does with `NumberOptions::range`. Markup reached it with `number "Size" { min: 10; max: 5 }`, which is how a range stands halfway through typing it.

## [0.1.7] - 2026-11-18

### Added

- Markup: a view can be written in a `.glass` file and read while the application runs, behind the `markup` feature (`glassmere::markup`, the new `glassmere-markup` crate). See [Markup and the designer](docs/markup.md) and `examples/markup.rs`.
  - Elements for the layout containers and the basic controls, stylesheet properties and classes on any element, expressions over the application's `Values`, `if`, `for`, components with parameters, `on-*` events and two-way `bind`.
  - `Markup::parse` for a view compiled into the binary, and `LiveMarkup` for one that follows its file on disk and keeps the last version that parsed.
  - Errors carry the line, the column and what was expected.
- `glassmere-designer`, a live preview and visual editor for markup files: an outline, picking an element in the preview, editing its text, class and properties, adding, moving and deleting elements, test data and an event log. Every change is written back to the file. `editors/vscode` adds syntax highlighting and a preview command to VS Code.
- Embedded: `glassmere-core` and `glassmere-scene` build for `no_std` targets with `alloc` when their new default `std` feature is off, and the new `glassmere-embedded` crate renders to a plain pixel buffer. See [Embedded](docs/embedded.md).
  - `EmbeddedRenderer` paints a scene into a `Framebuffer` of `Rgb565`, `Rgb888`, `Xrgb8888`, `Gray8` or `Mono` pixels: a whole frame, a band of one or only the damaged rectangles.
  - Bitmap fonts built in (6x13 and 9x18, regular and bold, about 35 KiB), with a `TextMeasurer` that agrees with them.
  - `EmbeddedUi` runs an `App` from a firmware loop: touch and key input in, the time from a hardware timer, damage out.
  - `examples/embedded-firmware` is a firmware crate checked for `thumbv7em-none-eabihf`, and `examples/embedded.rs` shows it in a window. The `embedded` feature re-exports the crate as `glassmere::embedded`.
- `Style::set_property` sets one property from its stylesheet name and value.
- `Ui::context` reads the tree as the previous frame left it, for hit tests from a view.

### Changed

- `glassmere-core` and `glassmere-scene` have a `std` feature, on by default. A dependency on either with `default-features = false` now has to ask for `std` to keep threads, files, channels and the text editors. The `regex` feature of `glassmere-core` implies `std`.

### Fixed

- An event that reaches an event area over content that does not react to it is handled at once. Before, the area's message waited for the next unrelated event, such as the pointer leaving the window.
- On Windows, the system's minimize, maximize and close buttons work in a window that does not draw its own. Before, their clicks were kept from Windows.
- A maximized window with a drawn title bar reacts to the pointer in its top 8 pixels. Before, the resize edge covered them.
- A line of text taller than the room its box leaves for it is no longer cut at the descenders, as in a grid cell.
- Checkboxes and radio buttons draw their marks as shapes, so they no longer depend on the font having the glyphs.
- A menu separator is a rule across the menu instead of a row of dashes.
- The close button of a tab and the labels of the rich text toolbar are centred, the strip beside a grid's sideways scrollbar is filled, and the "Value" header of a property grid clears the divider.
- A markup file may start with a byte order mark.

## [0.1.6] - 2026-11-15

### Added

- Basic controls, each taking a spec with `new(label)` and `style(..)`, and shown in `examples/controls.rs`:
  - `Ui::progress`, a bar for a value from 0 to 1, or an indeterminate one for `None`.
  - `Ui::spinner`, a busy indicator.
  - `Ui::switch`, an on and off toggle whose thumb eases with the style's transition.
  - `Ui::collapsible`, a header that shows or hides the content under it. The application owns the open state.
  - `Ui::link`, underlined text that sends a message.
  - `Ui::range_slider`, a slider with two thumbs that cannot cross.
  - `Label::selectable`, a label whose text can be selected and copied.
- Stylesheet variables and nesting. `--name: value` declares a variable and `var(--name)` or `var(--name, fallback)` uses it. A class may contain `&:state` and `&.name` blocks. Both work in files and in `style_sheet!`.
- Animated GIF, WebP and APNG play in image widgets. `Image::animated` builds one from `ImageFrame`s, and `Image::frames` and `Image::loops` read it back. A background image shows the first frame.
- Rows of a list or tree can differ in height: `ListModel::row_heights` returns a `RowHeights`, and `TreeModel::set_row_height` sets one row of a tree. Scrolling, paging and the scrollbar follow the real heights.
- Property grid:
  - A grid with a `height` declares only the rows in view, however many properties there are. Its rows and group headers are then 44 px high.
  - Several objects can be edited at once: `PropertyGrid::mixed` shows "Mixed" where they differ, and `#[derive(Properties)]` generates `properties_many`. A `flatten` group shows the first object's rows unless it is `#[property(flatten, many)]`, which needs `properties_many` on the nested type.
  - A fieldless enum that derives `PropertyChoice` is edited as a drop-down of its variants.
  - An `Option<T>` field is edited as `T` beside a box that sets or clears it, through `PropertyGrid::optional`.
- `Ui::calendar`, a month grid, and `Ui::date_picker`, a field that opens one, both on `Date` with `min`, `max` and the first weekday. `Date::new` checks a date, `Date::weekday` returns the new `Weekday`. `today` marks a day with a dot and is where a calendar without a selection starts. Month and weekday names are catalog keys, with German and Czech in `glassmere-fluent`. See `examples/dates.rs`.
- `Ui::command_menu` and `Ui::command_context_menu` show a `Menu` with separators and nested submenus, as a menu button and as a context menu. Their commands are sent the way `Ui::menu_bar` sends them.
- `Ui::tooltip_with` takes a `Tooltip` with a placement (`TooltipPlacement`: auto, above, below, before or after), a delay and a style. A placed tooltip flips at the window edge, and before and after mirror in a right-to-left window.
- `GridModel::footer` gives a data grid rows pinned under its body, for totals.

### Changed

- A tooltip also shows when its widget gets the focus from the keyboard, and hides on blur, on Escape or on a key that changes the value. A tooltip shown this way takes that Escape.
- A tooltip shows sooner when the pointer moves to it right after another one was on show.
- A submenu opens after the pointer has rested on its row for 200 ms, and stays open while the pointer crosses other rows on the way to it. A click, Enter and the arrow keys open it at once.

### Fixed

- A dock panel keeps its scroll position and undo history while another tab of its group is selected. Before, selecting another tab rebuilt the panel's content.
- A press on empty space outside a context menu removes it at once. Before, it could stay painted until the next redraw.
- A submenu that fits on neither side of its row in a narrow window stays inside the window and clear of the row that opened it, instead of covering its parent menu.

## [0.1.5] - 2026-11-14

### Changed

- Regex find matches across lines, and `^` and `$` match at the edges of every line. Before, `$` matched only at the end of the last line. A class that excludes a character, such as `[^x]*`, can now run over a line break. A plain query that contains a line break matches across lines too.
- Damage from `Context::damage_regions` is up to eight rectangles; changes that overlap or lie within 8 px share one. Both renderers repaint each on its own, so a custom wgpu draw is called once for each region it meets.
- An image drawn at under half its size is filtered down in halving steps, in linear light, before it is sampled, in both renderers. It no longer shimmers, and a large image shown small takes a small texture.

### Fixed

- A dock panel keeps its focus, scroll position and undo history when the layout changes around it: when its group moves into a split, its tab moves to another group, it floats or docks, or a neighbour closes. Before, any of these rebuilt the panel's content.
- A tab strip with more tabs than fit shows a scroll button and a shade at each side that has hidden tabs, in `Ui::tabs` and in dock groups. A held button keeps scrolling.
- In a data grid, Up, Down, PageUp, PageDown and the wheel step a number cell being edited by the column's step, and the editor reports as a spin button.
- `TextDocument::find_all_regex` returns char ranges. It returned byte offsets, so matches after any non-ASCII character were misplaced.
- The code editor's horizontal scrollbar no longer changes length as the view scrolls past lines of different widths.
- Several arrow, Home, End or page presses between two frames each move the caret in the rich editor, and visual left and right in a text area. Before, they moved it one step in total.
- `Style::base_direction` forces the paragraph direction in text fields, text areas, the code editor and the rich editor. Before, editors ignored it.
- The rich editor reshapes only the blocks an edit changed, also after a block is inserted or removed before them.
- Two changes far apart no longer repaint the box between them, and an exit animation repaints only where it is drawn instead of the whole window.
- An image with an embedded ICC profile for RGB or grey pixels (PNG, JPEG, WebP) is converted to sRGB when it is decoded, so a Display P3 or Adobe RGB photo no longer looks oversaturated or dull. `glassmere-image` now depends on `moxcms` directly.
- The yanked `async-executor` 1.14.0 in `Cargo.lock` is replaced with 1.14.1.

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
