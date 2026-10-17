# Milestone 13: outgoing OS drags, touch and gestures

Decisions taken on 2026-09-21, before the work: outgoing drags through the `drag` crate, with the core's side shaped so that a backend that can carry text fits in later without changing apps; an in-app drag becomes a system drag when the pointer leaves the window; a full multi-pointer model rather than touch mapped onto the one mouse pointer; kinetic scrolling after a touch pan.

Slices: 13a outgoing OS drags, 13b touch as a pointer with pan scrolling, inertia and long press together with 13c multi-pointer capture, 13d the gesture area with pinch and rotation, macOS gestures, Ctrl+wheel and pen pressure (all below). Milestone 13 is complete.

# Milestone 13a: outgoing OS drags

Implemented on 2026-09-21. A drag that leaves the window is handed to the operating system, so a file can be dragged from an app onto the desktop or into another application.

```rust
match ui.drag_source("export", "report.pdf", DragPayload::Files(vec![path]), style, |ui| ui.label("report.pdf")) {
    Some(DragOutcome::Dropped) => status = "Copied.",
    Some(DragOutcome::Cancelled) => status = "Cancelled.",
    None => {}
}
```

| Piece | Contract |
| --- | --- |
| Inside the window | Nothing changed: the drag is in-process, with `Item` and `Text` payloads, drop indicators and keyboard drags as before. |
| Hand-off | When the pointer leaves the window during an active drag, the host asks the platform to carry the payload. If it can, the in-app drag ends (`Context::export_drag`) and the system's begins; if it cannot, the drag stays in-process and may come back into the window. |
| Core protocol | `Context::dragging()` is the payload of the active drag, `export_drag()` takes it over and frees the pointer (the system swallows the button release), `finish_exported_drag(DragOutcome)` tells the source. None of the three looks at the kind of payload, which is what leaves room for a text backend. |
| `Ui::drag_source` | Now returns `Option<DragOutcome>`: `Dropped` or `Cancelled`, once, after a system drag. The framework never touches the files; an app that moves rather than copies acts on `Dropped`. |
| Platform | `seaglass-winit/src/os_drag.rs`, files only, through the `drag` crate on Windows (OLE, blocking until the drag ends) and macOS (the outcome arrives later and is polled). Always a copy. A small sheet-of-paper image follows the pointer. A path that does not exist is logged and the drag stays in-process. |
| Linux | Not supported: the `drag` crate needs a GTK window there, which winit does not provide. The dependency is not built on Linux and drags stay in-process. |
| Coming back | A system drag that returns to its own window arrives as an ordinary incoming file drop, as from any other application. |

## Validation

One new core test drives the protocol headlessly: nothing to export below the drag threshold, the payload once the pointer is outside, the source no longer dragging and the pointer released after the export, the outcome delivered exactly once, and an outcome without an export ignored. The suite is 236 tests plus 23 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features. The images example gained a file card to drag and passes its smoke run.

**Not verified:** the system drag itself. It needs real mouse input on the real desktop, which a background script cannot produce without taking over the user's pointer, so dragging the card from the images example onto the desktop or into Explorer wants a manual pass on Windows. macOS is compiled by CI only.

## Remaining limits

- Files only. Text, images and custom formats need a data object of our own, which means unsafe COM code that the workspace forbids today.
- Copy only; no move or link, and the drop effect the target chose is not reported.
- The drag image is a fixed icon, not a picture of the dragged node.
- While Windows runs the system drag our windows do not repaint.
- Cross-window drags of `Item` payloads between two windows of one app are still not possible.

# Milestone 13b and 13c: touch, and every contact a pointer of its own

Implemented on 2026-09-21. The two slices landed together, because the model chosen for many pointers is also the simplest way to add the first touch.

```rust
context.event(Event::Touch { id, phase: TouchPhase::Started, position, force: Some(0.4) });
```

| Piece | Contract |
| --- | --- |
| Model | Every contact has its own position, hover, capture and drags (`tree/touch_input.rs`). They are swapped into the context's pointer fields while that contact's events run, then swapped back. Widgets read the same fields as ever and needed no touch code; the mouse is simply the pointer that lives there between events. |
| `Event::Touch { id, phase, position, force }` | `TouchPhase` is `Started`, `Moved`, `Ended` or `Cancelled`. Ids may be reused once a contact has ended; a contact that starts twice lets go of its first press. `Context::touches()` lists the contacts that are down with their position and pressure. |
| Tap | Down and up within 10 px activates what is under the finger, exactly as a click does. No hover stays behind when the finger lifts. |
| Pan | A press that travels more than 10 px lets go of what it pressed without activating it and scrolls the innermost scroll view, collection or editor under the finger, on both axes. A slider, a split or grid divider, a dock handle and a dock tab follow the finger instead. |
| Inertia | On release the content keeps the speed of the last 100 ms and loses it exponentially (1/e in 0.325 s) until it falls below 50 px/s or the content ends. Any touch, press or wheel stops it; reduced motion never starts it. `Context::animating` is true meanwhile, so hosts keep drawing. |
| Long press | 500 ms without travel. On a drag source it picks the payload up, and the drag then follows the finger to a drop target. Anywhere else it requests the context menu at that point, as a right click does, and the release neither closes the menu nor clicks. `Context::next_wake` includes the deadline. |
| Many pointers | Two fingers move two sliders at the same time, and a finger and the mouse do not disturb each other. One pointer per control: a contact that lands on a control another pointer holds does nothing. |
| Pressed look | A node pressed by any contact paints as pressed. |
| Safety | A contact whose node disappears in a declaration lets go quietly; losing window focus ends all contacts and any fling. |
| Host | `WindowEvent::Touch` becomes `Event::Touch` in logical pixels with pressure normalized to 0..1; a window blocked by a modal dialog ignores touch like other input. On Windows winit answers the pointer messages, so the system does not repeat a touch as mouse events. |

A finding from the real-window run, fixed at its root: a fling scrolls a page every frame for about a second, which tripped the framework's own "excessive repainting" diagnostic after 60 frames. Frames that the framework scrolls itself no longer count toward it; a test reproduces the case.

## Validation

Seven headless tests in `tests/touch.rs`: a tap with a wobble; a pan that follows the finger exactly, does not click and then flings with a decaying speed to a total within the analytic bounds, without tripping the repaint diagnostic; a touch catching a fling, and reduced motion; the long-press menu with its wake time, surviving the release and taking a tapped choice; a drag source that pans when moved at once and is picked up after a rest; two sliders under two interleaved fingers with a third finger refused; and contacts outliving their node or the window's focus.

`scripts/test-touch.ps1` injects real touch input into the controls gallery through the Windows touch injection API, which exercises winit's pointer messages, the host and the core together. On this machine: a tap toggled a checkbox exactly once, a slider followed the finger from 50 to 95, and a swipe of 40 physical pixels scrolled 124, the rest by inertia, without pressing anything. The script keeps the window topmost while it runs and refuses to touch a point that belongs to another window.

The suite is 243 tests plus 23 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features; all 16 examples pass their smoke run.

## Remaining limits

- A pan that starts on a text field focuses it first, because focus moves on press.
- No overscroll bounce, no snap points, no scroll chaining to the parent once a view reaches its end during one pan.
- Text selection by touch (handles, magnifier) does not exist; a touch in an editor places the caret and pans.
- A fling belongs to the point where the finger lifted; content that moves under it keeps scrolling whatever is there.
- Tooltips never show for touch. Outgoing OS drags start from the mouse only.
- Not run on a physical touch screen or a pen; injected input reports a fixed pressure.

# Milestone 13d: the gesture area

Implemented on 2026-09-21. A surface that fingers move, scale and turn. This completes milestone 13.

```rust
let gesture = ui.gesture_area("canvas", style, |ui| { /* the picture, and controls if any */ });
if gesture.changed() {
    zoom *= gesture.scale;
    turn += gesture.rotation;
    // ...and move the view by pan_x, pan_y about gesture.center
}
```

| Piece | Contract |
| --- | --- |
| `Ui::gesture_area(key, style, content) -> GestureResponse` | Reports what happened since the last declaration, once: `pan_x`, `pan_y`, `scale` (a factor, 1 is no change), `rotation` (clockwise radians), `center` (where the change is centred, from the area's corner), `tapped`, and live `active` and `pressure`. The app owns the view state and applies the change; `changed()` says whether there is one. |
| One finger | Pans, after the same 10 px slop as elsewhere, catching up with the finger when it leaves the slop. Released within the slop it is a tap at that point. It scrolls nothing and flings nothing: the area keeps its fingers. |
| Two fingers | Scale by the ratio of their distances, turn by the change of the angle between them, and pan by the movement of the point between them, with no slop. A finger that lifts leaves no jump and no tap. A third finger works against the first only. |
| Trackpad and wheel | `Event::Zoom(factor)` and `Event::Rotate(radians)` apply to the gesture area under the mouse pointer and are not handled when there is none. The host sends a macOS pinch and rotation as these, and Ctrl with the wheel as a zoom of a tenth per notch, which is also how a pinch on a Windows touchpad arrives; where no gesture area takes it, the wheel scrolls as before. |
| Controls inside | A control inside the area is pressed as anywhere else, and whatever lies over the area (a popup, a dialog) keeps a contact from it. |
| Long press | Resting on the area requests the context menu of an enclosing `context_menu`. If none opens, the finger stays the area's, because resting before a pan is how people aim. |
| Pressure | `GestureResponse::pressure` is the firmest contact in the area, 0 to 1, where the device reports it; `Context::touches()` has it per contact. |

New example: `gestures`, a photo to pan, pinch and turn about the point between the fingers, with two sliders that can be dragged at once. `--demo` starts it moved, enlarged and turned.

## Validation

Five headless tests in `tests/gestures.rs`: a pan that waits out the slop, catches up, reports once, leaves the scroll view under it alone and does not fling, and a tap; two fingers giving exactly a factor of two, exactly a quarter turn, a pure pan when both move alike, and no jump or tap when one lifts; a control inside being pressed while a touch outside pans the page; zoom and rotate events landing on the area under the pointer, refusing nonsense and falling through elsewhere; and the long press with and without a menu.

`scripts/test-touch.ps1` now also drives the gestures example with two injected contacts: moving them from 80 to 160 px apart gave "Zoom 2.00" with no tap, and two fingers on the two sliders at once took them to 6 and 94. Together with the three checks of 13b it passed four runs in a row; in one earlier run the single slider of the controls gallery read 100 where the others read 95, which I could not reproduce or explain.

The suite is 248 tests plus 23 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features; all 17 examples pass their smoke run; `scripts/test-theming.ps1` and `scripts/test-controls.ps1` pass.

**Not verified:** Ctrl with the wheel and a touchpad pinch on a real window (a posted wheel message does not reach the app, and real wheel input would take over the user's pointer); the macOS pinch and rotation events, which only CI compiles; a physical touch screen or a pen.

## Remaining limits

- The mouse does not pan a gesture area by dragging; apps that want that read the pointer themselves.
- No double tap, no swipe recognizer, no inertia for a canvas pan.
- Gestures with more than two fingers use the first two.
- Pen tilt, barrel buttons and the eraser are not reported; a pen is a touch with pressure.
