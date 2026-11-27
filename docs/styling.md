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

### Margin and border

`Style::margin(8.0)` keeps 8 px clear outside the box on every side; `Style::margin_edges(Edges::new(top, right, bottom, left))` sets each side, and `Edges::xy(horizontal, vertical)` and `Edges::all` are shorter. A margin is not part of the box: the box keeps its size, the pointer over the margin is not over the box, and neighbours stay that far away. Margins do not collapse into each other as they do in a web page's block layout; they add, as they do in its flex layout.

`Style::border(2.0)` draws a border `2` px wide along the edge of the box, inside it, in `border_color`, or in the theme's `track` colour when none is set. It follows `radius`. A border takes room like padding does: the content starts inside both, `Style::inset()` is the sum of the two, and a box that sizes itself grows by it. A focus ring sits inside the border.

In a stylesheet: `margin: 8` (all sides), `margin: 4 8` (vertical, horizontal), `margin: 1 2 3 4` (top, right, bottom, left), `margin-top` and the other sides, `border: 2 #475a7a` (width and an optional colour), `border-width` and `border-color`. Padding is one value for all sides.

### Wrapping rows

`Style::wrap_items(true)` on a row or a column starts a new line when the next child does not fit along the main axis, as `flex-wrap: wrap` does, instead of overflowing it or, with `shrink`, squeezing it. The `gap` is the room between lines as well as between children. A stylesheet takes `wrap-items: true`. `Style::align_content(Justify::SpaceBetween)` (stylesheet `align-content`) shares the room left across the container among its lines, as `align-content` does; left unset, the lines are stretched as the layout engine does. A row that wraps needs a width to wrap at, from `width`, a parent that gives it one, or `Length::Percent`.

### The focus ring

A focused widget outlines itself with a ring: a stroke 2 px wide in the theme's `focus` colour, drawn inside the widget's box along its rounded edge, over its background and gradient. A widget with square corners gets four bars, which land exactly on pixels.

The ring follows how focus was given, as it does in a browser. A widget that takes no typed text shows it after the keyboard, Tab and the arrow keys, and not after a click or a touch; the next key that does something brings it back, and the next press hides it again. A field you type into always shows it, because it is where the text goes. A window shows it from the start, until the first press.

`Style::focus_ring(FocusRing::new(3.0).color(color).inset(2.0))` sets the width, the colour and how far the ring sits inside the edge, for one widget; `Style::no_focus_ring()` draws none, for a widget that shows focus some other way. The ring stays inside the widget's box, which is what is repainted when focus moves; outside it, it would be left behind. The colour of the default ring is the theme's `focus` token.

### What takes focus

Buttons, fields, checkboxes, sliders, list rows, tabs, links and the like take focus from a click and from Tab. A container that only lays things out does not: a column, a row, a grid, a pane, a spacer. Clicking one of them, or empty space inside a scroll area, focuses nothing.

Two kinds are in between. A scroll container takes focus only while it scrolls, because the arrow keys and Page Up and Page Down act on the one that has it; one that fits its content is layout, and is skipped. A pane divider takes focus so that the arrow keys, Home and End can move it, and shows its ring after the keyboard like anything else.

`Style::focusable(false)` takes any widget that would take focus out of the focus order, and a click on it leaves focus where it was. It still reacts to the pointer: a button with `focusable(false)` can be clicked and cannot be tabbed to. Use it for what is only for the mouse, or what shows its state some other way.

### The pointer

Over a widget that takes a press the pointer is a hand, over text it is an I-beam, over a divider it is the resize pointer of its direction, and over a layout container, a scroll area or a disabled widget it is the arrow. `Style::cursor(Cursor::Grab)` sets another for one node and for everything inside it that sets none of its own, so a container can give its whole area a pointer. `Context::cursor(id)` is the answer a host shows; the winit host takes it from the hovered node. Whether a widget can take focus has no part in it: a button with `focusable(false)` still shows the hand. To show the arrow over every pressable widget, as a browser does, set `AppOptions::pressable_cursor` (or `Context::set_pressable_cursor`) to `Cursor::Default`; text fields keep the I-beam and a node's own `Style::cursor` still wins.

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

## Padding per side

`Style::padding(8.0)` pads all four sides. On a node that holds other nodes (`row`, `column`, `grid`, `container`) `Style::padding_sides(Edges::new(top, right, bottom, left))` pads each side by its own amount instead, and the border still adds to it. A stylesheet takes `padding: 4 8` (vertical, horizontal), three or four numbers in CSS order, and `padding-left` and the like. A label, a button, a field, a scroll view and a list lay out their own content at the one `padding` and ignore `padding_sides`.
## Stacking

Overlapping siblings (nodes placed with `Style::offset`, or pushed over each other by a negative `margin`) are painted in the order they are declared, the last on top, and a click reaches the one on top. `Style::z_index(n)` (stylesheet `z-index: n`) changes that: a higher value is painted later and hit first, a lower one, negative or not, goes under. Siblings with the same value keep their declaration order. Layout and the Tab order still follow the declaration order.

It orders the children of one parent. Each parent is a stack of its own, so a child never goes over or under a node outside its parent. To put something above the whole window, use a `popover` or a `dialog`.

## Border per side

`Style::border_sides(Edges::new(top, right, bottom, left))` gives a node that holds other nodes a border of its own width on each side, drawn in `border_color`. A side of 0 draws nothing, so `Edges::new(0.0, 0.0, 2.0, 0.0)` is an underline. Each side takes room like padding does. In a stylesheet, `border-bottom-width: 2` and the like set one side, the others staying what `border-width` made them. The sides are straight lines along the inside of the box: a `radius` rounds the fill but not them, so a rounded outline is the one `border`. A label, a button, a field, a scroll view and a list keep the one `border`.
## Pinning to a corner

`Style::offset(x, y)` takes a node out of flow and puts its top-left corner at that place in its parent. `Style::offset_end(right, bottom)` does the same from the bottom-right corner, so a badge or a floating button stays in the corner whatever size its parent has. With both, the node is pinned on both sides, and one with no `width` or `height` stretches between them, as `left`, `right`, `top` and `bottom` together do in CSS. The edges are physical: the right edge is on the right in a right-to-left window too. The parent needs a size of its own, as the node no longer gives it one.
## Sharing space: grow, shrink and basis

`grow` and `shrink` start from the size a child has along the row (or column): its `width`, or its content. Two children with `grow(1.0)` share what is left after their own sizes, so a child with more in it ends up wider. For equal shares whatever the content, as `flex: 1` gives on the web, use `Style::flex(1.0)` (stylesheet `flex: 1`): it grows by that factor, shrinks, and starts from a `basis` of 0. `Style::basis(Length::Px(120.0))` (stylesheet `basis: 120`) is the size to start from when you want one of your own, and it wins over `width`. Children of `Style::flex(1.0)` and `Style::flex(2.0)` split a row one to two.

## Automatic margins

`Style::margin_left_auto()` (stylesheet `margin-left: auto`) gives a child the room left over on its left, which pushes it to the end of a row: the usual way to put a button at the far end of a toolbar. `margin_x_auto()` (`margin: 0 auto`) centres a child across a column, and in a row it takes the room left over on both sides; `margin_y_auto()` centres across a row. A side that is auto ignores its number, and the other sides keep theirs.
## Shielding what is underneath

A container is transparent to the pointer: only widgets that take a press, a key or a drag are hit, so a card, a panel or a floating toolbar drawn over a button leaves the button clickable through it. In a browser a box under the pointer takes it. `Style::blocks_pointer(true)` (stylesheet `blocks-pointer: true`) makes a node do that: a click or a hover on it does not reach what is under it, and it is what `hovered` reports there. What is inside it still takes the pointer first. Popovers, dialogs and menus already shield what they cover.
## Reversed flow

`Style::reverse(true)` (stylesheet `reverse: true`) lays a row or a column out from its end, as `flex-direction: row-reverse` and `column-reverse` do: the first child is at the end of the main axis and the others follow back from it, which is the way to put the primary action at the right of a dialog's button row without declaring it last. Only the layout is reversed; Tab still goes in the order the children are declared. `Justify::Start` and `Justify::End` are relative to the main axis, so in a reversed row `End` is the left edge.

## Text alignment

`Style::text_align(TextAlign::Center)` (stylesheet `text-align: center`) sets where text sits in a box wider than it. For wrapped text the shaper aligns every line between the edges (`Start`, `Center`, `End`, or `Justify`, which stretches the spaces of every line but the last), so a selection and the caret follow the drawn text. A single line sits at the start, the middle or the end of its box, so a right-aligned number in a fixed-width cell is `text_align(TextAlign::End)`. It is inherited, like the font: set it on a column and every label in it takes it, and a label's own value wins. It follows the reading direction: `Start` is the left edge in a left-to-right window and the right edge in a right-to-left one, and `End` the other. Without it text is where it was, or where `justify_content` puts a single line.

## Line height and letter spacing

`Style::line_height(1.6)` (stylesheet `line-height: 1.6`) sets the height of a line as a multiple of the font size, `1.4` unless set. Unlike CSS it takes only a multiple, not a length. `Style::letter_spacing(0.5)` (stylesheet `letter-spacing: 0.5`) adds pixels after every character; a negative value tightens. Both are inherited like the font, so set them on a column and every label inside takes them. Text areas and code editors keep their fixed line pitch.

## Hiding without reflowing

`Style::hidden(true)` (stylesheet `visibility: hidden`) keeps a node and its subtree where they are in the layout but draws nothing, takes no pointer or keyboard focus (Tab skips it) and leaves it out of the accessibility tree. Use it for a control that comes and goes while its neighbours stay put. To remove the space too, do not declare the node, which is `display: none`. Unlike CSS, a child of a hidden node cannot be made visible again.

## Overflow

A box cuts what is inside it at its edge, so a child larger than its parent shows only where the parent is, and the cut part cannot be clicked. `Style::unclipped(true)` (stylesheet `overflow: visible`) lifts that for a row, column, container or grid: its children are drawn and hit past its edge, still under the clip of whatever clips above it. A scroll view, a list, a tab strip, a popup and a popover always clip. While a window has an unclipped node, finding what is under the pointer searches the whole tree instead of skipping what a clip rules out, so use it for a few boxes, not for every one. A group opacity set on the unclipped node composites what hangs over it too, in an offscreen layer as big as the window, which costs more than a layer of the box; a transform moves it with the box (tested for a translation).

