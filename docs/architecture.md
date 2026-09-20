# Kernel architecture

```text
application state + Ui declarations
                |
        keyed reconciliation
                |
       retained node tree
       /        |        \
    input    Taffy       AccessKit
   + focus   layout    (OS accessibility)
                |
         ordered Scene
                |
       wgpu + glyphon
                |
          winit surface
```

The retained node is the shared identity for hierarchy, layout, hit testing, focus and semantics. Taffy's internal layout IDs are implementation details; public `NodeId`s are monotonically allocated within a `Context`. The application owns domain state in its closure. The kernel retains interaction state and cached geometry.

## Declaration and reconciliation

`Context::run` builds a temporary declaration using `Ui`. Existing children are matched by key within their parent and checked for widget-kind compatibility. Per-parent hash indexes make identity lookup and duplicate-key checks linear over a sibling declaration rather than quadratic. Property comparisons determine which subsystems need work. Removed subtrees leave both the UI tree and Taffy; focus and capture are cleared if their target disappears or becomes disabled.

Explicit keys and positional keys occupy separate namespaces. Positional keys are child indexes, including keyed siblings. Conditional sequences should therefore use explicit keys throughout.

Clicks produced by events are delivered to a matching button response once. Unhandled responses expire at the end of the declaration. The current response is boolean: multiple activations of the same button before a declaration coalesce. `FrameStats::activations` tells hosts when to declare again so state changed by callbacks reaches widgets declared earlier in the view.

`Ui::component` gives an application-owned `Component` a stable parent/key scope. The framework does not store opaque component state. `Ui::custom` accepts a leaf's intrinsic size, local scene primitives and semantic metadata; button roles route through existing activation. Clip stacks must balance inside that leaf. Built-in text editing and slider dragging have dedicated retained input models; arbitrary input contracts for custom leaves remain future work.

## Invalidation

| Change | Work requested |
| --- | --- |
| No property or input change | Reconciliation only |
| Color / radius / hover / pressed state | Affected paint chunks and ancestor caches, then scene flattening |
| Focus | Scene and semantic update flags |
| Text or font size | Text measurement, scene, semantic flags; layout only if measured size changes |
| Width / height / padding / gap / grow | Layout and scene |
| Child creation / deletion / reorder | Tree, layout, scene, semantic update flags |
| Viewport resize | Layout and scene |
| Scroll offset | Bounds, clips, paint and semantics; no Taffy layout request |
| Theme switch | Paint caches dropped and scene repainted; layout only if the resolved fonts change |
| Inherited color change | Paint of every descendant without its own value |
| Inherited font change | Re-measurement and layout of descendants without their own value |

Dirty flags aggregate subsystem work at context level. Taffy retains per-node layout caches and marks ancestors dirty when node inputs change. Paint uses shared subtree chunks: invalidation walks ancestors, while clean chunks are reused without regenerating descendants. Each node's own commands are stored once in the chunk tree rather than duplicating flattened scenes at every ancestor. The renderer-facing flat scene still copies all commands when painting changes, and declarations still traverse every widget. Counters distinguish root layout requests, newly painted nodes and reused cache roots.

`Context::damage_regions` reports conservative logical damage from the latest declaration, currently merged into one enclosing rectangle. Changes include old and new bounds, removed content and interaction decoration. A host doing several declarations before presentation must accumulate damage across them. The current GPU host fully repaints, so it does not rely on partial-damage validity yet.

## Layout and text

All core geometry uses logical pixels. Styles offer auto, pixel and percentage sizing with min/max bounds and aspect ratios, uniform padding, gaps, flex growth and opt-in shrinking, plus cross-axis alignment and main-axis distribution. Rows lay out horizontally; columns and containers vertically; grids place children into `Track` templates with implicit tracks and explicit `GridArea`s. Children do not shrink by default. Content exceeding bounds is clipped.

Scroll nodes retain a vertical offset and range. Bounds propagation subtracts each ancestor's offset before computing paint, hit-test and accessibility geometry. Offsets clamp when content shrinks. Wheel input chooses the deepest viewport under the pointer and propagates to ancestors at a boundary. Tab and accessibility focus reveal the target through scroll ancestors; arrow/page/Home/End keys scroll the focused viewport or its ancestors. Scrollbars indicate position; thumb dragging and virtualization are not implemented.

The renderer's `TextSystem` supplies shared cosmic-text buffers for measurement, glyphon painting and caret/selection geometry. Cache keys include text, family, size, weight, italic and wrapping width. Wrapped leaves are measured through Taffy's available content width; unwrapped text still uses intrinsic-size comparisons. Generic families resolve to installed system fonts. Buffers referenced by the current scene survive renderer preparation. The core's synthetic measurer is intentionally separate and only intended for tests.

Single-line text fields retain boxed editor state with grapheme boundaries, selection, composition, horizontal reveal and bounded undo/redo. Application values are updated through a queue consumed by declarations, like button activation. The host translates native keys, clipboard commands and IME events into core operations, enables IME only for the focused text field and initializes clipboard access lazily. Window reactivation restores a surviving control's focus without replaying held-key presses or IME composition.

Combo boxes and menus declare an absolute popup subtree under their retained owner. Painting and hit testing visit the active popup after normal content; its viewport is constrained to the window. Wheel input targets the popup before underlying scrollers. Accessibility publishes popup panels as root children with an owner-controls relationship, so intermediate scroll containers do not hide overlay choices. The highlighted option receives accessibility focus. Platform limitations and the remaining editable-text accessibility contract are recorded in [milestone 3](milestone-3.md).

## Collections, commands and drag

Collections separate application identity from widget identity. A `ListModel` answers by index and by `ItemId`; the framework never stores item data. Each declaration asks the model for the realized range only: a spacer before, one `CollectionRow` group per visible item keyed by item ID, and a spacer after, so a million-row list keeps a few dozen nodes. `ListState` owns selection, a reveal request and the scroll offset. Keyboard, pointer and accessibility input queue `CollectionInput` on the owner node and are consumed when the collection is declared again, like button activation. Trees are lists whose model rebuilds a visible order on expansion; grids add a header row of sort buttons and per-cell semantics.

Commands attach a `Shortcut` to an ordinary button. `Event::Shortcut` searches enabled command nodes, honors `while_editing`, and activates the match through the same one-shot response as a click. Declaring two enabled commands with one shortcut panics, since silent precedence would hide the conflict.

Drag state is retained in the context. A pointer press on a `DragSource` records a candidate; movement past the threshold activates it, clears capture and marks eligible `DropTarget`s. Release over a target queues a `ControlUpdate::Drop` consumed by the next `drop_target` declaration. Space/Enter and the `StartDrag`/`Drop` actions reuse the same path. `Event::DropAt` lets hosts inject external payloads at a logical point; successive file drops for one target merge.

`run_windows` keeps one wgpu device, renderer and clipboard for several `Application`s, each with its own `Context`, focus and IME target. Windows created after the first share the device through `WindowState::sharing` and must present with the same surface format. `WakeHandle::channel` builds a bounded mailbox whose sender wakes the loop once per pending batch.

## Styling

`Style` is per widget; `Theme` is per context. Text properties on a style are optional and resolve during reconciliation from the parent's `ResolvedText`, whose root values come from the theme, so the resolved values are what measurement and paint use and what the inspector reports. Resolution is compared like any other property: a color difference marks paint, a font difference marks text and layout. Backgrounds default per widget kind to a theme token, and interaction states pick theme tokens first and then the style's `StateStyles` overrides, so state changes stay repaint-only.

Named classes live in a `StyleSheet` on the context. The text format is line-based with no dependencies; tokens overlay whichever theme is active so an OS light/dark switch keeps an application's accent. The host installs a configured sheet at start and polls its modification time, keeping the last good sheet when an edit fails to parse.

## Tooling boundary

The kernel exposes facts, not views. `Context::snapshots` and `Context::details` give parent-first node data including key, kind, style, the `Dirty` flags the latest declaration changed on that node, and whether its paint chunk was regenerated. `FrameStats` carries a frame counter and CPU durations for the declare, layout and paint phases, and both the core and the host emit `tracing` spans (`rust_gui.declare`, `rust_gui.layout`, `rust_gui.paint`, `rust_gui.frame`, `rust_gui.render`) for external profilers.

`rust-gui-testing` drives a `Context` with the synthetic measurer through the same event and action paths as the host, finds nodes by key path or accessible label, and renders deterministic text snapshots of the tree, layout, accessibility and scene. `rust-gui-devtools` holds the per-window `Recorder` (frame and event ring buffers) and the inspector model and views, which are ordinary `Ui` declarations. The host captures a model from the inspected window before the inspector window declares, then applies the inspector's selection overlay, focus requests and CSV export to the target. Single and multi-window hosts share one desktop loop, so the inspector is just another window with a `Send`-free view.

## Input and presentation

Focus scopes and dialog layers keep a small retained state: the last focused child and the node focused before the scope was entered. Tab traversal runs inside the innermost trapping scope (a `FocusScope` with `trap`, or a modal dialog), entering a scope lands on its remembered child, and removing a restoring scope hands focus back to the recorded node during interaction repair. Dialog layers are declared anywhere but appended as the last root children, so they paint over content and win hit testing; a modal layer is itself a hit target (the scrim) and blocks wheel input to anything outside it. After each declaration the context moves focus into the top modal if it is elsewhere.

Hit testing follows reverse paint order and intersects ancestor clips. Disabled buttons cannot receive focus or activation. Pointer press captures the target; release activates only if the pointer is still over that target. Leaving the window clears hover but preserves capture until release or cancellation. Tab and Shift+Tab traverse enabled buttons in declaration order. Enter and Space activate on matching key release; repeats do not queue extra activation. Escape cancels a press; losing window focus clears interaction state.

The winit host requests frames for input, viewport changes, exposure, surface recovery or `WakeHandle` messages. `run_with_wakeup` supplies a cloneable thread-safe handle to setup code. Applications publish state or queue messages before waking; an atomic pending flag coalesces requests onto a winit event-loop proxy. The loop sleeps between events, and handles reject calls after shutdown. Pointer and wheel input use logical coordinates. The renderer receives physical size and DPI. The host skips zero-sized targets, waits on occlusion and delays timeout retries.

Graphics initialization tries one compiled backend per attempt, in Metal / Direct3D 12 / Vulkan / OpenGL order, filtered by platform support and an optional `WGPU_BACKEND` restriction. Each attempt requests a surface-compatible adapter and a device, and failed attempts are dropped before continuing. An explicit `GraphicsBackend` overrides the environment and disables fallback to other APIs. This avoids eagerly initializing unused driver stacks. The order favors the native APIs on macOS and Windows and Vulkan on Linux; it is not a guarantee that the first backend has the smallest footprint on every driver.

`GraphicsOptions` defaults to compact 4–16 MiB allocation block hints, 4,096 non-sampler resource bindings and a desired maximum frame latency of one. The binding limit avoids a large up-front Direct3D descriptor heap; it counts GPU bindings, not widgets. Compact allocation and shorter queues favor small event-driven UIs and can trade allocation/throughput headroom for memory savings. Applications can change these options, including selecting wgpu's `MemoryUsage` preset with `GpuMemoryPolicy::Balanced`. Actual allocation and queue behavior depends on the backend and driver.

Scenes preserve painter order, including quads that cover earlier text. Nested rectangular clips apply to both quads and text. Adjacent text areas share a glyphon renderer even across clip commands, retaining each area's bounds; a painted quad ends the batch. A spare pool avoids churn when small sections appear and disappear, retaining at most 16 inactive text renderers. Quad vertices use one growable GPU buffer and reusable CPU storage.

The host latches scene invalidation until rendering succeeds, including across surface timeouts and recreation. `Renderer::render_cached` reuses prepared draws on exposure redraws when scene, target size and DPI are unchanged. Ordinary `render` always prepares. Changed scenes upload into retained buffers. Partial GPU presentation is pending; quad buffers and CPU vector capacities retain their high-water allocation, and glyph/font/paint caches also contribute to memory use.

## Accessibility boundary

The desktop host creates its AccessKit adapter before showing the window, forwards window events to it, and lazily generates platform trees while accessibility is active. Platform nodes retain the exact core IDs and hierarchy, physical bounds, roles, names/text, disabled state, supported actions and focus. Scroll ranges use the same coordinate scale. Full updates remove stale nodes via the current parent/child graph. DPI changes refresh semantic bounds independently of logical scene changes.

Native activation/focus/scroll requests enter `Context::action`, including validation of disabled and removed targets; activation uses the same one-shot response queue as pointer/keyboard input. Semantic updates occur before surface acquisition, so occlusion or presentation timeouts do not suppress them. AccessKit may filter clipped descendants to visible items and adjacent off-screen items, allowing progressive reveal. Windows UI Automation tests exercise this bridge. Screen-reader speech/navigation quality and Linux/macOS runtime integration remain manual/platform validation work.
