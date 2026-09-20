# Milestone 3: text and controls

Implemented on 2026-09-20. The controls gallery adds a single-line editor, checkbox, radio buttons, slider, combo box and one-level menus. Text measurement, wrapping, painting and selection geometry share the renderer's shaping system. This is the implemented baseline; native IME candidate behavior, complete assistive text navigation and cross-platform interaction still need validation or further work as listed below.

## Run

```sh
cargo run -p rust-gui --example controls --release
```

The gallery demonstrates Unicode input, selection, clipboard shortcuts, undo/redo, submission, font families, wrapped paragraphs, disabled fields and each control. `--smoke` exits after three frames. Like the counter, it sleeps between events; the caret is steady rather than timer-driven.

```rust
let mut name = String::from("Hello");
let mut checked = false;
let mut volume = 50.0;
let mut theme = 0;

// Inside the view closure:
if ui.text_input("name", "Name", &mut name).submitted {
    // Handle submission once.
}
ui.checkbox("notifications", "Notifications", &mut checked);
ui.slider("volume", "Volume", &mut volume, 0.0..=100.0, 5.0);
ui.combo_box("theme", "Theme", &mut theme, &["Dark", "Light", "System"]);
```

## Contracts

| API | Behavior |
| --- | --- |
| `Style::font_family`, `font_weight`, `italic`, `wrap` | Generic sans/serif/monospace or named system families, weight 100–900, italic and word wrapping with glyph fallback for long words. Generic families resolve to installed fonts. Wrapped height follows the available content width. |
| `TextMeasurer` | Implement `measure_styled` and `geometry`; the existing `measure(text, size)` convenience method remains. Geometry uses UTF-8 byte offsets at grapheme boundaries. Headless `MonospaceMeasurer` supplies synthetic metrics, not real word shaping. |
| `Ui::text_input` / `text_input_styled` | Application-owned `String`; retained caret, anchor, composition, horizontal offset and history. Label supplies the accessible name. Styled input adds a placeholder and `Style`. `Response::changed` and `submitted` are delivered once. |
| `Ui::checkbox` | Application-owned `bool`, activated by pointer, Space/Enter or accessibility. |
| `Ui::radio` | Bind sibling options to one `usize`. Put each independent radio group in its own keyed row/column. Tab uses the selected enabled option, or the first enabled option; arrows move/select within that sibling group. |
| `Ui::slider` | Finite ordered inclusive bounds and positive step. Pointer capture, arrows, Page Up/Down, Home/End and semantic value actions share snapping/clamping. Both endpoints are reachable. |
| `Ui::combo_box` | Selected index into the provided labels. Popup arrows/Home/End navigate; Enter selects; Escape cancels; Tab closes and advances focus. Empty lists disable the control. |
| `Ui::menu` | Returns an activated item index once. `MenuItem::disabled` excludes an item from activation and keyboard navigation. Menus currently have one level. |
| `Ui::enabled` | Disables a declaration scope without changing widget identity. Disabled controls reject focus and mutation actions. |

The host declares again after consumed input updates, so labels placed earlier in the view also reflect the resulting state. Side effects should stay behind response flags or a returned menu choice. An external replacement of the input's value clears its editing history and composition. Newlines/tabs normalize to spaces in the single-line editor.

Popups paint after normal content, are constrained to the window, flip above their owner when needed, and scroll independently. Wheel events over a popup target it before underlying scroll containers. Keyboard navigation reveals its highlighted item without undoing subsequent wheel scrolling.

## Input and resource behavior

Left/Right, Shift-selection, Home/End, Backspace/Delete and word movement use Unicode graphemes and word boundaries. Mouse drag selection uses shaped cluster geometry. Ctrl (Cmd on macOS) + A/C/X/V/Z, Shift+Z and Y provide selection, clipboard and history. AltGr text is accepted. IME preedit is temporary; commit becomes one undoable edit. Focus loss, disabling a field and changing the external value cancel composition. Switching input targets resets native IME activation; candidate positioning uses the logical caret rectangle. Window reactivation restores a surviving focused control, without restoring its cancelled composition. Synthetic held-key events generated during focus synchronization are ignored.

Clipboard construction is lazy and its handle lives with the application. Failed clipboard writes do not delete a cut selection. Image clipboard support is excluded. Editing state is boxed only for text inputs; ordinary labels do not contain inline editor/history storage. Each undo and redo stack is capped at 128 snapshots or 1 MiB of text payload. Shaping buffers are retained for the active scene and input geometry is cached by text/style. Graphics backend probing and compact GPU allocation remain unchanged.

## Validation

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p rust-gui --no-default-features --locked
cargo test -p rust-gui-render-wgpu --test gpu -- --ignored --nocapture
```

Windows integration checks, after building:

```powershell
cargo build -p rust-gui --examples --release --locked
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-controls.ps1 -Screenshot
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-desktop.ps1
```

The controls script exercises native UI Automation value/toggle/selection/range patterns, retained IDs, popup options, disabled states, typing, Backspace and Enter. Keyboard checks require an undisturbed desktop while the demo owns focus. `-PointerOnly` skips keyboard checks and opens popups with pointer messages addressed to the demo window; it retains the value and accessibility action checks. Screenshots go to `target/controls.png` and `target/controls-popup.png`. The script closes only its own process. Clipboard transaction tests use an in-memory clipboard, without replacing the user's system clipboard.

Locally verified with Rust 1.98.0 on Windows:

- 41 regular tests: 32 core behavior tests, 2 real system-font shaping tests and 7 desktop/backend/accessibility tests.
- The explicit Direct3D 12 GPU test: clipping, paint ordering, shaped glyphs, four wrapped lines at 2× scale, retained allocations and DPI invalidation.
- Native text ValuePattern, checkbox TogglePattern, radio/list option SelectionItemPattern, slider RangeValuePattern, menu activation and disabled controls on the final build. Native typing/deletion/submission passed during development; later keyboard reruns received additional desktop input and failed their exact-text assertions. The final controls run used `-PointerOnly`; a clean full keyboard rerun on an undisturbed desktop remains pending. The existing workbench's final keyboard scrolling and worker/accessibility regression run passed.
- Gallery and popup screenshots inspected at the machine's 125% display scaling.

A counter memory spot check measured 67.74 MiB private resident RAM and 123.19 MiB private commit, close to milestone 2's 67.16 / 122.75 MiB. This is a local spot check with desktop activity, not a new performance benchmark.

## Remaining limits

- Editing is single-line. Navigation is in logical grapheme order, rather than full visual bidirectional caret order; caret positions inside ligatures divide the glyph advance. Multiline/rich editing, word selection by double-click, drag autoscroll and password fields are later work.
- Text values and names are exposed to accessibility, but full editable TextPattern/TextRun selection and character navigation are not implemented. Manual Narrator/NVDA/VoiceOver/Orca review remains outstanding.
- Pinned `accesskit_windows` 0.32.1 has no UIA ExpandCollapsePattern implementation. Core and AccessKit semantics carry expanded state and Expand/Collapse actions, but Windows automation opens combo/menu popups by focus plus keyboard or pointer input. Stateful popup owners also lack InvokePattern through this adapter.
- IME event routing, preedit geometry and commit/cancel semantics are implemented and covered at the model level. Real Japanese/Chinese/Korean candidate windows and dead-key interaction still need manual OS testing.
- Native Linux/macOS interaction is unverified here. The current arboard feature set uses X11/XWayland clipboard support on Linux; pure Wayland clipboard support is not enabled. Font appearance depends on installed families; the Linux CI setup installs DejaVu fonts.
- Menus have no submenus, accelerators or typeahead. Combo options use indices; virtualized lists and stable application item identities belong to milestone 4.
