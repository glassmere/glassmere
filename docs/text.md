# Text, editing and localization

## Fields

`Ui::text_input(label, &value, |text| Msg::Name(text))` is a single-line field. The application owns the string; the field keeps the caret, selection, IME composition and a bounded undo history. `Ui::number_input(label, value, NumberOptions, |n| ..)` commits a parsed `f64` on Enter, on leaving the field, on a stepper click and on Up and Down. `NumberOptions` has `range`, `step`, `decimals` and `suffix`; Escape does not revert a draft. Values beyond 2^53 lose precision. There is no password field.

`Autocomplete` and `suggest` back a field with a list of candidates; `ComboBox` and `Ui::menu` take indices rather than values.

## Documents

Multi-line editors work on a document the application owns and the view only reads. Each edit arrives as one operation in the order it happened, and `update` applies it:

```rust
Msg::Edit(op) => self.document.apply_with(op, effects),
// view:
ui.code_editor(&self.document, CodeEditorOptions::default(), Msg::Edit);
```

| Document | Operation | Widget | Notes |
| --- | --- | --- | --- |
| `TextDocument` | `TextOp` | `Ui::text_area`, `Ui::code_editor` | Rope-backed plain text with multiple cursors, undo, line operations and bracket matching. `TextDocument::new`, `from_text`, `text`. |
| `RichDocument` | `RichOp` | `Ui::rich_editor` | Blocks (paragraphs, headings, lists, quotes, code) with bold, italic, underline, strikethrough, inline code and links. Loaded and saved with `from_markdown` and `to_markdown`. |

`apply_with` takes `Effects` because a copy or cut goes to the clipboard in the same batch. `TextDocument::apply` and `RichDocument::apply` do the same without a clipboard.

`CodeEditorOptions` carries `label`, `style`, `font_size`, `line_numbers`, `read_only`, `auto_close`, `match_brackets`, `highlighter`, `diagnostics` and `find`. `text_area` is the same editor without the frame, and has no find bar.

- **Find and replace.** Give `CodeEditorOptions::find` a `FindState` the application owns and apply every `TextOp` with `FindState::apply` instead. Ctrl+F opens the bar and Ctrl+H adds replace. `FindState::matches` and `error` are read by the view. Searching is by line; with the `regex` feature (on by default) a pattern cannot match across a newline.
- **Highlighting.** `seaglass::syntax::SyntectHighlighter::new("rs", dark)` implements `Highlighter`. The grammars load on the first highlight.
- **Diagnostics.** `CodeEditorOptions::diagnostics` takes `Diagnostic`s the application computed; the severity colours are fixed.
- **Keys.** Tab indents and Escape followed by Tab leaves the editor. Ctrl+D selects the next occurrence, Alt+click adds a cursor, Ctrl+Alt+Up and Down add cursors by line, F3 finds the next match.

Rich editors shape every block on each change, so very long documents slow down. Paste is plain text, and links can be set from code but there is no link dialog.

## Caret and IME

IME preedit is shown inline and committed as one undo step; the host tells the platform where the caret is. The caret has affinity, so one offset can sit on either side of a direction boundary or a wrapped line. `AppOptions::caret_movement` (`CaretMovement::Visual` or `Logical`) chooses whether Left and Right follow the screen or reading order, and `Context::set_caret_movement` changes it at run time. A touch in an editor places the caret; there are no selection handles.

## Clipboard

`Effects::copy(Clip)` and `Effects::paste(|clip| Msg::Pasted(clip))`. A `Clip` holds text, an image, or a private format added with `Clip::custom(format, bytes)`. The private format is kept beside the text it was copied with and comes back only while that text is still on the clipboard. Text and an image cannot be copied together; the image wins.

## Localization

Texts come from a `Messages` source the application installs with `Effects::set_messages(Some(Box::new(..)))`; installing `None` goes back to the built-in English. Lookup order for `ui.tr(key)` and `ui.tr_args(key, &[("count", 3.into())])` is the installed messages, then the framework's own English, then the key itself, which `Context::missing_messages` then lists.

`seaglass::fluent::FluentMessages` reads Fluent catalogs:

```rust
let mut messages = FluentMessages::new("cs", "en")?; // locale, fallback
messages.load_dir("locales")?;                         // en.ftl, cs.ftl, ...
effects.set_messages(Some(Box::new(messages)));
```

`add(locale, source)` adds one catalog from a string and `isolating(false)` turns off the direction marks Fluent puts around arguments. Catalogs are loaded when asked; nothing reloads them on change (`Ui::watched_text` plus `add` does it by hand). Numbers passed as arguments select plural forms and are formatted plainly by Fluent.

`tr!(ui, "files", count = n)` is `tr_args` checked at compile time. Name the default catalog in the crate's manifest:

```toml
[package.metadata.seaglass]
messages = "locales/en.ftl"
```

An unknown key, an argument the message does not use and a variable the call does not pass are build errors. Only the default catalog is checked, keys starting with `seaglass-` are accepted unchecked, and without the manifest entry the macro checks nothing.

### Numbers, dates and times

`Effects::set_format(Some(Box::new(format)))` installs a `LocaleFormat`; `ui.format_number`, `format_date` and `format_time` use it, and so do `number_input` and the grid's number cells. Without one, numbers are plain, a decimal comma is accepted when typing, dates read `2026-09-21` and times `14:05`. A model's own cell text is the application's, so a grid shows numbers the way the model wrote them.

With the `icu` feature, `seaglass::icu::IcuFormat::new("cs-CZ")` answers from CLDR data: grouping and native digits, dates in `DateStyle::Short`, `Medium` and `Long`, and times in the locale's 12 or 24 hour convention. Locale tags may carry extensions such as `th-u-nu-thai`. It adds about a megabyte and a large dependency tree. There are no relative times, lists, currencies, units, time zones or calendars other than Gregorian.

Installing messages or a format takes effect from the next declaration, and everything repaints.
