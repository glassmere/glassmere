# Where state lives

Ordinary Rust state is the model. The application owns its data in plain structs, the view is a function of it, and the framework keeps only what it must to make a window behave: focus, scroll offsets, editing history, animation clocks, caches.

```rust
let mut count = 0;
run(options, move |ui| {
    if ui.button("Add one").clicked() {
        count += 1;
    }
    ui.label_keyed("count", format!("{count}"));
})
```

There is no store to register with, no reducer and no context provider. A closure that owns its state is the whole pattern, and `Arc<Mutex<_>>` is how two windows share one.

## What the framework keeps

Each node keeps what an application should not have to: whether it has focus, how far it is scrolled, what is selected in it, the undo history of a text field, where an animation is. That state belongs to the node's identity, which is its key, so it survives every declaration that keeps the key and dies with one that changes it. A key that changes every frame throws all of it away, and the framework says so as an `UnstableIds` diagnostic.

State an application does want to hold and pass in has an explicit type: `ListState`, `GridState`, `PropertyGridState`, `FindState`, `TextDocument`, `RichDocument`, `DockLayout`. Those are the ones worth saving to disk.

## Getting answers back

A view runs during a declaration and holds only a shared reference to the framework, so anything it wants done arrives afterwards through an outbox: the clipboard, focus requests, watched files, images, toasts, the theme, the menu, a file dialog. That is why `Ui::request_paste` and `Ui::chosen_files` are separate calls, and why a control's result is a `Response` read on the frame after the click rather than a callback during it.

Input taken between frames is queued against the node it landed on and applied at the next declaration. That is what makes a view a pure function of the application's state: nothing is mutated behind it.

## Work on other threads

```rust
let mut tasks = wake.tasks::<&str, Found>(256);
tasks.spawn("search", move |out| {
    for record in records {
        if out.cancelled() { return; }
        let _ = out.send_blocking(Found::Hit(record));
    }
});
for (key, message) in tasks.drain(128) { .. }
```

`Tasks` is the thread boundary. A job can be cancelled, a newer job supersedes the one before it under the same key, and a superseded job's results are dropped before the view sees them, which is what makes search-as-you-type correct. No executor is started: a job is a thread, or a future on whatever runtime the application already has. `WakeHandle::channel` is the plainer version when messages are all that is needed.

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

It is single threaded, like everything else a view touches. Work on another thread reaches a signal through `Tasks`.

## Declaring less

`Ui::memo` skips a subtree whose state has not changed:

```rust
ui.memo("list", &(&rows, selected), style, |ui| { .. });
```

The retained subtree stays exactly as it was, so it still paints, scrolls and animates. The state must cover everything the content reads. A frame that carries input for something inside is never skipped, so a click in a memoized list is not lost. `Store::version` and `Signal::version` are hashable, so a store plugs straight in.

## State that outlives the process

`Ui::hot_state` hands the development runner a string to keep across a rebuild, and `rust_gui::hot::saved_state` reads it back on the next start. It is for the edit-and-see loop, not for saving documents: an application saves its own files.
