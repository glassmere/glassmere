# Milestone 2: architecture validation

Implemented on 2026-09-20. The workbench exercises scrolling, application-owned components, a custom control, worker wakeups and native accessibility together. Core behavior and the Windows accessibility bridge are automated; manual screen-reader speech/navigation and native Linux/macOS runs remain outstanding.

## Run and verify

```sh
cargo run -p seaglass --example workbench --release
cargo run -p seaglass --example workbench --release -- --smoke
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo run -p seaglass-core --example tree_bench --release --locked
```

On Windows, build first and then run the native checks:

```powershell
cargo build -p seaglass --examples --release --locked
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-desktop.ps1
```

The script launches only its own workbench instances, checks worker wakeups and shutdown, invokes controls via UI Automation, and closes the windows. It verifies changing labels, stable runtime IDs, disabled controls, separate component state, custom activation, `ScrollIntoView`, and End-key scrolling to the last row. `-Screenshot` also captures its workbench window to `target/workbench.png`. Execution-policy bypass is process-local.

## API and behavior

| API | Contract |
| --- | --- |
| `Ui::scroll(key, style, content)` | A vertical viewport with a constrained height. Keyed offsets survive declarations and clamp after content shrinkage. Wheel input, keyboard input, hit testing, paint and accessibility share transformed bounds. |
| `Ui::component(key, style, &mut component)` | Calls `Component::ui` inside a stable parent scope. State belongs to the application's component object; reordering keyed component scopes preserves child identities. |
| `Ui::custom(key, style, widget)` | A custom leaf supplies intrinsic size, semantics and local scene commands. The local paint origin is the leaf's top-left corner. Commands remain inside the leaf's clip, and nested clip commands must balance. |
| `Context::action(id, action)` | Validates focus, activation and scrolling against live IDs. Disabled and removed activation targets are ignored. Button activation is delivered once through `Response::clicked()`. |
| `run_with_wakeup(options, setup)` | Setup receives a `WakeHandle` and returns the view closure. Worker threads or async tasks publish state first and then wake the host. The framework does not impose an async runtime. |
| `WakeHandle::request_redraw()` | Coalesces concurrent requests through the native event loop. Returns false after shutdown. `request_exit()` also rejects calls after shutdown. |
| `Context::damage_regions()` | Conservative logical damage from the most recent declaration, including changed/removed/moved content. The current GPU renderer still redraws the full surface. |
| `FrameStats::paint_nodes / paint_cache_hits` | Newly generated node paint chunks versus reused subtree roots. These are structural counters, not elapsed-time estimates. |

Custom controls currently have built-in button-style focus/activation or noninteractive semantics. Arbitrary pointer gestures, text editing and richer control roles will extend this contract when those controls are introduced. Scrollbars currently indicate position; thumb dragging, horizontal scrolling and virtualization are not implemented.

## Native accessibility

The host uses AccessKit's winit adapter, installed before the window becomes visible. It supplies full trees lazily while the adapter is active. Core node IDs, hierarchy, enabled state, focus, text and geometry map directly into platform nodes. Bounds and scroll ranges are converted to physical coordinates; DPI changes refresh the tree even when logical layout is unchanged. Actions return to the UI thread and use the core's existing validation and response queue.

The initial tree is sent in response to activation. Further semantic updates happen before acquiring a presentation surface, so a timeout or occlusion does not consume them. AccessKit's clipped-child filtering exposes visible items and adjacent off-screen items, rather than every distant row at once. The Windows test follows that contract when exercising `ScrollIntoView`.

The integration follows the [AccessKit winit adapter contract](https://docs.rs/accesskit_winit/0.32.2/accesskit_winit/struct.Adapter.html). Dependency/API behavior was checked against the pinned local sources. UI Automation tests validate the native tree and actions; they do not establish the quality of spoken output from Narrator, NVDA, VoiceOver or Orca.

Manual review should cover initial window announcement, names/roles/disabled state, focus traversal through a scrolled viewport, activation, changed labels, and removal of focused controls. No screen reader or OS setting was changed during automated validation.

## Large-tree measurements

Optimized Rust 1.98.0 build on the local Windows development machine. The headless probe uses `MonospaceMeasurer`, so these timings exclude system-font shaping, GPU work and active accessibility. Labels are grouped in batches of 100. Cold is one sample; each other phase runs 40 declarations. The table records mean and observed p95 in milliseconds. It is one local run, not a cross-machine performance guarantee.

| Labels | Phase | Mean (ms) | p95 (ms) |
| ---: | --- | ---: | ---: |
| 1,000 | Cold creation/layout/paint | 2.458 | 2.458 |
| 1,000 | Unchanged declaration | 0.673 | 0.703 |
| 1,000 | Change one label's color | 0.884 | 0.981 |
| 1,000 | Toggle one child | 1.514 | 1.756 |
| 10,000 | Cold creation/layout/paint | 32.645 | 32.645 |
| 10,000 | Unchanged declaration | 8.493 | 9.704 |
| 10,000 | Change one label's color | 10.841 | 11.545 |
| 10,000 | Toggle one child | 17.390 | 18.660 |

Unchanged frames requested no measurement, layout or painting. A single color change regenerated three paint nodes (root, group and label) at both sizes and requested no layout. It still flattened the complete scene. Inserting/removing a child changed group sizes and shifted later groups, so much more geometry/paint had to update. The [raw counters](benchmarks/tree-2026-09-20.csv) distinguish those cases.

Per-parent hash indexes removed quadratic sibling lookup/duplicate checks. Subtree paint chunks store each node's commands once and share unchanged descendants; they do not retain a duplicate flat scene per ancestor. Remaining costs include rebuilding declarations, property/key/string allocations, full scene flattening, and layout/geometry propagation when content moves. These results motivate later declaration skipping and virtualization; they do not claim that 10,000 changing widgets are already inexpensive.

## Validation and limits

- 21 core tests plus 3 backend/accessibility tests passed.
- Formatting, all-feature/all-target Clippy with warnings denied, and the facade without desktop features passed.
- GPU pixel readback passed on Direct3D 12, including clip boundaries, painter order, retained buffer reuse and DPI invalidation.
- Native worker updates 0, 1, 2 and 3 were observed without forced frame polling, followed by successful shutdown and rejection of late wakeups.
- Native Windows UI Automation interactions passed, and the workbench was visually checked at 125% scaling.
- The rebuilt idle counter measured 67.16 MiB private resident RAM, 122.75 MiB private commit and 0 ms process CPU over two seconds. This is consistent with the earlier memory improvements; see [memory methodology](memory.md).

Damage currently feeds diagnostics/host APIs, not partial GPU presentation. Active accessibility currently sends full updates rather than node deltas. Native Linux/macOS and manual screen-reader checks are unverified. The next implementation milestone is text and controls, beginning with width-aware wrapping and a shared editing/selection model before adding richer input controls.
