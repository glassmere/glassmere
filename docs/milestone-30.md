# Milestone 30: the layouts Windows offers

Implemented on 2026-09-22. A title bar the application draws gets no snap layouts flyout, because Windows asks the window itself where its maximize button is and our window did not answer. The question is `WM_NCHITTEST`, and the answer it wants is `HTMAXBUTTON`.

That message is *sent*, not posted, so no message loop can answer it: it reaches the window procedure directly and nothing else. Reaching the window procedure means subclassing it, and subclassing means unsafe code, which this workspace forbids everywhere.

## One crate with unsafe in it

`seaglass-win32` is that crate and the only one: about a hundred lines, one window procedure, a rectangle it reads and a list of what it saw. It is the only crate in the workspace that does not take the workspace lints, and what it does instead is spelled out in its manifest. Every other crate still forbids unsafe.

| Piece | Contract |
| --- | --- |
| `attach(hwnd)` | Subclasses the window and gives back a `Caption`. Dropping it puts the window back the way it was. |
| `Caption::set_maximize(rect)` | Where the maximize button is, in client pixels. `None` when the view has none, which is when nothing changes at all. |
| `Caption::take()` | What the procedure saw over the button: `Hover(x, y)` and `Left`. |
| `WM_NCHITTEST` | Answered by the default procedure first, so borders and everything else keep working; only the button's own rectangle is overridden with `HTMAXBUTTON`. |
| `WM_NCMOUSEMOVE`, `WM_NCMOUSELEAVE` | Recorded and then forwarded, because those are the messages Windows itself watches to decide when to offer its layouts. |
| Pressing it | Left entirely to Windows, which maximizes the window itself. The framework hears about it through `Ui::window().maximized` on the next frame. |

The framework side is `Context::maximize_button`, which is where `Ui::window_buttons` put the middle button, and the host publishes it in physical pixels after every layout. Because the button belongs to the caption as far as Windows is concerned, no ordinary mouse message reaches it any more, so the hover the procedure reports is turned back into `Event::PointerMoved` and the button lights up exactly as it did before.

## Validation

`scripts/test-titlebar.ps1` now asks the window directly, which is the whole contract in one line:

```
HITTEST Maximize -> 9
HITTEST Close -> 1
```

Nine is `HTMAXBUTTON` over the maximize button, one is `HTCLIENT` over the button beside it. The flyout itself was photographed over a hovered button in `target/snap-titlebar.png`, with the layout grid hanging below the framework's own teal hover highlight. A window with the system's own caption was hovered the same way as a control, and behaved the same.

The workspace suite is 326 tests; formatting and clippy with all features are clean, including the new crate under its own lints; all 20 default examples pass their smoke run.

## Remaining limits

- Windows only, and Windows 11 only: on Windows 10 the hit test is answered and nothing else happens, which is what should happen.
- The maximize button is the only caption button Windows is told about. There is no equivalent question for minimize or close.
- Pressing the button is the system's, so the framework never sees the press and cannot paint a pressed state for it. The hover state it does paint.
- Snapping by dragging a window to an edge is whatever the platform does with a move, and nothing here changes it.
