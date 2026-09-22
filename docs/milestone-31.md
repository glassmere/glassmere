# Milestone 31: caption buttons that belong

Implemented on 2026-09-22. `Ui::window_buttons` drew three rounded pills with an en dash, a square and a cross in them, which read as an application's buttons sitting where the system's ought to be. They are now the system's own, and they answer the pointer the way the system's do.

| Piece | Contract |
| --- | --- |
| Glyphs | On Windows, the shell's own: minimize `U+E921`, maximize `U+E922`, restore `U+E923` and close `U+E8BB` from Segoe Fluent Icons, at ten points, which is what the shell uses. Everywhere else they stay plain characters every font has. |
| At rest | Nothing: no fill, no corner radius. A title bar an application draws is the background here, not a strip of buttons. |
| Hovered and pressed | A wash of the text colour, a tenth of it under the pointer and a fifth while held, so it is light over a dark title bar and dark over a light one, whatever the theme. |
| Closing | Red under the pointer with a white glyph, darker while held. That is the one thing every desktop says the same way, so it is not a theme token. |
| The box | Each button hugs its glyph with nine pixels around it, because that is what centres it: a text leaf is drawn from its content corner, so a box given a size larger than its text puts the glyph in the corner. The shell's buttons are wider than tall; these are square. |
| Maximize | Hovered like the others, from the hover Windows reports through the window procedure, and pressed by Windows itself, which is why it has no pressed state of its own. |

## Validation

Photographed on a real window, one capture per state, in `target/caption-*.png`: at rest the three glyphs alone; minimize and close washed under the pointer and washed harder while held; close red, then darker, with a white cross; and maximize washed under the pointer with the snap layouts flyout hanging below it.

`scripts/test-titlebar.ps1` still passes end to end, including finding each button by its accessible name, which is the tooltip rather than the glyph: a private-use character says nothing to a screen reader.

The workspace suite is 326 tests; formatting and clippy with all features are clean; all 20 default examples pass their smoke run. The window is `target/titlebar-window.png`.

## Remaining limits

- The buttons are square where the shell's are 46 by 32. Matching that needs a text leaf that can be told to centre itself, which the layout does not offer.
- Segoe Fluent Icons is named outright. Windows 10 has the same code points in Segoe MDL2 Assets and the text system falls back to it, reporting the missing family as a font diagnostic on the way.
- The focus ring is the framework's, not the shell's, because a caption button that can be reached by Tab has to show it.
- There is no pressed state for maximize, and cannot be: Windows takes the press.
