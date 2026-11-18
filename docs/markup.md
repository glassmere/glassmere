# Markup and the designer

A view can be written in a `.glass` file instead of Rust. The file is read while the application runs, so it changes without a rebuild, and `glassmere-designer` previews and edits it without an application around it. It needs the `markup` feature of `glassmere`.

```text
// counter.glass
data { count: 3, name: "Ada" }            // values to preview with

column { padding: 24; gap: 12
    label.title "Hello, {name}"
    label "Clicked {count} times"
    row { gap: 8
        button "Decrease" { on-click: decrease; enabled: count > 0 }
        button "Increase" { on-click: increase; background: #16695f }
    }
    input "Name" { bind: name; placeholder: "Who are you?" }
    if count >= 10 { label "That is a lot." } else { label "Keep going." }
    for item, index in items { label "{index + 1}. {item.title}" }
}
```

## The language

An element is its kind, an optional `.class` from the stylesheet, optional text, and optional braces holding `name: value` properties and child elements. Properties end at the end of the line or at `;`. A property the element does not know is a style property in the [stylesheet's](styling.md) syntax (`padding: 12`, `hover.background: #22b0a0`); a style value may hold `{expressions}`.

| Element | Text | Properties | Events |
|---|---|---|---|
| `column` `row` `box` `grid` `scroll` | | | |
| `spacer` | | | |
| `label` | shown | `selectable` | |
| `button` `link` | shown | | `on-click` |
| `checkbox` `switch` | label | `checked`, `bind` | `on-toggle` |
| `radio` | label | `selected`, `option`, `bind` | `on-select` |
| `slider` | name | `value`, `min`, `max`, `step`, `bind` | `on-change` |
| `input` | name | `value`, `placeholder`, `password`, `bind` | `on-change`, `on-submit` |
| `number` | name | `value`, `min`, `max`, `step`, `decimals`, `suffix`, `bind` | `on-change` |
| `progress` | name | `value` (0 to 1; none for indeterminate) | |
| `spinner` | name | | |
| `combo` | name | `items`, `selected`, `bind` | `on-choose` |
| `image` | description | `src`, `fit` (`contain`, `cover`, `fill`, `none`) | |
| `tabs` holding `tab "Title" { }` | | `selected`, `bind` | `on-select` |
| `collapsible` | heading | `open`, `bind` | `on-toggle` |

Every element also takes `enabled`, `tooltip` and `key`, a name that follows the element when a loop reorders its items.

Values are expressions over the application's `Values`: names and `a.b` paths, numbers, `"text with {names}"`, `true`, `false`, `none`, `[lists]`, `{records: 1}`, the operators `! - * / % + - == != < <= > >= && ||`, `condition ? this : that` and `len(list)`.

- `if condition { } else if other { } else { }` and `for item, index in list { }` choose and repeat elements.
- `component Card(title, body) { ... }` at the top level defines an element, used as `Card { title: "Hello"; body: text }` or, for the first parameter, `Card "Hello"`.
- `data { ... }` holds the values the designer previews with. `Markup::defaults` returns them, so an application can start from the same ones.

A mistake is reported with its line and column, and names what was expected: ``line 2, column 5: unknown element `buton` ``.

## In an application

```rust
use glassmere::markup::{LiveMarkup, MarkupEvent, Value, Values};

struct Counter { view: LiveMarkup, values: Values }

impl App for Counter {
    type Message = MarkupEvent;

    fn update(&mut self, event: MarkupEvent, _: &mut Effects<'_, MarkupEvent>) {
        // Widgets with `bind` store their own value.
        if self.values.apply(&event) {
            return;
        }
        let count = self.values.get("count").and_then(Value::number).unwrap_or(0.0);
        match event.name.as_str() {
            "increase" => { self.values.set("count", count + 1.0); }
            "decrease" => { self.values.set("count", count - 1.0); }
            _ => {}
        }
    }

    fn view(&self, ui: &mut Ui<'_, MarkupEvent>) {
        self.view.show(ui, &self.values, |event| event);
    }
}
```

`on-click: name` sends a `MarkupEvent` called `name`, and `on-click: remove(item.id)` one with an argument. `bind: path` shows the value at `path` and sends an event that `Values::apply` stores back there.

`LiveMarkup::new(path)` reads the file again whenever it changes and keeps showing the last version that parsed, with the error above it. `Markup::parse(include_str!("counter.glass"))` is the same view compiled into the binary, for a release. A markup view is one part of `App::view` like any other: it can sit beside widgets declared in Rust, and several files can be shown in one window.

Widgets keep their identity across edits of the file as long as the elements before them stay; state such as focus, scroll position and a field's caret survives a reload.

## The designer

```sh
cargo install glassmere-designer
glassmere-designer view.glass [--style sheet.style]
```

The window shows the view with the values of its `data` block, and follows every change saved to the file from any editor. Without `--style`, a stylesheet with the file's name beside it (`view.style`) is used, and it reloads as well.

- **Outline** on the left lists the elements. Selecting one there, or with **Pick element** and a click in the preview, outlines it in the preview.
- **Element** on the right edits the selected element's text, class and properties, adds and removes properties, moves the element among its siblings, deletes it, and adds a new element inside or after it. Every change is written to the file at once; one that would not parse stays in its field with the error under it and the file is left alone.
- **Data** edits the values the view reads, to try it in other states, and **Events** lists what its widgets send.
- **Preview size** shows the view at a fixed size, such as a small display's.

The preview is the real widgets: buttons click, fields take text, and `bind` works.

### In VS Code

[`editors/vscode`](../editors/vscode) is an extension with syntax highlighting for `.glass` files and a **Glassmere: Show Preview** command that opens the designer on the file being edited. The preview follows saves; with `"files.autoSave": "afterDelay"` it follows typing.

## Limits

- The interpreter covers the elements in the table. Lists, trees, data grids, docking, editors and custom widgets are declared in Rust, beside the markup view.
- Expressions read values and do not change them: logic lives in `App::update`.
- The designer edits one file. A component is edited where it is defined, not through its uses.
