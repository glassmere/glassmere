//! Shared vocabulary of the multi-line editors: highlighting, options and the
//! operations input queues until the next declaration can apply them to the
//! app-owned document.
use crate::{Length, Motion, Style, TextDocument};
use rust_gui_scene::{Color, TextSpan};
use std::ops::Range;

/// Colour and font changes for a byte range of one line.
#[derive(Clone, Debug, PartialEq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub color: Color,
    pub weight: Option<u16>,
    pub italic: bool,
}

/// Syntax colouring, asked line by line for the visible lines in ascending
/// order. Implementations keep their own per-line state and use
/// `TextDocument::revision` and `changed_since` to reuse it across frames.
pub trait Highlighter {
    /// Ascending, non-overlapping spans over `document.line(line)`.
    fn highlight(&mut self, document: &TextDocument, line: usize) -> Vec<HighlightSpan>;
}

pub struct CodeEditorOptions<'a> {
    /// Accessible name.
    pub label: String,
    pub style: Style,
    pub font_size: f32,
    pub line_numbers: bool,
    /// Selection, copy and navigation still work.
    pub read_only: bool,
    pub highlighter: Option<&'a mut dyn Highlighter>,
}

impl Default for CodeEditorOptions<'_> {
    fn default() -> Self {
        Self {
            label: "Editor".into(),
            style: Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Px(320.0))
                .padding(8.0)
                .radius(8.0),
            font_size: 14.0,
            line_numbers: true,
            read_only: false,
            highlighter: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CodeEditorResponse {
    /// The document's text changed during this declaration.
    pub changed: bool,
    /// The primary selection changed during this declaration.
    pub selection_changed: bool,
}

/// How far a click selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Granularity {
    Char,
    Word,
    Line,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AreaOp {
    Insert(String),
    Backspace {
        word: bool,
    },
    Delete {
        word: bool,
    },
    Newline,
    Indent,
    Outdent,
    Move {
        motion: Motion,
        select: bool,
    },
    Pointer {
        line: usize,
        byte: usize,
        select: bool,
        granularity: Granularity,
    },
    SelectAll,
    CollapseSelections,
    Undo,
    Redo,
    Clipboard {
        cut: bool,
    },
}

/// One declared line: its text and the styled runs that paint it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AreaLine {
    pub text: String,
    pub spans: Vec<TextSpan>,
}

/// A selection clipped to the declared lines, as `(line, byte)` positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AreaSelection {
    pub start: (usize, usize),
    pub end: (usize, usize),
    pub cursor: (usize, usize),
    pub primary: bool,
}

/// Styled runs for a line: highlighter spans with the gaps filled in.
pub(crate) fn line_spans(text: &str, highlights: &[HighlightSpan]) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let mut at = 0;
    for highlight in highlights {
        let start = highlight.range.start.clamp(at, text.len());
        let end = highlight.range.end.clamp(start, text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) || start == end {
            continue;
        }
        if start > at {
            spans.push(TextSpan::new(&text[at..start]));
        }
        spans.push(TextSpan {
            text: text[start..end].to_owned(),
            color: Some(highlight.color),
            weight: highlight.weight,
            italic: highlight.italic.then_some(true),
            family: None,
        });
        at = end;
    }
    if at < text.len() || spans.is_empty() {
        spans.push(TextSpan::new(&text[at..]));
    }
    spans
}
