# Milestone 16: commands with scopes, bindings and a palette

Implemented on 2026-09-22. A command was a label, a shortcut and an enabled flag shared by a button. It is now the unit a window is driven by: scoped to where it belongs, bindable by the user, shown in menus with the keys that run it, and searchable in a palette.

| Piece | Contract |
| --- | --- |
| `Command::with_id(id, label, shortcut)` | An identity apart from the label, because labels are translated and a palette has to return something stable. `Command::new(label, shortcut)` still takes the label as the id. |
| `Command::scoped()` | The command fires only while the focus is inside the focus scope, dialog or popover that declares it. Without it a command is the window's, and fires wherever the focus is. |
| Dispatch | Where several enabled commands share a shortcut, the innermost one whose scope holds the focus wins, so a panel or a dialog may take keys the window also uses. Declaring two with the same shortcut in one scope still panics; in two scopes it is allowed. |
| `Context::set_binding(id, Option<Shortcut>)`, `clear_bindings`, `binding(id)` | What the user bound a command to. `None` unbinds it. `Ui::command_button` applies it, and `Ui::command(&command)` gives an app the command as the bindings leave it, for menus and palettes. |
| `Shortcut::describe()` | `Ctrl+Shift+S`, the way a menu writes it. |
| `MenuItem::command(&Command)` | A menu item from a command: its label, whether it is enabled, and its shortcut as the hint. |
| `MenuItem::hint` | Text written along the end edge of an item, mirrored in right-to-left windows, muted, and greyed with the item. A screen reader hears it as part of the item's name (`Save, Ctrl+S`). |
| `Ui::command_palette(key, open, query, commands) -> Option<String>` | Everything the window can do, filtered by what is typed. The app owns whether it is open and the query; the id of the chosen command comes back once and closes it. The field takes the focus as it opens, Enter takes the first command that can run, a disabled command cannot be chosen, and the palette declares no commands of its own, so it never takes a shortcut from the window. |

The palette's own strings are framework messages (`seaglass-palette`, `seaglass-palette-query`, `seaglass-palette-empty`), translated in the shipped Czech and German catalogs.

## Validation

Six tests in `seaglass-core/tests/commands.rs`: how a shortcut is written, including a named key and a menu item built from a command; a scoped command that is unreachable outside its panel while the panel's own command beats the window's when the focus is inside; rebinding, unbinding and clearing, with the menu hint following each change; a palette that lists every command with its keys, filters on what is typed, takes the first enabled match on Enter, reports the choice once, closes and clears, and refuses a command that cannot run; a menu item's hint painted inside the item along its end edge and read as part of its name; and a modal dialog binding the same keys as the window without a duplicate.

The workspace suite is 268 tests; formatting and clippy are clean; all 17 examples pass their smoke run, and `scripts/test-controls.ps1`, which drives menus and combo boxes through UI Automation, still passes.

## Remaining limits

- Nothing writes bindings to disk: an application that lets the user remap keys stores the map itself and calls `set_binding` at start.
- The palette matches on the label alone, without fuzzy matching or recently-used ordering, and shows at most twelve commands.
- A native menu bar is still not a thing the framework provides; `MenuItem::command` feeds the in-window menus.
- Scopes come from focus scopes, dialogs and popovers. There is no way to name a scope that is not one of those.
