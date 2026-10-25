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

## Theme

`Theme::dark()` and `Theme::light()` are the two built in. Every colour is a named token:

`window`, `surface`, `popup`, `control`, `control-hover`, `control-pressed`, `control-disabled`, `text`, `text-muted`, `text-disabled`, `accent`, `focus`, `selection`, `highlight`, `selection-inactive`, `row`, `row-alternate`, `track`, `thumb`, `scrollbar`, `scrim`.

`Theme::token` and `Theme::set_token` read and write them by name. `Effects::set_theme` changes the theme at run time. `AppOptions::theme` is `ThemeChoice::System` (follows the OS light or dark setting) or `ThemeChoice::Fixed(theme)`.

A theme carries colours and the base font. Spacing, radii and control sizes are per-style values. Quads and custom widgets that hard-code `Color::rgb` do not follow a theme switch; read `ui.theme()` instead.

## Stylesheets

`AppOptions::stylesheet` names a file that is installed at start and reloaded within half a second of a change; a sheet that fails to parse is reported and the last good one stays. `StyleSheet::parse` reads text directly, and `style_sheet!("styles/app.style")` parses a file while the crate compiles, so an unknown property is a build error naming the line. `ui.class("name")` returns the style of a class and records a warning when it is missing.

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

One property per line, or separated by `;`. Colours are `#rrggbb` or `#rrggbbaa`. There are no variables and no nesting. A token overlays whichever theme is active, so an OS light and dark switch keeps the application's accent.

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

`AppOptions::reduced_motion` skips animations. `true` forces them off; `false` follows the system. On Windows the host reads the "Animation effects" switch (`SPI_GETCLIENTAREAANIMATION`) at startup and whenever a window gains focus, and skips animations while it is off. macOS and Linux have no preference read yet, so only the application option applies there. An application cannot turn animations on against the system.

## Images and backgrounds

`Ui::image(&Image, ImageOptions)` and `Ui::image_file(path, options)` show an image; `ImageFit` is `Contain`, `Cover`, `Fill` or `None`, and `ImageTint` is `None`, `Foreground` or `Color`. `glassmere::image::decode` and `load` produce an `Image` from PNG, JPEG, WebP, GIF (first frame) and SVG; EXIF orientation is applied, colour profiles are ignored and pixels are taken as sRGB. `image_file` loads on a background thread and polls the file's modification time, so replacing the file updates the window.

`Style::background_image(BackgroundImage)` paints an image over the background colour and under the content, fitted like an image widget or nine-sliced with `BackgroundImage::slice` so one small frame skins boxes of any size. One background image per node; no tiling, and no position other than centred.

There are no mipmaps: an image drawn far below its natural size aliases, so shrink large photos first or use an SVG. A vector image is rasterised on the render thread the first time each size is drawn.

## Direction

A window runs left to right or right to left according to `AppOptions::direction` (`DirectionChoice::Auto` follows the OS locale). `Style::direction` overrides it for a subtree: layout mirrors, text aligns to the end side, and directional internals follow. Physical values stay physical: `offset`, transforms, slide effects, floating dock rectangles and `DockZone::Left` and `Right` mean the same pixels in both directions. `Style::base_direction` forces the base direction of a text paragraph; editors ignore it.
