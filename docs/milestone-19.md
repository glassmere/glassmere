# Milestone 19: text that starts where it is told, and layout that answers back

Implemented on 2026-09-22. Two coverage rows had short lists of missing pieces rather than missing foundations: text wanted a forced paragraph direction and regular expressions, and layout wanted real baselines, breakpoints and a way to place children by hand.

| Piece | Contract |
| --- | --- |
| `Style::base_direction(Direction)` | Forces the direction a paragraph starts in, whatever its first strong character is, and is inherited like the other text properties. It becomes `TextStyle::base`, which the text system applies as a Unicode isolate around the text, so the bidirectional algorithm starts the way the application asked. Editors leave it unset, because the isolate shifts the byte offsets a caret is placed by. |
| `FindState::regex` | The code editor's find bar reads the query as a regular expression. Case follows the same flag as a plain search; `whole_word` is ignored, since a pattern can say `\b` itself. A match never crosses a line, so `^` and `$` mean the line's edges. |
| `FindState::error()` | Why the last search found nothing, when it was the query's fault. The find bar writes it where the result count goes, and the `Whole word` box is disabled while a pattern is on. |
| The `regex` feature | On `rust-gui-core`, on by default through the facade, like `syntax`. Without it a search that asks for a pattern reports that the feature is off rather than searching for the pattern's text. |
| `FindState::refresh` | Now public, so an application can search a document without showing a find bar. |
| Baselines | `Align::Baseline` lines a row's children up by the baseline of their text, from the font's own ascent, rather than by their boxes. The measurer answers `TextMeasurer::ascent`; the real one shapes a line and takes its metrics, and the synthetic one keeps to four fifths of the size so headless tests stay predictable. A container takes the baseline of its first child that has one, so a label inside a box still lines up. |
| `Ui::responsive(key, style, content)` | A column that tells its content how much room it had on the last frame, which is how a view changes shape with the space it is given. The width is the content box, inside the padding, and zero on the frame the container appears, so a view that changes structure at a threshold settles on the second frame. |
| Custom layout | Place children yourself with `Style::offset`, which takes a node out of flow and puts its corner where you say, inside a `Ui::responsive` container that tells you how much room there is. That is the whole recipe; there is no custom layout trait. |

## Validation

Two tests in `rust-gui-core/tests/search_direction.rs`: a find bar searching by pattern, with case, an anchor meaning the line's edge, a pattern that cannot be compiled reporting why and finding nothing, an empty pattern that is not an error, and a literal search afterwards; and a forced paragraph direction resolved through inheritance, overridden inside, and absent when nobody asks for it.

The layout tests gained a rewritten baseline test, which now asserts what a baseline is: the text of three different sizes, one of them padded, sits on one line, the lowest baseline does not move, the others move down to it, and the bottoms no longer line up. A new test drives `Ui::responsive` through a window that is resized, and shows the first frame knowing nothing, the second changing shape, and the shape changing back when the window is made smaller.

The workspace suite is 284 tests; formatting and clippy are clean, with and without the `regex` feature; the facade builds without default features.

## Remaining limits

- A forced direction is an isolate around the text, so an editor cannot use one without moving its caret offsets; `base` is therefore ignored for editors rather than forbidden.
- Regular expressions search line by line, so a pattern cannot match across a newline.
- Baselines are lined up after layout by shifting children, which means a row's height is measured before the shift and can end a pixel or two short of the lowest descender.
- `Ui::responsive` is a frame behind, which is what any immediate-style framework can offer without laying out twice.
