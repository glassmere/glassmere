# Milestone 29: a title bar of your own

Implemented on 2026-09-22. `AppOptions::decorations` could already turn the system's frame off, and that was all: a window without it had no way to be moved, maximized or resized, because those belong to the frame that was no longer there. This is the other half.

```rust
ui.title_bar("bar", style, |ui| {
    ui.label("My application");
    ui.menu_bar("menu", &menu());
    ui.container("gap", Style::default().grow(1.0), |_| {});
    ui.window_buttons("buttons");
});
// Last in the view, so the edges lie over everything else.
ui.resize_frame("edges", 6.0);
```

| Piece | Contract |
| --- | --- |
| `Ui::title_bar(key, style, content)` | A row that moves the window when pressed, and maximizes or restores it when pressed twice within four hundred milliseconds and twenty-four pixels. Whatever is declared inside keeps working: a press that lands on a control there is that control's, because the hit test finds the control first. |
| `Ui::window_buttons(key)` | Minimize, maximize or restore, and close. The middle one follows `Ui::window().maximized`, and each is announced by its translated name rather than by its glyph. |
| `Ui::resize_frame(key, thickness)` | The eight edges and corners, out of flow and over the content, each pulling its own way with the platform's own resize cursor. Declared from the size the window had last frame, and skipped entirely for a window too small to have them. |
| `Ui::window_command(command)` | `Drag`, `Resize(edge)`, `Minimize`, `ToggleMaximize`, `Close`, for a frame of an entirely different shape. The three calls above are made of these. |
| `Ui::window()` | What the host says about the window: whether it is maximized, whether it has the keyboard. |
| `Ui::window_size()` | The logical size it was last declared at, which is what places the edges. |

A press asks; the platform does the rest. `drag_window` and `drag_resize_window` take the pointer over for as long as the drag lasts, so no move or release comes back, and the host runs them between frames where nothing of the window's is borrowed.

## A symbol is not a name

The window buttons are glyphs: an en dash, a square, a cross. The framework's own diagnostic already counted a control whose label is nothing but symbols as unnamed unless it had a tooltip, but the accessibility tree still published the glyph, so a screen reader said "cross" where it should have said "Close". A tooltip is now the accessible name whenever the label has no letter or digit in it, which fixes every icon button in every application, not only these three. The four names are built-in messages, translated with the rest.

## Validation

Six tests in the core, with the host played by the test: pressing the strip asks to move and pressing it twice asks to maximize, while a third press long afterwards is a move again; a control inside the bar takes the press instead; the three buttons ask for what they say; the middle one changes with what the host reports; the eight edges lie along the window, each asking for its own direction and offering its own cursor, while the middle of the window asks for nothing; and a window too small for its own edges has none.

`scripts/test-titlebar.ps1` drives a real window with synthetic mouse input: it moves it by the strip, maximizes it by pressing twice, restores it, resizes it by the bottom-right corner, and minimizes it with the button, which it finds by the accessible name:

```
MOVED by 120,90
STATE Maximized
RESIZED by -120,-73
STATE Minimized
TITLEBAR_OK
```

The `titlebar` example is the window in the screenshot `target/titlebar-window.png`. The workspace suite is 326 tests; formatting and clippy with all features are clean; all 20 default examples pass their smoke run.

## Remaining limits

- Snapping a window to a screen edge by dragging it there is the platform's, and comes with `drag_window` on Windows; where a platform does not do it, nothing here adds it.
- A double press is detected by the framework's own clock rather than the platform's double-click time.
- The edges are placed from the previous frame's size, so they follow a resize one frame late. Nothing can see it while the platform is drawing the drag.
- There is no way to turn the system frame back on while the window is open: `AppOptions::decorations` is read when it opens.
- Full screen is not among the commands.
