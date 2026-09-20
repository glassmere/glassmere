# Milestone 6: styling, theming and layout depth

Four slices so far: 6a styling and theming, 6b layout depth, 6c focus scopes and dialogs, and 6d tooltips, tabs, split panes, context menus and popovers (below).

# Milestone 6a: styling and theming

Implemented on 2026-09-21. Every built-in control now paints from a `Theme` of design tokens that switches at runtime and follows the OS preference. Text properties inherit from ancestors, styles carry per-state paint overrides, and named classes come from typed Rust or from a small stylesheet format that hot-reloads from disk. The remaining phase 6 items (grid layout, focus scopes and dialogs, animations) follow in later slices.

## Run

```sh
cargo run -p rust-gui --example theming --release
```

Edit `crates/rust-gui/examples/theming.style` while the window is open; classes and tokens reload within half a second. The Dark/Light buttons switch themes from the view. Start with `RUST_GUI_INSPECT=1` to see resolved fonts, inherited colors, state overrides and unknown-class warnings in the inspector.

```rust
// Typed classes are plain Rust.
let outlined = Style::button()
    .background(Color::TRANSPARENT)
    .foreground(ui.theme().accent)
    .on_hover(Paint::background(ui.theme().control_hover))
    .on_pressed(Paint { background: Some(ui.theme().accent), ..Paint::default() });

// Named classes come from the installed stylesheet.
let primary = ui.class("primary");
if ui.button_styled("save", "Save", true, primary).clicked() { save(); }

// Switch at runtime; the host declares once more so earlier token reads catch up.
if ui.button_keyed("light", "Light").clicked() { ui.set_theme(Theme::light()); }
```

```text
// theming.style
token accent #f59e0b
class primary {
    background: #f59e0b
    foreground: #1a1206
    padding: 12
    radius: 8
    hover.background: #fbbf24
    pressed.background: #d97706
}
class danger extends primary { background: #dc2626; foreground: #ffffff }
```

## Contracts

| API | Behavior |
| --- | --- |
| `Theme` | Twenty color tokens (window, surface, popup, control states, text states, accent, focus, selection, highlight, rows, track, thumb, scrollbar) plus the default font family and size. `Theme::dark()` reproduces the previous hardcoded palette; `Theme::light()` is new. `token`/`set_token` address tokens by name. |
| `Context::set_theme`, `Ui::theme`, `Ui::set_theme` | Runtime switching. Paint caches are dropped and the scene repaints; layout runs only when the resolved fonts differ. `Ui::set_theme` applies after the current declaration and counts as an activation so the host declares again. |
| `Style` text properties | `foreground`, `font_size`, `font_family`, `font_weight` and `italic` are `Option`s that resolve from the nearest ancestor, then the theme. `background: None` paints the widget kind's theme default (control surface for buttons, text inputs, combo and menu buttons; popup surface for popups; transparent otherwise). Builder methods (`.padding(..)`, `.foreground(..)`, `.font_size(..)`, …) avoid `Some(..)` noise. |
| `Paint`, `StateStyles`, `Style::on_hover/on_pressed/on_focused/on_disabled` | Paint-only overrides (background, foreground, radius) applied over the theme defaults. Precedence: disabled, then pressed, hover, focused. State changes stay repaint-only. |
| `ResolvedText`, `NodeDetails::resolved` | Concrete text properties after inheritance, what measurement and paint use. A color-only change repaints; font changes re-measure and relayout. |
| `StyleSheet::parse` / `class` / `insert` / `apply_to` | `theme dark|light` sets an optional base; `token name #rrggbb[aa]` overlays whichever theme is active; `class name [extends base] { property: value; … }` with `hover.`/`pressed.`/`focused.`/`disabled.` prefixed paint properties. Errors carry line numbers. |
| `Context::set_stylesheet`, `Ui::class`, `Context::style_warnings` | Installing a sheet applies its base theme and tokens and re-styles on the next declaration. Unknown class names return the default style and are recorded once; the inspector shows them. |
| `AppOptions::theme` (`ThemeChoice::System` default or `Fixed`) | System follows winit's theme at creation and on `ThemeChanged`. A stylesheet `theme` line or `Ui::set_theme` overrides until the next OS change. |
| `AppOptions::stylesheet` | Installed at start (a parse error fails startup) and polled every 500 ms for modification-time changes; a broken edit logs the error and keeps the previous sheet. |
| `AppOptions::background` | Now `Option<Color>`; `None` clears to the theme's window token. |

## Validation

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check -p rust-gui --no-default-features --locked
cargo run -p rust-gui --example theming -- --smoke
```

Windows integration check, after `cargo build -p rust-gui --examples --release --locked`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-theming.ps1
```

The script switches to the light theme through UI Automation, rewrites the stylesheet's accent and primary colors on disk, waits for the reload, switches back to dark and saves `target/theming-*.png`. It restores the file afterwards.

Locally verified with Rust 1.98.0 on Windows:

- 66 regular tests, including 6 styling tests: inheritance and re-measurement, theme switching without layout, state override precedence, stylesheet parsing and error lines, class installation and warnings, view-driven theme requests with token overlay.
- Existing scene snapshots were unchanged because the dark theme reproduces the previous colors.
- Screenshots confirmed dark and light rendering, hot reload of classes and tokens, and tokens surviving a theme switch.

## Remaining limits

- Themes carry colors and the base font only; spacing, radius and control sizes are still per-style constants.
- Stylesheets cannot express `Length::Auto` growth rules beyond width/height/grow, have no variables or nesting, and reload by polling rather than file notifications.
- Text inheritance covers color and font; there is no inheritance of `wrap`, padding or gap.
- State overrides are paint-only by design; transitions and animations are the next phase 6 slice.
- Custom widgets and application quads that hardcode `Color::rgb` do not follow theme switches; read `ui.theme()` instead.

# Milestone 6b: layout depth

Implemented on 2026-09-21. Containers gain CSS-style grid layout with auto-placement and explicit areas, min/max constraints, aspect ratios, main- and cross-axis alignment, per-child alignment and opt-in shrinking. Everything maps onto Taffy, whose `grid` feature is now enabled; existing flex layouts keep their exact geometry because children still do not shrink unless asked.

## Run

```sh
cargo run -p rust-gui --example layout --release
```

The example shows a `repeat(3, 1fr)` gallery with a two-column span, a form on `[auto, 1fr]` columns, space-between and baseline rows, 16:9 cards bounded by min/max widths and a row whose children shrink only while a checkbox is on.

```rust
ui.grid("gallery", Style::default()
    .width(Length::Percent(1.0))
    .columns(vec![Track::repeat(3, vec![Track::Fr(1.0)])])
    .auto_rows(vec![Track::Px(90.0)])
    .gap(12.0), |ui| {
    for index in 1..=5 {
        ui.container(format!("card-{index}"), card.clone(), |ui| { ui.label_keyed("label", format!("Card {index}")); });
    }
    ui.container("wide", card.clone().area(GridArea::span(1, 3, 2, 1)), |ui| { ui.label("spans two columns"); });
});
ui.row("toolbar", Style::default().justify_content(Justify::SpaceBetween).align_items(Align::Center), |ui| { /* ... */ });
```

## Contracts

| API | Behavior |
| --- | --- |
| `Ui::grid(key, style, content)` | A grid container. Children auto-place row-major into `style.columns`; overflow creates implicit rows sized by `auto_rows` (auto when empty). `rows`/`auto_columns` mirror this for the other axis. |
| `Track` | `Px`, `Percent` (fraction of the container), `Fr`, `Auto`, `MinMax(TrackSize, TrackSize)` and `Track::repeat(count, tracks)`. Sizing follows CSS: tracks grow to their maximum before `fr` tracks share the remainder. |
| `GridArea`, `Style::area` | Explicit 1-based placement with spans, e.g. `GridArea::span(1, 3, 2, 1)`. Items without an area continue auto-placement around it. |
| `Style::min_width/max_width/min_height/max_height` | `Length::Auto` leaves the bound unset. Unset minimums stay at zero so existing non-shrinking layouts are unchanged. |
| `Style::aspect_ratio` | Width divided by height; the unspecified dimension follows the specified one, and max constraints still win. |
| `Style::align_items`, `justify_content`, `align_self`, `justify_self` | `Align` (start, center, end, stretch, baseline) for the cross axis and per child; `Justify` adds space-between/around/evenly for the main axis. `Justify::Start` keeps the engine's normal behavior so grid tracks fill their container. |
| `Style::shrink` | Flex shrink factor, default 0. With `shrink(1.0)` children share overflow reduction, bounded by their minimum sizes. |
| Stylesheet properties | `min-width`, `max-width`, `min-height`, `max-height`, `aspect-ratio`, `shrink`, `align-items`, `align-self`, `justify-self`, `justify-content`, `columns`, `rows`, `auto-columns`, `auto-rows` (`120 50% 1fr auto minmax(100, 1fr) repeat(3, 1fr)`) and `area` (`column row [colspan rowspan]`). |

The inspector's node details list constraints, alignment and grid settings for the selected node.

## Validation

Seven layout tests cover auto-placement with implicit rows and explicit areas, `repeat`/`minmax`/percent track sizing, min/max/aspect constraints, alignment and distribution on rows and columns, opt-in shrinking with minimums, baseline alignment of mixed font sizes, and the stylesheet properties with their error messages. The full suite is 73 tests. The layout example passes `--smoke` and was screenshotted at 125% scaling with `scripts/screenshot.ps1`.

## Remaining limits

- Percentage widths on grid items resolve against the grid container, not the cell, in this Taffy version. Leave `width` at `Auto` to fill a cell.
- Baseline alignment aligns text bottoms; true font baselines need the measurer to report ascent, which is future work.
- No named grid lines or areas, no `auto-fill`/`auto-fit` repeats, no dense packing, no column-major flow and no per-axis gaps.
- Responsive breakpoints and custom layout primitives are not started; grids respond to size only through `fr`, percent tracks and min/max constraints.

# Milestone 6c: focus scopes and dialogs

Implemented on 2026-09-21. Focus gains scopes that trap Tab, remember their last child and restore focus when they disappear, plus opt-in arrow-key navigation by geometry. Dialogs come in two forms: an in-window layer (modal with scrim or floating non-modal) that shares the headless test path, and native modal windows through the multi-window desktop.

## Run

```sh
cargo run -p rust-gui --example dialogs --release
cargo run -p rust-gui --example dialogs --release -- --open   # starts with the confirm dialog open
```

The example has a modal confirm dialog with a nested "More options" dialog, a non-modal preferences panel, an arrow-key toolbar scope, a remembered scope and a native modal window.

```rust
ui.dialog("confirm", &mut state.confirm, DialogOptions::new("Delete file?"), |ui, open| {
    ui.label("The file will be removed permanently.");
    if ui.button_keyed("cancel", "Cancel").clicked() { *open = false; }
    if ui.button_keyed("delete", "Delete").clicked() { delete(); *open = false; }
});
ui.focus_scope("toolbar", FocusScope { arrows: true, ..FocusScope::default() }, Style::default(), |ui| { /* buttons */ });
let dialog = desktop.open_modal(owner, AppOptions { title: "Native modal".into(), ..AppOptions::default() }, move |ui| { /* … */ })?;
```

## Contracts

| API | Behavior |
| --- | --- |
| `Ui::focus_scope(key, FocusScope { trap, restore, arrows }, style, content)` | A group with focus memory. `trap` cycles Tab and Shift+Tab inside once focus is within. `restore` returns focus to the node focused before the scope when the scope disappears. Every scope remembers its last focused child, and Tab entering the scope from outside lands there. `arrows` moves focus to the nearest focusable in the arrow's direction within the scope, except from controls that use arrows (sliders, text inputs, lists, radios, scroll containers, open popups). |
| `Ui::dialog(key, &mut open, DialogOptions, content)` | Declares a dialog layer as a root child while `open`; later dialogs stack above earlier ones and nested dialogs above their parent. `content` receives the open flag so buttons inside can close it. Escape, or a scrim click on a modal dialog, clears `open` and reports `dismissed` once when `dismissible`. Keys are unique across the view. |
| `DialogOptions` | `title` (rendered as the panel heading and the accessible name), `modal` (default true), `dismissible` (default true), `width` (default 420 px) and a panel `style` whose unset background, radius and padding take dialog defaults. |
| Modal behavior | The layer covers the window with the theme's `scrim` token, wins hit testing so pointer and wheel input never reach content behind it, traps Tab, and moves focus inside on open (remembered child, else first focusable). Closing restores focus to the node focused before the dialog. Popups opened inside a dialog paint above it. |
| Non-modal behavior | No scrim; the panel blocks clicks on itself but content around it stays interactive and Tab flows through the dialog like any group. |
| `Role::Dialog`, `NodeSnapshot::modal` | The layer is exposed as a dialog with the title as its name; modal layers are flagged modal for assistive technology. |
| `DesktopHandle::open_modal(owner, options, view)` | A native window whose owner ignores pointer, keyboard, IME and file-drop input and hands focus back to the dialog while it is open. On Windows the dialog is an owned window, so it stays above its owner and minimizes with it. Closing the owner closes its dialogs. |

## Validation

Six focus tests cover scope trapping and memory, restoration on removal, modal dialogs (initial focus, Tab trap, scrim hit testing, wheel blocking, scrim and Escape dismissal, closing through the flag, restoration), non-modal dialogs and stacking order, arrow navigation and arrow-owning controls, and duplicate dialog keys. The full suite is 79 tests. The dialogs example passes `--smoke`, which opens the confirm dialog and a native modal window and exits when both windows close; a release screenshot with `--open` was reviewed.

## Remaining limits

- Dialogs are centered only; anchored popovers, tooltips and context menus are the next slice.
- Arrow navigation is geometric within one scope; it does not cross scopes or wrap.
- Native modals rely on host-side input blocking; only Windows gets true owned-window semantics, and there is no return value channel beyond shared state.
- Focus restoration targets a node id; if the opener was recreated (different key path or kind) while the dialog was open, focus is cleared instead.

# Milestone 6d: tooltips, tabs, split panes, context menus and popovers

Implemented on 2026-09-21. The core gains a clock and wake requests so hosts can sleep until a tooltip is due, closable tabs with automatic activation, draggable split panes, context menus anchored at the pointer or the focused control, and anchored non-modal popovers. Overlay painting, hit testing and dismissal now handle several open panels at once.

## Run

```sh
cargo run -p rust-gui --example panes --release
cargo run -p rust-gui --example panes --release -- --open   # starts with the share popover open
```

Hover the toolbar for tooltips, right-click the editor (or press Shift+F10 while a control inside it has focus) for a context menu, drag or arrow-key the dividers, and close the closable tabs.

```rust
ui.button_keyed("open", "Open…");
ui.tooltip("Open a document from disk");

let response = ui.tabs("documents", &mut selected, &tabs, |ui, index| { /* panel */ });
if let Some(index) = response.closed { tabs.remove(index); }

ui.split("workspace", &mut sidebar, Axis::Horizontal, SplitOptions::default(), |ui| { /* first */ }, |ui| { /* second */ });

if let Some(choice) = ui.context_menu("editor", &items, style, |ui| { /* content */ }) { apply(choice); }

if ui.button_keyed("share", "Share ▾").clicked() { share = !share; }
ui.popover("share-popover", &mut share, PopoverOptions::default(), |ui| { /* anchored panel */ });
```

## Contracts

| API | Behavior |
| --- | --- |
| `Ui::tooltip(text)` | Attaches to the previously declared widget. Shown after `TOOLTIP_DELAY` (500 ms) of steady hover, below the widget and kept inside the window; hidden by movement off the widget, any press, key, wheel or focus loss, and not re-armed until the pointer moves again. Doubles as the accessible description. |
| `Context::set_now`, `Context::next_wake` | The host supplies a monotonic clock before events and declarations and reads the next wake time; headless tests advance time explicitly. The desktop loop sleeps until the earliest wake, stylesheet poll or surface retry. |
| `Ui::tabs(key, &mut selected, &[Tab], content) -> TabsResponse` | Roles TabList/Tab/TabPanel. Clicking a tab or pressing Arrow/Home/End on the focused tab selects at once and keeps focus on the strip; only the selected tab is in the Tab order, so Tab moves into the panel. `Tab::closable` adds a close button reported as `closed`; Delete on a focused closable tab reports the same. |
| `Ui::split(key, &mut fraction, Axis, SplitOptions, first, second)` | The first pane takes `fraction` of the axis; `min_first`/`min_second` clamp both sides in pixels. The divider (role Splitter, value 0–100) drags with pointer capture, moves ten pixels per arrow key, jumps with Home/End and accepts `SetValue`. The host shows resize cursors. |
| `Ui::context_menu(key, &[MenuItem], style, content) -> Option<usize>` | Right-click inside the area, or the Menu key / Shift+F10 while a control inside has focus, opens the menu at the pointer (or below the focused control), kept on screen. Navigation, disabled items, Escape and outside clicks work as for `Ui::menu`; the choice is reported once and focus returns to where the menu was opened. |
| `Ui::popover(key, &mut open, PopoverOptions, content) -> PopoverResponse` | An anchored panel below the previously declared widget. Focus moves inside on open and returns to the anchor on close; Escape, or a press outside the panel and its anchor, clears `open` and reports `dismissed`. Several popovers can be open; each paints above content and blocks clicks through itself. |
| `Event::ContextMenu(Point)`, `Key::ContextMenu` | Host events for right-click release and the Menu key / Shift+F10. |

Text leaves that anchor a popup or popover keep their intrinsic size as a layout minimum, since Taffy ignores the measure function once a node has children.

## Validation

Five overlay tests cover tooltip timing (wake requests, dwell, hiding rules), tab selection by click and keys with roles and close reporting, divider dragging within minimums plus keyboard and semantic values, context menus opened by pointer and keyboard with once-only choices and on-screen clamping, and popovers with anchoring, focus, dismissal and coexistence. The full suite is 84 tests. The panes example passes `--smoke`; release screenshots confirmed the tab strip, the popover anchored under its button with the button keeping its size, and the context menu at the pointer. A native tooltip screenshot could not be produced: a background script cannot bring the window to the foreground, so the real cursor never reached it. The hover, wake and redraw path is covered headlessly and by code review only.

## Remaining limits

- Tooltips are pointer-only (no keyboard-focus tooltips) and fixed to one delay and one placement rule.
- Tabs do not reorder or overflow-scroll; docking is a later slice.
- Split panes are two-way only; nested splits work but there is no persisted workspace layout.
- Context menus are one level deep, like `Ui::menu`; popovers have no arrow or placement options beyond "below the anchor, kept on screen".
