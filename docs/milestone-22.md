# Milestone 22: optional reactive state, and the graph it makes

Implemented on 2026-09-22. Ordinary Rust state is still the way to write an application here, and nothing in the framework asks for a store. This is for the case ordinary state handles badly: a value derived from several others, expensive to produce, read in several places, and only sometimes out of date. Written by hand that is a cache and a set of invalidation rules, and the rules are what rot.

It also answers the other half of the goal. The reads a derived value makes are recorded as it computes, which is a dependency graph nobody had to declare, so the inspector can show one.

```rust
let store = Store::new();
let quantity = store.signal("quantity", 3.0);
let price = store.signal("price", 12.5);
let (count, each) = (quantity.clone(), price.clone());
let subtotal = store.memo("subtotal", move || count.get() * each.get());
```

| Piece | Contract |
| --- | --- |
| `Signal<T>` | A held value. `get`, or `with` to read without cloning. `set` writes, and writing the value it already holds changes nothing, so a view that writes back what it read costs nothing. `update` changes in place and always counts. Cloning gives another handle on the same value. |
| `Memo<T>` | A derived value. `compute` runs when the memo is read and something it read has changed, not when the change happened. What it read is recorded while it runs, so a branch not taken is not depended on, and reads that stop happening stop being edges. |
| Invalidation | Changing a signal marks everything derived from it, transitively, at the moment of the change; the recompute waits for the read. So a change is cheap and nothing is computed that nobody asks for. |
| `Store::version` | Bumps on every change to a signal. One comparison answers whether anything moved, and it passes straight to `Ui::memo` as the state of a subtree. `Signal::version` and `Memo::version` are the narrower bargain for one value. |
| `Store::graph` | Every value in creation order: name, kind, version, when it last changed, whether it is out of date, and what it read when it last computed. |
| `Ui::observe(&store)` | Shows that graph in the inspector. The snapshot is taken after the declaration, so it holds what this frame read whatever order the view called it in. Nothing is observed unless a view asks. |
| Inspector | A State tab beside the others, listing each value with its version, whether it is out of date, and its inputs by name. What changed at the newest store version is picked out in the accent colour. |
| Mistakes | A memo that reads itself panics with `reactive cycle: ... is derived from itself` rather than recursing. A memo that writes panics too: a memo that writes would make what others read depend on when they read it. |

## Validation

Seven unit tests: a memo computes once until what it read changes; writing the same value changes nothing while `update` always counts; a change reaches the far end of a chain and a memo read by two others computes once for both; a branch not taken is not depended on, in both directions; the graph names the values, their edges and what moved last; and the two panics say what went wrong.

Two integration tests: a view declared from a store shows the right graph to the inspector, with a change reaching the label through two memos; and a memoized subtree follows `Store::version`, declaring again when a signal moves and not otherwise.

One inspector test asserts the rows the State tab draws, text and all.

The real inspector window was driven through UI Automation over the `reactive` example, reading the tab's rows back through the accessibility tree:

```
ROW signal  quantity  v0
ROW memo    subtotal  v1  <- quantity, price, discount
ROW memo    total     v1  <- subtotal, tax
```

The screenshot is `target/inspector-state.png`. The workspace suite is 301 tests; formatting and clippy with all features are clean; all 19 examples pass their smoke run.

## Remaining limits

- Single threaded, like everything else a view touches. Work on other threads reaches a signal through `Tasks` (milestone 21), which is where the thread boundary is.
- A memo that recomputes to the same value still counts as changed, so what reads it recomputes too. Comparing would need `PartialEq` on every derived type.
- No effects. The view is the effect: it reads, and the framework declares again when input or a message arrives. Nothing runs by itself when a signal changes.
- No batching. Two writes mark twice; the recompute still happens once, at the next read, so the cost is the marking.
- The graph is a list with edges, not a drawing, and it has no history: it shows the store as it is now.
