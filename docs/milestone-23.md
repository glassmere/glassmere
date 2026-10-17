# Milestone 23: a widget of your own

Implemented on 2026-09-22. A custom leaf could already say how big it was, what it painted and what it was for. What it could not do was hear anything: the only input a custom widget received was the button activation path, so a knob, a canvas or a ruler had to be assembled out of built-in controls or go without.

The test of an extension point is whether someone outside can use it, so this milestone ends in a crate outside the framework. `examples/dial-widget` is a rotary dial that depends on `seaglass` and on nothing private to it. It has its own state, hears the pointer, the keyboard and the wheel, reads the theme's colours, sits in the tab order, tells assistive technology what it holds, and is tested headlessly with the same harness the built-in controls use. If any of that needed an internal type, it would not compile.

```rust
ui.widget("dial", Style::default(), |input| {
    for event in &input.events {
        self.apply(event);           // pointer positions are the widget's own
    }
    CustomWidget { size, semantics, primitives, input: listens }
});
```

| Piece | Contract |
| --- | --- |
| `Ui::widget(key, style, build)` | A leaf built from what it was given: `build` is handed a `WidgetInput` and answers with the `CustomWidget` to paint. |
| `WidgetInput` | The room it was given last frame, whether it is hovered, pressed, focused and enabled, and the events that have reached it since, in order. Pointer positions are relative to the widget's own top left corner, so nothing in the widget knows where on the screen it is. |
| `CustomWidget::input` | What it listens to, as an `EventArea`. Nothing is delivered until it asks. |
| Focus | `Semantics::focusable` now puts any role in the tab order and gives it the keyboard. It used to work for buttons alone. |
| `Semantics::value`, `Semantics::numeric` | What assistive technology reads out after the label, and where the value sits on its scale. A dial that reports neither is a dial that says nothing. |
| Keeping an event | A widget that captures and consumes keeps its keys and the wheel from the containers above it, which is why an arrow key turns the dial without also scrolling the page it sits on. |
| State | Where the rest of the application's state is: in a struct of the author's own, passed in. There is no widget-state registry, because the framework has nowhere better to keep it than the caller does. |
| Style | `ui.theme()` for the tokens, `ui.class(..)` for a stylesheet class, and the `style` argument for the node itself, so padding, size and background work as on any container. |
| Testing | `seaglass-testing`'s `Harness`, which drives events, reads the accessibility snapshot and finds nodes by label, with no window. |

## Two rules the frame keeps

Both were found by building the dial, and both were already wrong for `Ui::event_area`:

- **Tab and Escape are never consumed.** An area that consumed the keyboard could trap the focus inside itself with no way out. They are still delivered; they are just not kept.
- **A widget of the application's own hears pointer events but never keeps them.** Consuming them would rob itself: focus, hover, press and capture are the frame's services to the widget, and an area that consumes pointer events stands between the pointer and everything under it. The rule lives in one place, `may_consume_pointer`, and both the hit test and the delivery path ask it.

A third thing the dial needed was missing rather than wrong: a drag that wanders outside a widget went to whatever was under the pointer, so an area stopped hearing the drag it had started. Pointer moves and releases now follow the pointer capture, as the built-in controls always did.

## Validation

Four tests in the dial's own crate, driving it through the public harness:

- the keyboard turns it, coarse and fine, Home and End reach the ends, and Tab still leaves it;
- a drag turns it from where it was grabbed, goes on working outside the dial, and stops at the release;
- it announces `Slider "Gain" value="70%" number=70.0`, and is found by its label;
- an arrow key turns the dial without scrolling the page under it, while the same key on a button in the same page does scroll it.

The workspace suite is 305 tests, formatting and clippy with all features are clean, and all 19 framework examples pass their smoke run. The dial's own window was photographed with three dials, one focused: `target/dial-widget.png`.

## Remaining limits

- A custom widget is a leaf. It cannot contain other widgets, so a compound control is a `Component` that declares built-ins, not a `CustomWidget`.
- Its size is intrinsic. It cannot measure itself against the room it is given except by reading last frame's `WidgetInput::size`, which is a frame behind.
- The events it hears are the ones `UiEvent` carries. There is no drag-and-drop, no IME and no touch in that list; those reach a widget through the built-in controls that own them.
- `EventArea` says what to hear with one flag per kind. A widget that wants to keep some keys and pass others must consume them all and pass none.
- Nothing stops two widgets from claiming the same shortcut; commands are the way to share those.
