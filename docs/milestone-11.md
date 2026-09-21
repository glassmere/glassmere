# Milestone 11: localization

Row 39 of the coverage table had right-to-left layout and locale detection but no translated text. Decisions taken at the start: Fluent catalogs through the fluent crates; `ui.tr` plus a compile-time checked macro; the framework's own strings as overridable keys with built-in English and shipped Czech and German; full CLDR formatting through ICU4X behind a feature. Slices: 11a messages and Fluent, 11b the framework's own strings (both below), 11c the `tr!` macro, 11d ICU4X numbers and dates.

# Milestone 11a: messages and Fluent catalogs

Implemented on 2026-09-21. The core knows how to ask for text and nothing about formats; a new crate answers with Fluent.

## Run

```powershell
cargo run -p rust-gui --example localization --release
cargo run -p rust-gui --example localization --release -- --locale cs-CZ
```

Three catalogs under `crates/rust-gui/examples/locales`, a language switch, plurals, and framework controls that follow the language. German leaves one key untranslated to show the fallback.

## Use

```rust
let mut messages = rust_gui::fluent::FluentMessages::new("cs-CZ", "en")?;   // tries cs-CZ, cs, en
messages.load_dir("locales")?;                         // locales/cs.ftl, locales/en/*.ftl, ...
ui.set_messages(Some(Box::new(messages)));             // at start, or later to switch language

ui.label(ui.tr("title"));
ui.label(ui.tr_args("files", &[("count", 3.into())])); // "Vybrány 3 soubory"
```

## Contracts

| Piece | Contract |
| --- | --- |
| `Messages` (core) | A trait: `locale()` and `text(key, args) -> Option<String>`. The core has no format of its own, so any source can implement it. `MessageArg` is text or a number; numbers select plural forms. It converts from strings and every number type. |
| Lookup | `Ui::tr(key)`, `Ui::tr_args(key, args)`, `Context::tr`. The installed messages answer first, then the framework's built-in English, and otherwise the key itself is shown, which makes a missing text visible without breaking the app. |
| Missing keys | `Context::missing_messages()` lists each key that had no text anywhere, in first-seen order, at most 256, since the messages were last installed. |
| Installing and switching | `Ui::set_messages(Some(..))` from the view, or `Context::set_messages` from a host. The change applies after the declaration that asked, everything repaints, and the host declares again before presenting. `Ui::locale()` and `Context::locale()` give the tag, `en` without messages. |
| `rust-gui-fluent` (`rust_gui::fluent`, feature `fluent`, on by default) | `FluentMessages::new(locale, fallback)` builds the chain: the locale, its bare language, the fallback. `add(locale, source)` adds a catalog to one locale of the chain, later ones overriding earlier ones key by key, and returns false for a locale outside the chain so a loader can offer every file it has. `load_dir(dir)` reads `<dir>/<locale>.ftl` and `<dir>/<locale>/*.ftl` in name order for each locale of the chain. |
| Errors | `FluentError` has the locale, the file, the line where the broken entry starts and the parser's message, and prints as `file:line: message`. A tag that is not a language tag is refused. |
| Plurals | Fluent's CLDR plural rules: Czech one, few and other are tested. |
| Isolation marks | Off by default. Fluent normally wraps arguments in Unicode directional isolates, which are right for mixed-direction text but show as boxes in fonts that lack them; `FluentMessages::isolating(true)` turns them on. |
| Host | The desktop host and the test harness now declare again while activations keep being consumed, at most three more times, instead of exactly once. |

That last row is a fix found with the example: a click on a language button was consumed by one declaration, the view asked for the new messages in the host's single follow-up declaration, and nothing declared a third time, so the window stayed in the old language until the next event. A button that switches the theme had the same flaw.

# Milestone 11b: the framework's own strings

Implemented on 2026-09-21. Every text the framework itself shows or announces is a key with built-in English, and translations ship for Czech and German.

| Piece | Contract |
| --- | --- |
| `BUILTIN_MESSAGES` (core) | Twenty-seven keys, all starting with `rust-gui-`: toast dismiss, the number input's buttons and "Enter a number.", the grid's header menu, resize and filter names, the property grid's columns, group and reset tooltips, the splitter and floating panel names, tab close and the find bar. Arguments are written `{ $name }` as in Fluent; the core substitutes them itself for the built-in English. |
| Overriding | A catalog that defines one of these keys wins, in any language including English. |
| Shipped translations | `rust_gui_fluent::FRAMEWORK_CATALOGS` holds Czech and German. `FluentMessages::new` adds them first when the chain contains that language, so app catalogs still override them. Other languages fall back to English until someone writes the file. |
| Accessible names made by the engine | "Splitter", "Floating panel" and the two floating panel handles are translated where node snapshots are built, because a node alone cannot reach the messages. |

Not localizable: the default `GridModel::set_cell` error, "This grid is read-only.", which a trait default cannot look up; a model that is read-only by accident shows it in English.

## Validation

Eight new tests. Core: substitution in built-in texts, and unique prefixed keys. Fluent crate: Czech plurals, fallback along the chain and a regional override; catalog text, shipped Czech, built-in English for a language without a catalog, the key for an unknown one, and the missing list cleared by a new language; the line of a broken entry, a bad tag, and directory loading in name order; a view installing and switching its language with the host asked to declare again; the shipped catalogs translating every built-in key, using every argument and nothing unknown; and a property grid, a number input and a toast showing Czech names.

The full suite is 226 tests plus 18 GPU tests; formatting and clippy (all features, and core alone) are clean; the facade builds without default features and with `desktop` alone; the localization, controls, theming, grid and properties examples pass `--smoke` and the theming script passes. In the real localization example one click on Čeština switched every text, three files read "Vybrány 3 soubory", and saving raised the toast "Uloženo: report.pdf".

The German and Czech texts were written by me and have not been reviewed by native-speaking translators; treat them as a starting point.

## Remaining limits

- Catalogs are loaded when the app asks; nothing reloads them when a file changes. `Ui::watched_text` plus `add` does it by hand.
- Numbers inside messages are formatted plainly until 11d.
- Missing keys are listed by the context but not yet shown in the inspector.
- No message for a date, list or relative time yet: 11d.
