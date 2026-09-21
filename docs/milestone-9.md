# Milestone 9: the missing widgets

Row 18 of the coverage table listed four widgets as missing: number input, autocomplete, toasts and a property grid. Slices: 9a number input and autocomplete (below), 9b toasts, 9c property grid, 9d a derive macro for property grids.

# Milestone 9a: number input and autocomplete

Implemented on 2026-09-21. Both are the existing single-line text field with one more capability each, so selection, clipboard, undo, IME and bidirectional carets come for free.

## Run

```powershell
cargo run -p rust-gui --example controls --release
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
