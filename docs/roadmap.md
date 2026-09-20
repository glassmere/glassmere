# Development roadmap

The shared discussion describes a multi-phase desktop UI platform. This sequence keeps each step demonstrable and testable.

## 1. Kernel baseline — implemented

- Native window and event-driven host.
- Retained identity, keyed reconciliation and cleanup.
- Basic flex layout and an ordered renderer-independent scene.
- wgpu presentation and shaped label/button text.
- Button, Label, Row, Column and Container.
- Pointer capture, focus, keyboard activation and headless behavior tests.
- Semantic metadata with stable IDs.
- Sequential backend detection, configurable compact allocations and startup memory probes.
- Retained GPU buffers, adjacent text batching and cached exposure redraws.
- Intrinsic-size comparisons that skip layout for same-size text changes.

## 2. Architecture validation — implementation delivered

- [x] AccessKit mapping and actions, tested through the Windows UI Automation bridge.
- [x] Vertical scroll containers with matching paint/hit-test transforms, nesting, keyboard scrolling and focus reveal.
- [x] Subtree paint caches and conservative logical damage bounds; GPU presentation still covers the full surface.
- [x] Coalesced worker/async wakeup handles with explicit shutdown behavior.
- [x] 1,000/10,000-label declaration and mutation measurements with paint/layout counters.
- [x] Application-owned component scopes and a declarative custom leaf contract.
- [ ] Manual screen-reader speech/navigation review, and native Linux/macOS validation.

The [milestone report](milestone-2.md) records the APIs, native checks, measurements and remaining limits. The workbench demonstrates the implemented slice. Passing UI Automation checks verifies the platform tree/actions; it does not substitute for a manual screen-reader review.

## 3. Text and controls — implementation delivered

- [x] Width-aware wrapping, installed generic/named font families, weight and italic.
- [x] Single-line selection, grapheme/word navigation, clipboard, bounded undo/redo and IME event routing.
- [x] TextInput, Checkbox, Radio, Slider, ComboBox and one-level menus.
- [x] Input/focus tests, shaped geometry checks, GPU pixel checks and Windows UI Automation control checks.
- [ ] Real OS IME candidate/dead-key review and native Linux/macOS interaction.
- [ ] Full accessible editable-text selection/navigation and Windows ExpandCollapsePattern support; the pinned AccessKit adapter does not implement the latter.

The [milestone report](milestone-3.md) describes the controls gallery, contracts, validation and limits. The counter remains near its previous memory baseline; editor storage and clipboard initialization are limited to the features that use them.

## 4. Application support — implementation delivered

- [x] Stable logical item identity (`ItemId`, `ListModel`, `ItemOrder`) distinct from realized widget identity.
- [x] Virtualized lists, trees and a sortable DataGrid realizing only the viewport.
- [x] Commands with shortcuts sharing the button's enabled state and one-shot response.
- [x] In-process drag/drop with keyboard and accessibility paths; incoming OS file drops.
- [x] Bounded worker/async mailbox and multi-window `run_windows` on one shared device.
- [x] Headless collection, drag and command tests plus a two-window smoke run.
- [ ] Variable row heights, multi-selection, cell editing, outgoing OS drags and native Linux/macOS multi-window checks.

The [milestone report](milestone-4.md) records the contracts, validation and limits. The application workbench demonstrates the implemented slice.

## 5. Tools — implementation delivered

- [x] `rust-gui-testing`: headless harness, semantic finders, real input paths and text snapshots of tree, layout, accessibility and scene.
- [x] Deterministic pixel snapshots with a bundled font in the opt-in GPU test.
- [x] Per-node change and repaint flags, declare/layout/paint/render timings, `tracing` spans and CSV export.
- [x] F12 inspector window: tree, node details with computed style, accessibility view, event log, frame timeline, selection/damage/repaint overlays.
- [x] Single- and multi-window hosts on one desktop loop.
- [ ] GPU timings, memory statistics, framework diagnostics (relayout/repaint/accessibility warnings) and cross-driver pixel tolerance in CI.

The [milestone report](milestone-5.md) describes the crates, contracts and validation. [Feature coverage](feature-coverage.md) maps all 62 goals of the [original feature list](feature-list.md) to their status.

## 6. Styling, theming and layout depth — next implementation milestone

- Typed style classes, themes, tokens and pseudo-states with runtime theme switching and OS appearance.
- Grid layout, min/max constraints, aspect ratio and baseline alignment.
- Focus scopes, modal focus and dialogs; tooltips, tabs, split panes and context menus.
- Transitions and property animations with GPU-friendly transforms and opacity.

## 7. Advanced features

- Docking, rich text/code editing, BiDi caret order, hot reload and framework diagnostics.
- CPU rendering, outgoing OS drags, gestures and additional platform adapters.

Each phase needs its own acceptance criteria before expanding the public API. Items that appear only in the feature list are tracked in the coverage table.
