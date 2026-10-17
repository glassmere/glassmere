# Milestone 17: events that travel, and the rest of touch

Implemented on 2026-09-22. Two rows of the coverage table asked for the same thing from different sides: something above a widget that can watch what reaches it and stop it, and the gestures a touch screen expects.

## Events that travel: `Ui::event_area`

```rust
let arrived = ui.event_area("canvas", EventArea::pointer().capturing().consuming(), style, |ui| {
    // Nothing in here sees the pointer while the area holds it.
});
```

| Piece | Contract |
| --- | --- |
| `Ui::event_area(key, area, style, content) -> Vec<UiEvent>` | A column that is told what reached the widgets inside it, in the order it arrived, once. |
| `EventArea` | What it watches (`pointer`, `keyboard`, `scroll`, `custom`, or `all`), whether it sees the event before the widgets (`capturing`) or after (the default), and whether it lets the event through (`consuming`). |
| Path | A pointer event travels from the root down to the innermost node under it and back up; a key or text event follows the focus. Capturing areas are offered it on the way down, outermost first; the others on the way up, innermost first. A consuming area ends the journey there. |
| Consuming a pointer event | A capturing area that consumes pointer events stands between the pointer and everything under it, like a scrim: nothing under it is hovered, pressed or dragged, though the context still knows where the pointer is. |
| `UiEvent` | `PointerDown`, `PointerUp`, `PointerMoved`, `Scroll`, `Key`, `Text` and `Custom`. |
| `Ui::emit(name, data)` | A widget tells the areas above it something of its own. It travels up from the container being declared and arrives at the next declaration, like every other input, and asks for that declaration itself. |
| Cost | A view that declares no areas takes exactly the path it took before: the dispatch begins with a check that the set is empty. |

## Touch: a second tap, a flick, and selecting text

| Piece | Contract |
| --- | --- |
| `GestureResponse::double_tapped` | A tap within 400 ms and 24 px of the one before it. A third tap starts again rather than pairing with the second. |
| `GestureResponse::swipe` | A flick, reported when the finger lifts: a direction and a speed in logical pixels a second. A drag slower than 350 px/s is a pan and reports nothing. |
| Text | Resting a finger in an editor selects the word under it and keeps the press, so the drag that follows extends the selection rather than panning. A second tap selects the word too, as a double click does. The context menu is not opened for a rest in text. |
| `Pen` | Tilt along each axis, the barrel button and the eraser, carried by `Event::Touch` and `Context::touches`. No desktop backend reports them today: winit 0.30 gives pressure alone, and reading the platform's pen messages needs unsafe code the workspace forbids, so the host leaves them empty. |

## Validation

Six tests in `seaglass-core/tests/event_areas.rs`: an area told what reached the widgets, once, with the widgets still working; a capturing area that consumes taking the event from an inner area and the button, leaving nothing hovered; an inner area answering first on the way up and keeping the event from the outer one; keys and text following the focus while scrolling follows the pointer; a widget emitting to the area above it, arriving on the next declaration and only once; and a view with no areas dispatching exactly as before.

Touch has two new tests: a second tap told apart from a first and from a third, a flick told apart from a slow pan, with the direction and speed; and a rest in a text field selecting a word, the drag extending it, a second tap selecting again, and typing replacing what was selected.

The workspace suite is 276 tests plus 26 opt-in GPU tests; formatting and clippy are clean; all 17 examples pass their smoke run, and `scripts/test-touch.ps1` still passes with real injected touch.

## Remaining limits

- An area hears about events, it does not handle them: there is no way to answer an event from inside the dispatch, because everything in this framework is applied at the next declaration.
- `Ui::emit` from a subtree that does not exist yet is lost, since there is no node to send it from and no area to receive it on that frame.
- Selection handles, the knobs a finger drags to adjust a selection, do not exist; a drag from the word that was selected is the only way to extend it.
- Pen tilt and buttons have no producer.
- A swipe is reported by a gesture area alone. Elsewhere a flick scrolls, which is what a list wants.
