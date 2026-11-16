//! The caret, the movements and the indentation that text editors share,
//! apart from the document that holds the text.
use alloc::string::String;
use core::ops::Range;

/// A caret (`anchor == cursor`) or a directed range, in chars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub cursor: usize,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            cursor: at,
        }
    }
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
    pub fn is_caret(self) -> bool {
        self.anchor == self.cursor
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    /// First non-blank character, or column zero when already there.
    LineStart,
    LineEnd,
    Up,
    Down,
    /// Up or down by this many lines, keeping the column.
    Lines(isize),
    DocumentStart,
    DocumentEnd,
}

/// What Tab inserts and how wide a tab character counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indentation {
    pub unit: String,
    pub tab_width: usize,
}

impl Default for Indentation {
    fn default() -> Self {
        Self {
            unit: "    ".into(),
            tab_width: 4,
        }
    }
}
