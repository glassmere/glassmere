# Kernel architecture

```text
   App::update  <--  messages  <--  handlers
        |                              ^
   App::view -> Ui declarations        |
                |                      |
        reconciliation -----------------+
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

The retained node is the shared identity for hierarchy, layout, hit testing, focus and semantics. Taffy's internal layout IDs are implementation details; public `NodeId`s are monotonically allocated within a `Context`. The application owns domain state in its `App`. The kernel retains interaction state, cached geometry and the message handlers the last declaration left on each node.

## Messages and batches

Each widget that sends something is declared with the message, or the function building it, that it sends. Reconciliation keeps that handler in a slot of the node, outside the element, so comparing elements never touches it. Input taken between frames is queued against the node it landed on; `AppDriver` resolves the queue through the handlers into messages, delivers the batch to `App::update` together with whatever tasks, channels and effects answered, and then declares once. A view therefore runs at most once per batch and reads state that is already up to date, and nothing a view does can cause another declaration on its own. `FrameStats::messages` counts what a batch delivered and `FrameStats::declared` whether the view ran.

What `update` asks of the framework goes through `Effects`: clipboard, focus, file dialogs, toasts, notifications, theme, localisation, windows, tasks and channels. Requests that answer later answer with a message. Hosts call `AppDriver::frame` (or `batch` and `declare` for several windows); `seaglass::run` is that loop on winit, and `Harness::app` is it with no window.

## Declaration and reconciliation

`AppDriver::declare` builds a temporary declaration using `Ui`; `Context::run` does the same for a view with no `App`, which tests use for layout. Existing children are matched by key within their parent and checked for widget-kind compatibility. Per-parent hash indexes make identity lookup and duplicate-key checks linear over a sibling declaration rather than quadratic. Property comparisons determine which subsystems need work. Removed subtrees leave both the UI tree and Taffy; focus and capture are cleared if their target disappears or becomes disabled.

A widget without an explicit key is identified by its call site: the source location of the `Ui` method that declared it (every declaring method is `#[track_caller]`), and how many siblings that same location declared before it. A widget declared conditionally therefore does not shift the identity of the siblings after it; only repeats of one location, such as a loop body, are told apart by order, and those want `Ui::keyed` when they can be inserted, removed or reordered. A helper function of the application's that declares widgets can take `#[track_caller]` too, so its widgets are identified by where the helper is called. Explicit keys and call-site keys occupy separate namespaces.

A click produces the button's message once per activation. A `Response` reports only what the widget looked like on the last frame: `hovered`, `pressed` and `focused`.

`Ui::component` gives an application-owned `Component` a stable parent/key scope and maps its messages into the parent's. The framework does not store opaque component state. `Ui::widget` accepts a leaf's intrinsic size, local scene primitives and semantic metadata, and its `on_event` turns the events that reach it into messages; `Ui::custom` is the simpler leaf that sends one message when activated. Clip stacks must balance inside a leaf.

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

`Context::damage_regions` reports conservative logical damage from the latest declaration, merged into one enclosing rectangle. Changes include old and new bounds, removed content and interaction decoration. A host doing several declarations before presentation must accumulate damage across them. The desktop host hands the damage to its renderer only for a frame that was declared and did not resize the window; otherwise it passes none, which means the whole surface.

## Layout and text

All core geometry uses logical pixels. Styles offer auto, pixel and percentage sizing with min/max bounds and aspect ratios, uniform padding, gaps, flex growth and opt-in shrinking, plus cross-axis alignment and main-axis distribution. Rows lay out horizontally; columns and containers vertically; grids place children into `Track` templates with implicit tracks and explicit `GridArea`s. Children do not shrink by default. Content exceeding bounds is clipped.

Scroll nodes retain a vertical offset and range. Bounds propagation subtracts each ancestor's offset before computing paint, hit-test and accessibility geometry. Offsets clamp when content shrinks. Wheel input chooses the deepest viewport under the pointer and propagates to ancestors at a boundary. Tab and accessibility focus reveal the target through scroll ancestors; arrow/page/Home/End keys scroll the focused viewport or its ancestors. Scrollbars indicate position and cannot be dragged. Virtualization belongs to the collections, not to scroll nodes.

The renderer's `TextSystem` supplies shared cosmic-text buffers for measurement, glyphon painting and caret/selection geometry. Cache keys include text, family, size, weight, italic and wrapping width. Wrapped leaves are measured through Taffy's available content width; unwrapped text still uses intrinsic-size comparisons. Generic families resolve to installed system fonts. Buffers referenced by the current scene survive renderer preparation. The core's synthetic measurer is intentionally separate and only intended for tests.

Single-line text fields retain boxed editor state with grapheme boundaries, selection, composition, horizontal reveal and bounded undo/redo. Edits are queued on the field and sent as the field's message in the next batch. The host translates native keys, clipboard commands and IME events into core operations, enables IME only for the focused text field and initializes clipboard access lazily. Window reactivation restores a surviving control's focus without replaying held-key presses or IME composition.

Combo boxes and menus declare an absolute popup subtree under their retained owner. Painting and hit testing visit the active popup after normal content; its viewport is constrained to the window. Wheel input targets the popup before underlying scrollers. Accessibility publishes popup panels as root children with an owner-controls relationship, so intermediate scroll containers do not hide overlay choices. The highlighted option receives accessibility focus. Platform limits and the editable-text accessibility contract are in [limitations](limitations.md).

## Collections, commands and drag

Collections separate application identity from widget identity. A `ListModel` answers by index and by `ItemId`; the framework never stores item data. Each declaration asks the model for the realized range only: a spacer before, one `CollectionRow` group per visible item keyed by item ID, and a spacer after, so a million-row list keeps a few dozen nodes. `ListState` owns selection, a reveal request and the scroll offset. Keyboard, pointer and accessibility input queue `CollectionInput` on the owner node, which the collection's handler turns into `GridMsg` messages for `ListState::apply` or `GridState::apply`. Trees are lists whose model rebuilds a visible order on expansion; grids add a header row of sort buttons and per-cell semantics.

Commands attach a `Shortcut` to an ordinary button. `Event::Shortcut` searches enabled command nodes, honors `while_editing`, and activates the match through the same path as a click, so it sends the button's message. Declaring two enabled commands with one shortcut in one scope is a misuse, since silent precedence would hide the conflict: a debug build panics, a release build reports it (see [the stability note](api-stability.md)).

Drag state is retained in the context. A pointer press on a `DragSource` records a candidate; movement past the threshold activates it, clears capture and marks eligible `DropTarget`s. Release over a target queues a `ControlUpdate::Drop`, which the target's handler sends as its `on_drop` message, and the source sends its `on_drag_end` message. Space/Enter and the `StartDrag`/`Drop` actions reuse the same path. `Event::DropAt` lets hosts inject external payloads at a logical point; successive file drops for one target merge.

`run` keeps one wgpu device, renderer and clipboard for every window an `App` opens with `Effects::open_window`, each with its own `Context`, focus and IME target, and all of them feeding one `update`. Windows created after the first share the device through `WindowState::sharing` and must present with the same surface format. `Effects::channel` builds a bounded mailbox whose sender wakes the loop once per pending batch.

## Styling

`Style` is per widget; `Theme` is per context. Text properties on a style are optional and resolve during reconciliation from the parent's `ResolvedText`, whose root values come from the theme, so the resolved values are what measurement and paint use and what the inspector reports. Resolution is compared like any other property: a color difference marks paint, a font difference marks text and layout. Backgrounds default per widget kind to a theme token, and interaction states pick theme tokens first and then the style's `StateStyles` overrides, so state changes stay repaint-only.

Named classes live in a `StyleSheet` on the context. The text format is line-based with no dependencies; tokens overlay whichever theme is active so an OS light/dark switch keeps an application's accent. The host installs a configured sheet at start and polls its modification time, keeping the last good sheet when an edit fails to parse.

## Tooling boundary

The kernel exposes facts, not views. `Context::snapshots` and `Context::details` give parent-first node data including key, kind, style, the `Dirty` flags the latest declaration changed on that node, and whether its paint chunk was regenerated. `FrameStats` carries a frame counter and CPU durations for the declare, layout and paint phases, and both the core and the host emit `tracing` spans (`seaglass.declare`, `seaglass.layout`, `seaglass.paint`, `seaglass.frame`, `seaglass.render`) for external profilers.

`seaglass-testing` drives a `Context` with the synthetic measurer through the same event and action paths as the host, finds nodes by key path or accessible label, and renders deterministic text snapshots of the tree, layout, accessibility and scene. `seaglass-devtools` holds the per-window `Recorder` (frame and event ring buffers) and the inspector model and views, which are ordinary `Ui` declarations. The host captures a model from the inspected window before the inspector window declares, then applies the inspector's selection overlay, focus requests and CSV export to the target. Single and multi-window hosts share one desktop loop, so the inspector is just another window with a `Send`-free view.

## Input and presentation

Focus scopes and dialog layers keep a small retained state: the last focused child and the node focused before the scope was entered. Tab traversal runs inside the innermost trapping scope (a `FocusScope` with `trap`, or a modal dialog), entering a scope lands on its remembered child, and removing a restoring scope hands focus back to the recorded node during interaction repair. Dialog layers are declared anywhere but appended as the last root children, so they paint over content and win hit testing; a modal layer is itself a hit target (the scrim) and blocks wheel input to anything outside it. After each declaration the context moves focus into the top modal if it is elsewhere.

Overlays come in three shapes. Combo, menu and context-menu popups share one `PopupState` (owner, highlighted item, optional pointer anchor and the focus to return to); the panel is the owner's first child and is painted and hit-tested after everything else. Popovers are `Kind::Popover` children of their anchor widget: any number may be open, each is painted after content and hit-tested before it, and a press outside a popover and its anchor queues its dismissal. Tooltips are not nodes at all: the context tracks hover dwell against a host-supplied clock, reports `next_wake` so the loop can sleep until the tooltip is due, and appends the tooltip primitives after all overlays when painting.

Hit testing follows reverse paint order and intersects ancestor clips. Disabled buttons cannot receive focus or activation. Pointer press captures the target; release activates only if the pointer is still over that target. Leaving the window clears hover but preserves capture until release or cancellation. Tab and Shift+Tab traverse enabled buttons in declaration order. Enter and Space activate on matching key release; repeats do not queue extra activation. Escape cancels a press; losing window focus clears interaction state.

The winit host requests frames for input, viewport changes, exposure, surface recovery or messages from tasks and channels. Each sender wakes the loop after queueing its message; an atomic pending flag coalesces requests onto a winit event-loop proxy. The loop sleeps between events, and handles reject calls after shutdown. Pointer and wheel input use logical coordinates. The renderer receives physical size and DPI. The host skips zero-sized targets, waits on occlusion and delays timeout retries.

Graphics initialization tries one compiled backend per attempt, in Metal / Direct3D 12 / Vulkan / OpenGL order, filtered by platform support and an optional `WGPU_BACKEND` restriction. Each attempt requests a surface-compatible adapter and a device, and failed attempts are dropped before continuing. An explicit `GraphicsBackend` overrides the environment and disables fallback to other APIs. This avoids eagerly initializing unused driver stacks. The order favors the native APIs on macOS and Windows and Vulkan on Linux; it is not a guarantee that the first backend has the smallest footprint on every driver.

`GraphicsOptions` defaults to compact 4 to 16 MiB allocation block hints, 4,096 non-sampler resource bindings and a desired maximum frame latency of one. The binding limit avoids a large up-front Direct3D descriptor heap; it counts GPU bindings, not widgets. Compact allocation and shorter queues favor small event-driven UIs and can trade allocation/throughput headroom for memory savings. Applications can change these options, including selecting wgpu's `MemoryUsage` preset with `GpuMemoryPolicy::Balanced`. Actual allocation and queue behavior depends on the backend and driver.

Scenes preserve painter order, including quads that cover earlier text. Nested rectangular clips apply to both quads and text. Adjacent text areas share a glyphon renderer even across clip commands, retaining each area's bounds; a painted quad ends the batch. A spare pool avoids churn when small sections appear and disappear, retaining at most 16 inactive text renderers. Quad vertices use one growable GPU buffer and reusable CPU storage.

The host latches scene invalidation until rendering succeeds, including across surface timeouts and recreation. `Renderer::render_cached` reuses prepared draws on exposure redraws when scene, target size and DPI are unchanged. Ordinary `render` always prepares. Changed scenes upload into retained buffers. The wgpu renderer keeps a canvas texture so a damaged frame redraws only its rectangle; quad buffers and CPU vector capacities retain their high-water allocation, and glyph/font/paint caches also contribute to memory use.

## Accessibility boundary

The desktop host creates its AccessKit adapter before showing the window, forwards window events to it, and lazily generates platform trees while accessibility is active. Platform nodes retain the exact core IDs and hierarchy, physical bounds, roles, names/text, disabled state, supported actions and focus. Scroll ranges use the same coordinate scale. Full updates remove stale nodes via the current parent/child graph. DPI changes refresh semantic bounds independently of logical scene changes.

Native activation/focus/scroll requests enter `Context::action`, including validation of disabled and removed targets; activation uses the same input queue as pointer/keyboard input, so it sends the widget's message. Semantic updates occur before surface acquisition, so occlusion or presentation timeouts do not suppress them. AccessKit may filter clipped descendants to visible items and adjacent off-screen items, allowing progressive reveal. Windows UI Automation tests exercise this bridge. Screen-reader behaviour and the Linux and macOS runtime integration have not been checked by hand; see [limitations](limitations.md).

Related: [state](state.md), [styling](styling.md), [text](text.md), [platform](platform.md), [renderers](renderers.md), [performance](performance.md), [accessibility](accessibility.md).
