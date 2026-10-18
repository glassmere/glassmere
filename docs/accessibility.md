# Accessibility

Every node carries semantics, and the host publishes them to the platform: AccessKit on Windows, macOS and Linux, from the same tree the window draws. Nothing has to be declared twice, and nothing is bolted on afterwards.

## What a node says

```rust
pub struct Semantics {
    pub role: Role,            // Button, Slider, ListBox, TextInput, Dialog, ...
    pub label: String,         // what it is called
    pub disabled: bool,
    pub focusable: bool,       // in the tab order, and takes the keyboard
    pub value: Option<String>, // what it holds, read after the label
    pub numeric: Option<NumericValue>,  // where that sits on a scale
}
```

Built-in controls fill this in. A button's label is its text, a slider reports its value and range, a list reports its rows and which is selected, a toast is a live region so it is announced without stealing focus, a modal dialog hides what is behind it.

A control that ends up with no name is reported as an `Inaccessible` diagnostic, in debug builds, with the path to the node. That is the check that catches an icon button before a user does.

## Naming things

- A control's text is its name. An icon button has none, so give it one: give the button text a label, or a tooltip, which becomes the name whenever the text has no letter or digit in it.
- `VirtualListOptions::label`, `CodeEditorOptions::label` and the other options structs carry the name for controls that have no text of their own.
- Labels are translated through `Ui::tr`; identities are not, which is why `Command::with_id` separates the two.

## The keyboard

Tab and Shift+Tab move between focusable nodes in declaration order. A focus scope traps the keyboard inside a dialog or a panel and restores what had it when the scope goes away. Arrow keys move within a group: a radio set, a list, a menu, a tab strip. Escape dismisses a popup, a menu or a dialog.

Tab and Escape are never consumed by an event area or a widget of the application's own, so nothing can trap the keyboard.

Shortcuts belong to commands, which are also what a menu item and a tray entry activate, so one declaration serves all three.

## Typing

IME preedit is shown inline and committed as one undo step; the host tells the platform where the caret is so the candidate window follows it. Bidirectional text has a caret with affinity, and `CaretMovement` chooses whether Left and Right move on screen or in reading order.

## Testing it without a screen reader

```rust
let mut harness = Harness::app(size, app);
harness.run();
assert_eq!(harness.by_label("Save"), harness.find(&["toolbar", "save"]));
assert!(harness.snapshot_accessibility().contains("Slider \"Gain\" value=\"70%\" number=70.0"));
harness.action(harness.by_label("Save"), Action::Activate);
```

`snapshot_accessibility` prints the whole tree with roles, names, values, checked and selected states and what has focus, which is what a snapshot test should assert on rather than pixels. `by_label` finds a node the way assistive technology would, so a test that cannot find a control is a control a screen reader cannot find either.

`Action` is the platform's vocabulary: `Focus`, `Activate`, `Increment`, `Decrement`, `Expand`, `Collapse`, `SetText`, `SetNumber`, `ScrollIntoView`, `StartDrag`, `Drop`. Everything a pointer can do, an action can do, which is both how assistive technology drives the window and how the tests do.

## A widget of your own

Fill in `Semantics` and it is announced like anything else: `focusable` puts it in the tab order whatever its role, `value` and `numeric` are what is read out, and `Role::Button` opts into the same activation path as a built-in button. A dial that reports neither value nor numeric is a dial that says nothing; `examples/dial-widget` shows one that does.

## What has not been checked

The tree is checked by tests and by UI Automation scripts on Windows. Speech and navigation with a real screen reader, and the platform layers on macOS and Linux, have not been exercised by hand; [limitations](limitations.md) lists the rest.
