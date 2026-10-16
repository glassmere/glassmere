# Building a widget

There are three ways to add something to the framework, and the first one that fits is the right one.

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

`examples/dial-widget` is a whole crate that does this from outside the framework: a rotary dial with its own state, input, painting, semantics and tests, depending on `rust-gui` and nothing private to it.

## What a widget cannot do

- Contain other widgets. A leaf is a leaf; something that holds children is a component that declares them.
- Measure itself against the room it is given, except by reading last frame's `WidgetInput::size`.
- Hear drag-and-drop, IME or touch directly. Those reach a widget through the built-in controls that own them.
