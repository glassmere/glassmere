# Milestone 25: carrying things about

Implemented on 2026-09-22. Three coverage rows were about moving data: the clipboard carried text and nothing else, a drag stayed inside the window it started in unless the platform would take it, and the diagnostics said nothing about files that failed to load or fonts that were not there.

## The clipboard

A clipboard holds one thing in several forms at once, so `Clip` is a struct rather than a choice. A copied shape is bytes to this application and a line of text to everything else, and both go on together.

| Piece | Contract |
| --- | --- |
| `Clip` | `text` is what other applications see; `image` is pixels; `custom` is bytes under a name of the application's own. `Clip::text`, `Clip::image` and `Clip::custom` make one, and `with_text` and `with_image` add the other forms. |
| `Ui::copy(clip)` | Puts it on the clipboard once the declaration is over. |
| `Ui::request_paste`, `Ui::pasted` | Asking is separate from reading, because reading a clipboard is the host's business and can be slow. What the host read stays readable until the next request. |
| Private formats | Kept by the host, beside the text they were copied with. They come back only while that text is still on the clipboard, which is how the framework knows nothing else has copied since. Putting a private format on the system clipboard needs platform calls the framework does not make, so these stay inside the application and travel between its windows. |
| Images | Behind the `images` feature, which is on by default. An image and text cannot go on the system clipboard together through this platform layer, so an image wins. Scenes hold premultiplied pixels and clipboards want straight ones, so they are converted both ways. |

## A drag between two windows

A drag that leaves its window used to be offered to the operating system, and to stay in the window when the platform would not take it. Files can leave that way; an item or a piece of text cannot. Now the application's own windows are tried first.

The complication is that the window a drag starts in keeps the pointer while the button is held, so the window the pointer is over never sees it move. The desktop carries the drag instead: it takes the payload out of the first window, gives it to whichever of its windows is under the pointer, and drives that window from the events the first one is still receiving.

| Piece | Contract |
| --- | --- |
| `Context::adopt_drag(payload, at)` | Takes over a drag that started elsewhere. Nothing here is its source, so no node shows as the one being dragged, but drop targets light up and a release drops it. |
| `Context::withdraw_drag` | Takes it back when the pointer moves on to another window. Unlike `export_drag` it tells nobody the drag ended: the window it started in still owns the source. |
| `Context::accepts_drop(at, payload)` | Whether a drop there would land on a target that takes it, which is how the host knows what to report back to the source. |
| The source | Hears `DragOutcome::Dropped` or `Cancelled` through its `Ui::drag_source`, exactly as it does for a drag the system took. |

## Diagnostics for what is missing

| Piece | Contract |
| --- | --- |
| `DiagnosticKind::Resource` | A file the view asked for could not be read or decoded. Raised where the host delivers the failure, for images and watched files alike, with the path and the reason. |
| `DiagnosticKind::Font` | A style asked for a family that is not installed, so the text is drawn in another one. `TextSystem::has_family` answers, `Context::requested_families` names what a declaration asked for, and the host reports each family once. |

## Validation

- Three clipboard tests in the core: a copy reaches the host and what the host reads reaches the view, a clip carries every form at once, and copying from inside a container disturbs nothing.
- One opt-in test takes over the real system clipboard: `cargo test -p rust-gui-winit --features images -- --ignored` writes a half-transparent image and reads it back with its premultiplication restored, then shows private bytes surviving while the clipboard is still ours and vanishing when something else copies.
- Two core tests for the drag: one carries a payload from one context to another, drops it, and hears the outcome once in the window it came from; the other takes an adopted drag back without ending it.
- `scripts/test-cross-window-drag.ps1` does it for real. It opens the application workbench and its details window, puts them side by side, selects a ticket and drags it across with synthetic mouse input:

```
LINKED Linked: #0
CROSS_WINDOW_DRAG_OK
```

- One diagnostics test covers a failed image and a family that is not installed.

The workspace suite is 312 tests; formatting and clippy with all features are clean; all 19 examples pass their smoke run.

## Remaining limits

- Overlapping windows are not ordered when the desktop decides which one a drag is over: the first match wins, because the platform's stacking order is not asked for.
- A private clipboard format is the application's own. Two separate processes of the same application do not share it.
- Text and an image cannot be copied together; the image wins.
- The font check knows whether a family is installed, not whether the glyphs a text needs are in it, so a family that is there but lacks the script is not reported.
