use crate::Theme;
use crate::animation::{Animation, Effect, Transform, Transition};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use glassmere_scene::{Color, FontFamily, TextAlign, TextStyle, TextWrap};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    /// A fraction of the parent's size, e.g. 1.0 for 100%.
    Percent(f32),
}

/// Cross-axis alignment of children, or of one child through `align_self`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
    /// Lines up the text baselines of the children.
    Baseline,
}

/// Main-axis distribution of children.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// One bound of a grid track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackSize {
    Px(f32),
    Percent(f32),
    /// Fraction of the free space; only meaningful as a maximum.
    Fr(f32),
    Auto,
}

/// A grid track template entry.
#[derive(Clone, Debug, PartialEq)]
pub enum Track {
    Px(f32),
    Percent(f32),
    Fr(f32),
    Auto,
    MinMax(TrackSize, TrackSize),
    Repeat(u16, Vec<Track>),
    /// As many repetitions as fit the container, empty ones included. The
    /// tracks need a definite size: `Px`, `Percent`, or a `MinMax` with one.
    AutoFill(Vec<Track>),
    /// Like `AutoFill`, but repetitions no item landed in collapse to nothing.
    AutoFit(Vec<Track>),
}

impl Track {
    pub fn repeat(count: u16, tracks: Vec<Track>) -> Self {
        Self::Repeat(count, tracks)
    }
    pub fn auto_fill(tracks: Vec<Track>) -> Self {
        Self::AutoFill(tracks)
    }
    pub fn auto_fit(tracks: Vec<Track>) -> Self {
        Self::AutoFit(tracks)
    }
}

/// How a grid places children that have no explicit `area`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AutoFlow {
    /// Fills each row in turn, adding rows as needed.
    #[default]
    Row,
    /// Fills each column in turn, adding columns as needed.
    Column,
    /// `Row`, but a smaller child may fill a hole an earlier larger one left.
    RowDense,
    /// `Column`, with the same hole filling.
    ColumnDense,
}

/// Checks a `Style::areas` template and returns it in the engine's form. Each
/// string is one row of whitespace-separated names, with `.` for an empty cell;
/// every row has the same number of cells and a name covers one rectangle.
pub(crate) fn template_areas(
    rows: &[String],
) -> Result<Vec<taffy::style::GridTemplateArea<String>>, String> {
    let cells: Vec<Vec<&str>> = rows
        .iter()
        .map(|row| row.split_whitespace().collect())
        .collect();
    let columns = cells.first().map_or(0, Vec::len);
    if cells.iter().any(|row| row.len() != columns) {
        return Err("every row of `areas` needs the same number of cells".to_owned());
    }
    let mut areas: Vec<taffy::style::GridTemplateArea<String>> = Vec::new();
    for (row, names) in cells.iter().enumerate() {
        for (column, name) in names.iter().enumerate() {
            if *name == "." {
                continue;
            }
            let (row, column) = (row as u16, column as u16);
            match areas.iter_mut().find(|area| area.name == *name) {
                Some(area) => {
                    area.row_start = area.row_start.min(row);
                    area.row_end = area.row_end.max(row + 1);
                    area.column_start = area.column_start.min(column);
                    area.column_end = area.column_end.max(column + 1);
                }
                None => areas.push(taffy::style::GridTemplateArea {
                    name: (*name).to_owned(),
                    row_start: row,
                    row_end: row + 1,
                    column_start: column,
                    column_end: column + 1,
                }),
            }
        }
    }
    for area in &areas {
        for row in area.row_start..area.row_end {
            for column in area.column_start..area.column_end {
                if cells[row as usize][column as usize] != area.name {
                    return Err(format!("area `{}` is not a rectangle", area.name));
                }
            }
        }
    }
    Ok(areas
        .into_iter()
        .map(|area| taffy::style::GridTemplateArea {
            // Taffy counts lines from one.
            row_start: area.row_start + 1,
            row_end: area.row_end + 1,
            column_start: area.column_start + 1,
            column_end: area.column_end + 1,
            ..area
        })
        .collect())
}

/// Explicit grid placement in 1-based lines; spans default to one cell. A line
/// of 0 leaves that axis to auto-flow, so the child still takes `column_span`
/// or `row_span` tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridArea {
    pub column: u16,
    pub row: u16,
    pub column_span: u16,
    pub row_span: u16,
}

impl GridArea {
    pub fn cell(column: u16, row: u16) -> Self {
        Self {
            column,
            row,
            column_span: 1,
            row_span: 1,
        }
    }
    /// Placed by auto-flow, covering the given number of tracks.
    pub fn auto_span(column_span: u16, row_span: u16) -> Self {
        Self::span(0, 0, column_span, row_span)
    }
    pub fn span(column: u16, row: u16, column_span: u16, row_span: u16) -> Self {
        Self {
            column,
            row,
            column_span,
            row_span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    Column,
    Row,
    Grid,
}

/// Reading and layout direction. Rows, grids, tab strips and split panes run
/// from the start side, which is the right in `Rtl`; text blocks align there.
/// Pixel values (`offset`, transforms, dock sides) stay physical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

/// Paint properties a pseudo-state may override. Layout never changes with state,
/// so state transitions stay repaint-only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paint {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub radius: Option<f32>,
    pub opacity: Option<f32>,
    pub transform: Option<Transform>,
}

impl Paint {
    pub fn opacity(opacity: f32) -> Self {
        Self {
            opacity: Some(opacity),
            ..Self::default()
        }
    }
    pub fn transform(transform: Transform) -> Self {
        Self {
            transform: Some(transform),
            ..Self::default()
        }
    }
    pub fn background(color: Color) -> Self {
        Self {
            background: Some(color),
            ..Self::default()
        }
    }
    pub fn foreground(color: Color) -> Self {
        Self {
            foreground: Some(color),
            ..Self::default()
        }
    }
}

/// Overrides applied on top of the theme defaults while a state is active.
/// Precedence when several apply: disabled, then pressed, hover, focused.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateStyles {
    pub hover: Paint,
    pub pressed: Paint,
    pub focused: Paint,
    pub disabled: Paint,
}

/// Layout and paint properties of one widget. Text properties are `None` by
/// default and inherit from the nearest ancestor, then the theme; `background`
/// `None` paints the widget kind's theme default.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub width: Length,
    pub height: Length,
    /// `Auto` means unconstrained.
    pub min_width: Length,
    pub max_width: Length,
    pub min_height: Length,
    pub max_height: Length,
    /// width / height; the unspecified dimension follows the other.
    pub aspect_ratio: Option<f32>,
    pub padding: f32,
    /// Padding per side, for a node that holds other nodes: `row`, `column`,
    /// `grid`, `container` and the like. When set it replaces `padding` there
    /// (the border still adds to it). A control that draws its own content, such
    /// as a label, a button, a field, a scroll view or a list, ignores it and
    /// keeps the one `padding`, because it lays that content out itself.
    pub padding_sides: Option<Edges>,
    /// Room kept clear outside the box, per side. It is not part of the box:
    /// the box's size and its hit area are what they were, and neighbours
    /// stay this far from it.
    pub margin: Edges,
    /// The sides whose margin is `auto`: it takes the room left over along
    /// that side, as `margin: auto` does in CSS, so `margin_left_auto` pushes a
    /// child to the end of a row and auto margins on both sides of the cross
    /// axis centre it. The `margin` of a side that is auto is ignored.
    pub margin_auto: AutoEdges,
    /// The width of a border drawn along the edge of the box, inside it. It
    /// takes room like padding does, so the content starts inside it.
    pub border: f32,
    /// The border's colour; `None` takes the theme's `track` colour.
    pub border_color: Option<Color>,
    /// The border's width per side, for a node that holds other nodes (see
    /// `padding_sides`); when set it replaces `border` there. A side whose
    /// width is 0 draws nothing, so one edge can carry a rule. The sides are
    /// drawn straight, so `radius` rounds the fill but not these.
    pub border_sides: Option<Edges>,
    pub gap: f32,
    /// Overrides `gap` between rows when set.
    pub row_gap: Option<f32>,
    /// Overrides `gap` between columns when set.
    pub column_gap: Option<f32>,
    pub grow: f32,
    /// Children that do not fit along the main axis go on to a new line,
    /// as `flex-wrap: wrap` does, instead of overflowing or shrinking. `gap`
    /// (or `row_gap` and `column_gap`) is the room between the lines too.
    pub wrap_items: bool,
    /// The size along the main axis that `grow` and `shrink` start from, as
    /// `flex-basis` is in CSS. `Auto` starts from the width (or height) and,
    /// without one, from the content, so two children with `grow` 1 share
    /// what is left after their own sizes and end up unequal. A basis of 0 (or
    /// [`Style::flex`]) shares the whole space by `grow`, so they end up equal
    /// whatever is in them. Wins over `width` (`height` in a column) for the
    /// size it starts from.
    pub basis: Length,
    /// Lays the children of a row or a column out from the end instead of the
    /// start, as `flex-direction: row-reverse` and `column-reverse` do: the
    /// first child is at the end of the main axis, and `justify_content`'s
    /// start is that end. Only the layout is reversed; the focus order and the
    /// order the children are declared in stay what they were.
    pub reverse: bool,
    /// Flex shrink factor; children do not shrink unless this is set.
    pub shrink: f32,
    pub radius: f32,
    pub align_items: Align,
    pub justify_content: Justify,
    /// How the lines of a wrapping container (`wrap_items`) share the room
    /// left across it, as `align-content` does; `None` leaves them as the
    /// layout engine does. It has an effect only when the container is taller
    /// (or wider, in a column) than its lines.
    pub align_content: Option<Justify>,
    pub align_self: Option<Align>,
    /// Grid items only: inline-axis alignment inside the cell.
    pub justify_self: Option<Align>,
    /// Grid containers: explicit column and row templates.
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
    /// Sizes for implicitly created tracks; `Auto` when empty.
    pub auto_columns: Vec<Track>,
    pub auto_rows: Vec<Track>,
    /// Grid containers: how children without an `area` are placed.
    pub auto_flow: AutoFlow,
    /// Grid containers: named areas, one string per row. Each holds
    /// whitespace-separated names, `.` marking an empty cell, as CSS
    /// `grid-template-areas` does: `"header header"`, `"nav main"`. A template
    /// that is ragged or has a name that is not one rectangle is reported as
    /// misuse by `Ui::grid`.
    pub areas: Vec<String>,
    /// Grid items only: explicit placement instead of auto-flow.
    pub area: Option<GridArea>,
    /// Grid items only: placement in the named area of the parent's `areas`.
    /// Wins over `area`.
    pub area_name: Option<String>,
    pub background: Option<Color>,
    /// A ramp of colours instead of a flat `background`, painted under
    /// `background_image`.
    pub gradient: Option<Box<Gradient>>,
    /// Painted behind the node, or inside its edges. Boxed because most
    /// nodes have none.
    pub shadow: Option<Box<Shadow>>,
    /// Painted over `background` and under the content; see `BackgroundImage`.
    pub background_image: Option<crate::BackgroundImage>,
    pub foreground: Option<Color>,
    pub font_size: Option<f32>,
    pub font_family: Option<FontFamily>,
    pub font_weight: Option<u16>,
    pub italic: Option<bool>,
    /// Where the lines of wrapped text sit between the edges of the box, and
    /// where a single line sits in a box wider than it: `Start`, `Center`,
    /// `End` or `Justify`. Inherited by what is inside, like the font. `None`
    /// takes the parent's, and at the top `Start`, where text is unless
    /// `justify_content` places it.
    pub text_align: Option<TextAlign>,
    /// The height of a line of text as a multiple of the font size, `1.4`
    /// unless set. Inherited by what is inside, like the font.
    pub line_height: Option<f32>,
    /// Extra advance after every character in pixels, `0` unless set; negative
    /// tightens. Inherited by what is inside, like the font.
    pub letter_spacing: Option<f32>,
    /// The node and everything inside it keeps its place in the layout but is
    /// not drawn, hit, focused or reported to accessibility, like CSS
    /// `visibility: hidden`. Unlike CSS a hidden subtree cannot show a child.
    pub hidden: bool,
    /// What is inside is not cut off at the edge of this box: a badge or a
    /// shadow-like decoration can hang over the corner, as with CSS
    /// `overflow: visible`. It still lies under the clip of the nearest
    /// ancestor that clips, and a scroll view, a list or a popup always clips.
    pub unclipped: bool,
    pub wrap: TextWrap,
    pub states: Option<Box<StateStyles>>,
    /// Group opacity of the node and its subtree, 0..=1.
    pub opacity: f32,
    /// Visual-only transform about the node's center; layout, hit testing and
    /// accessibility keep using the untransformed bounds.
    pub transform: Transform,
    /// Animates background, foreground, radius, opacity and transform changes.
    pub transition: Option<Transition>,
    /// Animates the node from its previous position and size after layout moves it.
    pub layout_transition: Option<Transition>,
    /// Played from this look when the node first appears.
    pub enter: Option<Effect>,
    /// Played toward this look after the node is removed; the fading image is
    /// not interactive.
    pub exit: Option<Effect>,
    pub animation: Option<Animation>,
    /// Takes the node out of flow and places its top-left corner at this
    /// offset from the parent's top-left corner.
    pub offset: Option<(f32, f32)>,
    /// How the node shows keyboard focus; `None` takes the theme's ring.
    pub focus_ring: Option<FocusRing>,
    /// Whether the node can take focus, by a click or by Tab. `false` keeps a
    /// widget that is focusable by kind, a divider or a scroll container,
    /// out of the focus order, and a click on it leaves focus where it was.
    /// It still reacts to the pointer. A container that scrolls nothing is
    /// not focusable whatever this says.
    pub focusable: bool,
    /// The pointer over the node and, when a descendant sets none, over it.
    /// `None` takes the parent's, and at the top what the widget is: a hand
    /// over one that takes a press, an I-beam over text.
    pub cursor: Option<Cursor>,
    /// Where the node stands among its siblings when they overlap, as
    /// `z-index` does on a flex item: a higher value is painted later, so over
    /// the lower ones, and is hit first. Siblings with the same value keep the
    /// order they were declared in, and 0 is where a node is unless it says
    /// otherwise. It orders siblings only, so a child never leaves its parent's
    /// place in the stack; use an overlay (`popover`, `dialog`) to put
    /// something above the whole window. Layout and the focus order still
    /// follow the declaration order.
    pub z_index: i32,
    /// Takes the node out of flow and places its bottom-right corner at this
    /// distance (right, bottom) from the parent's bottom-right corner, as
    /// `position: absolute; right; bottom` does, so a badge or a floating
    /// button stays in the corner whatever size the parent is. With `offset`
    /// as well the node is pinned on both sides, and a node with no width or
    /// height stretches between them. The edges are physical: the right edge
    /// stays on the right in a right-to-left window.
    pub offset_end: Option<(f32, f32)>,
    /// Takes the pointer wherever the node is, as a box does in a browser: a
    /// click or a hover on it does not reach what lies under it. A container
    /// is otherwise transparent to the pointer, so a card, a panel or a
    /// floating toolbar drawn over a button leaves the button clickable through
    /// it; with this set it shields what is under it. What is inside the node
    /// still takes the pointer first.
    pub blocks_pointer: bool,
    /// Inherited like text properties; `None` takes the parent's direction.
    pub direction: Option<Direction>,
    /// Forces the direction a paragraph starts in, whatever its first strong
    /// character is. Inherited. `None` lets the text decide, which is what
    /// mixed content usually wants.
    pub base_direction: Option<Direction>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            max_width: Length::Auto,
            min_height: Length::Auto,
            max_height: Length::Auto,
            aspect_ratio: None,
            base_direction: None,
            gradient: None,
            shadow: None,
            padding: 0.0,
            padding_sides: None,
            margin: Edges::ZERO,
            margin_auto: AutoEdges::NONE,
            border: 0.0,
            border_color: None,
            border_sides: None,
            gap: 12.0,
            row_gap: None,
            column_gap: None,
            grow: 0.0,
            wrap_items: false,
            basis: Length::Auto,
            reverse: false,
            shrink: 0.0,
            radius: 0.0,
            align_items: Align::Start,
            justify_content: Justify::Start,
            align_content: None,
            align_self: None,
            justify_self: None,
            columns: Vec::new(),
            rows: Vec::new(),
            auto_columns: Vec::new(),
            auto_rows: Vec::new(),
            auto_flow: AutoFlow::Row,
            areas: Vec::new(),
            area: None,
            area_name: None,
            background: None,
            background_image: None,
            foreground: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            italic: None,
            text_align: None,
            line_height: None,
            letter_spacing: None,
            hidden: false,
            unclipped: false,
            wrap: TextWrap::None,
            states: None,
            opacity: 1.0,
            transform: Transform::IDENTITY,
            transition: None,
            layout_transition: None,
            enter: None,
            exit: None,
            animation: None,
            offset: None,
            focus_ring: None,
            focusable: true,
            cursor: None,
            z_index: 0,
            offset_end: None,
            blocks_pointer: false,
            direction: None,
        }
    }
}

/// The shape of the pointer over a node, as `Style::cursor` sets it and
/// `Context::cursor` reports it for a host to show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    #[default]
    Default,
    /// The hand over something that can be pressed.
    Pointer,
    /// The I-beam over text.
    Text,
    Crosshair,
    Move,
    Grab,
    Grabbing,
    NotAllowed,
    Help,
    Wait,
    /// Left and right edges, as over a column divider.
    ColResize,
    /// Top and bottom edges, as over a row divider.
    RowResize,
}

/// Which sides of a box have an automatic margin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoEdges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl AutoEdges {
    pub const NONE: Self = Self {
        top: false,
        right: false,
        bottom: false,
        left: false,
    };
    /// Left and right.
    pub const HORIZONTAL: Self = Self {
        top: false,
        right: true,
        bottom: false,
        left: true,
    };
    /// Top and bottom.
    pub const VERTICAL: Self = Self {
        top: true,
        right: false,
        bottom: true,
        left: false,
    };
    pub const ALL: Self = Self {
        top: true,
        right: true,
        bottom: true,
        left: true,
    };
}

/// A length for each side of a box, as a margin takes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const ZERO: Self = Self::all(0.0);

    /// The same on every side.
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// `horizontal` on the left and right, `vertical` on the top and bottom.
    pub const fn xy(horizontal: f32, vertical: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// In the order CSS gives them: top, right, bottom, left.
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// The outline a node draws while it has keyboard focus. It is a stroked
/// rounded rectangle that follows the node's radius and lies inside the
/// node's box, so it is painted and repainted with the node.
///
/// By default a node shows it after keyboard navigation and not after a
/// click, as a browser does, except in widgets that take typed text, which
/// always show it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusRing {
    /// Stroke width in logical pixels. Zero or less draws no ring.
    pub width: f32,
    /// The ring's colour; `None` takes the theme's `focus` colour.
    pub color: Option<Color>,
    /// The gap between the node's edge and the ring, inside the node.
    pub inset: f32,
}

impl FocusRing {
    /// No ring at all.
    pub const NONE: Self = Self {
        width: 0.0,
        color: None,
        inset: 0.0,
    };

    /// The theme's colour, flush with the node's edge, `width` thick.
    pub const fn new(width: f32) -> Self {
        Self {
            width,
            color: None,
            inset: 0.0,
        }
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub const fn inset(mut self, inset: f32) -> Self {
        self.inset = inset;
        self
    }
}

macro_rules! setter {
    ($name:ident, $field:ident, $type:ty) => {
        pub fn $name(mut self, value: $type) -> Self {
            self.$field = value;
            self
        }
    };
    ($name:ident, $field:ident, $type:ty, some) => {
        pub fn $name(mut self, value: $type) -> Self {
            self.$field = Some(value);
            self
        }
    };
}

/// A ramp of colours filling a node. `from` and `to` are in the node's own
/// units, where (0, 0) is its top left corner and (1, 1) its bottom right.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub from: (f32, f32),
    pub to: (f32, f32),
    /// At least two; fewer draws nothing.
    pub stops: Vec<glassmere_scene::GradientStop>,
    /// A ramp outward from `from` rather than along the line to `to`.
    pub radial: bool,
}

impl Gradient {
    /// Top to bottom.
    pub fn down(from: Color, to: Color) -> Self {
        Self::linear((0.0, 0.0), (0.0, 1.0), from, to)
    }

    pub fn linear(from: (f32, f32), to: (f32, f32), start: Color, end: Color) -> Self {
        Self {
            from,
            to,
            stops: vec![
                glassmere_scene::GradientStop {
                    at: 0.0,
                    color: start,
                },
                glassmere_scene::GradientStop {
                    at: 1.0,
                    color: end,
                },
            ],
            radial: false,
        }
    }

    /// Outward from the middle to the edge.
    pub fn radial(centre: Color, edge: Color) -> Self {
        Self {
            radial: true,
            ..Self::linear((0.5, 0.5), (1.0, 0.5), centre, edge)
        }
    }

    pub fn stops(mut self, stops: Vec<glassmere_scene::GradientStop>) -> Self {
        self.stops = stops;
        self
    }
}

/// A blurred rounded rectangle behind a node, or inside its edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    /// Standard deviation of the blur, in logical pixels.
    pub blur: f32,
    /// Grows the rectangle before blurring it.
    pub spread: f32,
    pub offset: (f32, f32),
    pub inset: bool,
}

impl Shadow {
    pub fn new(color: Color, blur: f32) -> Self {
        Self {
            color,
            blur,
            spread: 0.0,
            offset: (0.0, 0.0),
            inset: false,
        }
    }

    /// The usual lift: down a little, blurred, behind the node.
    pub fn lift(color: Color, blur: f32, down: f32) -> Self {
        Self {
            offset: (0.0, down),
            ..Self::new(color, blur)
        }
    }

    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }
}

impl Style {
    /// Forces the direction paragraphs inside start in.
    pub fn base_direction(mut self, direction: Direction) -> Self {
        self.base_direction = Some(direction);
        self
    }

    pub fn gradient(mut self, gradient: Gradient) -> Self {
        self.gradient = Some(Box::new(gradient));
        self
    }

    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(Box::new(shadow));
        self
    }

    /// Themed control surface with the default control padding and radius.
    pub fn button() -> Self {
        Self {
            padding: 12.0,
            radius: 8.0,
            ..Self::default()
        }
    }

    setter!(width, width, Length);
    setter!(height, height, Length);
    setter!(min_width, min_width, Length);
    setter!(max_width, max_width, Length);
    setter!(min_height, min_height, Length);
    setter!(max_height, max_height, Length);
    setter!(aspect_ratio, aspect_ratio, f32, some);
    setter!(
        background_image,
        background_image,
        crate::BackgroundImage,
        some
    );
    setter!(padding, padding, f32);
    setter!(padding_sides, padding_sides, Edges, some);
    /// The same margin on every side.
    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = Edges::all(margin);
        self
    }
    /// An automatic margin on these sides, which take the room left over.
    pub fn margin_auto_edges(mut self, edges: AutoEdges) -> Self {
        self.margin_auto = edges;
        self
    }
    /// An automatic margin on the left: in a row, the child goes to the end.
    pub fn margin_left_auto(mut self) -> Self {
        self.margin_auto.left = true;
        self
    }
    /// An automatic margin on the right: in a row, what follows goes to the end.
    pub fn margin_right_auto(mut self) -> Self {
        self.margin_auto.right = true;
        self
    }
    /// An automatic margin on the top.
    pub fn margin_top_auto(mut self) -> Self {
        self.margin_auto.top = true;
        self
    }
    /// An automatic margin on the bottom.
    pub fn margin_bottom_auto(mut self) -> Self {
        self.margin_auto.bottom = true;
        self
    }
    /// Automatic margins left and right, as `margin: 0 auto`: the child is
    /// centred across a column, and in a row it takes the room left over.
    pub fn margin_x_auto(mut self) -> Self {
        self.margin_auto.left = true;
        self.margin_auto.right = true;
        self
    }
    /// Automatic margins on top and bottom: the child is centred across a row.
    pub fn margin_y_auto(mut self) -> Self {
        self.margin_auto.top = true;
        self.margin_auto.bottom = true;
        self
    }
    /// A margin per side.
    pub fn margin_edges(mut self, margin: Edges) -> Self {
        self.margin = margin;
        self
    }
    /// The border's width, in logical pixels; its colour is `border_color`.
    pub fn border(mut self, width: f32) -> Self {
        self.border = width;
        self
    }
    setter!(border_color, border_color, Color, some);
    setter!(border_sides, border_sides, Edges, some);

    /// How far the content starts inside the box: the padding and the
    /// border, which both take room.
    pub fn inset(&self) -> f32 {
        self.padding.max(0.0) + self.border.max(0.0)
    }
    /// Sets the gap between rows and between columns.
    pub fn gap(mut self, value: f32) -> Self {
        self.gap = value;
        self.row_gap = None;
        self.column_gap = None;
        self
    }
    setter!(row_gap, row_gap, f32, some);
    setter!(column_gap, column_gap, f32, some);
    setter!(grow, grow, f32);
    setter!(wrap_items, wrap_items, bool);
    setter!(basis, basis, Length);
    /// `flex: factor` as CSS has it: grows by `factor`, shrinks, and starts
    /// from a basis of 0, so that children sharing a row each take a share of
    /// the whole space by their factor, whatever is in them.
    pub fn flex(mut self, factor: f32) -> Self {
        self.grow = factor;
        self.shrink = 1.0;
        self.basis = Length::Px(0.0);
        self
    }
    setter!(reverse, reverse, bool);
    setter!(shrink, shrink, f32);
    setter!(radius, radius, f32);
    setter!(align_items, align_items, Align);
    setter!(justify_content, justify_content, Justify);
    setter!(align_content, align_content, Justify, some);
    setter!(align_self, align_self, Align, some);
    setter!(justify_self, justify_self, Align, some);
    setter!(columns, columns, Vec<Track>);
    setter!(rows, rows, Vec<Track>);
    setter!(auto_columns, auto_columns, Vec<Track>);
    setter!(auto_rows, auto_rows, Vec<Track>);
    setter!(auto_flow, auto_flow, AutoFlow);
    /// Declares the named areas of a grid, one string per row.
    pub fn areas<I>(mut self, rows: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        self.areas = rows.into_iter().map(Into::into).collect();
        self
    }
    setter!(area, area, GridArea, some);
    setter!(area_name, area_name, String, some);
    setter!(wrap, wrap, TextWrap);
    setter!(opacity, opacity, f32);
    setter!(transform, transform, Transform);
    setter!(transition, transition, Transition, some);
    setter!(layout_transition, layout_transition, Transition, some);
    setter!(enter, enter, Effect, some);
    setter!(exit, exit, Effect, some);
    setter!(animation, animation, Animation, some);
    setter!(direction, direction, Direction, some);
    setter!(z_index, z_index, i32);
    /// See [`Style::offset_end`].
    pub fn offset_end(mut self, right: f32, bottom: f32) -> Self {
        self.offset_end = Some((right, bottom));
        self
    }
    setter!(blocks_pointer, blocks_pointer, bool);

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Some((x, y));
        self
    }
    /// Draws this node's focus ring as `ring` says instead of the theme's.
    pub fn focus_ring(mut self, ring: FocusRing) -> Self {
        self.focus_ring = Some(ring);
        self
    }
    /// Draws no focus ring on this node. Whatever the node does when it has
    /// focus, such as a text caret or a highlighted row, is still drawn.
    pub fn no_focus_ring(mut self) -> Self {
        self.focus_ring = Some(FocusRing::NONE);
        self
    }
    /// Takes the node out of the focus order and stops a click from
    /// focusing it, when `focusable` is false.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }
    /// The pointer shown over this node, and over what is inside it that
    /// sets no cursor of its own.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }
    setter!(background, background, Color, some);
    setter!(foreground, foreground, Color, some);
    setter!(font_size, font_size, f32, some);
    setter!(font_family, font_family, FontFamily, some);
    setter!(font_weight, font_weight, u16, some);
    setter!(italic, italic, bool, some);
    setter!(text_align, text_align, TextAlign, some);
    setter!(hidden, hidden, bool);
    setter!(unclipped, unclipped, bool);
    setter!(line_height, line_height, f32, some);
    setter!(letter_spacing, letter_spacing, f32, some);

    fn state(mut self, apply: impl FnOnce(&mut StateStyles)) -> Self {
        apply(self.states.get_or_insert_with(Default::default));
        self
    }
    pub fn on_hover(self, paint: Paint) -> Self {
        self.state(|states| states.hover = paint)
    }
    pub fn on_pressed(self, paint: Paint) -> Self {
        self.state(|states| states.pressed = paint)
    }
    pub fn on_focused(self, paint: Paint) -> Self {
        self.state(|states| states.focused = paint)
    }
    pub fn on_disabled(self, paint: Paint) -> Self {
        self.state(|states| states.disabled = paint)
    }

    /// Fills unset text properties from the parent's resolved values.
    pub fn resolve_text(&self, inherited: &ResolvedText) -> ResolvedText {
        ResolvedText {
            foreground: self.foreground.unwrap_or(inherited.foreground),
            font_size: self.font_size.unwrap_or(inherited.font_size),
            font_family: self
                .font_family
                .clone()
                .unwrap_or_else(|| inherited.font_family.clone()),
            font_weight: self.font_weight.unwrap_or(inherited.font_weight),
            italic: self.italic.unwrap_or(inherited.italic),
            align: self.text_align.unwrap_or(inherited.align),
            line_height: self.line_height.unwrap_or(inherited.line_height),
            letter_spacing: self.letter_spacing.unwrap_or(inherited.letter_spacing),
            direction: self.direction.unwrap_or(inherited.direction),
            base_direction: self.base_direction.or(inherited.base_direction),
        }
    }

    pub(crate) fn layout(&self, flow: Flow) -> taffy::Style {
        use taffy::prelude::*;
        use taffy::style::{
            AlignContent, GridAutoFlow, GridPlacement, GridTemplateComponent,
            MaxTrackSizingFunction, MinTrackSizingFunction, RepetitionCount, TrackSizingFunction,
        };
        let gap = |value: Option<f32>| LengthPercentage::length(value.unwrap_or(self.gap).max(0.0));
        let dimension = |value| match value {
            Length::Auto => Dimension::auto(),
            Length::Px(value) => Dimension::length(value.max(0.0)),
            Length::Percent(value) => Dimension::percent(value.max(0.0)),
        };
        // Unset minimums stay at zero, so non-shrinking children keep their size;
        // the automatic minimum only matters when shrinking.
        let minimum = |value| match value {
            Length::Auto => Dimension::length(0.0),
            other => dimension(other),
        };
        let align = |value: Align| match value {
            Align::Start => AlignItems::Start,
            Align::Center => AlignItems::Center,
            Align::End => AlignItems::End,
            Align::Stretch => AlignItems::Stretch,
            // Taffy would take a leaf's bottom edge for its baseline; the
            // real one comes from the font, so the rows are lined up after
            // layout instead. See `Context::align_baselines`.
            Align::Baseline => AlignItems::FlexStart,
        };
        // Relative to the main axis, as `flex-start` and `flex-end` are: the
        // same as the writing mode's start and end everywhere but in a
        // reversed row or column, where the start of the main axis is the end.
        let justify = |value: Justify| match value {
            Justify::Start => AlignContent::FlexStart,
            Justify::Center => AlignContent::Center,
            Justify::End => AlignContent::FlexEnd,
            Justify::Stretch => AlignContent::Stretch,
            Justify::SpaceBetween => AlignContent::SpaceBetween,
            Justify::SpaceAround => AlignContent::SpaceAround,
            Justify::SpaceEvenly => AlignContent::SpaceEvenly,
        };
        let min_track = |size: TrackSize| -> MinTrackSizingFunction {
            match size {
                TrackSize::Px(value) => length(value.max(0.0)),
                TrackSize::Percent(value) => percent(value.max(0.0)),
                TrackSize::Fr(_) | TrackSize::Auto => auto(),
            }
        };
        let max_track = |size: TrackSize| -> MaxTrackSizingFunction {
            match size {
                TrackSize::Px(value) => length(value.max(0.0)),
                TrackSize::Percent(value) => percent(value.max(0.0)),
                TrackSize::Fr(value) => fr(value.max(0.0)),
                TrackSize::Auto => auto(),
            }
        };
        let sizing = |track: &Track| -> TrackSizingFunction {
            match track {
                Track::Px(value) => minmax(
                    min_track(TrackSize::Px(*value)),
                    max_track(TrackSize::Px(*value)),
                ),
                Track::Percent(value) => minmax(
                    min_track(TrackSize::Percent(*value)),
                    max_track(TrackSize::Percent(*value)),
                ),
                Track::Fr(value) => minmax(auto(), max_track(TrackSize::Fr(*value))),
                Track::Auto => minmax(auto(), auto()),
                Track::MinMax(min, max) => minmax(min_track(*min), max_track(*max)),
                // Nested repeats are not part of the contract; treat as auto.
                Track::Repeat(..) | Track::AutoFill(_) | Track::AutoFit(_) => {
                    minmax(auto(), auto())
                }
            }
        };
        let template = |tracks: &[Track]| -> Vec<GridTemplateComponent<String>> {
            tracks
                .iter()
                .map(|track| match track {
                    Track::Repeat(count, inner) => repeat(
                        RepetitionCount::Count((*count).max(1)),
                        inner.iter().map(sizing).collect(),
                    ),
                    Track::AutoFill(inner) => repeat(
                        RepetitionCount::AutoFill,
                        inner.iter().map(sizing).collect(),
                    ),
                    Track::AutoFit(inner) => {
                        repeat(RepetitionCount::AutoFit, inner.iter().map(sizing).collect())
                    }
                    other => GridTemplateComponent::Single(sizing(other)),
                })
                .collect()
        };
        let placement = |start: u16, count: u16| Line::<GridPlacement> {
            start: if start == 0 {
                GridPlacement::Auto
            } else {
                line(i16::try_from(start).unwrap_or(i16::MAX))
            },
            end: span(count.max(1)),
        };
        // A name alone resolves to the `-start` and `-end` lines of its area.
        let named = |name: &str| Line::<GridPlacement> {
            start: GridPlacement::NamedLine(name.to_owned(), 0),
            end: GridPlacement::NamedLine(name.to_owned(), 0),
        };
        taffy::Style {
            display: match flow {
                Flow::Grid => Display::Grid,
                Flow::Row | Flow::Column => Display::Flex,
            },
            size: Size {
                width: dimension(bounded(self.width, self.min_width, self.max_width)),
                height: dimension(bounded(self.height, self.min_height, self.max_height)),
            },
            min_size: Size {
                width: minimum(self.min_width),
                height: minimum(self.min_height),
            },
            max_size: Size {
                width: dimension(self.max_width),
                height: dimension(self.max_height),
            },
            aspect_ratio: self
                .aspect_ratio
                .filter(|ratio| ratio.is_finite() && *ratio > 0.0),
            padding: if self.padding_sides.is_some() || self.border_sides.is_some() {
                let padding = self.padding_sides.unwrap_or(Edges::all(self.padding));
                let border = self.border_sides.unwrap_or(Edges::all(self.border));
                let side = |padding: f32, border: f32| {
                    LengthPercentage::length(padding.max(0.0) + border.max(0.0))
                };
                Rect {
                    left: side(padding.left, border.left),
                    right: side(padding.right, border.right),
                    top: side(padding.top, border.top),
                    bottom: side(padding.bottom, border.bottom),
                }
            } else {
                Rect::length(self.inset())
            },
            margin: {
                let side = |length: f32, auto: bool| {
                    if auto {
                        LengthPercentageAuto::auto()
                    } else {
                        LengthPercentageAuto::length(length)
                    }
                };
                Rect {
                    left: side(self.margin.left, self.margin_auto.left),
                    right: side(self.margin.right, self.margin_auto.right),
                    top: side(self.margin.top, self.margin_auto.top),
                    bottom: side(self.margin.bottom, self.margin_auto.bottom),
                }
            },
            gap: Size {
                width: gap(self.column_gap),
                height: gap(self.row_gap),
            },
            flex_direction: match (flow == Flow::Row, self.reverse) {
                (true, false) => FlexDirection::Row,
                (true, true) => FlexDirection::RowReverse,
                (false, false) => FlexDirection::Column,
                (false, true) => FlexDirection::ColumnReverse,
            },
            align_items: Some(align(self.align_items)),
            // `Start` keeps the engine's normal behavior, which lets grid tracks fill.
            justify_content: (self.justify_content != Justify::Start)
                .then(|| justify(self.justify_content)),
            align_content: self.align_content.map(justify),
            align_self: self.align_self.map(align),
            justify_self: self.justify_self.map(align),
            flex_wrap: if self.wrap_items {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            },
            flex_grow: self.grow.max(0.0),
            flex_basis: dimension(self.basis),
            flex_shrink: self.shrink.max(0.0),
            grid_template_columns: template(&self.columns),
            grid_template_rows: template(&self.rows),
            grid_auto_columns: self.auto_columns.iter().map(sizing).collect(),
            grid_auto_rows: self.auto_rows.iter().map(sizing).collect(),
            grid_auto_flow: match self.auto_flow {
                AutoFlow::Row => GridAutoFlow::Row,
                AutoFlow::Column => GridAutoFlow::Column,
                AutoFlow::RowDense => GridAutoFlow::RowDense,
                AutoFlow::ColumnDense => GridAutoFlow::ColumnDense,
            },
            grid_template_areas: template_areas(&self.areas).unwrap_or_default(),
            grid_column: match (&self.area_name, self.area) {
                (Some(name), _) => named(name),
                (None, Some(area)) => placement(area.column, area.column_span),
                (None, None) => Default::default(),
            },
            grid_row: match (&self.area_name, self.area) {
                (Some(name), _) => named(name),
                (None, Some(area)) => placement(area.row, area.row_span),
                (None, None) => Default::default(),
            },
            position: if self.offset.is_some() || self.offset_end.is_some() {
                Position::Absolute
            } else {
                Position::Relative
            },
            inset: {
                let (left, top) = self
                    .offset
                    .map_or((None, None), |(x, y)| (Some(x), Some(y)));
                let (right, bottom) = self
                    .offset_end
                    .map_or((None, None), |(x, y)| (Some(x), Some(y)));
                let side = |value: Option<f32>| {
                    value.map_or_else(LengthPercentageAuto::auto, LengthPercentageAuto::length)
                };
                Rect {
                    left: side(left),
                    top: side(top),
                    right: side(right),
                    bottom: side(bottom),
                }
            },
            ..taffy::Style::default()
        }
    }
}

/// An explicit pixel size held to its own minimum and maximum, as CSS holds
/// `width: 400px; max-width: 100px` to 100. The layout engine sizes a
/// container from its children's sizes before it applies their limits, so
/// a parent of such a child would otherwise be as large as the size that was
/// asked for and not the size the child ends up. Anything that is not a pixel
/// size against pixel limits is left to the engine.
fn bounded(size: Length, min: Length, max: Length) -> Length {
    let Length::Px(mut value) = size else {
        return size;
    };
    if let Length::Px(max) = max {
        value = value.min(max);
    }
    if let Length::Px(min) = min {
        // A minimum above the maximum wins, as in CSS.
        value = value.max(min);
    }
    Length::Px(value)
}

/// Concrete text properties after inheritance; what measurement and paint use.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedText {
    pub foreground: Color,
    pub font_size: f32,
    pub font_family: FontFamily,
    pub font_weight: u16,
    pub italic: bool,
    pub align: TextAlign,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub direction: Direction,
    /// A paragraph direction the application forced.
    pub base_direction: Option<Direction>,
}

impl ResolvedText {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            foreground: theme.text,
            font_size: theme.font_size,
            font_family: theme.font_family.clone(),
            font_weight: 400,
            italic: false,
            align: TextAlign::Start,
            line_height: 1.4,
            letter_spacing: 0.0,
            direction: Direction::Ltr,
            base_direction: None,
        }
    }

    pub fn rtl(&self) -> bool {
        self.direction == Direction::Rtl
    }

    pub fn text_style(&self, wrap: TextWrap) -> TextStyle {
        TextStyle {
            font_size: self.font_size,
            family: self.font_family.clone(),
            weight: self.font_weight,
            italic: self.italic,
            align: self.align,
            line_height: self.line_height,
            letter_spacing: self.letter_spacing,
            wrap,
            rtl: self.rtl(),
            base: self.base_direction.map(|base| base == Direction::Rtl),
        }
    }
}
