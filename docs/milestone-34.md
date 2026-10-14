# Milestone 34: one name per widget, and a key only when it means something

Implemented on 2026-09-22. Two things a newcomer met on the first line of the first example, both of them the framework's doing rather than Rust's.

## Three names per widget

`label`, `label_keyed` and `label_styled` were the same widget three times, and so were `button`, `text_input` and `combo_box`. Rust has no named arguments, so a method cannot grow an optional parameter; what it can take is a value that carries them.

Each of those is now one method whose first argument is either the plain thing or a builder of the same name:

```rust
ui.button("Save");
ui.button(Button::new("Save").style(danger).enabled(!saving));

ui.label("Ready");
ui.label(Label::new(count).style(heading));

ui.text_input("Name", &mut name);
ui.text_input(TextInput::new("Name").placeholder("Type a name").style(wide), &mut name);

ui.combo_box("Theme", &mut choice, &themes);
ui.combo_box(ComboBox::new("Theme").style(wide), &mut choice, &themes);
```

`Label`, `Button`, `TextInput` and `ComboBox` each carry `From<&str>` and `From<String>`, which is what makes the short form the same call as the long one. There are four builders and not one per widget, because a builder exists only where there was a `_styled` variant to absorb.

## Keys everywhere

Every container and every control demanded a key, so a key was on almost every line, and most of them repeated the label beside them: `ui.checkbox("notifications", "Notifications", &mut on)`. A key that is ceremony teaches nothing, and it hid the one case where a key decides something.

A widget is now identified by where it is declared. `Ui::keyed` names one when that is not enough:

```rust
for pane in &panes {
    ui.keyed(pane.id.to_string(), |ui| {
        ui.column(Style::default(), |ui| ui.label(&pane.title));
    });
}
```

That is the rule in full, and it is Flutter's: position and kind identify a widget, an explicit key overrides it. The reconciler already matched on kind, so nothing underneath had to change; what changed is that `WidgetKey::Position` is now the default rather than something only `ui.label` used.

`Ui::dialog` is the one exception and keeps its key. A dialog is pushed to the window's root layer list rather than to the parent it is declared in, so a position in that parent would not identify it, and the key namespaces `$dialog:`. `Ui::request_focus` keeps its argument too, but that names a target to focus rather than declaring anything.

## When a key does matter, it says so

A key built from data that changes every frame throws away focus, scroll offset, selection and undo history for everything under it. The `UnstableIds` diagnostic has always caught it, in the F12 inspector, where a newcomer does not know to look.

It now also prints once, to stderr, in debug builds:

```
rust-gui: unstable ids under page/list
  12 children were recreated in each of the last 12 frames; their keys change
  every frame, so they lose focus, scroll and editing state.
  Give each one a stable key: ui.keyed(item.id, |ui| ..)
  (printed once per place; debug builds only)
```

Release builds are unchanged, and so is the inspector.

## What a call looks like now

| Before | After |
| --- | --- |
| `ui.label_keyed("count", text)` | `ui.label(text)` |
| `ui.label_styled("title", text, style)` | `ui.label(Label::new(text).style(style))` |
| `ui.button_keyed("save", "Save")` | `ui.button("Save")` |
| `ui.button_styled("save", "Save", enabled, style)` | `ui.button(Button::new("Save").enabled(enabled).style(style))` |
| `ui.text_input("name", "Name", &mut name)` | `ui.text_input("Name", &mut name)` |
| `ui.text_input_styled("name", "Name", &mut name, hint, style)` | `ui.text_input(TextInput::new("Name").placeholder(hint).style(style), &mut name)` |
| `ui.checkbox("notify", "Notifications", &mut on)` | `ui.checkbox("Notifications", &mut on)` |
| `ui.slider("volume", "Volume", &mut v, 0.0..=100.0, 5.0)` | `ui.slider("Volume", &mut v, 0.0..=100.0, 5.0)` |
| `ui.column("page", style, content)` | `ui.column(style, content)` |
| `ui.tabs("views", &mut tab, &tabs, content)` | `ui.tabs(&mut tab, &tabs, content)` |

The framework's own composite widgets still name the parts they are made of, a number field's `$field` and its two steppers, a popup's `$popup`. That structure is fixed and internal, so it goes through `pub(crate)` helpers rather than through `Ui::keyed`.

`Ui::keyed` returns what its closure returns, so a keyed control still hands back its `Response`:

```rust
let saved = ui.keyed("save", |ui| ui.button("Save")).clicked();
```

`Label`, `Button`, `TextInput` and `ComboBox` are exported from the crate root.

## Migration

Every call was rewritten by one rule. Tests, the devtools and the host's own tests keep the key they had, because they find nodes by it: `ui.m("key", rest)` became `ui.keyed("key", |ui| ui.m(rest))`, which identifies the node exactly as before. The examples dropped a key that was a string literal and kept `keyed` only for a key computed from data, which is the style this milestone is for.

## The public-API snapshot missed most of the widgets

`crates/rust-gui/tests/api.rs` looked for the modules `ui.rs` declares beside it, in `src/`, rather than in `src/ui/`, so no method declared in `src/ui/*.rs` or `src/tree/*.rs` was ever recorded: `checkbox`, `slider`, `data_grid`, the builders. That is how the signature changes in those files passed the guard. Modules of any file other than `lib.rs`, `mod.rs` and `main.rs` are now read from its own directory, and the snapshot grew by 158 lines.

## Validation

- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` are clean.
- `cargo test --workspace --all-features`: 329 passed, 0 failed, 42 ignored (the GPU suites).
- Every example and `dials` smoke-run with `--smoke` and exit 0.
- `scripts/test-controls.ps1`, `scripts/test-inspector.ps1` and `scripts/test-theming.ps1` pass.

## Remaining limits

- Identity is still the sibling position, so an unkeyed widget after conditional content takes over the state of whatever was at its index before. Identity by call site replaces it next.
- `Ui::dialog` and `Ui::request_focus` still take a name.
