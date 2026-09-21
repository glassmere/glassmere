# Milestone 10: the data grid

Row 19 of the coverage table lists what a professional grid still lacks: column resizing and reordering, filtering, inline editing, frozen and virtual columns. Decisions taken at the start: column layout lives in the app-owned `GridState`; the model filters, like it sorts; editing uses a cell cursor with typed editors; and wide grids build on general two-axis scrolling in the engine rather than a grid-only mechanism. Slices: 10a horizontal scrolling (below), 10b column resize, reorder and hide, 10c cell cursor and inline editing, 10d the filter row, 10e the horizontally scrolling grid with frozen and virtual columns.

# Milestone 10a: horizontal scrolling

Implemented on 2026-09-21. Every scroll view and virtual collection now scrolls along whichever axis its content overflows. Nothing is opted into: content wider than its view used to be clipped without a way to reach it, and now it scrolls.

## Run

```powershell
cargo run -p rust-gui --example layout --release
```

The last section is a strip of sixteen cards in a full-width view. Shift+wheel or a touchpad scrolls it; so do Left and Right once Tab has reached it.

## Contracts

| Piece | Contract |
| --- | --- |
| Overflow | A scroll view's scrollable width is how far its direct children reach past its end side, plus its padding: past the right edge, or past the left edge in a right-to-left view. Less than half a pixel is ignored. Every node clips its own content, so a wide grandchild inside a full-width child does not count: put wide content directly in the view, or in a child that is itself wide (a row without a width is as wide as its children). Wrapped text and `Percent(1.0)` children never overflow. |
| `ScrollState` | Gains `offset_x` and `maximum_x`. `offset_x` counts from the start side, so zero always shows the beginning in both directions. |
| `Event::Scroll { delta_x, delta_y }` | `delta_x` is physical: positive moves the view right. One diagonal gesture moves both axes. Per axis, the innermost view under the pointer that can still move takes it; a gesture that exhausts an inner view does not spill into the outer one, the next gesture does. |
| Desktop host | Horizontal wheels and touchpads deliver `delta_x`; Shift turns a plain wheel sideways. |
| Keyboard | Left and Right move a focused scroll view by 40 px when it can move sideways. Up, Down, PageUp, PageDown, Home and End stay vertical. |
| Focus reveal | Focusing a control, by Tab or by assistive technology, brings it into view on both axes through every enclosing view. The start edge wins when the control is wider than the view. |
| Scrollbar | A 4 px thumb along the bottom, sized and placed like the vertical one, starting at the start side. It is an indicator, as the vertical one is: neither can be dragged yet. |
| Actions | `Action::ScrollByX` (physical pixels) and `Action::ScrollToX` (offset from the start). |
| Accessibility | A view that can move sideways reports its horizontal offset and range and supports scroll left, scroll right and set-offset on both axes. |
| Right to left | Content overflows to the left, the thumb starts at the right, and physical directions keep their meaning: scrolling left goes toward the end. |

## Validation

Six new tests. Core: overflow measured, the wheel moving children, clipping and hit testing following, a diagonal gesture and clamping; both thumbs sized and placed, and no sideways thumb for wrapped text and full-width children; arrow keys and focus reveal in both directions; the wheel going to the innermost view that can still move; right-to-left overflow, thumb, physical direction and reveal. Host: the accessibility tree reports the horizontal range only where there is one and takes scroll right, left and set-offset.

The full suite is 203 tests plus 18 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features. All fourteen examples pass `--smoke`, and the theming and controls scripts pass.

Because the change applies to every scroll view, every example was captured and the bottom edge of each window inspected: none had grown a horizontal scrollbar. The one that did appear was earned: the heading I first wrote for the new section was one unwrapped line a few pixels wider than the window, which used to be clipped silently and now made the page scroll. The heading was shortened.

In the real window, nine Tabs reached the strip (the page scrolled down to reveal it) and six presses of Right moved it 240 px, with the second card clipped at the view's left edge and the thumb travelling along the bottom.

Two verification routes failed for reasons outside this change and are recorded so they are not tried again: wheel messages posted to the window never reached the app although winit handles them, and UI Automation offers no scroll pattern at all because accesskit's Windows adapter does not implement it. `scripts/screenshot.ps1` now presses Tab before it types, and has a `-Wheel` option that did not work here.

## Remaining limits

- Scrollbars cannot be dragged or clicked, on either axis.
- Only direct children extend the scrollable area, because every node clips its content.
- The data grid's header does not follow its rows sideways yet; that is 10e.
- Popups scroll vertically only.
- Real horizontal wheel and touchpad input was not exercised here, only the key, focus and headless event paths.
