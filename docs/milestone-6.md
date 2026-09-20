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
