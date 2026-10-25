//! Moving, sizing and closing the window from inside it.
//!
//! A window without the system's title bar has no way to be moved or resized
//! but the one the application draws, so these are what a title bar of your
//! own is made of. They are requests: the view asks, the host does it, and
//! on a press the platform takes the pointer over for as long as the drag
//! lasts.

/// Which edge or corner a resize pulls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl ResizeEdge {
    /// Every edge and corner, in the order `Ui::resize_frame` declares them.
    pub const ALL: [Self; 8] = [
        Self::North,
        Self::South,
        Self::West,
        Self::East,
        Self::NorthWest,
        Self::NorthEast,
        Self::SouthWest,
        Self::SouthEast,
    ];

    /// A short name, which is also the key each grip is declared under.
    pub fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::NorthEast => "north-east",
            Self::NorthWest => "north-west",
            Self::SouthEast => "south-east",
            Self::SouthWest => "south-west",
        }
    }
}

/// What a view asks its window to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowCommand {
    /// Moves the window with the pointer until it is released. Ask on a
    /// press; the platform runs the drag itself, so no move or release
    /// arrives while it lasts.
    Drag,
    /// The same, pulling an edge or a corner.
    Resize(ResizeEdge),
    Minimize,
    /// Maximizes it, or restores it when it already is.
    ToggleMaximize,
    Close,
}

/// A window `Effects::open_window` opened, for `App::view_window`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

impl WindowId {
    /// The window the app opens first, which declares with `App::view`.
    pub const MAIN: WindowId = WindowId(0);
}

/// A window to open: its title, its logical size, and the window it is modal
/// over, if it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowRequest {
    pub title: String,
    pub size: Option<glassmere_scene::Size>,
    pub modal_owner: Option<WindowId>,
}

/// What the host says about the window, for a view that draws its own frame.
/// Everything here is of the last frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowInfo {
    pub maximized: bool,
    pub focused: bool,
}

/// What the three caption buttons look like. On Windows they are the
/// system's own: the same glyphs from the same font, at the size and in the
/// boxes the shell uses, so a title bar an application draws is not a
/// near-miss of the one beside it. Everywhere else they are drawn from
/// characters every font has.
pub(crate) mod caption {
    use glassmere_scene::Color;

    /// The font the glyphs come from, when the platform has one of its own.
    /// Windows 10 has the same code points in Segoe MDL2 Assets, which the
    /// text system falls back to, and says so as a font diagnostic.
    #[cfg(windows)]
    pub(crate) const FONT: Option<&&str> = Some(&"Segoe Fluent Icons");
    #[cfg(not(windows))]
    pub(crate) const FONT: Option<&&str> = None;

    /// Segoe's caption glyphs, and plain characters where there is no Segoe.
    #[cfg(windows)]
    pub(crate) const MINIMIZE: &str = "\u{e921}";
    #[cfg(windows)]
    pub(crate) const MAXIMIZE: &str = "\u{e922}";
    #[cfg(windows)]
    pub(crate) const RESTORE: &str = "\u{e923}";
    #[cfg(windows)]
    pub(crate) const CLOSE: &str = "\u{e8bb}";
    #[cfg(not(windows))]
    pub(crate) const MINIMIZE: &str = "\u{2013}";
    #[cfg(not(windows))]
    pub(crate) const MAXIMIZE: &str = "\u{25a1}";
    #[cfg(not(windows))]
    pub(crate) const RESTORE: &str = "\u{2750}";
    #[cfg(not(windows))]
    pub(crate) const CLOSE: &str = "\u{2715}";

    /// The box each one sits in and the glyph's size in it, which are the
    /// shell's own measurements at a scale factor of one.
    pub(crate) const WIDTH: f32 = 46.0;
    pub(crate) const HEIGHT: f32 = 32.0;
    pub(crate) const SIZE: f32 = 10.0;

    /// How much of the text colour a hovered or pressed button is washed
    /// with, out of 255.
    pub(crate) const WASH_HOVER: u8 = 26;
    pub(crate) const WASH_PRESSED: u8 = 46;

    /// What closing looks like under the pointer, which is the one colour
    /// every desktop agrees on.
    pub(crate) const CLOSE_HOVER: Color = Color::rgb(196, 43, 28);
    pub(crate) const CLOSE_PRESSED: Color = Color::rgb(155, 35, 23);
}
