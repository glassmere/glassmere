# Coming from the web

glassmere lays out with flexbox, so most of what you know carries over. This page maps CSS to a `Style` and lists where the defaults and the behaviour differ, so that a first screen does not turn on a surprise.

Everything here was checked against Chromium (the engine behind Tauri's WebView2) with the parity bench in `tools/parity`, which builds the same scenario in a page, in Qt and in glassmere and drives all three with real mouse and keyboard input. Where the bench found no difference, this page says they agree. Where it found one, this page says what it is.

## The defaults that differ

These are the ones to set on purpose, or the first screen looks off.

| | The web | glassmere |
| --- | --- | --- |
| `gap` | 0 | **12** on every container made with `Style::default()`. Set `.gap(0.0)` where you do not want it, or start from `Style::web()`. |
| `flex-shrink` | 1, so children shrink to fit | **0**: children keep their size and overflow. Set `.shrink(1.0)` to squeeze them, as three 100 px panels in a 200 px row do on the web, or start from `Style::web()`, which has CSS's `gap` 0, `shrink` 1 and stretching `align-items`. |
| `align-items` | `stretch` | **`Start`**: children keep their own size across the axis. Set `.align_items(Align::Stretch)` to fill it. |
| `box-sizing` | `content-box` | **border box**, as `* { box-sizing: border-box }` gives you, which most pages set. `width` includes padding and border. |
| padding | per side | one number for all four sides |
| text is selectable | yes | no: `Label::new(..).selectable()` |
| pointer over a button | the arrow, unless styled | **a hand** over anything that takes a press |

## CSS to `Style`

| CSS | glassmere |
| --- | --- |
| `display: flex; flex-direction` | `ui.row(style, ..)`, `ui.column(style, ..)` |
| `gap`, `row-gap`, `column-gap` | `gap`, `row_gap`, `column_gap` |
| `align-items`, `align-self` | `align_items`, `align_self` |
| `justify-content` | `justify_content`: `Start`, `Center`, `End`, `SpaceBetween`, `SpaceAround`, `SpaceEvenly` |
| `flex-grow`, `flex-shrink` | `grow`, `shrink` |
| `flex-wrap: wrap` | `wrap_items(true)` |
| `width`, `height` | `Length::Px`, `Length::Percent`, `Length::Auto` |
| `min-width`, `max-width`, `min-height`, `max-height` | `min_width`, `max_width`, `min_height`, `max_height` |
| `aspect-ratio` | `aspect_ratio` |
| `padding` | `padding` (one value) |
| `margin` | `margin`, `margin_edges(Edges::new(top, right, bottom, left))` |
| `border` | `border(width)`, `border_color(color)` |
| `border-radius` | `radius` |
| `position: absolute; left; top` | `offset(x, y)`: the node leaves the flow and sits at that place in its parent |
| `z-index` | `Style::z_index`, among siblings; later siblings are on top at the same value |
| `overflow: hidden` | every container clips its children to its box |
| `overflow: auto` | `ui.scroll(style, ..)` |
| `display: grid` | `ui.grid(style, ..)` with `columns`, `rows`, `areas` |
| `display: none` | do not declare the widget |
| `<fieldset disabled>` | `ui.enabled(false, ..)` |
| `cursor` | `Style::cursor(Cursor::Grab)` and the others |
| `outline` on `:focus-visible` | `Style::focus_ring(FocusRing::new(2.0).color(c))`, `Style::no_focus_ring()` |

A stylesheet takes the same properties by name: `gap`, `padding`, `margin`, `border`, `grow`, `wrap-items`, `radius`, `background` and so on. See [styling](styling.md).

### What agrees

The bench found glassmere and Chromium in agreement on where boxes land for: alignment and distribution along both axes, `grow`, `shrink` where it is set, minimum and maximum sizes, percentage sizes inside padded parents, margin, border and wrapping rows, overlapping and stacking order, a disabled parent (the state and the focus order both follow), a widget that is not shown, and a child larger than its parent (the clipped part cannot be clicked).

### What cannot be said yet

Padding and border are per side on `row`, `column`, `grid` and `container` through `Style::padding_sides` and `Style::border_sides`, and `margin` is per side everywhere; a label, button, field, scroll view or list has the one `padding` and the one `border`, and a per-side border is straight, whatever the `radius`. `z_index` orders siblings only: each parent is a stack of its own, and a popover or dialog is how something goes above the whole window. `Style::default()` keeps the 12 px `gap` and `shrink` 0, since changing them breaks every layout and waits for 0.2.0; `Style::web()` is CSS's flex defaults for anyone who wants them now.

Three things that were on this list are no longer: `Style::align_content` shares the room left across the lines of a wrapped row, `Ui::measure_text` and `Ui::measure_text_wrapped` size a painted control from the text the layout will measure, and `NumberOptions::field_style`, `NumberOptions::stepper_style` and `Autocomplete::style` make those fields compact. A button with an icon is a custom widget with `Role::Button` and `WidgetBuilder::on_click`.

## Behaviour that differs

- **Focus ring.** It shows after the keyboard and not after a click, as `:focus-visible` does, and always on a field you type in. It is a stroked outline inside the box that follows `radius`. One difference: Chromium keeps the ring on a slider that was already focused with the keyboard when you click it; glassmere hides it on the press.
- **What takes focus.** A container that only lays things out does not, and a click on it, on empty space or on a spacer leaves focus where it was. A scroll area takes focus only while it scrolls, so that the arrow keys scroll the one you clicked. Chromium does not focus a scroller on a click. `Style::focusable(false)` keeps any widget out of the focus order while it still reacts to the pointer.
- **Tab order.** Tab wraps from the last widget to the first. A browser leaves the page there, to its own chrome.
- **A click.** A press that began on a widget and is released on it. Drag away and let go and nothing happens, as on the web. For a widget you paint yourself, `WidgetBuilder::on_click(message)` gives you that, with the keyboard activation, and a role such as `Checkbox` tells a screen reader what it is.
- **Radio buttons.** Choosing the one that is already chosen sends nothing, as `change` does not fire.
- **Switch.** A browser says `switch`, and so does glassmere: `Ui::switch` is a `Role::Checkbox` with `Semantics::switch` set, which the accessibility tree reads as a switch. A test that looks one up by `Role::Checkbox` still finds it.
- **Pointer.** The hand over every pressable widget is glassmere's choice. `Style::cursor(Cursor::Default)` on a node and what is inside it gives the arrow.

## Performance, compared

A list of rows is cheap in the browser up to many thousands, because it scrolls on the compositor. In glassmere a long list is declared and laid out each time the view runs, though it paints only what is in view and a wheel step does not run the view. On the bench, a plain list of 1,000 rows costs about 5 ms to change one label and about 1 ms to scroll, on par with Chromium; at 5,000 rows it is about 40 ms to change one label and about 4 ms to scroll.

Past a thousand rows or so, use a virtual list, which declares only the rows in view and stays under 1 ms at any size, or put the rows in a `Ui::memo` so the view does not declare them again while they are unchanged. See [performance](performance.md).

## Try it

`tools/parity` is the bench these findings came from. `node run.mjs` there writes a report with both toolkits next to glassmere for each scenario, so you can see a difference before you meet it. A scenario is one JSON file.
