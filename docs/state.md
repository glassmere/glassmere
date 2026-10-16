# Where state lives

Ordinary Rust state is the model. The application owns its data in a plain struct, `update` changes it in answer to a message, and `view` declares what it looks like. The framework keeps only what it must to make a window behave: focus, scroll offsets, editing history, animation clocks, caches.

```rust
#[derive(Clone)]
enum Msg { Add }

struct Counter { count: u32 }

impl App for Counter {
    type Message = Msg;
    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        match message { Msg::Add => self.count += 1 }
    }
    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.button(Button::new("Add one").on_click(Msg::Add));
        ui.label(format!("{}", self.count));
    }
}

run(options, Counter { count: 0 })
```

There is no store to register with and no context provider. `view` reads `&self`, so only `update` changes the state; a widget says what it sends, and the framework delivers it. Small programs that would rather not name messages can use `run_with`, whose message is an `Action<S>`: a closure applied to the state.

## What the framework keeps

Each node keeps what an application should not have to: whether it has focus, how far it is scrolled, what is selected in it, the undo history of a text field, where an animation is. That state belongs to the node's identity, which is normally where the node is declared among its siblings, so it survives every declaration of the same shape.

Declare children conditionally or in an order that moves and position stops meaning the same thing from one frame to the next, so the state lands on the wrong row. `ui.keyed(item.id, |ui| ..)` names one from the data instead:

```rust
for row in &self.rows {
    let id = row.id;
    ui.keyed(row.id.to_string(), |ui| {
        ui.text_input("Name", &row.name, move |name| Msg::Rename(id, name))
    });
}
```

A key that changes every frame throws all of it away. The framework says so as an `UnstableIds` diagnostic, and in debug builds prints it once on stderr as well.

State an application does want to hold and pass in has an explicit type: `ListState`, `GridState`, `PropertyGridState`, `FindState`, `TextDocument`, `RichDocument`, `DockLayout`. Each has an `apply` for the messages its widget sends, and those are the ones worth saving to disk.

## Getting answers back

Input taken between frames is turned into messages through the handlers the last declaration left on each node, and the whole batch is delivered to `update` before the view runs again. The view therefore reads state that is already up to date, and runs at most once per batch.

What `update` wants done that is not a change to its own state goes through `Effects`: the clipboard (`copy`, `paste`), focus requests, a file dialog (`choose_files`), toasts, the theme, notifications, windows. Anything that answers later, such as a paste or a chosen file, answers with a message of the application's choosing.

## Work on other threads

```rust
fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
    match message {
        Msg::Search(query) => {
            effects.spawn("search", move |out| {
                for record in find(&query) {
                    if out.cancelled() { return; }
                    let _ = out.send_blocking(record);
                }
            }, Msg::Found);
        }
        Msg::Found(record) => self.results.push(record),
    }
}
```

A job can be cancelled, a newer job supersedes the one before it under the same key, and a superseded job's results are dropped before `update` sees them, which is what makes search-as-you-type correct. No executor is started: a job is a thread, or a future on whatever runtime the application already has. `Effects::channel` is the plainer version when messages from elsewhere are all that is needed; it is typically opened in `App::init`.

## When ordinary state is not enough

`Store` is optional and small: signals hold values, memos derive them and record what they read, a change marks what depends on it, and a memo recomputes when it is next read.

```rust
let store = Store::new();
let quantity = store.signal("quantity", 3.0);
let price = store.signal("price", 12.5);
let (count, each) = (quantity.clone(), price.clone());
let subtotal = store.memo("subtotal", move || count.get() * each.get());
```

It earns its place when a value is derived from several others, expensive to produce and read in several places. Written by hand that is a cache and a set of invalidation rules, and the rules are what rot. `Ui::observe(&store)` shows the graph in the inspector's State tab: what each derived value read, and which one moved last.

It is single threaded, like everything else a view touches. Work on another thread reaches a signal through a message.

## Declaring less

`Ui::memo` skips a subtree whose state has not changed:

```rust
ui.memo(&(&self.rows, self.selected), style, |ui| { .. });
```

The retained subtree stays exactly as it was, so it still paints, scrolls and animates, and the handlers it left keep sending. The state must cover everything the content reads. `Store::version` and `Signal::version` are hashable, so a store plugs straight in.

## State that outlives the process

`App::save_state` hands the development runner a string to keep across a rebuild, and `rust_gui::hot::saved_state` reads it back on the next start. It is for the edit-and-see loop, not for saving documents: an application saves its own files.
