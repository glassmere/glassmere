# Styling

A `Style` belongs to one node, a `Theme` to a window, and a `StyleSheet` names styles so they can live in a file. Everything here is plain data; none of it reaches layout unless a size changes.

## Style

`Style` is a builder. Sizes are `Length::Auto`, `Px(f32)` or `Percent(f32)`:

```rust
let card = Style::default()
    .width(Length::Percent(1.0))
    .padding(12.0)
    .gap(8.0)
    .radius(8.0)
    .background(ui.theme().surface)
    .shadow(Shadow::lift(Color::rgba(0, 0, 0, 80), 12.0, 4.0));
```

Layout is Taffy's: flexbox for `row` and `column`, grid for `Ui::grid` with `columns`, `rows`, `auto_columns`, `auto_rows`, `auto_flow`, `areas` and `area`. Children do not shrink unless `shrink` is set. Percentage widths on a grid item resolve against the grid container, not the cell; leave `width` at `Auto` to fill a cell. Baseline alignment lines up text bottoms, not font baselines.

A grid has the CSS shapes for its tracks and placement:

- `Track::auto_fill(tracks)` and `Track::auto_fit(tracks)` repeat as many times as fit. Their tracks need a definite size (`Px`, `Percent`, or a `MinMax` with one), so `auto_fill(vec![Track::MinMax(TrackSize::Px(120.0), TrackSize::Fr(1.0))])` makes as many columns of at least 120 as the width holds. `auto_fit` collapses repetitions no child landed in, so the others grow into the space; `auto_fill` keeps them. `Track::repeat(n, tracks)` repeats a fixed count and takes `MinMax` inside. Repeats apply to `columns` and `rows`, not to `auto_columns` and `auto_rows`, and do not nest.
- `row_gap` and `column_gap` override `gap` on one axis; `gap` sets both.
- `auto_flow` is `AutoFlow::Row`, `Column`, `RowDense` or `ColumnDense`. Dense packing lets a later, smaller child fill a hole an earlier larger one left, so children can end up out of order. It matters for children that span tracks without a fixed position: `GridArea::auto_span(columns, rows)`, or a `GridArea` with a line of 0 on one axis.
- `areas` names regions, one string per row: `.areas(["head head", "side body", "foot ."])`, with `.` for an empty cell. A child picks one with `area_name("body".to_owned())`, wherever it is declared. Every row needs the same number of cells and a name must cover one rectangle; otherwise `Ui::grid` reports misuse. A name with no matching area falls back to implicit lines.

Text properties (`foreground`, `font_size`, `font_family`, `font_weight`, `italic`, `wrap`) are optional and inherit from the parent. Padding, gap and `wrap` do not inherit. Backgrounds default to a theme token per widget kind.

Pointer and focus states are overrides on top of the base style: `on_hover`, `on_pressed`, `on_focused` and `on_disabled`, each taking a `Paint` (background, foreground, radius, opacity, transform). They repaint; they never lay out again.

### The focus ring

A focused widget outlines itself with a ring: a stroke 2 px wide in the theme's `focus` colour, drawn inside the widget's box along its rounded edge, over its background and gradient. A widget with square corners gets four bars, which land exactly on pixels.

The ring follows how focus was given, as it does in a browser. A widget that takes no typed text shows it after the keyboard, Tab and the arrow keys, and not after a click or a touch; the next key that does something brings it back, and the next press hides it again. A field you type into always shows it, because it is where the text goes. A window shows it from the start, until the first press.

`Style::focus_ring(FocusRing::new(3.0).color(color).inset(2.0))` sets the width, the colour and how far the ring sits inside the edge, for one widget; `Style::no_focus_ring()` draws none, for a widget that shows focus some other way. The ring stays inside the widget's box, which is what is repainted when focus moves; outside it, it would be left behind. The colour of the default ring is the theme's `focus` token.

### What takes focus

Buttons, fields, checkboxes, sliders, list rows, tabs, links and the like take focus from a click and from Tab. A container that only lays things out does not: a column, a row, a grid, a pane, a spacer. Clicking one of them, or empty space inside a scroll area, focuses nothing.

Two kinds are in between. A scroll container takes focus only while it scrolls, because the arrow keys and Page Up and Page Down act on the one that has it; one that fits its content is layout, and is skipped. A pane divider takes focus so that the arrow keys, Home and End can move it, and shows its ring after the keyboard like anything else.

`Style::focusable(false)` takes any widget that would take focus out of the focus order, and a click on it leaves focus where it was. It still reacts to the pointer: a button with `focusable(false)` can be clicked and cannot be tabbed to. Use it for what is only for the mouse, or what shows its state some other way.

## Theme

`Theme::dark()` and `Theme::light()` are the two built in. Every colour is a named token:

`window`, `surface`, `popup`, `control`, `control-hover`, `control-pressed`, `control-disabled`, `text`, `text-muted`, `text-disabled`, `accent`, `focus`, `selection`, `highlight`, `selection-inactive`, `row`, `row-alternate`, `track`, `thumb`, `scrollbar`, `scrim`.

`Theme::token` and `Theme::set_token` read and write them by name. `Effects::set_theme` changes the theme at run time. `AppOptions::theme` is `ThemeChoice::System` (follows the OS light or dark setting) or `ThemeChoice::Fixed(theme)`.

A theme carries colours and the base font. Spacing, radii and control sizes are per-style values. Quads and custom widgets that hard-code `Color::rgb` do not follow a theme switch; read `ui.theme()` instead.

## Stylesheets

`AppOptions::stylesheet` names a file that is installed at start and reloaded within about a tenth of a second of a change (about half a second when nothing has changed for two minutes), except that a sheet that is empty or missing is taken up only half a second after the change, so that a save in progress is not shown; a sheet that fails to parse is reported and the last good one stays. `StyleSheet::parse` reads text directly, and `style_sheet!("styles/app.style")` parses a file while the crate compiles, so an unknown property is a build error naming the line. `ui.class("name")` returns the style of a class and records a warning when it is missing.

```text
// comments start with //
theme dark                  // optional base; tokens overlay whichever theme is active
token accent #1a9c8f

class primary {
    background: #1a9c8f
    foreground: #ffffff
    padding: 12
    radius: 8
    hover.background: #22b0a0
    pressed.background: #157d72
}
class danger extends primary { background: #c0392b }
```

One property per line, or separated by `;`. Colours are `#rrggbb` or `#rrggbbaa`. A token overlays whichever theme is active, so an OS light and dark switch keeps the application's accent.

### Variables

A line outside any class declares a variable, `--name: value`, with one or more declarations per line separated by `;`. A name is `--` followed by letters, digits, `-` or `_`. `var(--name)` stands for its value wherever a value is written: in a property, in a `token` line, or in another variable. `var(--name, fallback)` uses the fallback, which may itself contain `var()`, when no variable of that name has been declared.

Variables are read from the top of the sheet down, with one scope for the whole sheet, and they are replaced before the value is parsed. So a variable can use only the ones declared above it, a later declaration of the same name replaces the value for the lines below it, and a variable that uses itself is an error. Using one before it is declared is an error naming the line (`unknown variable`) unless a fallback is given. A value is checked where it is used: `--size: wide` is fine until a `padding: var(--size)` reads it. Text inside double quotes is left alone, so `"var(--x)"` is a font name.

Variables are plain text for the sheet and not theme tokens. `var()` never reads a token, and a class holds literal colours, so a colour taken from a variable does not follow a theme switch; the value of `token accent var(--brand)` is the one the sheet gives it, as `token accent #1a9c8f` would be. There is no per-class scope and no cascade.

### Nesting

A class may contain `&:state` and `&.name` blocks, on their own lines or on one line with the class:

```text
--brand: #1a9c8f
--brand-light: #22b0a0

class card {
    background: var(--brand)
    padding: 12
    &:hover { background: var(--brand-light) }   // the same as `hover.background: ...`
    &:disabled {
        background: #3a4a4f
        opacity: 0.6
    }
    &.selected {                                  // the class `card.selected`
        background: #ffffff
        &:hover { background: #e6fffb }           // the state of `card.selected`
        &.compact { padding: 6 }                  // the class `card.selected.compact`
    }
}
```

- `&:hover`, `&:pressed`, `&:focused` and `&:disabled` set that state's properties, the ones a `hover.`-prefixed line can set (`background`, `foreground`, `radius`, `opacity`, `translate`, `scale`, `rotate`), without the prefix. A state block holds properties only.
- `&.name` is the class `parent.name`, written as `class parent.name extends parent { ... }` would be, and found with `ui.class("card.selected")`. It starts from the finished parent, whatever lines come after the block, and its own lines override it. `name` is letters, digits, `-` and `_`. Like `extends`, it is a copy: a class that extends `card` does not get `card.selected`.
- In one class, `&:state` blocks and `hover.`-prefixed lines apply in the order written, so the later line of a property wins. A second `&.name` block with the same name replaces the first, as two classes of one name do.

A nested rule that does not start with `&:` or `&.` is an error. A class is a named style, not a selector matched against a tree, so there are no descendant or child selectors (`.title { }`, `& .title { }`, `&>.title { }`), no `&.a.b` or `&.a:hover` in one block (nest `&:hover` inside `&.a`), no `&` elsewhere in a selector, and no nested `class`. Nesting is flattened into ordinary classes while the sheet is parsed.

| Group | Properties |
| --- | --- |
| Size | `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height` (`auto`, `50%`, `120`), `aspect-ratio`, `grow`, `shrink` |
| Spacing and shape | `padding`, `gap`, `radius`, `opacity` |
| Alignment | `align-items`, `align-self`, `justify-self` (`start`, `center`, `end`, `stretch`, `baseline`), `justify-content` (also `space-between`, `space-around`, `space-evenly`) |
| Grid | `columns`, `rows`, `auto-columns`, `auto-rows` (`120 50% 1fr auto minmax(100, 1fr) repeat(3, 1fr) repeat(auto-fill, minmax(120, 1fr))`), `row-gap`, `column-gap` (`gap` sets both), `auto-flow` (`row`, `column`, `row dense`, `column dense`), `areas` (`"head head" "side body"`, `.` for an empty cell), `area` (`column row [colspan rowspan]`, `auto` for a line left to auto-flow, or an area name such as `body`) |
| Colour | `background`, `foreground`, `shadow` (`x y blur colour [spread] [inset]`), `gradient` (`x1 y1 x2 y2 colour colour`, or `radial ...`) |
| Text | `font-size`, `font-weight`, `font-family` (`sans`, `serif`, `monospace`, `"Name"`), `italic`, `wrap` (`word`, `none`), `direction` (`ltr`, `rtl`) |
| Image | `background-image` (`url(panel.png)` relative to the sheet, or `none`), `background-fit` (`cover`, `contain`, `fill`, `none`), `background-slice` (nine-slice insets in image pixels: one, two or four numbers), `background-tint` (`foreground`, a colour, `none`) |
| Motion | `translate` (`x y`), `scale` (`factor` or `x y`), `rotate` (clockwise degrees), `transition` and `layout-transition` (`150ms ease-out [delay]`), and `enter-` / `exit-` prefixed `opacity`, `translate`, `scale`, `rotate`, `transition` |
| States | `hover.`, `pressed.`, `focused.`, `disabled.` before `background`, `foreground`, `radius`, `opacity`, `translate`, `scale`, `rotate` |

Easings are `linear`, `ease-in`, `ease-out`, `ease-in-out` and `cubic-bezier(x1, y1, x2, y2)`. A state cannot swap the background image; tint through a state `foreground` instead.

## Motion

`Style::transition` eases a node's background, foreground, radius, opacity and transform when they change. One transition covers all of them; there is no per-property timing and no spring. `layout_transition` animates position and size changes relative to the parent. `enter` and `exit` take an `Effect` (`fade`, `slide`, `zoom`); an exiting node is drawn as an image until it finishes, so it does not reflow. `Style::animation` runs looping `Keyframe`s (`Animation::pulse`, `spin`, or `Animation::new` with keyframes). Gradients and shadows are not animated.

Ancestors clip to their bounds, so a node that slides or scales needs room inside its parent. A transformed layer is a bilinear-filtered image, so text softens while it is scaled or turned.

`AppOptions::reduced_motion` skips animations. `true` forces them off, and then the system is not asked; `false` follows the system. On Windows the host reads the "Animation effects" switch (`SPI_GETCLIENTAREAANIMATION`) at startup and whenever a window gains focus, and skips animations while it is off. On Linux and the BSDs it asks `gsettings get org.gnome.desktop.interface enable-animations` on a background thread at startup and, at most once a second, whenever a window gains focus, and treats `false` as reduced motion, whatever the desktop; without `gsettings`, that setting or a session bus there is no preference. KDE Plasma's own animation setting is not read and macOS has no preference read yet, so only the application option applies where there is no answer. `follow_system_reduced_motion(false)`, called before `run`, or `GLASSMERE_SYSTEM_MOTION=off` in the environment keeps the host from asking the system on any platform (on Linux and the BSDs that is what starts `gsettings`), and the option alone decides. Otherwise an application cannot turn animations on against the system.

## Images and backgrounds

`Ui::image(&Image, ImageOptions)` and `Ui::image_file(path, options)` show an image; `ImageFit` is `Contain`, `Cover`, `Fill` or `None`, and `ImageTint` is `None`, `Foreground` or `Color`. `glassmere::image::decode` and `load` produce an `Image` from PNG, JPEG, WebP, GIF and SVG; EXIF orientation is applied, and an embedded RGB or grey ICC profile is converted to sRGB (CMYK profiles are ignored, and pixels without a profile are taken as sRGB). `image_file` loads on a background thread and polls the file's modification time and size, so replacing the file updates the window.

An animated GIF, WebP or APNG decodes to an animated image: `Image::frames` lists the full-picture stills with their delays and `Image::loops` the number of plays (`None` forever), and `Image::animated` builds one from your own stills. `Ui::image` and `Ui::image_file` play it from the moment it is first shown. The window sleeps between frames, wakes at each frame's deadline and repaints only the image's own rectangle; an image scrolled out of view does not run, and with reduced motion the first frame stays. After the last loop it rests on the last frame. A decoded animation keeps all its frames in memory, so one is kept only while they add up to 32 MiB (a 480 by 270 GIF keeps about 64 frames); a longer one shows its first frame, as a still image. A background image always shows the first frame.

`Style::background_image(BackgroundImage)` paints an image over the background colour and under the content, fitted like an image widget or nine-sliced with `BackgroundImage::slice` so one small frame skins boxes of any size. One background image per node; no tiling, and no position other than centred.

There is no mipmap chain, but a photo drawn at under half its natural size is box-filtered down in halving steps, so it does not alias; shrinking large photos first only saves that work, which runs on the render thread the first time each step is drawn. A vector image is rasterised on the render thread the first time each size is drawn.

## Direction

A window runs left to right or right to left according to `AppOptions::direction` (`DirectionChoice::Auto` follows the OS locale). `Style::direction` overrides it for a subtree: layout mirrors, text aligns to the end side, and directional internals follow. Physical values stay physical: `offset`, transforms, slide effects, floating dock rectangles and `DockZone::Left` and `Right` mean the same pixels in both directions. `Style::base_direction` forces the base direction of a text paragraph, in labels and in text fields, text areas, code editors and rich editors (every line or block takes it; a text field and a rich editor align to it, a text area and a code editor only order by it).
