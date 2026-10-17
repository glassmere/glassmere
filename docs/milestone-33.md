# Milestone 33: everything drawn over the window

Implemented on 2026-09-22. The menu that left its pixels behind was one of a class, so the class was swept: everything the framework draws over a window rather than as part of it.

Three things are painted straight into the scene instead of being declared as nodes, which means nothing invalidates them when they move or go: the exit ghosts of a removed subtree, the tooltip, and the indicators of a dock drag. Ghosts were already covered, conservatively, by damaging the whole window while any of them exists. The other two were not covered at all.

| Piece | Contract |
| --- | --- |
| `Context::painted_overlay(rect)` | An overlay names the rectangle it drew. The rectangle is damaged now, and again on the next frame, so the frame it stops being drawn repaints what it covered. |
| The tooltip | Names its panel. A tooltip that appeared, moved to another control or went used to leave itself on the window. |
| The dock drag | Names its drop preview, each compass indicator and the label that follows the pointer. |

## What was found

A tooltip disappearing left 6,271 stale pixels of a 320 by 360 window. It affected every window with a tooltip in it, which is most of them, including the caption buttons of the title bar from [milestone 31](milestone-31.md).

## Validation

`crates/seaglass-render-cpu/tests/damage.rs` gained a sweep of every overlay a window can have, each painted once with damage and once in full and compared pixel for pixel: a tooltip on a dwell and after the pointer leaves, a context menu opened and dismissed with Escape, a popover, a modal dialog with its scrim, and a toast that expires on its own clock. It is the test that found the tooltip.

In a real window, the panes example was hovered until its tooltip appeared and then left: `target/tip-hovering.png` and `target/tip-after.png`, the second of which is the window exactly as it was before.

The workspace suite is 329 tests plus 31 opt-in GPU tests; formatting and clippy with all features are clean; all 20 default examples pass their smoke run.

## Remaining limits

- An overlay's rectangle is damaged for one extra frame after it stops being drawn, which costs a repaint of that area and nothing else.
- Ghosts still damage the whole window rather than their own rectangles, because a transform can carry them anywhere. A window with an exiting toast repaints entirely for the length of the fade.
