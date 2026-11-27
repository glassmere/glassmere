# Building a widget

There are three ways to add something to the framework, and the first one that fits is the right one.

## The built-in controls

All of them are methods of `Ui`, take a value and a message constructor, and are listed with their signatures in the API snapshot.

| Kind | Methods |
| --- | --- |
| Layout | `row`, `column`, `grid`, `container`, `scroll`, `split`, `responsive`, `focus_scope`, `enabled` |
| Text and values | `label` (`Label::selectable` lets its text be selected and copied), `rich_text`, `text_input`, `number_input`, `autocomplete`, `checkbox`, `radio`, `slider`, `combo_box`, `progress`, `spinner`, `switch`, `collapsible`, `link`, `range_slider`, `calendar`, `date_picker` |
| Editors | `text_area`, `code_editor`, `rich_editor` (see [text](text.md)) |
| Collections | `virtual_list`, `tree_view`, `data_grid`, `property_grid` |
| Overlays | `dialog`, `popover`, `tooltip`, `menu`, `command_menu`, `context_menu`, `command_context_menu`, `command_palette`, toasts through `Effects::toast` |
| Panes | `tabs`, `tabs_close`, `split`, `dock`, `dock_window` (see [state](state.md)) |
| Windows | `title_bar`, `window_buttons`, `resize_frame`, `menu_bar` (see [platform](platform.md)) |
| Media | `image`, `image_file` |
| Input | `drag_source`, `drop_target`, `gesture_area`, `event_area` |
| Your own | `component`, `widget`, `custom`, `map`, `keyed`, `memo` |

`calendar` is a month of days to pick one from, and `date_picker` is a field that shows a date and opens the same calendar in a popup. Both take an `Option<Date>` and send the day picked, and their builders (`Calendar`, `DatePicker`) carry a `min` and a `max`, which disable the days outside them, the `first_weekday`, and `today`; `DatePicker` also has a `date_style` for the field and a `placeholder`. The framework has no clock, so the application tells the control what today is with `today(date)`: that day gets a dot in the accent colour, which stays on a selected day, a dimmed one and one that cannot be picked, and a control without `today` marks nothing. The month on show is the control's own: it starts at the selection, or at `today` when there is none, or else at `min`, and the buttons beside its name and the keys move it. The arrows move by day and week (Left and Right follow the reading direction), Page Up and Page Down by month, Home and End to the ends of the week, and Enter or Space picks. A date picker opens on a click, Enter, Space or Alt+Down, keeps the keyboard itself and forwards the keys to its popup, and Escape closes the popup without a pick. Month and weekday names are built-in messages (`glassmere-month-1` to `glassmere-month-12`, `glassmere-weekday-1` to `glassmere-weekday-7`, `glassmere-weekday-short-1` to `glassmere-weekday-short-7`, `glassmere-calendar-title` and the two month buttons' names), so a catalog translates them like the rest, and a day is named for assistive technology with `Ui::format_date`.

`Ui::tooltip(text)` gives the widget declared just before it a tooltip; `Ui::tooltip_with(Tooltip::new(text).placement(..).delay(..).style(..))` sets how it behaves. `TooltipPlacement` is `Auto` (below the widget, or above it with no room), `Above`, `Below`, or `Before` and `After`, the start and end sides, which mirror in a right-to-left window. A placed tooltip flips to the opposite side when it would leave the window and is always kept inside it. `delay` replaces `TOOLTIP_DELAY` for the widget, and `style` sets the bubble's `background`, `foreground`, `padding`, `radius`, `max_width` and font. A tooltip shows after the delay when the pointer rests on the widget, or when focus arrives by keyboard and stays; it hides on leaving, blurring, Escape, or a key that changes the widget's value, and only one is shown at a time. Moving to a neighbouring tooltip soon after one has shown skips most of the wait. The text is also the control's accessible description.

Collections take a model rather than data: `ListModel` answers by index and by `ItemId`, only the visible range is declared, and `ListState` and `GridState` hold selection and scroll. Rows of a list or tree are `VirtualListOptions::row_height` high unless the model returns a `RowHeights` from `ListModel::row_heights`, which gives some rows another height (`TreeModel::set_row_height` does it for a tree); scrolling, revealing, paging and the scrollbar then follow the real heights. A grid's columns can be resized, reordered, hidden and frozen through `GridColumns`, filtered per column, pinned under the body with total rows from `GridModel::footer`, and edited in place through typed cell editors whose values the model may refuse with a message.

A property grid with a `height` declares only the rows in view, however many properties the view names; groups, the filter and Tab to the next editor work over all of them.

`#[derive(Properties)]` turns a struct into property grid rows. Fields get an editor by type, and `#[property(..)]` sets `group`, `name`, `min`, `max`, `step`, `suffix`, `default`, `choices(..)`, `readonly`, `flatten` (`flatten, many` when the nested type has `properties_many`, so that its rows show where several objects differ), `with` and `skip`. A fieldless enum that derives `PropertyChoice` is edited as a drop-down of its variants (named from the variant, or `#[property(name = "..")]`), and `choices(..)` over a `usize` still works. An `Option<T>` field is edited as `T` beside a box that sets or clears it, through `PropertyGrid::optional`: setting gives the value `default` names, or a neutral one.

A grid can edit several objects at once: the view shows the first one's values and marks the rows where they differ with `PropertyGrid::mixed`, which shows "Mixed" instead of a value, and the application stores each message in every object. `Card::properties_many(&[&a, &b], grid, on)` does it for a derived struct.

## A function

Most "widgets" are a function that declares built-in ones. It needs no trait, no state of its own and no registration:

```rust
#[track_caller]
fn labelled_slider<M: 'static>(
    ui: &mut Ui<'_, M>,
    label: &str,
    value: f64,
    on_change: impl Fn(f64) -> M + 'static,
) {
    ui.row(Style::default().gap(8.0), |ui| {
        ui.label(label);
        ui.slider(label, value, 0.0..=1.0, 0.05, on_change);
    });
}
```

The row scopes everything inside it, so the same function used twice in the same place keeps two separate identities: the two rows are in different positions. Called in a loop over data that reorders, wrap the call in `ui.keyed(item.id, |ui| ..)` so each row keeps its own.

## A component

When it has state and messages of its own, make it a struct and let the application own it. `Component` is the shape:

```rust
struct Search { query: String, results: Vec<String> }

#[derive(Clone)]
enum SearchMsg { Query(String) }

impl Component for Search {
    type Message = SearchMsg;
    fn update(&mut self, message: SearchMsg, _: &mut Effects<'_, SearchMsg>) {
        let SearchMsg::Query(query) = message;
        self.results = look_up(&query);
        self.query = query;
    }
    fn view(&self, ui: &mut Ui<'_, SearchMsg>) {
        ui.text_input("Search", &self.query, SearchMsg::Query);
        for result in &self.results {
            ui.label(result.clone());
        }
    }
}

// In the parent's view, and in its update:
ui.component(Style::default(), &self.search, Msg::Search);
Msg::Search(message) => self.search.update(message, &mut effects.map(Msg::Search)),
```

The framework keeps nothing for it. The struct is the state, the application holds it, and `Ui::component` gives it a stable scope and turns its messages into the parent's.

A message is built from what the view declared, so two activations of a checkbox in one batch both send `Toggle(true)` when it shows unchecked, and `update` sets the state it is told rather than flipping it; a component that must flip on every activation sends a message that carries no value.

## A widget of your own

When it draws something the built-in controls cannot, it is a leaf: `Ui::widget` hands it what it needs and takes back what it paints, and `WidgetBuilder::on_event` turns what it hears into a message.

```rust
pub fn view<M: Clone + 'static>(
    &self,
    ui: &mut Ui<'_, M>,
    label: &str,
    size: f32,
    wrap: impl Fn(DialMsg) -> M + 'static,
) {
    let (face, ring) = (ui.theme().control, ui.theme().accent);
    let widget = WidgetBuilder::new(|input: &WidgetInput| CustomWidget {
        size: Size::new(size, size),
        semantics: Semantics { role: Role::Slider, label: label.into(), focusable: true, .. },
        primitives: self.paint(input, face, ring),
        input: EventArea { pointer: true, keyboard: true, capture: true, consume: true, .. },
    })
    // Pointer positions in the event are the widget's own.
    .on_event(move |event| Some(wrap(DialMsg::Heard(event))));
    ui.widget(Style::default(), widget);
}

pub fn update(&mut self, message: DialMsg) {
    let DialMsg::Heard(event) = message;
    self.apply(&event);
}
```

What `WidgetInput` carries is of the last frame, because that is what there is to know while this one is being declared: the room it was given, and whether it is hovered, pressed, focused and enabled. The events that reach the widget go to `on_event` when the batch is resolved, with pointer positions relative to its own top left corner, and what it returns goes to `update`. `UiEvent::Activate` comes only from the keyboard and assistive technology; a pointer click arrives as `PointerDown` and `PointerUp`.

- **Input** is what `CustomWidget::input` asks for. Nothing is delivered until it does. A widget that uses the arrow keys should capture and consume the keyboard, or the page it sits on will scroll as well. Pointer events are heard but never kept: focus, hover and press are the framework's services to the widget.
- **Focus** is `Semantics::focusable`, for any role. A focusable widget joins the tab order and receives keys.
- **Semantics** is how it is announced. A widget that holds a value sets `value` and `numeric`; without them it says nothing to a screen reader. `Role::Button` also opts into the activation path, so Enter and Space work.
- **Style** is `ui.theme()` for the tokens, `ui.class(..)` for a stylesheet class, and the `style` argument for the node itself, which lays out like any container.
- **Paint** is a list of primitives in the widget's own coordinates: rounded rectangles, gradients, shadows, text, images, clips and layers. Clips and layers must balance.

### Dashed lines

`Stroke` is a width and a colour, with the stroke's own mitred corners and square ends. For a dashed line or outline, cut the path first: `path.dashed(&[6.0, 4.0], phase)` returns the path as dashes (a dash of 6, a gap of 4; one entry is read twice over, as `stroke-dasharray` does) to stroke without a fill. Changing `phase` frame by frame makes the dashes march. Round caps and joins are not available.

### Testing it

The same harness the built-in controls use, with no window:

```rust
let mut harness = Harness::app(Size::new(320.0, 240.0), Page { dial: Dial::new(0.5) });
harness.run();
harness.focus(harness.find(&["dial"]));
harness.press(Key::ArrowUp);
assert_eq!(harness.state().dial.value(), 0.55);
assert!(harness.snapshot_accessibility().contains("value=\"55%\""));
```

`examples/dial-widget` is a whole crate that does this from outside the framework: a rotary dial with its own state, input, painting, semantics and tests, depending on `glassmere` and nothing private to it.

## What a widget cannot do

- Contain other widgets. A leaf is a leaf; something that holds children is a component that declares them.
- Measure itself against the room it is given, except by reading last frame's `WidgetInput::size`.
- Hear drag-and-drop, IME or touch directly. Those reach a widget through the built-in controls that own them.
