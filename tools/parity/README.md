# Parity bench

Compares glassmere with the web (Chromium, the engine behind Tauri's WebView2 on Windows) and with Qt,
on behaviour as well as looks. It answers "does glassmere do what a Tauri or Qt app does here?" with
data, not impressions.

One set of **scenarios** (`scenarios/*.json`) describes a small UI and a script of mouse and keyboard
steps. Three **runners** build that UI with each framework's own widgets, perform the same steps with
real input, and write what they observed. The **report** puts the three side by side and marks where
glassmere (or Qt) differs from the web.

| Runner | How it drives the UI | What it reads back |
| --- | --- | --- |
| `web/run.mjs`, `web/perf.mjs` | Playwright and Chromium: real mouse and keyboard | the DOM, `:focus-visible`, the browser's accessibility tree, computed cursors, screenshots |
| `qt/run.py`, `qt/perf.py` | PySide6 on the `offscreen` platform, `QTest` input | `QApplication.focusWidget()`, `QAccessible`, widget cursors, `QWidget.grab()` |
| `glassmere/` | `Context::event` and `AppDriver`, as a host does | the node tree, accessibility snapshots, `Context::cursor`, the scene, the CPU renderer |

No window or display is needed for any of them.

## Running it

```sh
cd tools/parity
npm install && npx playwright install chromium     # the web side
python -m pip install -r requirements.txt          # the Qt side (optional)
node run.mjs                                       # everything installed, then the report
node run.mjs --skip qt                             # leave one out
```

On Linux with no display the Qt runner uses the `offscreen` platform and needs `libegl1`, `libgl1`,
`libxkbcommon0` and `libfontconfig1`. `PARITY_PYTHON` names the Python that has PySide6. Open
`out/report.html`: one file with the screenshots in it.

Each runner can be run alone: `node web/run.mjs --out out/web [scenario ids]`,
`python qt/run.py --out out/qt [ids]`,
`cargo run --release --manifest-path glassmere/Cargo.toml -- --out out/glassmere [ids]`.
`node report.mjs out --list` prints the behavioural differences as text.

The glassmere runner is not a member of the workspace, so it adds nothing to CI. It uses the cursor,
focus-ring, `focusable` and custom-widget changes of the 0.1.9 branch, so build it from a tree that
has them.

## What is covered

Widgets and input: buttons, toggles, text input and editing from the keyboard (word-wise delete and move, selecting to the ends, select all), slider, scrolling, focus order.
Layout and parents: alignment and distribution, flex grow and shrink (including what each toolkit
does by default), percentage sizes, margin, border and padding per side, `align-content` for wrapped lines, per-child `align-self`, minimum and maximum sizes and aspect ratio, nested percentages, row and column gaps, `space-between` with margins, `flex-basis` (`flex: 1` against an automatic basis), `margin: auto` (to the end, centred, shared), reversed rows and columns (`row-reverse`, with justify), a panel drawn over a button (which one a click reaches, with and without `blocks_pointer`), grids (fixed, fractional and auto tracks, spans, automatic placement), a disabled parent, a hidden child, `visibility: hidden` (the space is kept, Tab skips it, a click reaches nothing; the one difference is the Tab wrap below), `overflow: visible` (a click on the part that hangs over a parent reaches the child), `line-height` as a multiple of a pixel font size (the height of a one-line box agrees whatever the font; `letter-spacing` is set there too but its effect on width depends on the font, so only the shaper's own test checks it), a number field (arrow steps, the limits, letters typed into it), a modal dialog (focus goes to the first button, Tab stays inside, Escape closes it and focus returns to the button that opened it; what is outside it is read back as ignored by assistive technology, as the tree marks the dialog modal), overlapping
widgets and stacking order (`z-index`), and a child larger than its parent.
Performance: a long list of rows, built the ways each toolkit offers (see below).

Where a toolkit cannot express a property (Qt has no wrapping row, percentage size
or per-widget margin) the scenario says so and the property is left out, so the
report lists it as a limitation instead of a difference.

## What is compared

For every `observe` step: which widget has focus, whether a focus ring is shown on it, the value of
every control, the events the application received, how far each scroll area has scrolled, the
accessibility role, name, checked and disabled state of every interactive widget, and where every box
sits. `cursor` steps record the pointer shape over each target; `tabSequence` steps record the focus
order.

- **Layout** is compared only for boxes with a fixed size (panels, spacers, containers). Fonts differ
  between frameworks, so the size of anything with text in it is shown but not flagged.
- **An omitted `gap` or `padding`** is each framework's own default (web 0, Qt the style's, glassmere
  12 for a gap). That is deliberate: the `layout` scenario exists to show it.
- **Scroll distance** is informational: a wheel notch or an arrow key scrolls a different distance in
  every toolkit. Only whether it moved at all is compared.

## Performance

`node run.mjs` also times a list of 100, 1,000 and 5,000 rows (a label, a button and a check box
each, in a 400 px scroll area): the first frame, one label changing, and one scroll step.
`PARITY_ROWS=100,1000 node run.mjs` changes the sizes. glassmere is measured three ways because how a
list is built is most of its cost: plain (the view declares every row each frame), plain inside
`Ui::memo`, and a virtual list. `PARITY_DIAG=1` prints glassmere's own frame statistics for the last
scroll step. The web figure is main-thread time from the DevTools protocol and leaves out raster,
which runs on other threads; Qt's is wall time for `QWidget.grab()`; glassmere's is a whole frame
plus a full software raster. Read the ratios, not the digits.

## Limits of the measurement

- Chromium stands in for Tauri. Tauri on macOS and Linux uses WebKit, which differs in places
  (`:focus-visible`, scroll-snapping, cursors).
- Qt draws its focus frame from the style and cannot be asked whether it is showing, so the ring is
  not compared for Qt. Qt Widgets has no switch; a check box stands in.
- A web page's `cursor: auto` is the browser's choice. The runner reads it as an I-beam over text and
  the arrow elsewhere.
- Qt gives the first control focus when its window is shown, as a real Qt application does, so its
  tab order starts one step in.
- The web side uses plain, unstyled elements. A Tauri app that styles its buttons (a hand cursor, its
  own focus ring) will differ from this baseline in exactly those places.

## Adding a scenario

Write `scenarios/<id>.json`: a `size`, a `ui` tree and `steps`.

Widgets: `column`, `row` (`gap`, `padding`, `align`, `justify`), `stack` (children placed by
`position` and ordered by `z`), `scroll`, `panel`, `spacer` (`width`, `height`, `background`),
`label`, `button`, `checkbox`, `switch`, `radiogroup` (`options`, `value`), `input` (`value`,
`label`), `slider` (`min`, `max`, `step`, `value`, `label`). Every widget takes an `id`.

Properties on any widget: `width`, `height`, `widthPercent`, `heightPercent`, `minWidth`, `maxWidth`,
`grow`, `shrink`, `margin`, `border`, `overflow`, `disabled` (on a container, disables what is inside
it) and `visible: false`. List the ones a scenario depends on in `features` so a toolkit that cannot
express them is reported as unable to, not as different.

Steps: `observe`, `click`, `clickAt` (`fx`, `fy`), `hover`, `dragOff`, `wheel` (`dy`), `press` (`Tab`,
`Shift+Tab`, `Enter`, `Space`, arrows, `Home`, `End`, `Backspace`, `Control+a`), `type`, `cursor`
(`targets`) and `tabSequence` (`count`, `shift`). Nothing else has to change: all three runners read
the same file. A new widget or step type has to be taught to each runner once.
