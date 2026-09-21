# Milestone 7: advanced features

Slices so far: 7a docking and 7b rich text with the code editor foundation (below).

# Milestone 7a: docking

Implemented on 2026-09-21. An app-owned `DockLayout` describes a tree of splits and tab groups, floating groups inside the window and detached windows. `Ui::dock` renders it, lets tabs be dragged onto drop indicators, other tab strips or empty space, and gives every group a panel menu with the same moves for keyboard and screen-reader users. Layouts persist through a built-in text format or, with the `serde` feature, any serde format.

## Run

```powershell
cargo run -p rust-gui --example docking --release
cargo run -p rust-gui --example docking --release -- --open    # starts with a floating panel, saves nothing
cargo run -p rust-gui --example docking --release -- --reset   # ignores the saved layout
```

The example saves its layout to the temp directory on every change and restores it at start, detached windows included.

```rust
let mut layout = DockLayout::new(DockNode::row(
    0.25,
    DockNode::tabs(["files", "search"]),
    DockNode::column(0.7, DockNode::tabs(["editor", "preview"]), DockNode::tabs(["output"])),
));
let panels = [DockPanel::pinned("editor", "Editor"), DockPanel::new("files", "Files") /* ... */];

let response = ui.dock("dock", &mut layout, &panels, DockOptions::default(), |ui, id| match id {
    "editor" => editor(ui, state),
    _ => ui.label(id),
});
if response.changed { std::fs::write(path, layout.to_text())?; }

layout.show("search");                                   // reopen or select a panel
layout.move_panel("output", DockTarget::Edge { area: DockArea::Main, zone: DockZone::Right });
let restored = DockLayout::parse(&text)?;                // line-numbered errors
```

```text
dock v1
main {
    split horizontal 0.25 {
        tabs 0 "files" "search"
        split vertical 0.7 {
            tabs 1 "editor" "preview"
            tabs 0 "output"
        }
    }
}
float 120 80 320 240 {
    tabs 0 "inspector"
}
window 1 640 480 {
    tabs 0 "console"
}
```

## Contracts

| API | Behavior |
| --- | --- |
| `DockLayout { root, floating, windows }`, `DockNode::{Tabs, Split}` | Plain public data owned by the app. Panels are app-chosen string ids. `DockNode::tabs`, `row` and `column` build trees. |
| `DockLayout::move_panel(panel, DockTarget) -> bool` | Targets: `Group { anchor, zone }` (beside or into the group holding `anchor`), `Tab { anchor, index }` (position in the visible strip), `Edge { area, zone }` (a quarter along a whole edge, `Center` joins the first group), `Float { .. }` and `Window { .. }`. Unknown panels or anchors and moves that change nothing return false and leave the layout untouched. Emptied groups, floats and windows disappear and one-sided splits collapse. Floating groups take tabs but never split. |
| `show`, `activate`, `close`, `contains`, `panels`, `group_of`, `groups_in`, `place_of`, `retain_panels`, `redock`, `remove_window`, `normalize` | Query and edit helpers. `normalize` drops duplicates and empty containers and repairs non-finite or out-of-range numbers; `Ui::dock` calls it, so hand-edited or deserialized layouts are safe. |
| `to_text`, `DockLayout::parse` | The `dock v1` format above with `//` comments, `\"` and `\\` escapes and `DockLayoutError { line, message }`. Parsing normalizes. |
| `serde` feature (`rust-gui/serde`, `rust-gui-core/serde`) | `Serialize` and `Deserialize` on the layout types and `Axis`. |
| `Ui::dock(key, &mut layout, &[DockPanel], DockOptions, content) -> DockResponse` | Declares the main tree with draggable dividers, tab groups and floating groups. `content(ui, id)` runs for each visible panel. Layout ids without a declared `DockPanel` are skipped, so old files survive app changes; declared panels missing from the layout stay hidden until `show`. `DockPanel::pinned` has no close button. The response reports `changed`, `closed` panel ids and newly `detached` window ids. |
| Tab dragging | After six pixels a tab drag shows a compass on the group under the pointer (left, right, top, bottom, centre), four whole-dock edge indicators, a translucent preview and the tab title. Tab strips accept insertion and reordering. A release over no indicator floats the panel there; outside the window it detaches when `native_windows` is set and otherwise changes nothing. Escape or focus loss cancels. |
| Panel menu | Each group has a `▾` menu button acting on its active panel: Float, Move to new window, Move to main window, Dock left/right/top/bottom, Move to next group, Close. Items that would change nothing are disabled. Right-clicking a tab (which also activates it) or Shift+F10 / the Menu key on a focused tab opens the same menu. |
| Floating groups | Drawn above the tree in `floating` order, back to front; their surface blocks clicks to content below. The grip moves and the corner resizes, by pointer or arrow keys (ten pixels), within the dock and down to 160 by 100. Moving, resizing or pressing inside a float raises it. |
| `Ui::dock_window(key, &mut layout, DockWindowId, ..)` | The dock of one detached window, with its own compass and edges. It declares nothing once the layout no longer has the window. |
| `DockWindows::sync(&desktop, &layout, open)`, `DockWindowView`, `DockWindows::redraw` | Host helper that keeps one OS window per `layout.windows` entry: it opens missing ones, closes windows whose entry left, and runs `on_close` when a window is gone for any reason, where apps usually call `layout.redock(id)`. Views share state through `Arc<Mutex<..>>` as for any `run_windows` app. |
| `Style::offset(x, y)` | New general style property: takes a node out of flow at an offset from its parent's top-left corner. Floating groups use it. |

## Validation

Ten docking tests. Four cover the model: moves between groups, edges, floats and windows with collapsing containers, no-op and unknown moves, tab positions, `show`/`redock`/`retain_panels`, and the text format's round trip, repair of damaged input and error lines. One asserts the serde trait bounds under `--all-features`; no serde format crate is in the lockfile, so there is no serialized round-trip test. Five drive `Ui::dock` headlessly: active panels and fill geometry, tab switching and closing, drags onto a compass indicator, another strip, empty space and a dock edge with Escape cancelling, floating groups moved and resized by pointer and keys with click blocking and raising, the panel menu from the keyboard including detach and return through `dock_window`, right-click menus, and releases outside the window with and without native windows. The full suite is 103 tests plus 9 GPU tests; clippy is clean with and without the `serde` feature. The docking example passes `--smoke`; screenshots confirmed the split tree, tab groups, a floating group with grip and resize corner, and the panel menu opened by right-clicking a tab.

Not verified natively: a real pointer drag with its indicators, and tearing a panel off into an OS window and closing it. A background script cannot drive the pointer into the window, so these paths are covered headlessly and by the `DockWindows` code being small. They deserve a manual pass.

## Remaining limits

- A tab cannot be dragged from one OS window into another; use "Move to main window" or close the detached window, which re-docks its panels in the example.
- Detached windows open at the OS-chosen position, and resizing them is not written back to the layout.
- Retained widget state under a panel (focus, scroll offset, text-edit history, enter animations) resets when its group is re-parented by a structural change, because node identity follows the tree path. Switching tabs already behaves this way.
- Floating groups are clipped to the dock and cannot split.
- Tab strips do not scroll; with many tabs the panel menu button can be pushed out of view, though right-click and Shift+F10 still reach the menu.
- The panel menu has nine items and the shared popup height cap makes it scroll.
- The wheel over a floating group can still scroll a scroll view underneath it.
- "Move to next group" and edge docking are the keyboard vocabulary; there is no keyboard equivalent of dropping onto a specific group's side.

# Milestone 7b: rich text and the code editor foundation

Implemented on 2026-09-21. Text can now be a block of differently styled runs, and a virtualized code editor works on an app-owned, rope-backed document. This is the first of three text slices: 7c adds syntect highlighting, find and replace, bracket features, multiple cursors in the UI and diagnostics; 7d adds the WYSIWYG rich text editor.

## Run

```powershell
cargo run -p rust-gui --example editor --release
```

```rust
let intro = RichText::new()
    .text("See ").link("the docs", "https://example.test/docs")
    .text(" for ").bold("bold").text(", ").italic("italic").text(" and ").code("code()");
if let Some(target) = ui.rich_text("intro", &intro, Style::default().wrap(TextWrap::Word)) {
    open(target);
}

let mut document = TextDocument::from_text(&source);      // app state
let response = ui.code_editor("code", &mut document, CodeEditorOptions {
    label: "Rust source".into(),
    highlighter: Some(&mut highlighter),                   // any `Highlighter`
    ..CodeEditorOptions::default()
});
if response.changed { mark_dirty(); }
```

## Contracts

| API | Behavior |
| --- | --- |
| `TextSpan`, `Primitive::RichText { rect, spans, style, color }`, `TextMeasurer::geometry_rich` | One shaped block of runs. Each span may override colour, weight, slant and family; font size, line height and wrapping belong to the block. Geometry byte offsets index the concatenated span texts. The wgpu renderer shapes with glyphon rich text and caches rich blocks separately from plain ones. |
| `RichText`, `RichRun`, `Ui::rich_text(key, &RichText, style) -> Option<String>` | A read-only paragraph of runs: bold, italic, inline code (monospace on a tinted background), underline, strikethrough, colours, backgrounds and links. It measures and wraps with its real span fonts. A block with links is focusable: a click, or Enter on the active link (arrow keys move between links), returns the link target once. The plain text is the accessible label. |
| `TextDocument` | App-owned document on a `ropey` rope. Offsets are chars; `position`/`offset` convert to `(line, byte)`. Several selections edit together (`add_selection`, merged when they overlap). Edits: `insert`, `backspace`, `delete`, `newline` (keeps indentation, adds one unit after an opening bracket), `indent`/`outdent` (whole lines for multi-line selections, to the next stop at a caret), `replace_range(s)`, `cut`/`copy_text` (the caret's line when nothing is selected). `move_cursors(Motion, select)` covers graphemes, words, smart line start, sticky-column vertical moves, pages and document ends. Undo is edit-based, merges plain typing and restores selections. `revision`, `is_modified`/`mark_saved` and `changed_since(revision)` (first changed line) serve save prompts and incremental consumers. CRLF input is normalized and written back by `text()`. |
| `Ui::code_editor(key, &mut TextDocument, CodeEditorOptions) -> CodeEditorResponse` | A monospace, non-wrapping editor. Input is queued as operations and applied at the next declaration, so the app's document is never touched between frames. Only the visible lines plus eight on each side are declared, shaped and painted, so cost does not depend on document length. Gutter with line numbers, current-line band, multi-caret painting, selection across lines, double-click word and triple-click line selection, Shift+click and drag selection, wheel scrolling, caret reveal in both directions, PageUp/PageDown, Ctrl+Home/End, read-only mode. |
| Keyboard focus | Tab and Shift+Tab indent. Escape followed by Tab moves focus out, and read-only editors never take Tab. Escape still reaches dialogs unless it collapses several selections. |
| IME | Preedit text shows underlined at the primary caret without touching the document; commit inserts it. `ime_cursor_rect`, `accepts_text` and `composing_text` cover editors. |
| Clipboard | The text lives with the app, so copy and cut are queued with `Context::request_clipboard(cut)` and the text arrives through `Context::take_clipboard()` after the declaration; the desktop host does both. Paste is an ordinary insert. |
| `Highlighter`, `HighlightSpan` | Per-line colouring asked only for declared lines in ascending order. Implementations keep their own state and use `revision`/`changed_since` to reuse it. |
| Accessibility | Role TextInput with the option label; the value is the caret's line. |

## Validation

Fourteen new core tests. Five unit tests cover the document: typing coalescing and undo/redo, grapheme, word, smart-home and sticky-column movement over combining marks and ZWJ emoji, multi-selection edits undone as one step, newline/indent/outdent/cut with CRLF round trip, and revision tracking. Six drive the code editor headlessly: queued edits applied at declaration with auto-indent and undo, a 100,000-line document declaring only 11 visible lines with wheel, page and caret-reveal scrolling, pointer placement with drag, Shift+click, double and triple clicks, the clipboard outbox with read-only behaviour, a highlighter called for visible lines only with its spans reaching the scene, and IME preedit, accessibility value and the Escape-Tab exit. Three cover rich text: spans, backgrounds and rules, wrapping and re-layout when runs change, and links by click and keyboard. The renderer gained a unit test for rich geometry and two GPU tests (span colours reach the pixels, cached replay is identical). The full suite is 118 tests plus 11 GPU tests. The editor example passes `--smoke`; a screenshot confirmed wrapped rich text with styles and links, and the code editor with gutter, coloured spans, caret and current-line band.

Not verified natively: typing, IME composition and clipboard round trips in the real window, because a background script cannot give the window keyboard focus for input. They are covered headlessly.

## Remaining limits

- Copy is asynchronous by one declaration, and a cut whose clipboard write fails has already removed the text (undo restores it).
- The editor has no horizontal scrollbar or wheel; horizontal scrolling follows the caret only. Dragging a selection past the edge does not auto-scroll.
- Vertical moves keep a display column that counts graphemes, with tabs expanded to `tab_width`; wide CJK characters count as one column, and the renderer's own tab stops may differ from `tab_width`.
- Very long single lines are shaped whole, and line operations copy the line.
- Rich blocks have one font size; links share one pointer cursor with the whole block; assistive technology sees the block as a label, not separate link objects.
- On macOS, Cmd+Up/Down are not mapped to document start and end yet.
