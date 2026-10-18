use crate::Theme;
use crate::animation::{Animation, Effect, Transform, Transition};
use glassmere_scene::{Color, FontFamily, TextStyle, TextWrap};

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
}

impl Track {
    pub fn repeat(count: u16, tracks: Vec<Track>) -> Self {
        Self::Repeat(count, tracks)
    }
}

/// Explicit grid placement in 1-based lines; spans default to one cell.
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
    pub gap: f32,
    pub grow: f32,
    /// Flex shrink factor; children do not shrink unless this is set.
    pub shrink: f32,
    pub radius: f32,
    pub align_items: Align,
    pub justify_content: Justify,
    pub align_self: Option<Align>,
    /// Grid items only: inline-axis alignment inside the cell.
    pub justify_self: Option<Align>,
    /// Grid containers: explicit column and row templates.
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
    /// Sizes for implicitly created tracks; `Auto` when empty.
    pub auto_columns: Vec<Track>,
    pub auto_rows: Vec<Track>,
    /// Grid items only: explicit placement instead of auto-flow.
    pub area: Option<GridArea>,
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
            gap: 12.0,
            grow: 0.0,
            shrink: 0.0,
            radius: 0.0,
            align_items: Align::Start,
            justify_content: Justify::Start,
            align_self: None,
            justify_self: None,
            columns: Vec::new(),
            rows: Vec::new(),
            auto_columns: Vec::new(),
            auto_rows: Vec::new(),
            area: None,
            background: None,
            background_image: None,
            foreground: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            italic: None,
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
            direction: None,
        }
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
    setter!(gap, gap, f32);
    setter!(grow, grow, f32);
    setter!(shrink, shrink, f32);
    setter!(radius, radius, f32);
    setter!(align_items, align_items, Align);
    setter!(justify_content, justify_content, Justify);
    setter!(align_self, align_self, Align, some);
    setter!(justify_self, justify_self, Align, some);
    setter!(columns, columns, Vec<Track>);
    setter!(rows, rows, Vec<Track>);
    setter!(auto_columns, auto_columns, Vec<Track>);
    setter!(auto_rows, auto_rows, Vec<Track>);
    setter!(area, area, GridArea, some);
    setter!(wrap, wrap, TextWrap);
    setter!(opacity, opacity, f32);
    setter!(transform, transform, Transform);
    setter!(transition, transition, Transition, some);
    setter!(layout_transition, layout_transition, Transition, some);
    setter!(enter, enter, Effect, some);
    setter!(exit, exit, Effect, some);
    setter!(animation, animation, Animation, some);
    setter!(direction, direction, Direction, some);
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Some((x, y));
        self
    }
    setter!(background, background, Color, some);
    setter!(foreground, foreground, Color, some);
    setter!(font_size, font_size, f32, some);
    setter!(font_family, font_family, FontFamily, some);
    setter!(font_weight, font_weight, u16, some);
    setter!(italic, italic, bool, some);

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
            direction: self.direction.unwrap_or(inherited.direction),
            base_direction: self.base_direction.or(inherited.base_direction),
        }
    }

    pub(crate) fn layout(&self, flow: Flow) -> taffy::Style {
        use taffy::prelude::*;
        use taffy::style::{
            AlignContent, GridPlacement, GridTemplateComponent, MaxTrackSizingFunction,
            MinTrackSizingFunction, RepetitionCount, TrackSizingFunction,
        };
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
        let justify = |value: Justify| match value {
            Justify::Start => AlignContent::Start,
            Justify::Center => AlignContent::Center,
            Justify::End => AlignContent::End,
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
                Track::Repeat(..) => minmax(auto(), auto()),
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
                    other => GridTemplateComponent::Single(sizing(other)),
                })
                .collect()
        };
        let placement = |start: u16, count: u16| Line::<GridPlacement> {
            start: line(i16::try_from(start.max(1)).unwrap_or(i16::MAX)),
            end: span(count.max(1)),
        };
        taffy::Style {
            display: match flow {
                Flow::Grid => Display::Grid,
                Flow::Row | Flow::Column => Display::Flex,
            },
            size: Size {
                width: dimension(self.width),
                height: dimension(self.height),
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
            padding: Rect::length(self.padding.max(0.0)),
            gap: Size::length(self.gap.max(0.0)),
            flex_direction: if flow == Flow::Row {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            },
            align_items: Some(align(self.align_items)),
            // `Start` keeps the engine's normal behavior, which lets grid tracks fill.
            justify_content: (self.justify_content != Justify::Start)
                .then(|| justify(self.justify_content)),
            align_self: self.align_self.map(align),
            justify_self: self.justify_self.map(align),
            flex_grow: self.grow.max(0.0),
            flex_shrink: self.shrink.max(0.0),
            grid_template_columns: template(&self.columns),
            grid_template_rows: template(&self.rows),
            grid_auto_columns: self.auto_columns.iter().map(sizing).collect(),
            grid_auto_rows: self.auto_rows.iter().map(sizing).collect(),
            grid_column: self.area.map_or_else(Default::default, |area| {
                placement(area.column, area.column_span)
            }),
            grid_row: self
                .area
                .map_or_else(Default::default, |area| placement(area.row, area.row_span)),
            position: if self.offset.is_some() {
                Position::Absolute
            } else {
                Position::Relative
            },
            inset: self.offset.map_or_else(Rect::auto, |(x, y)| Rect {
                left: LengthPercentageAuto::length(x),
                top: LengthPercentageAuto::length(y),
                right: LengthPercentageAuto::auto(),
                bottom: LengthPercentageAuto::auto(),
            }),
            ..taffy::Style::default()
        }
    }
}

/// Concrete text properties after inheritance; what measurement and paint use.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedText {
    pub foreground: Color,
    pub font_size: f32,
    pub font_family: FontFamily,
    pub font_weight: u16,
    pub italic: bool,
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
            wrap,
            rtl: self.rtl(),
            base: self.base_direction.map(|base| base == Direction::Rtl),
        }
    }
}
