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

/// What the host says about the window, for a view that draws its own frame.
/// Everything here is of the last frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowInfo {
    pub maximized: bool,
    pub focused: bool,
}
