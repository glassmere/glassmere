# Milestone 35: messages, an update, and one view per frame

Planned on 2026-10-15. Release blocker 2: the view may run more than once per input event.

## The problem

An action runs inside the view today. `Response::clicked()` is true during the declaration that consumes the click, so whatever the click changes is visible only to what is declared after the button; a label declared above it is stale. The hosts cover that by declaring again while `FrameStats::activations > 0`, up to three more times (`seaglass-winit/src/lib.rs`, the settle loop after `Context::run`; `seaglass-testing/src/lib.rs`, `Harness::run`). A theme request adds an activation in `Context::run` (`tree/run.rs:91-97`), format and messages in `apply_outbox` (`run.rs:184-193`), and `Context::set_binding` (`tree.rs:950-959`) another, for the same reason.

So a click costs up to four declarations, and a view is never sure it is looking at the state it will be presented with.

## The goal, and the performance contract that comes with it

The target is applications that render well above 360 frames per second, where a frame has 2.78 ms in all. Nothing in the hosts caps the rate: `PresentMode::Fifo` follows the monitor, and the event loop waits rather than ticks. What costs frames is declaring more than needed. The contract this milestone establishes:

1. **The view runs at most once per host batch.** A batch is every event the host collected since the last frame, however many: a 1000 Hz mouse produces one batch per frame, not one declaration per event.
2. **The view runs only when something it reads may have changed.** Exactly these, and nothing else:
   - `update` ran at least once in the batch;
   - an `Effects` request that changes what the view reads was applied (theme, messages, format, bindings);
   - host-set state the view reads changed: window size or scale factor, window info (maximized, focused), a watched file, the stylesheet, opened files (see the host results table);
   - the view called `Ui::request_redraw` during its last declaration;
   - a hover, focus or pressed state the view observed changed (below);
   - the first frame, and a hot reload.

   Any other frame (unobserved hover, focus, scroll, caret, selection, kernel-owned transitions, image decodes) is layout and paint over the retained tree, with no declaration.
3. **A batch with no events and nothing dirty allocates nothing and declares nothing.** A declaring frame does not allocate per widget for its messages outside `Ui::map` (see storage and composition below).
4. The settle loops and `FrameStats::activations` are removed. `FrameStats` gains `declared: bool` and `messages: usize`.

### Gates

- A new test file `crates/seaglass-core/tests/allocations.rs` with a counting `#[global_allocator]` (its own test binary, so other tests are unaffected): after a warm-up frame, a batch with no events and nothing dirty performs zero allocations and `stats.declared == false`.
- In the same file: a batch that only moves the pointer over an unobserved button has `declared == false`; one with one click has `declared == true` and `messages == 1`.
- `examples/bench.rs` keeps its timing output and adds both cases to it; timing is reported, not gated.

## The shape

```rust
use seaglass::{App, AppOptions, Button, Effects, Ui, run_app};

#[derive(Clone)]
enum Msg { Increment, Name(String) }

struct Counter { count: i32, name: String }

impl App for Counter {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Increment => self.count += 1,
            Msg::Name(name) => self.name = name,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.label(format!("{} clicked {}", self.name, self.count));
        ui.button(Button::new("+1").on_click(Msg::Increment));
        ui.text_input("Name", &self.name, Msg::Name);
    }
}

fn main() -> seaglass::Result<()> {
    run_app(AppOptions::default(), Counter { count: 0, name: String::new() })
}
```

`App` lives in `seaglass-core` (`src/app.rs`), with `Effects` and `Handler`, because `Harness` needs them:

```rust
pub trait App: 'static {
    type Message: Clone + 'static;
    /// Once, before the first view: open channels, spawn startup work.
    fn init(&mut self, _effects: &mut Effects<'_, Self::Message>) {}
    fn update(&mut self, message: Self::Message, effects: &mut Effects<'_, Self::Message>);
    fn view(&self, ui: &mut Ui<'_, Self::Message>);
    /// What `seaglass-dev` keeps across a rebuild; replaces `Ui::hot_state`.
    fn save_state(&self) -> Option<String> { None }
    /// A window opened with `Effects::open_window`; the main window uses `view`.
    fn view_window(&self, _window: WindowId, ui: &mut Ui<'_, Self::Message>) { self.view(ui) }
}
```

**Signature convention.** Where this doc writes `fn(T) -> M` for a message constructor, the real signature is `impl Fn(T) -> M + 'static`, stored boxed, so that `Ui::map` and `Effects::map` can compose it and capturing closures work. The "After" columns in the tables below show the names as they are after phase 3; during phases 1 and 2 the colliding ones carry the `_msg` suffix (see work order).

The entry point is `run_app` until phase 3, which removes the closure `run` and renames `run_app` to `run`.

### What a frame does

1. The host collects a batch of events.
2. Each event is resolved against the tree **as it was declared last time** and turned into zero or more messages, in arrival order, into one `Vec<M>`. A second event in the same batch resolves against the same tree, not against what the first event's message will produce. AccessKit actions (`tree/actions.rs`), shortcuts and `Event::Command` resolve to a node exactly as today (`tree/events.rs:272-281`, `activate`) and then through that node's handler.
3. `update` runs on every message in order. Kernel-owned state (focus, scroll, caret, selection, undo, hover) is updated by the events themselves, as today.
4. Effects queued by `update` are applied (below).
5. If the view rule says so, `view` runs once, then reconcile, layout, paint. Otherwise layout and paint only, and only where dirty.

### 1. Message typing: `Ui<'_, M>` is generic, the kernel is not

`Ui<'a, M: 'static = ()>`. The default `()` lets display-only helpers stay `fn header(ui: &mut Ui<'_>)`; helpers that declare controls take `<M>`. `M: Clone + 'static`, because one handler can fire more than once in a batch.

`Context`, the nodes, the tree modules, devtools, AccessKit and the hosts stay non-generic. Making `Context` generic would push `M` through the whole kernel and monomorphize it per application. Type erasure happens once per batch, not per node: the context owns one `Box<dyn Any>` holding the typed handler storage, downcast at the start of a batch by the `App` driver, where a mismatch cannot occur by construction (one `App`, one `M`). A mismatch is a framework bug and panics.

Reporting a mismatch as a diagnostic instead was rejected: unlike the misuses `api-stability.md` makes panic in debug, a wrong message type would be wrong only when clicked.

### Handler storage, without an allocation per widget

```rust
pub(crate) enum Handler<M> {
    Message(M),                                   // on_click(Msg::Save)
    Map(Box<dyn Fn(ControlUpdate) -> Option<M>>), // text_input(.., Msg::Name), slider(.., Msg::Volume)
    Event(Box<dyn Fn(UiEvent) -> Option<M>>),     // widget/event_area .on_event(..)
}
```

- A node that sends something gets a handler slot. Slots live in a typed slab owned by the context, inside the one `Box<dyn Any>`, indexed by a `u32` on the node (`Node::handler: Option<u32>`), reused when a node is removed. Steady state allocates nothing.
- Handlers never live in `Element`, `Control` or `CustomWidget`, which are `Clone + PartialEq` and compared in `update_node` (`run.rs:504`). The element carries only the index into this frame's declared handlers.
- `Msg::Name` is a function item, a zero-sized type, so `Box<dyn Fn>` around it does not allocate. Each widget's `Map` wraps the user's `fn(T) -> M` in a closure that matches its own `ControlUpdate` variant; when the wrapped function is zero-sized the wrapper is too. Only a capturing closure (`move |s| Msg::Rename(id, s)`) allocates, once per declaration of that node.
- Declaration pushes handlers into a per-frame `Vec<Handler<M>>` whose capacity is kept; reconcile moves each into its node's slot.
- A subtree skipped by `Ui::memo` keeps its slots. Its handlers are still right, because the memo hash covers the state they were built from. `awaiting`, `mark_awaiting` and `awaits_declaration` (`run.rs:34-41,136-144`, `tree.rs:976`) become dead: memo is decided by the hash alone. They are removed in phase 3.
- A message from a removed node cannot be produced: events resolve against the last declared tree, and a removed node's slot is freed in the same reconcile.

`ControlUpdate` stays `pub(crate)` in `controls.rs:395`. Each widget's public signature names the concrete `fn(T) -> M` it takes, never `ControlUpdate`. A checkbox is a click today (`ui/controls.rs:140`); its handler is a `Message` built at declaration from the value it shows: `on_toggle(!checked)`.

### Composition: `Ui::map` and `Component`

```rust
pub fn map<N: Clone + 'static>(
    &mut self,
    f: impl Fn(N) -> M + 'static,
    content: impl FnOnce(&mut Ui<'_, N>),
)
```

`map` declares no node. The child `Ui<'_, N>` pushes `Handler<N>`s into a per-frame list of its own; at the end of `map` each is converted to `Handler<M>` with an `Rc` of `f` shared by the subtree. A `Message(N)` is mapped eagerly and allocates nothing; a `Map` or `Event` handler is re-wrapped in a new `Box`, one allocation per such handler per declaration inside a `map`. That is the price of components; memo-skipped subtrees pay nothing.

The existing `Component` (`widget.rs:6-8`, `fn ui(&mut self, ..)`) and `Ui::component` (`ui.rs:1210`) are replaced in phase 1:

```rust
pub trait Component {
    type Message: Clone + 'static;
    fn update(&mut self, message: Self::Message, effects: &mut Effects<'_, Self::Message>);
    fn view(&self, ui: &mut Ui<'_, Self::Message>);
}
```

`Ui::component(style, &impl Component, f)` declares the container and calls `map(f, |ui| c.view(ui))`. The parent forwards in its own `update`; `Effects::map(f)` gives the component an `Effects<'_, N>` that maps its requests back. Its two users, `tests/architecture.rs:247` and `examples/workbench.rs:18`, migrate in phase 1.

### 2. Bound inputs: a value and a message, so `view(&self)` is pure

Every `&mut` binding in a declaring method becomes a value plus a message constructor. A `&mut` can only be written while the view runs, so keeping it as shorthand would keep mutation in the view, stale earlier labels and the settle loop.

| Before | After |
| --- | --- |
| `ui.button("Save").clicked()` | `ui.button(Button::new("Save").on_click(Msg::Save))` |
| `ui.text_input("Name", &mut name)` | `ui.text_input("Name", &self.name, Msg::Name)` |
| `.submitted()` on a text input | `TextInput::new("Name").on_submit(Msg::Submit)` |
| `ui.checkbox("Notify", &mut on)` | `ui.checkbox("Notify", self.on, Msg::Notify)` |
| `ui.slider("Volume", &mut v, 0.0..=100.0, 5.0)` | `ui.slider("Volume", self.v, 0.0..=100.0, 5.0, Msg::Volume)` |
| `ui.combo_box("Theme", &mut i, &themes)` | `ui.combo_box("Theme", self.i, &themes, Msg::Theme)` |
| `ui.tabs(&mut tab, &tabs, content)` | `ui.tabs(self.tab, &tabs, Msg::Tab, content)` |
| `ui.split(&mut ratio, ..)` | `ui.split(self.ratio, Msg::Split, ..)` |
| `ui.popover(&mut open, ..)` / dialogs | `ui.popover(self.open, Msg::Popover, ..)`, the message carries the new `bool` |
| `ui.menu(label, &items) -> Option<usize>` | `ui.menu(label, &items, Msg::Picked)` with `fn(usize) -> M` |
| `ui.context_menu(..)`, `command_palette(&mut open, &mut query, ..)` | value plus `fn(usize) -> M`, `fn(bool) -> M`, `fn(String) -> M` |
| `ui.command_button(&cmd) -> Response` | `ui.command_button(&cmd, Msg::Save)` |
| `ui.rich_text(..) -> Option<String>` | `ui.rich_text(.., Msg::Link)` with `fn(String) -> M` |

`number_input_msg(label, value: f64, options, on: impl Fn(f64) -> M + 'static)` commits in the kernel, at resolve time in `tree/handlers.rs`, because the parse needs the context's locale. On Enter, leaving the field, a stepper click, Up/Down, PageUp/PageDown or the wheel, the kernel computes `constrain(parse_in(draft).unwrap_or(value) + steps * step)` from the node's stored `NumericValue` and the context's format, and sends a message only when that differs from the declared value. A draft that does not parse reverts without a message. The draft text stays kernel field state, as today.

The rule for anything not listed: a return value that reports user input becomes a constructor parameter of the same type; a `&mut T` becomes `T` (or `&T`) plus `fn(T) -> M`; a declaration that returns nothing user-driven keeps its signature.

`ui.button("Save")` without `on_click` stays valid and sends nothing. `Button::on_click` is a builder method, which fits milestone 34's one method and one builder per widget and keeps the `#[track_caller]` site on `button()`.

A command (`command.rs`), native menu pick or shortcut resolves to the node that declared that command and fires its handler; a command with no declaring node does nothing, as today.

The kernel already works by value inside: text edits are `ControlUpdate::Text`, slider drags `ControlUpdate::Number`, and `Editor::synchronize` (`editor.rs:138`) clears undo and IME state only when the declared value differs from the editor's. Because `update` runs before the one view, the declared value matches after an edit, so per-keystroke messages keep undo, caret and IME composition. `synchronize` compares `&str` first and only builds its copy when they differ; the per-declaration `String` in `Element.control` stays for now.

**Composites with their own state** get a public message type and an `apply`. These are internal types today, made public:

| Widget | State | Message |
| --- | --- | --- |
| `dock` | `DockLayout` | `DockAction` (`controls.rs:410`) made public; `DockLayout::apply(&mut self, DockAction)` |
| `data_grid`, lists, tree views | the model and `GridState` the app owns | `GridMsg { Sort(..), ResizeColumn(..), MoveColumn(..), Filter { column, text }, FiltersApplied, Select(..), Edit { row, column, value }, Expand { row, open } }` |
| `text_area`, `code_editor` | `TextDocument` | `AreaOp` made public as `TextOp`; `TextDocument::apply(&mut self, TextOp)` |
| `rich_editor` | `RichDocument` | `RichOp` made public; `RichDocument::apply` |
| `property_grid` | `PropertyGridState` | `PropertyMsg`; `#[derive(Properties)]` generates `XMsg` and `X::apply` |

Collections take their model by shared reference (`data_grid(model: &impl GridModel, ..)`), because `ui/collections.rs:675,1073` call `model.sort` and `model.set_cell` while declaring and tree views call `set_expanded` (`collection.rs:265-288`). Those trait methods stay; the docs say the application calls them from `update` on `GridMsg`: `sort` on `Sort`, `filter` (`collection.rs:558`) on `FiltersApplied`, `set_cell` on `Edit`, `set_expanded` on `Expand`.

`data_grid` also takes `GridState` (`collection.rs:574`) by shared reference; the view mutates it today (`ui/collections.rs:677,679`). `GridState::apply(&mut self, &GridMsg)` handles `Sort` (the sort field), `ResizeColumn` and `MoveColumn` (columns), `Filter` (the typed `filters`), `FiltersApplied` (copies `filters` to `applied_filters`) and `Select`. `scroll_x`, cursor clamping and the 200 ms filter delay move to kernel-owned node state; when the delay elapses the kernel sends `FiltersApplied` through the grid's handler.

Documents may be large: `TextOp`/`RichOp` carry the edit, never a copy of the document.

### 3. The old API is removed before 0.1

`Response::clicked`, `Response::changed`, `Response::submitted`, every `&mut` binding, `Ui::emit`, `Tasks::drain` inside a view and `run(options, |ui| ..)` with a mutating closure are removed. `api-stability.md` starts deprecation at 0.2, and milestone 34 already removed outright for the same unpublished-crate reason. `Response` fields become private; `hovered()` and `focused()` stay as methods (observed state below).

For small programs and for migrating tests, closures are one message type, not a second model:

```rust
pub type Action<S> = std::rc::Rc<dyn Fn(&mut S)>;
pub fn run_with<S: 'static>(
    options: AppOptions,
    state: S,
    view: impl Fn(&S, &mut Ui<'_, Action<S>>) + 'static,
) -> Result<()>;
```

Its `update` calls the action on the state. It allocates one `Rc` per handler per declaration, which the docs say; examples and docs use enum messages.

## Effects: what `update` asks of the framework

`Effects<'a, M>` borrows the `App` driver (its task tables, channels and queued requests; `running(key)` reads the tables). It is passed to `init` and `update` and applied after the batch's updates, before the view. It holds:

- `set_theme`, `set_messages`, `set_format`, `set_binding` (from `Ui::set_theme`, `Ui::set_messages`, `Ui::set_format`, `Context::set_binding`);
- `copy(Clip)`, `paste(fn(Clip) -> M)`;
- `choose_files(request, fn(Vec<PathBuf>) -> M)`;
- `request_focus(key)`, `notify(Notification)`, `window_command(WindowCommand)`, `toast(..)`;
- `spawn` and `channel` (below);
- `map<N>(f) -> Effects<'_, N>` for components.

What describes the current state stays declarative in the view and is diffed as today: menu, tray, watched paths, requested images.

## Host results the view reads today

| Today | After |
| --- | --- |
| `Ui::pasted` (`ui.rs:256`), host `set_clip` | `Effects::paste(fn(Clip) -> M)`; the result arrives as a message |
| `Ui::chosen_files` (`ui.rs:271`), host `set_chosen_files` | `Effects::choose_files(req, fn(Vec<PathBuf>) -> M)` |
| `Ui::opened_files` (`ui.rs:543`), host `set_opened_files` | stays a view read; a change triggers the view rule |
| `Ui::watched_bytes`/`watched_text` (`ui.rs:570-584`), `set_watched_file` | stays a view read; a change triggers the view rule |
| stylesheet reload (winit `lib.rs:845-860`) | host-set; triggers the view rule |
| `Ui::window()`, `window_size()` (`ui.rs:302-311`), `responsive` (`ui.rs:995`) | stay view reads; a size, scale or window-info change triggers the view rule |
| `Ui::hot_state` (`ui.rs:591`) | `App::save_state` |
| `Ui::toast_action` (`ui/toasts.rs:43`) | the toast carries `fn(ToastAction) -> M` |
| `drag_source -> Option<DragOutcome>` (`ui.rs:622`) | `on_drag_end(fn(DragOutcome) -> M)` |
| `drop_target -> Option<DragPayload>` (`ui.rs:670`) | `on_drop(fn(DragPayload) -> M)` |
| `gesture_area -> GestureResponse` (`ui.rs:651`) | `on_gesture(fn(GestureResponse) -> M)`; `active` is observed state |
| `menu_bar`, `command_button`, `command_palette`, `context_menu`, `menu` picks | through the declaring node's handler (table above) |
| `rich_text -> Option<String>` (`ui/rich_text.rs:9`) | `fn(String) -> M` |
| `set_clip`/`set_chosen_files`/`set_opened_files` setting `redraw_requested` (`tree.rs:768-837`) | removed; the first two are messages, the third the view rule |

## Workers, tasks and the mailbox

The `App` driver owns the task tables. `Effects::spawn<T: Send + 'static>(key: impl Into<String>, job: impl FnOnce(TaskSender<String, T>) + Send + 'static, map: fn(T) -> M) -> Cancel` keeps one `Tasks<String, T>` per `TypeId` of `T` (boxed behind a small object-safe trait `drain(&mut self, limit, out: &mut Vec<M>)`), each with a `key -> fn(T) -> M` map. `M` does not have to be `Send`; workers send `T`. A delivery wakes the host, and the drained messages start a batch of their own. `Effects::cancel(key)` and `Effects::running(key)` replace the view's `Tasks::running`.

`Effects::channel<T: Send + 'static>(capacity, map: fn(T) -> M) -> MessageSender<T>` does the same for the mailbox, typically from `init`. `WakeHandle::tasks` and `WakeHandle::channel` (`winit/src/wakeup.rs:72-82`) are removed in phase 3. Image decode completions stay kernel-internal and only repaint.

## Observed state: hover, focus, pressed

`Response` becomes `Response<'a>` carrying `&'a Context` and the `NodeId` (or the declared index, resolved at reconcile), with `hovered()` and `focused()` methods that set `Node::observed: Cell<bool>` (`Ui` holds `&Context`, `ui.rs:191`). The public fields of the same names stay beside the methods until phase 3 makes them private. The bit is cleared per node in `update_node` when that node is declared again, never tree-wide, so a memo-skipped subtree keeps its observations. A change in an observed node's hover, focus or pressed state triggers the view rule; a change in an unobserved one only repaints. `WidgetInput` (custom widgets) marks its node observed whenever it is handed out.

## Custom widgets and event areas

`Ui::widget`, `event_area` and `gesture_area` deliver events through the next declaration today (`WidgetInput::events`). They take a mapper instead, `.on_event(|event: UiEvent| -> Option<M>)`, stored as `Handler::Event` and called when the event is resolved, with the event already moved into the widget's coordinates (`WidgetInput::local`, `widget.rs:45`, moves to resolve time). `WidgetInput` keeps size, enabled and the observed states and loses `events`. `Ui::emit` is removed: a component sends its own `M`. Milestone 23's contract changes accordingly.

`examples/dial-widget`, which mutates `self` in its build closure, is the acceptance test: it becomes a third-party widget with its own message and no mutation in the view.

## Multiple windows

`run_windows` and `DesktopHandle::open_window` (`desktop.rs:44-60`, a view closure per window) with `Arc<Mutex<State>>` locked inside each view (`examples/dialogs.rs`) become one `App` with

```rust
fn view_window(&self, window: WindowId, ui: &mut Ui<'_, Self::Message>) {}
```

and `Effects::open_window(options) -> WindowId`, `close_window(id)`. All windows share one `update` and one message queue, and a batch declares only the windows whose view rule fires. The inspector window (`desktop.rs:710`) keeps a closure view of its own, internal to the crate.

## The test harness

`Harness::app(size, app: A) -> Harness<A>` (with `A: App`) beside the existing `Harness::new` until phase 3.

- `harness.send(msg)` runs `update` with it, applies effects, and runs the view rule once.
- `click`, `press`, `type_text` and the other input helpers dispatch one batch: resolve, update, at most one view.
- `harness.app()`, `harness.app_mut()` and the last `FrameStats` are exposed; `state()`/`state_mut()` stay on the closure form until it is removed.

## Purity is a convention

`view(&self)` is pure by signature only. `Signal::set(&self)` (`reactive.rs`) and any `Cell` the application owns can still mutate during the view. The docs say so, and a thread-local "declaring" flag set around the view in `Context` makes a `Signal` write during a declaration a debug-build `Misuse` diagnostic.

## Work order

The milestone lands in phases. Within a phase, packages own disjoint files; a file not listed belongs to the phase 1 implementer.

**Phase 1, the kernel (one implementer, serial).**

1. Split `ui.rs` first, behaviour unchanged: window and title-bar methods to `ui/window.rs`, event areas, gestures, drag and drop and `widget` to `ui/areas.rs`, `dialog` to `ui/dialog.rs`, the `*_at` helpers (`ui.rs:1309-1360`) to the existing `ui/fields.rs`. Verify the workspace builds and tests pass before going on.
2. `Ui<'_, M>` with `impl<M> Ui<'_, M>` over every existing method, so the old signatures compile unchanged.
3. `app.rs`: `App`, `Effects`, `Handler`, the slab and its one `Box<dyn Any>`, `Ui::map`, the new `Component`, the observed bits, the view rule. `Context` stays non-generic: it resolves a batch to node activations and control updates; the generic driver in `app.rs` turns those into `Vec<M>` through the slab, runs `update`, then calls `Context`'s declaration.
4. `run_app` in the winit host, `run_with`/`Action<S>`, `Harness::app`, and `App::save_state` wired into `seaglass-dev`'s hot reload; all run update then at most one view. Migrate `tests/architecture.rs:247` and `examples/workbench.rs` (via `run_app`) to the new `Component`. The old `clicks`/`updates`/`submitted` path and the closure `run` keep working until phase 3.
5. `button`, `text_input`, `checkbox` and `slider` gain their new form as the reference for everyone else. Until phase 3 the old form keeps its name and the new one is added beside it with a `_msg` suffix (`button_msg`, `text_input_msg`, `checkbox_msg`, `slider_msg`); phase 3 renames them over the old ones. Builders that carry a handler become generic with a default, `Button<M = ()>`, `TextInput<M = ()>`, so `Button::on_click` needs no suffix and `ui.button` (which takes `impl Into<Button<M>>`) needs no `button_msg`. Every phase 2 package follows the same two rules: builder methods for handlers, `_msg` only where a positional signature collides.
6. Pre-declare in `lib.rs` every public name phase 2 adds (`GridMsg`, `TextOp`, `RichOp`, `PropertyMsg`, `DockAction`, `Effects`, `App`, `Action`, `WindowId`, `WindowRequest`), as stubs where the package fills them, so no package edits `lib.rs`. `TextOp` and `RichOp` are re-exports of `AreaOp`/`RichOp` made `pub` in `controls.rs` (package E does not touch `controls.rs`); a message carries one op. Add `WindowId(u64)` and a core `WindowRequest` (title, size, modal owner) in `window.rs`, and `Effects::open_window(WindowRequest) -> WindowId` and `close_window(WindowId)` as stubs that package H implements. Mark `crates/seaglass/tests/api.rs` `#[ignore]` until phase 3.
7. `Effects::spawn`/`channel` and the task tables (`task.rs`, `mailbox.rs`).

Gate: workspace builds; all existing tests pass; new tests for message order within a batch, one view per batch, a memo-skipped subtree's handler still firing, `Ui::map`, and the allocation tests above.

**Phase 2, widgets and hosts (parallel, one package each).**

| Package | Files | After |
| --- | --- | --- |
| A. Remaining controls and fields, commands, menus | `ui/controls.rs`, `ui/fields.rs`, `ui/builders.rs`, `command.rs`, `ui/window.rs` | |
| B. Collections and grids, `GridMsg` | `ui/collections.rs`, `collection.rs`, `tree/collection_input.rs` | |
| C. Dock, panes, split, `DockAction` | `ui/dock.rs`, `dock.rs`, `panes.rs`, `tree/dock_input.rs`, `seaglass-winit/src/dock.rs` | |
| D. Overlays, dialogs, popovers, toasts | `ui/overlays.rs`, `ui/dialog.rs`, `ui/toasts.rs`, `toast.rs`, `tree/overlay_input.rs`, `tree/toasts.rs` | |
| E. Text editors, `TextOp`, `RichOp`, rich text links | `ui/text_area.rs`, `ui/rich_editor.rs`, `ui/rich_text.rs`, `text_area.rs`, `rich_editor.rs`, `document.rs`, `rich_document.rs`, `tree/text_area_input.rs`, `tree/rich_editor.rs`, `tree/rich_text.rs` | |
| F. Property grid and the derive | `ui/property_grid.rs`, `property_grid.rs`, `seaglass-derive` | A |
| G. Custom widgets, event areas, gestures, drag and drop, `dial-widget` | `widget.rs`, `ui/areas.rs`, `drag.rs`, `tree/drag_input.rs`, `tree/touch_input.rs`, `examples/dial-widget` | |
| H. Multi-window, hot state | `seaglass-winit/src/desktop.rs`, `seaglass-winit/src/hot.rs`, `seaglass-winit/src/wakeup.rs` | |

Each package converts the tests in its files, and adds a test per converted widget through `Harness::app`. Gate per package: `cargo check --workspace --all-targets` and its own tests. Packages work in separate worktrees off the phase 1 commit and are merged one at a time.

**Phase 3, migration and removal.** In parallel by file: the 22 examples in `crates/seaglass/examples`, `examples/application.rs:187`'s mailbox, `seaglass-fluent/tests/messages.rs`, `seaglass-render-cpu/tests/damage.rs`, `seaglass-winit/src/accessibility.rs` tests, the devtools inspector (`devtools/src/lib.rs:477-697`, gets an `InspectorMsg`), and the remaining kernel tests (via `Action<S>` where mechanical). Then one implementer, serially: removes the old forms and renames the `_msg` ones; removes `Response::clicked/changed/submitted`, `Ui::emit`, the one-shot `Ui` calls, the `clicks`/`updates`/`submitted` maps, `awaiting`, `FrameStats::activations` and the settle loops; renames `run_app` to `run`; un-ignores `api.rs` and regenerates `public-api.snap`; updates `state.md`, `architecture.md`, `widgets.md` and `README.md`.

## Validation

- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` are clean.
- `cargo test --workspace --all-features` passes.
- Every example and `dials` smoke-run with `--smoke` and exit 0.
- `scripts/test-controls.ps1`, `scripts/test-inspector.ps1` and `scripts/test-theming.ps1` pass.
- The allocation and declaration tests hold.
- A test where a button declared below a label changes the label's text: the presented frame shows the new text after one declaration.
