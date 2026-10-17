# Milestone 9: the missing widgets

Row 18 of the coverage table listed four widgets as missing: number input, autocomplete, toasts and a property grid. Slices: 9a number input and autocomplete, 9b toasts, 9c the property grid and 9d a derive macro for it (all below). Milestone 9 is complete.

# Milestone 9a: number input and autocomplete

Implemented on 2026-09-21. Both are the existing single-line text field with one more capability each, so selection, clipboard, undo, IME and bidirectional carets come for free.

## Run

```powershell
cargo run -p seaglass --example controls --release
```

The right column has a Quantity field (0 to 20 in steps of 0.5, shown as kilograms) and a City field that suggests from ten names.

## Use

```rust
let options = NumberOptions::default().range(0.0, 20.0).step(0.5).suffix(" kg");
if ui.number_input("quantity", "Quantity", &mut quantity, options).changed { /* committed */ }

let matches = suggest(&city, &CITIES, 6);              // or any list the app computes
let response = ui.autocomplete("city", "City", &mut city, &matches);
if let Some(index) = response.picked { /* matches[index] was taken; `city` already holds it */ }
if response.response.submitted { /* Enter on free text */ }
```

## Contracts

| Piece | Contract |
| --- | --- |
| `Ui::number_input(key, label, &mut f64, NumberOptions)` | A field with decrease and increase buttons. Typing is a draft that the app never sees: Enter, leaving the field or any step commits it. Committing parses, clamps to the range, snaps to the step and reformats; text that is not a number reverts to the value. `changed` is reported only for committed changes. If the app changes the number while a draft exists, the app wins. |
| Stepping | Up and Down step once, PageUp and PageDown ten times, several presses between two frames add up. The wheel steps only while the field has both keyboard focus and the pointer, so scrolling a page never edits values in passing. The buttons step on click, are skipped by Tab (the field has the keys), carry the tooltips "Decrease" and "Increase", and switch off at the ends of the range. |
| `NumberOptions` | `range(min, max)` (unbounded by default), `step` (1; zero or less disables snapping; snapping counts from `min`, or from zero without one), `decimals` (default: as many as the step has), `suffix` such as `" kg"` (shown, optional when typing), `style` (160 wide by default). `format`, `parse` and `constrain` are public. Parsing accepts a decimal comma when there is no point, a typographic minus and exponents; NaN and infinities are rejected. Floating-point dust is rounded away at nine decimals. |
| Accessibility | `Role::SpinButton` with the label, the numeric value, minimum, maximum and step; assistive technology can increment, decrement and set a number, which commits like Enter. |
| `Ui::autocomplete(key, label, &mut String, &[impl AsRef<str>])` | The app passes the entries for the current text each frame: from memory, an index or the latest answer of a remote search. Typing opens the list below the field with nothing highlighted, so Enter still submits free text. Up and Down move through the list (Up enters at the bottom), Enter or a click takes the highlighted entry, Escape or leaving the field closes the list, Down reopens it. Every other key, Space, Home and End included, keeps editing the text. Taking an entry puts its text in the field with the caret at the end and keeps focus there. |
| `AutocompleteResponse` | `response` is the field's ordinary response; `picked` is the index taken this frame in the list that was offered when it was taken. |
| `suggest(typed, candidates, limit)` | The simple source: entries containing the text, ignoring case, nothing for empty text. Compute it before the call; the field borrows the text mutably. |
| Highlighting | In each row the first occurrence of the typed text is drawn bold, matched without regard to case on the row's own characters, so letters whose lowercase form has another byte length are safe. |
| Accessibility | The field reports `expanded` while rows are showing and the highlighted row as its active descendant; rows are list options. |

## Validation

Five new core tests: commit on Enter and on blur with clamping, snapping and reformatting, revert of text that is no number, and the app's value winning over a draft; arrows, pages, accumulated steps, a decimal comma with a suffix, buttons that Tab skips and that switch off at the ends, the wheel only with focus and pointer, the spin button role and range, and increment and set-number actions; `NumberOptions` formatting, parsing and constraining including negative zero, dust and NaN; the autocomplete flow with typing while open, Down and Enter, Enter on free text, Down to reopen, Escape, a click keeping focus, and Tab closing; and the bold match for a capital dotted I.

The full suite is 182 tests plus 18 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features. The controls example passes `--smoke`, also with `--rtl`. `scripts/test-controls.ps1`, which drives that example through UI Automation, passed three times in a row after the example changed; its first run failed once at its first step and did not repeat, and a run piped through Git Bash hung, which did not happen under PowerShell. Checked by eye in a release build: the number field with its buttons, and the City field after really typing "bu" into the window, with Bucharest and Budapest listed and "Bu" bold.

`scripts/screenshot.ps1` gained `-TypeText` (letters and digits, sent as key presses because winit derives text from them) and now focuses the focusable element of a name, since a visible label often shares the name of its field.

## Remaining limits

- The value is an `f64`: integers beyond 2^53 lose precision. There are no expressions ("2*3"), no drag-to-scrub and no auto-repeat while a button is held.
- Escape does not revert a draft; retyping or an invalid entry does.
- Number formatting is not locale-aware beyond accepting a decimal comma.
- Clicking a stepper button moves focus out of the field.
- The autocomplete list shows at most what fits in 280 px and scrolls; rows are plain text with one bold match, no icons or secondary text.
- The field's role stays a text input with an expandable list rather than a dedicated editable combo box role.

# Milestone 9b: toasts

Implemented on 2026-09-21. Short messages that the framework owns: the app raises one and forgets it. They stack in the bottom corner at the end of the reading direction, above everything including modal dialogs, time out on the context's clock, hold while someone is reading or reaching for them, announce themselves to screen readers, and report a clicked action once. A cloneable sender raises them from any thread.

## Use

```rust
if saved { ui.toast(Toast::success("Draft saved").action("Undo").tag("draft")); }
if let Some(clicked) = ui.toast_action() { if clicked.tag == "draft" { undo(); } }

let toasts = ui.toast_sender();                       // once, during any declaration
std::thread::spawn(move || toasts.send(Toast::error("Export failed: the disk is full.")));
```

In the controls example, Actions > Save draft raises a toast with Undo, and "Export in the background" raises an error from a worker thread a moment later.

## Contracts

| Piece | Contract |
| --- | --- |
| `Toast` | `new`, `success`, `warning`, `error`; `.kind`, `.action(label)`, `.tag(text)`, `.duration`, `.sticky`. Five seconds by default, eight for errors and for toasts with an action; sticky toasts stay until dismissed. |
| `Ui::toast(toast) -> ToastId` | Raised during a declaration, visible from the next one, which the context requests. Call it when something happens, not every frame. A toast equal to one already showing or waiting restarts that one instead of stacking, and the newest id is the valid one. |
| Queue | Four toasts show at once, oldest on top; later ones wait and their time starts when they show. |
| Time | Expiry runs on the `set_now` clock and `Context::next_wake` includes the next expiry, so an idle window wakes exactly when a toast is due. While the pointer or keyboard focus is on any toast, none expire and no wake is scheduled; afterwards each has at least 1.5 seconds left, however long the pointer rested. |
| `Ui::toast_action() -> Option<ToastAction>` | Each click on an action button, once, with the toast's id and tag; the click also dismisses the toast. The report arrives in the declaration after the one that consumed the click, which hosts run in the same frame. At most 32 unclaimed actions are kept. |
| `ToastSender` | From `Ui::toast_sender`; `Clone + Send`. `send` queues the toast for that window and wakes it through the host, so the toast is in the very next declaration. The desktop host installs the wake per window; other hosts call `Context::set_toast_wake`. |
| `Context::push_toast`, `toasts`, `dismiss_toast` | For hosts and tests. |
| Layout and look | The stack is the last root layer, absolutely placed 16 px from the bottom and from the end side (left in right-to-left windows). Each toast is a 360 px row: an accent stripe (the theme accent, green, amber or red by kind), wrapped text, the optional action and a close button with the tooltip "Dismiss". Toasts slide in, fade out and the stack glides when one leaves; reduced motion applies as everywhere. They take clicks over whatever is under them, including a modal dialog's scrim. |
| Accessibility | A toast is `Role::Status` with a polite live region, an error is `Role::Alert` with an assertive one; the text is the name. The action and close buttons are ordinary buttons. |

## Validation

Six new core tests: placement in the corner, roles and live settings, the requested follow-up frame, the wake time and expiry at five and eight seconds; holding with the pointer for a minute with no wake scheduled, then 1.5 seconds of grace; an action reported once with its id and tag, eight seconds for action toasts, sticky toasts and the named close button; equal toasts restarting with the newest id valid, four visible with the waiting ones timed from when they show; a sender used from another thread waking the host twice and showing both toasts in the next declaration; and a toast above a modal dialog whose close button wins the hit test.

One of those tests found a bug before it shipped: a toast moving up from the waiting queue got its timer one declaration late, so an idle window had no wake scheduled for it and it would have stayed until the next input. Timers now start after the leavers are removed, in the same pass.

The full suite is 188 tests plus 18 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features. The controls, dialogs, application and images examples pass `--smoke`, controls also with `--rtl`, and `scripts/test-controls.ps1` passes. Checked in a release build by really driving the window: Save draft through the keyboard showed the green toast with Undo, and the worker thread's red error appeared about 1.5 seconds after its button was invoked, wrapped over two lines. UI Automation reports that toast 21 px above the client area's bottom and 20 px from its right edge, which is 16 logical pixels at 125 % scaling.

The example change first broke `scripts/test-controls.ps1`: a new button pushed the `State:` label below the visible part of the scroll view, and nodes outside the viewport are not in the accessibility tree. The new widgets now sit below that label. `scripts/screenshot.ps1` gained `-InvokeNames`; its captures have always lost the bottom strip of the window (it sizes the bitmap to the client area but prints the window with its title bar), which is why a toast looks cut off in them.

## Remaining limits

- Kind colours are fixed, not theme tokens; the position is fixed to the bottom end corner.
- Toasts under a modal dialog take clicks but are outside the dialog's focus scope, so Tab does not reach them there.
- Toast state is per window and is not kept across a development restart.
- No progress toasts and no updating a toast in place; raise a new one.
- Windows screen reader speech for the live regions was not listened to here; the roles and live settings are what AccessKit is given.

# Milestone 9c: property grid

Implemented on 2026-09-21. A two-column editor for named values, declared through a builder that binds each row straight to the app's own variables. It is composed from controls that already exist (text and number fields, the checkbox, the combo box, split panes, scroll views), so every editor keeps its keyboard, clipboard and accessibility behaviour.

## Run

```powershell
cargo run -p seaglass --example properties --release
```

A card next to the grid that edits it: text, a checkbox, a choice, numbers with units and ranges, two colours, nested groups, reset buttons, a read-only row and a custom row.

## Use

```rust
let response = ui.property_grid("inspector", &mut state, PropertyGridOptions::new("Card").height(600.0), |grid| {
    grid.group("Layout", |grid| {
        grid.default(260.0).number("Width", &mut card.width, NumberOptions::default().range(80.0, 420.0).suffix(" px"));
        grid.group("Box", |grid| { grid.number("Radius", &mut card.radius, NumberOptions::default()); });
    });
    grid.group("Appearance", |grid| {
        grid.default(true).bool("Visible", &mut card.visible);
        grid.choice("Align", &mut card.align, &["Start", "Center", "End"]);
        grid.color("Fill", &mut card.fill);
        grid.text("Title", &mut card.title);
        grid.readonly("Characters", card.title.len().to_string());
        grid.custom("Preset", |ui, label| ui.button_keyed("$value", format!("{label}: sunset")).clicked());
    });
});
for path in &response.changed { /* "Layout/Width", "Appearance/Fill", ... */ }
```

## Contracts

| Piece | Contract |
| --- | --- |
| `Ui::property_grid(key, &mut PropertyGridState, PropertyGridOptions, builder)` | A filter box, a header and the rows. `PropertyGridOptions`: `label` (names the filter box), `filter` (on), `height` (rows scroll inside it; otherwise the grid grows), `style` (full width by default). |
| `PropertyGridState` | App-owned: `filter`, `collapsed` (group paths such as `Layout/Box`) and `name_share` (0.4). Keep it with the rest of a workspace to persist the grid's shape. |
| Rows | `number` (the number input with all its options), `bool`, `text`, `choice` (index into labels), `color`, `readonly`, `custom`. Each returns whether it changed, and every editor carries the property's name as its accessible label, so the visible name in the first column is not repeated on screen. |
| `grid.default(value)` | Gives the next row a reset button with the tooltip "Reset NAME", enabled while the value differs. A reset counts as a change. A default of another type than the row's value panics at declaration with both type names; a default followed by no row panics too. |
| `color` | A swatch and a `#rrggbb` or `#rrggbbaa` field. The colour follows the text whenever it is a whole colour; anything else is a draft that is dropped when the field is left. |
| Groups | Collapsible headers (buttons with "Collapse NAME" or "Expand NAME" tooltips) that nest; names indent by depth while the value column stays aligned. |
| Filter | Case-insensitive on row and group names. While it has text: collapsed groups open, groups with no matching row hide, and a group whose own name matches shows all its rows. Clearing it brings the collapsed state back. |
| Columns | The header is a split pane: its divider (draggable, focusable, arrow keys) sets `name_share`, and every row's name takes that share, so values start exactly under "Value" at every depth. |
| `PropertyGridResponse` | `changed`: paths of the properties that changed this frame; `any()`. |
| Also new | `Ui::combo_box_styled`. |

## Validation

Six new core tests: editors named after their properties, the value column aligned with the header at three depths, and a number, checkbox, text and custom row each reporting its path; reset restoring the default, enabled only while the value differs and absent without a default; colours following whole hex values including alpha and dropping a draft on blur; collapsing, the filter opening collapsed groups, hiding groups without matches, a group name showing all its rows, and the collapsed state returning; the header divider moving the value column; and the panic message for a default of the wrong type.

The full suite is 194 tests plus 18 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features; the new example passes `--smoke`. Checked by eye in a release build, including after really typing "ra" into the filter: Characters, Radius inside Layout/Box, and all of Appearance (whose name contains "ra") remained. A headless probe confirmed every row ends exactly at the grid's right edge.

That probe was needed because the capture made the rows look too wide. `scripts/screenshot.ps1` sized its bitmap from the UI Automation rectangle while `PrintWindow` renders the whole window rectangle including Windows 11's invisible resize borders, so every capture it ever made was shifted and lost its right and bottom edges (the toast that looked cut off in 9b was the same thing). It now sizes the bitmap from `GetWindowRect`.

## Remaining limits

- Rows are declared, not virtualized: thousands of properties should be filtered or split across grids.
- No arrow-key movement between rows; Tab visits the editors row by row, with each group header and reset button on the way.
- No multi-object editing (mixed values), no per-row validation messages, no colour picker beyond the hex field.

# Milestone 9d: `#[derive(Properties)]`

Implemented on 2026-09-21. A struct's fields become property grid rows. The derive expands to exactly the builder calls of 9c, so derived and hand-written rows mix freely. This completes milestone 9 and turns row 18 of the coverage table to Done.

## Use

```rust
#[derive(Properties)]
struct Card {
    #[property(group = "Content")]
    title: String,
    #[property(choices("Start", "Center", "End"))]
    align: usize,
    #[property(group = "Layout", min = 80, max = 420, step = 10, suffix = " px", default = 260)]
    width: f64,
    #[property(name = "Corner radius", max = 60)]
    corner_radius: u8,
    #[property(flatten)]
    shadow: Shadow,                       // another Properties struct: a nested group
    #[property(group = "Appearance", default = Color::rgb(22, 105, 99))]
    fill: Color,
    #[property(readonly)]
    revision: u32,
    #[property(with = edit_anchor)]       // fn(&mut Ui<'_>, &str, &mut Anchor) -> bool
    anchor: Anchor,
    #[property(skip)]
    cache: Vec<u8>,
}

ui.property_grid("inspector", &mut state, options, |grid| {
    card.properties(grid);
    grid.group("Extras", |grid| grid.readonly("Characters", card.title.len().to_string()));
});
```

## Contracts

| Piece | Contract |
| --- | --- |
| `Properties` (core) | `fn properties(&mut self, grid: &mut PropertyGrid)`. The derive implements it; it can be written by hand. |
| `seaglass-derive` | A proc-macro crate, re-exported as `seaglass::Properties` behind the default `derive` feature. It depends on `syn` 2 with only its derive features, `quote` and `proc-macro2`, all of which the workspace already built. Generated code names `::seaglass::...`, so it needs the facade crate. |
| Editors by type | `bool`, `String`, `Color`; `f64` and `f32`; every integer type, which steps by one, shows no decimals and keeps its own range unless `min` or `max` narrow it; `usize` with `choices(...)` is a choice. All numbers travel through the grid as `f64`. |
| Options | `name` (default: the field name with spaces and a capital), `group`, `default`, `min`, `max`, `step`, `decimals`, `suffix`, `choices("...")`, `readonly` (uses `Display`), `flatten`, `with = function`, `skip`. |
| Groups | `group = "Name"` stays in force for the fields that follow until another is given; `group = ""` returns to the top level. Consecutive fields of one group share one `grid.group`. `flatten` nests the other struct's rows in a group named after the field, inside the current group. |
| Errors | Reported at the offending field or option: an unsupported type lists the four ways out (`skip`, `flatten`, `readonly`, `with`), an unknown option lists the known ones, number options on other types and `choices` on a non-`usize` field say so, and enums or tuple structs are refused by name. |

The `properties` example now derives its grid and adds two rows by hand. A scrolling grid also keeps its content 3 % narrower than the scroll view, so the scrollbar has a margin of its own instead of covering the editors' ends (seen in the first correct capture).

## Validation

Two new tests. In the facade crate, a derived struct with a nested one: every expected group and row exists and the skipped field does not; a float has its range, step, unit and default and resets; a `u8` clamps to its own minimum and the given maximum and rounds typed decimals; a nested struct reports nested paths; choices reset to their default; a read-only value and a `with` editor work. In core, a scrolling grid keeps its rows clear of the scrollbar and under the header. The four kinds of compile error were checked by compiling a throwaway file and reading the messages; there is no compile-fail test suite.

The full suite is 196 tests plus 18 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features and with `desktop` alone (no derive, no decoders). All fourteen examples pass `--smoke` and the theming script reports `THEMING_OK`. The derived grid was checked by eye in a release build.

## Remaining limits

- Integers beyond 2^53 lose precision on their way through `f64`.
- No enums as choices yet: a `usize` index with `choices(...)`, or `with`, covers them for now.
- No `Option<T>` or collection fields.
- `readonly` needs `Display`.
- The derive needs the `seaglass` facade crate; a project depending only on `seaglass-core` writes `Properties` by hand.

# Fix after milestone 9: focus rings flooding transparent nodes

Reported on 2026-09-21: in the theming example a click on the window's background turned the whole page amber. The scene has no stroke primitive, so since milestone 6a the focus ring was painted as a rectangle in the focus colour with the node's background drawn back over it, inset by 2 px. A node without a background, such as a scroll page, had nothing to draw back, so the focus colour stayed. Every focusable node without an opaque background was affected; the theming example only made it obvious because its stylesheet sets a bright focus token.

The fill-and-cover ring is now used only over an opaque background, where it keeps rounded corners on buttons and fields. Transparent and translucent nodes paint their background as usual and get four 2 px edges. A regression test clicks an empty scroll page and requires that no focus-coloured quad covers its middle, and that an opaque button keeps its single inset ring. Verified in the real theming window by clicking its background: the page keeps its colour and shows a thin ring. 197 tests pass and the theming script reports `THEMING_OK`. `scripts/screenshot.ps1` gained `-ClickAt "x,y"`.

Not changed: a mouse click still shows the focus ring. Showing it only for keyboard focus would be a separate decision.
