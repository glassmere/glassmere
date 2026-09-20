1\. \*\*Immediate-mode ergonomics, retained-mode internals\*\*



&#x20;  \* Simple API like `ui.button(...)`

&#x20;  \* Persistent widget state, focus, scroll position, animations, caches

2\. \*\*Stable widget identity\*\*



&#x20;  \* Explicit or automatic stable IDs

&#x20;  \* Shared across state, focus, accessibility, testing, animation, reconciliation, and debugging

3\. \*\*Incremental updates\*\*



&#x20;  \* Only update widgets affected by changed data

&#x20;  \* Avoid rebuilding, laying out, or repainting the entire UI unnecessarily

4\. \*\*Incremental layout\*\*



&#x20;  \* Recalculate only dirty subtrees

&#x20;  \* Stop propagation when geometry no longer changes

5\. \*\*Incremental rendering\*\*



&#x20;  \* Damage tracking

&#x20;  \* Partial repainting

&#x20;  \* Render caching

&#x20;  \* GPU batching

6\. \*\*First-class virtualization\*\*



&#x20;  \* Virtual lists

&#x20;  \* Virtual trees

&#x20;  \* Virtual tables/grids

&#x20;  \* Huge documents and editors

&#x20;  \* Millions of items without millions of widget objects

7\. \*\*Excellent text system\*\*



&#x20;  \* Unicode

&#x20;  \* Grapheme-aware cursor movement

&#x20;  \* BiDi / RTL

&#x20;  \* Font fallback

&#x20;  \* Ligatures

&#x20;  \* Emoji

&#x20;  \* IME

&#x20;  \* Selection

&#x20;  \* Clipboard

&#x20;  \* Rich text

&#x20;  \* Syntax highlighting

&#x20;  \* Undo/redo

&#x20;  \* Large-document virtualization

8\. \*\*Accessibility from day one\*\*



&#x20;  \* AccessKit integration

&#x20;  \* Semantic roles

&#x20;  \* Labels/descriptions

&#x20;  \* Accessible actions

&#x20;  \* Keyboard navigation

&#x20;  \* Screen-reader support

&#x20;  \* Accessibility tree visible in devtools

9\. \*\*Powerful focus system\*\*



&#x20;  \* Keyboard focus

&#x20;  \* Pointer capture

&#x20;  \* Focus scopes

&#x20;  \* Tab traversal

&#x20;  \* Modal focus

&#x20;  \* Directional navigation

&#x20;  \* Predictable focus restoration

10\. \*\*Modern layout engine\*\*



&#x20;   \* Flexbox

&#x20;   \* Grid

&#x20;   \* Absolute positioning

&#x20;   \* Intrinsic sizing

&#x20;   \* Min/max constraints

&#x20;   \* Aspect ratio

&#x20;   \* Baseline alignment

&#x20;   \* Responsive layout

&#x20;   \* Custom layout primitives

11\. \*\*Typed styling system\*\*



&#x20;   \* Classes

&#x20;   \* Themes

&#x20;   \* Variables/tokens

&#x20;   \* Pseudo-states such as `hover`, `focus`, `disabled`, `active`

&#x20;   \* Inheritance where useful

&#x20;   \* Compile-time validation

&#x20;   \* Hot reload

12\. \*\*Excellent theming\*\*



&#x20;   \* Light/dark modes

&#x20;   \* Design tokens

&#x20;   \* User themes

&#x20;   \* Runtime theme switching

&#x20;   \* OS preference integration

13\. \*\*Animations built into the property system\*\*



&#x20;   \* Transitions

&#x20;   \* Keyframes

&#x20;   \* Spring animations

&#x20;   \* Interruptible animations

&#x20;   \* Layout animations

&#x20;   \* GPU-friendly transforms and opacity

14\. \*\*Renderer-independent UI core\*\*



&#x20;   \* Widgets shouldn't know about `wgpu`

&#x20;   \* Produce a display/scene list instead

15\. \*\*Multiple rendering backends\*\*



&#x20;   \* GPU renderer

&#x20;   \* CPU/software renderer

&#x20;   \* Potential future web/backend support

16\. \*\*Efficient GPU architecture\*\*



&#x20;   \* Instanced quads

&#x20;   \* Texture atlases

&#x20;   \* Glyph atlases

&#x20;   \* Render batching

&#x20;   \* Clipping

&#x20;   \* Layers

&#x20;   \* Shadows

&#x20;   \* Paths

&#x20;   \* Gradients

&#x20;   \* Custom shaders

17\. \*\*Custom rendering escape hatch\*\*



&#x20;   \* Easy canvas/custom paint API

&#x20;   \* Ability to embed 2D/3D renderers

&#x20;   \* Direct GPU integration when necessary

18\. \*\*Rich widget library\*\*



&#x20;   \* Buttons

&#x20;   \* Checkboxes

&#x20;   \* Radios

&#x20;   \* Sliders

&#x20;   \* Inputs

&#x20;   \* Number inputs

&#x20;   \* Select/combo boxes

&#x20;   \* Autocomplete

&#x20;   \* Menus

&#x20;   \* Context menus

&#x20;   \* Popovers

&#x20;   \* Tooltips

&#x20;   \* Dialogs

&#x20;   \* Toasts

&#x20;   \* Tabs

&#x20;   \* Split panes

&#x20;   \* Docking

&#x20;   \* Trees

&#x20;   \* Virtual lists

&#x20;   \* Data grids

&#x20;   \* Property grids

&#x20;   \* Rich text

&#x20;   \* Code editor

19\. \*\*Professional-grade DataGrid\*\*



&#x20;   \* Virtual rows and columns

&#x20;   \* Sorting

&#x20;   \* Filtering

&#x20;   \* Resizing

&#x20;   \* Reordering

&#x20;   \* Frozen rows/columns

&#x20;   \* Selection

&#x20;   \* Inline editing

&#x20;   \* Huge datasets

20\. \*\*Professional docking/layout system\*\*



&#x20;   \* Dockable panels

&#x20;   \* Tabs

&#x20;   \* Floating panels

&#x20;   \* Split views

&#x20;   \* Persistable workspace layouts

21\. \*\*Powerful event system\*\*



&#x20;   \* Capture and bubbling

&#x20;   \* Pointer events

&#x20;   \* Keyboard events

&#x20;   \* Gestures

&#x20;   \* Drag-and-drop

&#x20;   \* Custom events

&#x20;   \* Event cancellation

22\. \*\*Input abstraction independent of the OS backend\*\*



&#x20;   \* Framework widgets should not depend directly on `winit`

23\. \*\*Great mouse/touch/stylus support\*\*



&#x20;   \* Multi-pointer

&#x20;   \* Pointer capture

&#x20;   \* Gestures

&#x20;   \* Pressure where available

&#x20;   \* Smooth scrolling

24\. \*\*First-class keyboard shortcuts\*\*



&#x20;   \* Commands

&#x20;   \* Shortcut scopes

&#x20;   \* Platform-specific bindings

&#x20;   \* User-remappable shortcuts

&#x20;   \* Command palette integration

25\. \*\*Command architecture\*\*



&#x20;   \* Application commands separate from raw input

&#x20;   \* `Save`, `Undo`, `Open`, `Rename`, etc.

&#x20;   \* Makes menus, shortcuts, command palettes, and automation share the same actions

26\. \*\*Clipboard and drag-and-drop\*\*



&#x20;   \* Text

&#x20;   \* Images

&#x20;   \* Files

&#x20;   \* Custom MIME/data types

&#x20;   \* Cross-window drag-and-drop

27\. \*\*Multi-window support\*\*



&#x20;   \* Independent windows

&#x20;   \* Dialog windows

&#x20;   \* Shared app state

&#x20;   \* Multi-monitor handling

&#x20;   \* Proper DPI scaling

28\. \*\*Strong async integration\*\*



&#x20;   \* `async/await`

&#x20;   \* Background tasks

&#x20;   \* Cancellation

&#x20;   \* Streams

&#x20;   \* Async commands

&#x20;   \* Easy Tokio integration without forcing Tokio

29\. \*\*Optional reactive state\*\*



&#x20;   \* Signals/computed values/effects available

&#x20;   \* But don't force the entire application into your state-management model

30\. \*\*Ordinary Rust state should work\*\*



&#x20;   \* `\&mut App`

&#x20;   \* Structs

&#x20;   \* enums

&#x20;   \* collections

&#x20;   \* normal Rust ownership

31\. \*\*Observable dependency graph\*\*



&#x20;   \* Show exactly why a widget updated

&#x20;   \* Avoid invisible reactive magic

32\. \*\*Built-in native DevTools\*\*



&#x20;   \* UI tree inspector

&#x20;   \* Box-model/layout inspector

&#x20;   \* Computed styles

&#x20;   \* Event inspection

&#x20;   \* Focus state

&#x20;   \* Accessibility tree

&#x20;   \* Reactive dependencies

&#x20;   \* Repaint regions

&#x20;   \* Render layers

&#x20;   \* GPU timings

&#x20;   \* Frame timings

&#x20;   \* Memory statistics

33\. \*\*Performance profiler\*\*



&#x20;   \* Widget update time

&#x20;   \* Layout time

&#x20;   \* Text shaping time

&#x20;   \* Paint time

&#x20;   \* GPU time

&#x20;   \* Number of dirty nodes

&#x20;   \* Draw calls

&#x20;   \* Allocations

34\. \*\*Hot reload\*\*



&#x20;   \* Styles

&#x20;   \* Themes

&#x20;   \* UI declarations where feasible

&#x20;   \* Assets

&#x20;   \* Ideally without losing application state

35\. \*\*Headless UI testing\*\*



&#x20;   \* No real window required

&#x20;   \* Find widgets by ID/semantics

&#x20;   \* Simulate clicks and typing

&#x20;   \* Inspect application state

36\. \*\*Snapshot testing\*\*



&#x20;   \* Render snapshots

&#x20;   \* Layout snapshots

&#x20;   \* Accessibility snapshots

&#x20;   \* Widget-tree snapshots

37\. \*\*Semantic testing API\*\*



&#x20;   \* Prefer `find("Save").click()` over pixel coordinates

38\. \*\*Deterministic rendering/testing\*\*



&#x20;   \* Important for CI and reproducible screenshots

39\. \*\*Localization\*\*



&#x20;   \* Runtime locale switching

&#x20;   \* Plurals

&#x20;   \* RTL

&#x20;   \* Locale-aware numbers/dates

&#x20;   \* Fluent or equivalent integration

40\. \*\*High-DPI support\*\*



&#x20;   \* Per-monitor DPI

&#x20;   \* Fractional scaling

&#x20;   \* Crisp text and borders

41\. \*\*Platform integration\*\*



&#x20;   \* Native file dialogs

&#x20;   \* Notifications

&#x20;   \* Menus

&#x20;   \* Tray icons

&#x20;   \* Open-with/file associations

&#x20;   \* Window decorations

&#x20;   \* System appearance

&#x20;   \* Accessibility APIs

42\. \*\*Native-first desktop focus\*\*



&#x20;   \* Windows

&#x20;   \* macOS

&#x20;   \* Linux

&#x20;   \* Make these excellent before chasing every platform

43\. \*\*Clean platform abstraction\*\*



&#x20;   \* Keep OS-specific code isolated so mobile/web can be added later

44\. \*\*Low allocation architecture\*\*



&#x20;   \* Arena/slot-based nodes

&#x20;   \* Reuse allocations

&#x20;   \* Avoid `Box<dyn Widget>` everywhere if possible

45\. \*\*Predictable ownership\*\*



&#x20;   \* Avoid callback/lifetime gymnastics

&#x20;   \* Avoid pervasive `Rc<RefCell<\_>>`

&#x20;   \* Make normal Rust ownership feel natural

46\. \*\*Good escape hatches\*\*



&#x20;   \* Framework should make common cases easy without making advanced cases impossible

47\. \*\*Modular crates\*\*



&#x20;   \* `core`

&#x20;   \* `layout`

&#x20;   \* `text`

&#x20;   \* `renderer`

&#x20;   \* `widgets`

&#x20;   \* `accessibility`

&#x20;   \* `devtools`

&#x20;   \* `testing`

&#x20;   \* platform backends

48\. \*\*No forced proprietary DSL\*\*



&#x20;   \* Rust should be sufficient for building the UI

&#x20;   \* Optional macro/stylesheet syntax can improve ergonomics

49\. \*\*Compile-time checked UI APIs\*\*



&#x20;   \* Invalid properties or widget combinations should fail early where practical

50\. \*\*Excellent documentation\*\*



&#x20;   \* Architecture guide

&#x20;   \* Widget-building guide

&#x20;   \* Custom renderer guide

&#x20;   \* State-management guide

&#x20;   \* Accessibility guide

&#x20;   \* Performance guide

51\. \*\*Stable public API\*\*



&#x20;   \* Don't make application developers rewrite their UI every six months

52\. \*\*Strong benchmarks\*\*



&#x20;   \* 1,000,000-row table

&#x20;   \* 100,000-node tree

&#x20;   \* Multi-megabyte text file

&#x20;   \* Hundreds of animated elements

&#x20;   \* Thousands of visible widgets

&#x20;   \* Low idle CPU usage

53\. \*\*Low idle CPU/GPU usage\*\*



&#x20;   \* Do not continuously redraw unchanged applications

&#x20;   \* Sleep when nothing is happening

54\. \*\*Fast startup\*\*



&#x20;   \* Important for desktop tools

&#x20;   \* Avoid browser-scale runtime overhead

55\. \*\*Small deployment footprint\*\*



&#x20;   \* Especially valuable compared with Electron-class solutions

56\. \*\*Excellent error diagnostics\*\*



&#x20;   \* Invalid layout

&#x20;   \* Duplicate IDs

&#x20;   \* Cyclic dependencies

&#x20;   \* Missing fonts/resources

&#x20;   \* Accessibility problems

&#x20;   \* Performance warnings

57\. \*\*Framework-level diagnostics\*\*



&#x20;   \* Warn about things such as:

&#x20;   \* unstable widget IDs

&#x20;   \* unnecessary relayout

&#x20;   \* excessive repainting

&#x20;   \* inaccessible controls

&#x20;   \* oversized render layers

&#x20;   \* non-virtualized huge collections

58\. \*\*A small core with strong extension points\*\*



&#x20;   \* Third parties should be able to build controls without internal/private APIs

59\. \*\*First-class component ecosystem\*\*



&#x20;   \* Package reusable widgets as normal crates

&#x20;   \* Minimal framework-version coupling

60\. \*\*Excellent custom-widget API\*\*



&#x20;   \* Custom widgets should get layout, events, semantics, state, styles, rendering, and testing through clean APIs

61\. \*\*A clear performance model\*\*



&#x20;   \* Developers should understand what causes:

&#x20;   \* rebuild

&#x20;   \* relayout

&#x20;   \* repaint

&#x20;   \* GPU upload

62\. \*\*The central promise should be:\*\*



&#x20;   \* \*\*As easy to start with as egui\*\*

&#x20;   \* \*\*As capable of complex applications as GPUI\*\*

&#x20;   \* \*\*As incremental as Floem/Xilem\*\*

&#x20;   \* \*\*As tooling-focused as Slint\*\*

&#x20;   \* \*\*As styleable as web UI\*\*

&#x20;   \* \*\*With accessibility and text quality treated as core architecture rather than later additions\*\*



