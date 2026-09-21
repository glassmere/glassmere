# Milestone 13: outgoing OS drags, touch and gestures

Decisions taken on 2026-09-21, before the work: outgoing drags through the `drag` crate, with the core's side shaped so that a backend that can carry text fits in later without changing apps; an in-app drag becomes a system drag when the pointer leaves the window; a full multi-pointer model rather than touch mapped onto the one mouse pointer; kinetic scrolling after a touch pan.

Slices: 13a outgoing OS drags (below), 13b touch as a pointer with pan scrolling, inertia and long press, 13c multi-pointer capture, 13d the gesture area with pinch and rotation, macOS gestures, Ctrl+wheel and pen pressure.

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
| Platform | `rust-gui-winit/src/os_drag.rs`, files only, through the `drag` crate on Windows (OLE, blocking until the drag ends) and macOS (the outcome arrives later and is polled). Always a copy. A small sheet-of-paper image follows the pointer. A path that does not exist is logged and the drag stays in-process. |
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
