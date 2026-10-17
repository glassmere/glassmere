# Milestone 7: advanced features

Slices so far: 7a docking, 7b rich text with the code editor foundation, 7c code intelligence, 7d the WYSIWYG rich text editor, 7e bidirectional caret order, 7f text direction with right-to-left layout, 7g hot reload and 7h framework diagnostics (below). Phase 7 is complete.

# Milestone 7a: docking

Implemented on 2026-09-21. An app-owned `DockLayout` describes a tree of splits and tab groups, floating groups inside the window and detached windows. `Ui::dock` renders it, lets tabs be dragged onto drop indicators, other tab strips or empty space, and gives every group a panel menu with the same moves for keyboard and screen-reader users. Layouts persist through a built-in text format or, with the `serde` feature, any serde format.

## Run

```powershell
cargo run -p seaglass --example docking --release
cargo run -p seaglass --example docking --release -- --open    # starts with a floating panel, saves nothing
cargo run -p seaglass --example docking --release -- --reset   # ignores the saved layout
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
| `serde` feature (`seaglass/serde`, `seaglass-core/serde`) | `Serialize` and `Deserialize` on the layout types and `Axis`. |
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
cargo run -p seaglass --example editor --release
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

# Milestone 7c: code intelligence

Implemented on 2026-09-21. The code editor gains syntect highlighting, find and replace, bracket pairing and matching, multiple cursors from the keyboard and pointer, and app-supplied diagnostics.

## Run

```powershell
cargo run -p seaglass --example editor --release
cargo run -p seaglass --example editor --release -- --open   # starts with the find bar searching "counts"
```

```rust
use seaglass::syntax::SyntectHighlighter;

let mut highlighter = SyntectHighlighter::new("rs", dark);   // extension, token or syntax name
let mut find = FindState::default();                         // app state
let diagnostics: Vec<Diagnostic> = language_server_results();

ui.code_editor("code", &mut document, CodeEditorOptions {
    highlighter: Some(&mut highlighter),
    find: Some(&mut find),
    diagnostics: &diagnostics,
    ..CodeEditorOptions::default()                            // auto_close and match_brackets are on
});
```

## Contracts

| API | Behavior |
| --- | --- |
| `seaglass-syntax`, `seaglass::syntax::SyntectHighlighter` (facade feature `syntax`, on by default) | syntect's bundled Sublime grammars and themes through the pure-Rust `fancy-regex` engine, so no C toolchain is needed. `new(language, dark)` picks `base16-ocean.dark` or `InspiredGitHub`; `with_theme`, `languages()`, `themes()` and `set_language` cover the rest. Unknown languages highlight as plain text. |
| Incremental highlighting | Parser state is checkpointed every 32 lines and spans are cached per line. An edit re-parses from its first changed line (`TextDocument::changed_since`). Jumping far into a large file parses at most 1,500 catch-up lines per declaration: lines stay plain meanwhile, `Highlighter::pending` is true and the editor keeps requesting frames until colours arrive. |
| `Highlighter::begin`, `pending`; `Ui::request_redraw` | New optional trait hooks for per-declaration budgets, and a general way for a view to ask for another frame; `Context::animating` reports it to hosts. |
| `FindState`, `CodeEditorOptions::find` | With a find state, Ctrl+F opens a find bar above the text (Ctrl+H adds the replace row), seeded from a single-line selection. The query field takes focus; Enter, F3 and Shift+F3 step through matches with wrap-around; "Match case" and "Whole word" narrow them; the count reads "3 of 17". Replace acts on the selected match and moves on; Replace all is one undo step. Escape closes the bar and returns focus to the text. All matches in view are marked, the selected one in the accent colour. Searches re-run only when the document or the query changes. |
| `TextDocument::find_all`, `replace_ranges`, `select_next_occurrence`, `add_cursor_vertically`, `matching_bracket`, `insert_paired`, `backspace_paired` | The document-level operations behind the features, usable without the widget. |
| Bracket pairing (`auto_close`) | Typing `(`, `[`, `{`, `"` or a backtick inserts the partner, or wraps the selection; typing a closer that is already there steps over it; a quote right after a word does not pair; Backspace inside an empty pair removes both; Enter between a pair opens an indented block. |
| Bracket matching (`match_brackets`) | The bracket at or just before the caret and its partner are marked, scanning up to 20,000 characters. |
| Multiple cursors | Alt+click adds a caret, Ctrl+Alt+Up/Down add one above or below in the same column, Ctrl+D selects the word and then each next occurrence, Escape returns to one cursor. All cursors type, delete, move and undo together. |
| `Diagnostic { range, severity, message }`, `Severity` | Underlined in the severity colour with a gutter dot per line (the highest severity wins). Resting the pointer on one shows its message next to the pointer after the tooltip delay. |
| `Event::Editor(EditorCommand)`, `Context::editor_focused` | Host-neutral commands: `Find`, `Replace`, `FindNext`, `FindPrevious`, `CloseFind`, `SelectNextOccurrence`, `AddCursorAbove`, `AddCursorBelow`, `AddCursorAt(point)`. The desktop host maps the keys above while an editor or its find bar has focus; apps can send the same commands from menus. |
| `FocusScope::autofocus` | New: a scope that appears moves focus to its first control. With `restore`, closing it returns focus. The find bar uses both. |

`Ui::code_editor` is now a small composite: a container holding the optional find bar and the text area. Its `style` sizes the container; padding, radius and background apply to the text area.

## Validation

Ten new tests. Two document unit tests cover pairing, wrapping, step-over, pair deletion and block opening, and search, next-occurrence selection, vertical cursors and bracket matching. Four editor tests drive find and replace from commands, the query field, F3 stepping with wrap, options, Replace and Replace all with undo, and Escape restoring focus; multiple cursors from Alt+click, vertical adds and Ctrl+D with joint typing and undo; bracket pairing while typing plus the two bracket marks; and diagnostics with underline and gutter quads, a wake request after the dwell and the message shown and hidden by pointer movement. Four syntax tests check that keywords, strings and comments differ and that a block comment colours the next line, that an edit re-highlights later lines and undo restores them, that a far jump in a 6,000-line file takes several declarations while `pending` and then serves nearby lines from checkpoints, and that unknown languages and theme names are handled. The full suite is 128 tests plus 11 GPU tests; clippy is clean, and the facade builds with and without the `syntax` feature. A screenshot of the example confirmed syntect colours, the focused find bar with seven marked matches, and the warning and error underlines with gutter dots.

Not verified natively: the key mappings (Ctrl+F, Ctrl+H, F3, Ctrl+D, Ctrl+Alt+Up/Down, Alt+click) and hover messages in the real window, since a background script cannot focus the window for input. The commands behind them are covered headlessly.

## Remaining limits

- syntect adds noticeable build time and binary size; disable the facade's `syntax` feature to leave it out. Loading the bundled grammars costs a one-time delay on the first highlight.
- Search is plain text, single line and case folding is per character; there is no regular expression search.
- Bracket features do not know about strings and comments.
- Pressing Ctrl+F while the bar is already open does not move focus back to the query field.
- Severity colours are fixed, not theme tokens.
- The find bar's controls are not yet styleable.

# Milestone 7d: the WYSIWYG rich text editor

Implemented on 2026-09-21. An app-owned `RichDocument` of blocks and marked runs, edited in place by `Ui::rich_text_editor` with wrapped paragraphs, a formatting toolbar and keyboard shortcuts, and loaded from and saved to a Markdown subset. This completes the rich text and code editor work of phase 7.

## Run

```powershell
cargo run -p seaglass --example richtext --release
```

The example edits a document on the left and shows its Markdown, highlighted by syntect, on the right.

```rust
let mut document = RichDocument::from_markdown(&std::fs::read_to_string(path)?);   // app state

let response = ui.rich_text_editor("notes", &mut document, RichEditorOptions {
    label: "Release notes".into(),
    ..RichEditorOptions::default()                                                 // toolbar on
});
if response.changed { std::fs::write(path, document.to_markdown())?; }

document.toggle_mark(Mark::Bold);                 // the same operations are public
document.set_block_kind(BlockKind::Heading(2));
document.set_link(Some("https://example.test"));
```

## Contracts

| API | Behavior |
| --- | --- |
| `RichDocument`, `Block { kind, runs }`, `InlineRun { text, marks }`, `Marks`, `BlockKind` | Blocks are paragraphs, headings 1 to 3, bulleted and numbered list items, quotes and code blocks; marks are bold, italic, underline, strikethrough, inline code and a link target. Runs with equal marks merge. A `RichPosition` is a block and a byte offset; there is one selection. |
| Editing | `insert_text` (line breaks start blocks, or stay inside a code block), `newline` (a heading is followed by a paragraph; an empty list item or quote becomes a paragraph; a second Enter at the end of a code block leaves it; formatting continues on the new line), `backspace`/`delete` with word variants (at the start of a styled block Backspace first makes it a paragraph, then joins blocks), deletes across blocks, `move_cursor` over graphemes, words, blocks and the document, `select_word`, `select_block`, `select_all`, `cut`, `selected_text`. |
| Formatting | `toggle_mark` removes the mark when the whole selection has it and adds it otherwise; at a caret it applies to what is typed next. `set_link`, `set_block_kind` (toggles back to paragraph), `marks()` and `block_kind()` report the state at the cursor. |
| Undo | Whole-document snapshots, 300 deep, with consecutive typing merged into one step. |
| `to_markdown`, `from_markdown` | `#` headings, `-` and `1.` lists, `>` quotes, fenced code, `**bold**`, `*italic*`, `~~strike~~`, `<u>underline</u>`, `` `code` `` and `[text](target)`, with backslash escapes. Marks are written as a stack so neighbouring runs share delimiters, and reading what was written gives the same document. |
| `Ui::rich_text_editor(key, &mut RichDocument, RichEditorOptions) -> RichEditorResponse` | Each block is one wrapped rich text block: headings scale the body size, list items get bullets or numbers, quotes a bar, code blocks a monospace panel. Input is queued and applied at declaration like the code editor, including the clipboard outbox and inline IME preedit. Up, Down, Home, End, PageUp and PageDown follow the wrapped visual lines with a sticky column, resolved from the shaped text. Click, drag, Shift+click, double-click word and triple-click block selection. The caret is kept in view and the wheel scrolls. `response.link_at_cursor` reports the link under the cursor. |
| Toolbar and shortcuts | The optional toolbar toggles marks and block kinds, shows the state at the cursor and returns focus to the text after a click. Ctrl+B, Ctrl+I, Ctrl+U, Ctrl+E (code) and Ctrl+Shift+X (strikethrough) arrive as `EditorCommand::Toggle*`. Tab moves focus, as in ordinary fields. |
| `Ui::request_focus(key)` | New: moves focus to a sibling widget after the declaration is reconciled. |
| Accessibility | Role TextInput with the option label; the value is the text of the cursor's block. |

One renderer fix came out of this slice: cosmic-text shapes an ASCII word in a single run when every explicit span overlapping it is compatible, and default-attribute text is not an explicit span, so punctuation after a bold or monospace run took that run's style. The renderer now gives rich blocks defaults that no run uses, which makes every run explicit.

## Validation

Ten new tests. Five unit tests cover the document: typing marks, selection marks and links splitting and merging runs; Enter, Backspace and cross-block deletes; code blocks; grapheme, word and block movement; and the Markdown round trip with nested marks, links and escapes. Five drive the widget headlessly: queued typing with bold carried across Enter, italic on a keyboard selection and undo, plus role, label and value; visual-line keys over wrapped text with End staying on its line and the sticky column through a short paragraph; pointer placement, a drag across blocks with its selection quads, double and triple clicks; the toolbar formatting the selection, showing active state and returning focus; and the clipboard outbox, IME preedit and commit, scrolling to the end of a 60-block document and read-only mode. The full suite is 138 tests plus 11 GPU tests; clippy is clean. The richtext example passes `--smoke`; screenshots confirmed headings, marks, a wrapped paragraph, bullets, numbers, the quote bar, the code block, the active toolbar button, and the punctuation fix above.

Not verified natively: typing, shortcuts, IME and the toolbar in the real window, because a background script cannot give the window keyboard focus. They are covered headlessly.

## Remaining limits

- All blocks are shaped every time their content or the width changes; there is no virtualization, so very long documents will slow down. Undo snapshots copy the whole document.
- Visual-line keys resolve against the last shaped frame, so several Up or Down presses between two frames move one line.
- (Resolved in 7e: End on a wrapped line now lands at the true end of the line through caret affinity.)
- One font size per block; no images, tables, nested lists, alignment or colours.
- Links can be set from code and are reported at the cursor, but there is no link dialog and no click-to-follow.
- Paste is plain text; copy puts plain text on the clipboard.
- Marks continue across Enter but a link does not.

# Milestone 7e: bidirectional caret order

Implemented on 2026-09-21. Left and Right now move the caret on screen through mixed left-to-right and right-to-left text, the caret has an affinity so one offset can sit on either side of a direction boundary or wrapped line, and all three editors use it. Logical movement stays available as a setting. Text direction styles and layout mirroring follow in 7f.

## Run

```powershell
cargo run -p seaglass --example editor --release   # the field above the code mixes Latin, Hebrew and Arabic
```

```rust
context.set_caret_movement(CaretMovement::Logical);              // default: CaretMovement::Visual
run(AppOptions { caret_movement: CaretMovement::Logical, ..AppOptions::default() }, view)?;
```

## Contracts

| API | Behavior |
| --- | --- |
| `Affinity::{Downstream, Upstream}` | Which neighbour a caret belongs to: the leading edge of the character after the offset, or the trailing edge of the one before. They differ at direction boundaries and between wrapped lines. |
| `TextGeometry::lines`, `TextLine { y, height, rtl, bytes }` | Every measurer now reports visual lines with their base direction and byte range. A wrapped line ends at the offset where the next one starts. |
| `TextGeometry::caret_at(byte, affinity)`, `hit_affinity(point)`, `visual_step(byte, affinity, right)`, `line_edge(byte, affinity, end)`, `line_at`, `mixed_direction` | Caret rectangle for an affinity; a hit that remembers which side of a character was clicked; one step left or right on screen, landing on the far edge of the character stepped over and returning `None` at the visual end of a line; the logical start and end of the caret's visual line (the end is upstream, so it is drawn on that line); and the direction to flag when a line mixes directions. |
| `CaretMovement::{Visual, Logical}`, `Context::set_caret_movement`, `AppOptions::caret_movement` | Visual (the default): Left is left on screen in every run, and Ctrl+Left and Ctrl+Right follow the direction of the run they are in. Logical: Left is backward in the text. Shift keeps extending the logical selection to wherever the caret lands, so a selection that crosses a boundary can show as two pieces. |
| Single-line text input | Visual steps, affinity from clicks, typed text keeps the caret with the character just entered, Home and End set the matching affinity, and a selection collapses to the edge on the side of the arrow. If the shaped text is stale the move falls back to logical. |
| Code editor | A single cursor on a line that contains right-to-left text steps visually; at the visual end of the line it leaves toward the neighbouring line on that side. Pure left-to-right lines, several cursors and logical mode use the document's motions as before. |
| Rich text editor | The same inside blocks with right-to-left text, including right-to-left paragraphs. Up, Down, Home, End and clicks carry affinity, so End on a wrapped line now lands at the true end of that line instead of one character early, and Home from there stays on the line. |
| Caret flag | In mixed-direction lines a small flag at the top of the caret points the way the text at the caret runs. |
| `MonospaceMeasurer` | The headless measurer now lays out Hebrew and Arabic ranges right to left with a small reordering (runs against the base direction reverse, a right-to-left line mirrors), and reports lines, so bidirectional behaviour is testable without fonts. |

## Validation

Five new tests. Four core tests: geometry (both places of a boundary offset, a rightward walk that visits every screen position once with the expected offsets, the walk back, hits on either side of the boundary, a right-to-left paragraph starting at its right end, wrapped-line affinity, the flag); the text input moving one cell right per press in visual mode and jumping in logical mode, the logical selection produced by visual Shift+Left, clicks on both sides of the boundary, and typing at it; the code editor stepping across a Hebrew string literal and leaving the line at its visual end; and the rich editor in mixed and right-to-left paragraphs with the caret flag. One renderer test runs the real glyphon shaper with system fonts: twelve strictly increasing caret places for eleven characters of mixed text, two places for the boundary offset, and an upstream line end on wrapped text. The rich editor test for End on a wrapped line was updated to the true end. The full suite is 143 tests plus 11 GPU tests; clippy is clean. A screenshot of the example confirmed Latin, Hebrew and Arabic shaped in one field with the caret and its flag.

Not verified natively: pressing the arrow keys in the real window. The stepping logic was run against the real shaper's geometry in a test, and the key path is covered headlessly.

## Remaining limits

- Visual moves in the two document editors resolve against the last shaped frame, so several arrow presses between two frames move one step.
- In the code editor, visual movement applies to a single cursor; several cursors move logically.
- Word moves follow the direction of the adjacent run but still stop at logical word boundaries.
- The headless measurer's reordering is a simplification (neutrals follow the previous strong character, digits count as left-to-right); the real shaper implements the Unicode algorithm.
- Paragraph direction is still detected from the first strong character; forcing it, and mirroring layouts for right-to-left locales, is the next slice.

# Milestone 7f: text direction and right-to-left layout

Implemented on 2026-09-21. A window, or any subtree, can run right to left: layout mirrors through the layout engine's own direction support, text aligns to the right, and directional widget internals follow. The direction comes from the OS locale by default.

## Run

```powershell
cargo run -p seaglass --example controls --release -- --rtl
cargo run -p seaglass --example richtext --release -- --rtl
cargo run -p seaglass --example panes --release -- --rtl
```

```rust
run(AppOptions { direction: DirectionChoice::Rtl, ..AppOptions::default() }, view)?;   // default: Auto
context.set_direction(Direction::Rtl);                                                  // at runtime
ui.row("phone", Style::default().direction(Direction::Ltr), |ui| { /* stays left to right */ });
```

```text
class mirrored { direction: rtl }
```

## Contracts

| API | Behavior |
| --- | --- |
| `Direction::{Ltr, Rtl}`, `Style::direction`, stylesheet `direction` | Inherited like text properties: a node without one takes its parent's, so one subtree can differ from the window. |
| `Context::set_direction`, `Context::direction` | The window's direction; switching at runtime re-lays out and repaints everything. |
| `AppOptions::direction`, `DirectionChoice::{Auto, Ltr, Rtl}`, `locale_direction` | `Auto` (the default) reads the OS locale through `sys-locale` and is right-to-left for Arabic, Hebrew, Persian, Urdu, Yiddish, Pashto, Sindhi, Uyghur, Dhivehi, Central Kurdish and Syriac. |
| Layout | Taffy's `direction` is set per node, so rows, wrapped rows, grid columns and the cross axis of columns run from the right. `Align::Start` and `Justify::Start` are the right edge. Split panes put `first` on the right; tab strips run right to left. |
| Physical values stay physical | `Style::offset`, transforms, slide effects, floating dock rectangles and `DockZone::Left/Right` mean the same pixels in both directions, as in CSS where flex order flips and `left` does not. |
| Text | Blocks in right-to-left UI align right: wrapped blocks through the shaper (`TextStyle::rtl`), single lines in wider boxes by moving the text. The paragraph's base direction still comes from its content, so a Latin label right-aligns but keeps its punctuation order. |
| Text input | Text shorter than the field sits at the right; clicks, the caret and the IME rectangle account for it. |
| Widgets | Sliders have their minimum on the right and Left increases the value. Split dividers drag and step the way they look. Left selects the next tab. Tree expanders are hit at the right end of the row. Combo and menu popups hang from the owner's right edge, context menus open to the left of the pointer, tooltips align with the owner's right edge, and scrollbars are on the left. |
| Rich text editor | Paragraphs align right; list markers, the quote bar and indents move to the right side. Code blocks stay left-to-right. |
| Code editor | Always left-to-right, text, gutter and scrollbar, whatever surrounds it; its find bar mirrors like any other row. |

## Validation

Five new tests. Four core tests: rows, column start alignment and grid columns mirrored, a left-to-right subtree inside a right-to-left window, switching back at runtime and the stylesheet property; right alignment of a single line in a wider leaf and the `rtl` style on a wrapped block; slider pointer and key direction, split pane order with divider drag and key steps, tab order with the Left key, and a combo popup hanging from its owner's right edge; a right-aligned text field that still places the caret where it was clicked, and a code editor that stays left-to-right at the right side of a mirrored column. One host test maps locales to directions. The full suite is 148 tests plus 11 GPU tests; clippy is clean; the controls, panes and richtext examples also pass `--smoke --rtl`. Screenshots of the mirrored controls and richtext examples confirmed swapped columns and panes, right-aligned text and fields, the mirrored slider, list markers and quote bar on the right, and code kept left-to-right. They also exposed two defects that were fixed: a clipped slider label and a right-aligned code block.

Not verified natively: `Auto` on a right-to-left OS locale (this machine is left-to-right; the mapping is unit-tested), and pointer and key interaction in the mirrored window, which is covered headlessly.

## Remaining limits

- The base direction of a paragraph cannot be forced, so `Hello!` in a right-to-left label shows as `Hello!` at the right, not `!Hello`.
- Collection rows, data grid cells and dock tab strips mirror through layout, but their keyboard Left and Right (tree expand and collapse, grid cell moves) still use left-to-right meanings.
- Dock side zones, floating panels and the inspector stay physical by design; an app that wants a mirrored default dock layout builds it that way.
- Buttons with a minimum width align their label to the start side, like left-to-right buttons do.
- Checkbox and radio marks follow the direction of their label text, since the mark is part of the label.

# Milestone 7g: hot reload

Implemented on 2026-09-21. Stylesheets and themes already reloaded from disk. This slice adds files the view depends on, and Rust code itself through a development runner that rebuilds and restarts the app while keeping its state and window.

## Run

```powershell
cargo run -p seaglass-dev -- -p seaglass --example counter
```

Click the counter a few times, then edit `crates/seaglass/examples/counter.rs` (or any `.rs` or `.toml` under the current directory): the runner rebuilds, the window closes and reopens in the same place with the same count. Edit `crates/seaglass/examples/counter-motd.txt` and the line under the title changes without any restart. A build error leaves the running app untouched.

```rust
// Files: current content, reloaded on change, redraw on reload.
let message = ui.watched_text("assets/motd.txt");            // Option<Arc<str>>
let bytes = ui.watched_bytes("assets/palette.bin");          // Option<Arc<[u8]>>

// State across development restarts.
let mut state: AppState = seaglass::hot::saved_state()
    .and_then(|text| AppState::parse(&text))
    .unwrap_or_default();
run(options, move |ui| {
    ui.hot_state(|| state.to_text());     // called only when a restart is about to happen
    view(ui, &mut state);
})?;
```

## Contracts

| API | Behavior |
| --- | --- |
| `Ui::watched_bytes(path)`, `Ui::watched_text(path)` | The content of a file the host keeps current. `None` until the first load, which happens right after the declaration that first asks; each change on disk delivers new content and a redraw. A failed reload keeps the last good content. A view that stops asking stops the watching. `watched_text` is `None` for bytes that are not UTF-8. |
| `Context::watched_paths`, `watched_known`, `set_watched_file`, `watch_error` | The host side of the same contract, so other hosts and headless tests can deliver files themselves. The core does no file IO. |
| Desktop host | One modification-time poller per window serves the stylesheet and the watched files, on a 500 ms tick. The tick now runs by the clock rather than by timer wakeups, so it also fires while a window redraws every frame; before, a running animation starved stylesheet reloads. |
| `seaglass-dev [--watch PATH]... [--poll MS] [cargo build args]... [-- app args]` | Builds with `cargo build`, takes the executable from cargo's JSON messages and runs it directly. `.rs` and `.toml` files under the watch roots (default: the current directory; `target` and dot directories are skipped) are polled; a change rebuilds. After a successful build the app is asked to stop, given five seconds, and the new build is started. A failed build leaves the running app alone. Closing the app ends the session with its exit code. |
| Stop protocol | The runner passes two paths in `SEAGLASS_HOT_STATE` and `SEAGLASS_HOT_STOP`. When the stop file appears the host runs one more declaration with `Context::request_hot_save`, writes the text from `Ui::hot_state` plus the window's outer position and logical size to the state file, and closes. No signals and no unsafe code. |
| `Ui::hot_state(save)`, `seaglass::hot::saved_state()`, `hot::active()` | The app chooses its own state format. The closure runs only for a restart. On the next start `saved_state` returns the text and the first window reopens with the saved position and size. Outside the runner all of it is inert. |

## Validation

Seven new tests. Two core tests cover watched files (request, delivery, change, identical content, an error that keeps the last good content and reports once, non-UTF-8 bytes, a view that stops asking) and the hot state hook (the closure runs only on request, the text is taken once). Two host tests cover the state file format, including state text that contains the format's own keywords, and the poller for new, changed and missing files. Three runner tests cover argument splitting, extracting the executable from cargo's messages with escaped Windows paths, and source snapshots that ignore `target`, dot directories and other file types. The full suite is 155 tests plus 11 GPU tests; clippy is clean.

Run for real on this machine: the counter example under the stop protocol stopped within one second of the stop file appearing, wrote `window 64 64 900 650.4` and `387 true true`, and a second start resumed from 387 and later saved 863. The `seaglass-dev` binary built the example, started it, detected a change in a watched directory, rebuilt, stopped the app without hitting the timeout, started the new build and exited 0 when that one closed. The first attempt exposed the starved tick described above, which is fixed. The existing `scripts/test-theming.ps1` stylesheet reload check still passes through the shared poller.

Not verified: that the restored window position looks right on a second monitor or a different scale factor, and editing `counter-motd.txt` while watching the window (the delivery path is tested headlessly and the poller in isolation).

## Remaining limits

- A restart costs a rebuild; this is not in-process code swapping, which would need native code loading and `unsafe`.
- One window's state is saved: with several windows the last one to save wins, and only the first window's geometry is restored.
- Polling has up to 500 ms latency and scans the watch roots on every tick; there are no OS change notifications.
- Focus, scroll positions and other retained widget state are not part of the saved state unless the app includes them.
- There are still no image or asset types; `watched_bytes` is the hook they will use.

# Milestone 7h: framework diagnostics

Implemented on 2026-09-21. The framework now warns about six kinds of mistakes it can see from the inside, through one collector with three views: an API for tests, a tab in the F12 inspector, and log output. This completes phase 7.

## Use

```rust
for entry in context.diagnostics() {
    // kind, node, path such as `page/list`, message, count, last frame
}
context.set_diagnostics(true);      // on in debug builds, off in release builds by default
context.clear_diagnostics();
```

In a desktop app, each new diagnostic is printed once to stderr as `seaglass diagnostic: <Kind> at `<path>`: <message>`, emitted as a `tracing` warn event with target `seaglass::diagnostics`, and listed in the inspector's Diagnostics tab, where selecting an entry shows its node in the tree.

## Contracts

| Kind | Reported when | Threshold |
| --- | --- | --- |
| `UnstableIds` | A container gains exactly as many children as it loses, frame after frame: keys built from changing data or positions recreate widgets and lose their focus, scroll and editing state. Reported on the container. | `UNSTABLE_ID_FRAMES` = 8 consecutive frames |
| `Relayout` | Layout ran but no node's bounds changed, frame after frame. Reported on the first node that made layout dirty, typically a wrapped text or layout style that changes every frame yet measures the same. | `RELAYOUT_FRAMES` = 30 |
| `Repaint` | More than half of the tree repainted, frame after frame, in a tree of at least `REPAINT_MIN_NODES` = 20 nodes. Reported for the window. | `REPAINT_FRAMES` = 60 |
| `Inaccessible` | A button, text input, checkbox, radio, slider, combo box, menu button or tab has an empty accessible name, or a name made only of symbols, and no tooltip to describe it. Checked for controls a declaration created or changed. | each control once |
| `OversizedLayer` | A node composited as an offscreen layer (opacity or transform) covers more than `LAYER_AREA` = 4,096,000 logical pixels. | each node once |
| `HugeCollection` | A container other than a virtual collection has more than `HUGE_CHILDREN` = 500 direct children. | each container once |

| API | Behavior |
| --- | --- |
| `Context::diagnostics() -> &[FrameDiagnostic]` | One entry per kind and node; repeats raise `count` and refresh `frame` and `message`. At most 256 entries are kept. |
| `Context::set_diagnostics`, `diagnostics_enabled`, `clear_diagnostics` | The checks run when `debug_assertions` are on unless switched. When off they cost one branch per hook. |
| Inspector | `InspectorModel::diagnostics` and a `Diagnostics (n)` tab; an entry with a node selects it in the Tree tab. |

Two of the framework's own buttons were named only by a symbol and would have been reported: the tab close button and the find bar's close button. Both now carry a tooltip ("Close notes.md", "Close the find bar"), which is also their accessible description.

## Validation

Five new tests. Four core tests: churning keys reported once on their container while stable keys and a genuinely growing list stay quiet; a wrapped label alternating between two equally sized strings reported as relayout on that label, thirty recoloured labels reported as wholesale repainting, and a settled tree staying quiet; unnamed and symbol-named controls reported while a tooltip, a label and the built-in closable tabs are not, and unchanged controls not re-reported; an oversized layer and a 501-child column reported while a small layer is not, `clear_diagnostics`, and nothing collected when switched off. One inspector test: the model captures diagnostics, the tab lists them, activating an entry switches to the tree with that node selected, and the inspector's own UI raises none. The full suite is 160 tests plus 11 GPU tests; clippy is clean.

Dogfooding: all twelve examples, six of them also in their `--open` or `--rtl` variants, run without a single diagnostic in debug builds. A throwaway example with a nameless button and frame-numbered keys printed both expected lines through the desktop host and was removed. A release build of the counter example prints nothing, as designed.

## Remaining limits

- The relayout and repaint checks are heuristics with fixed thresholds; an app that legitimately repaints everything each frame (a game-like view) should switch them off.
- `UnstableIds` sees only wholesale replacement with equal counts; keys that churn while the list also changes size are not caught.
- Accessibility checks cover names only, not contrast, focus order or target size.
- Diagnostics are per window; nothing aggregates them across windows.
- GPU timings, memory statistics and cross-driver pixel tolerance from the phase 5 list are still open.
