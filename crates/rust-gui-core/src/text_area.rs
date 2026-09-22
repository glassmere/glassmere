//! Shared vocabulary of the multi-line editors: highlighting, search,
//! diagnostics, options and the operations input queues until the next
//! declaration can apply them to the app-owned document.
use crate::{Length, Motion, Style, TextDocument};
use rust_gui_scene::{Color, Point, TextSpan};
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
    /// Called once per declaration before the visible lines are asked for.
    fn begin(&mut self, _document: &TextDocument) {}

    /// Ascending, non-overlapping spans over `document.line(line)`.
    fn highlight(&mut self, document: &TextDocument, line: usize) -> Vec<HighlightSpan>;

    /// True while lines were returned plain because parsing has not reached
    /// them yet; the editor keeps asking for frames until it turns false.
    fn pending(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Hint,
    Info,
    Warning,
    Error,
}

impl Severity {
    pub(crate) fn color(self) -> Color {
        match self {
            Self::Error => Color(229, 83, 75, 255),
            Self::Warning => Color(229, 192, 70, 255),
            Self::Info => Color(90, 160, 230, 255),
            Self::Hint => Color(140, 150, 160, 255),
        }
    }
}

/// An app-supplied problem over a char range: underlined in the text, marked
/// in the gutter, and its message shown when the pointer rests on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub range: Range<usize>,
    pub severity: Severity,
    pub message: String,
}

/// Editor commands beyond text editing. Hosts send them as `Event::Editor`
/// (the desktop host maps Ctrl+F, Ctrl+H, F3, Shift+F3, Ctrl+D,
/// Ctrl+Alt+Up/Down and Alt+click); apps may send them from menus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorCommand {
    Find,
    Replace,
    FindNext,
    FindPrevious,
    CloseFind,
    /// A caret selects its word; a selection adds its next occurrence.
    SelectNextOccurrence,
    AddCursorAbove,
    AddCursorBelow,
    /// Adds a caret under this window point.
    AddCursorAt(Point),
    /// Rich text editors: toggle a mark on the selection or for typing.
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    ToggleStrikethrough,
    ToggleCode,
}

/// App-owned search state of one editor. While `open`, the editor shows its
/// find bar and highlights every match.
#[derive(Clone, Debug, Default)]
pub struct FindState {
    pub open: bool,
    /// Also show the replace row.
    pub replace: bool,
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Read the query as a regular expression. `whole_word` is ignored then;
    /// write the boundaries into the pattern.
    pub regex: bool,
    matches: Vec<Range<usize>>,
    searched: Option<(u64, String, bool, bool, bool)>,
    error: Option<String>,
}

impl FindState {
    /// Matches of the last declaration, ascending.
    pub fn matches(&self) -> &[Range<usize>] {
        &self.matches
    }

    /// Why the last search found nothing, when it was the query's fault.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Searches again only when the document or the query changed. The find
    /// bar calls this itself; an application that searches without one calls
    /// it before reading `matches`.
    pub fn refresh(&mut self, document: &TextDocument) {
        let key = (
            document.revision(),
            self.query.clone(),
            self.case_sensitive,
            self.whole_word,
            self.regex,
        );
        if self.searched.as_ref() != Some(&key) {
            (self.matches, self.error) = if self.regex {
                match document.find_all_regex(&self.query, self.case_sensitive) {
                    Ok(matches) => (matches, None),
                    Err(error) => (Vec::new(), Some(error)),
                }
            } else {
                (
                    document.find_all(&self.query, self.case_sensitive, self.whole_word),
                    None,
                )
            };
            self.searched = Some(key);
        }
    }

    /// Selects the next or previous match from the primary selection, wrapping.
    pub(crate) fn step(&mut self, document: &mut TextDocument, forward: bool) -> bool {
        self.refresh(document);
        let current = document.primary().range();
        let target = if forward {
            self.matches
                .iter()
                .find(|range| range.start >= current.end && **range != current)
                .or(self.matches.first())
        } else {
            self.matches
                .iter()
                .rev()
                .find(|range| range.end <= current.start && **range != current)
                .or(self.matches.last())
        };
        match target.cloned() {
            Some(range) => {
                document.set_selection(range.start, range.end);
                true
            }
            None => false,
        }
    }

    /// Replaces the selected match and moves on; selects the next match first
    /// when the selection is not one.
    pub(crate) fn replace_one(&mut self, document: &mut TextDocument) -> bool {
        self.refresh(document);
        let current = document.primary().range();
        if !self.matches.contains(&current) {
            return self.step(document, true);
        }
        let changed = document.replace_range(current, &self.replacement);
        self.step(document, true);
        changed
    }

    pub(crate) fn replace_all(&mut self, document: &mut TextDocument) -> bool {
        self.refresh(document);
        let edits = self
            .matches
            .iter()
            .map(|range| (range.clone(), self.replacement.clone()))
            .collect();
        document.replace_ranges(edits)
    }
}

pub struct CodeEditorOptions<'a> {
    /// Accessible name.
    pub label: String,
    pub style: Style,
    pub font_size: f32,
    pub line_numbers: bool,
    /// Selection, copy, search and navigation still work.
    pub read_only: bool,
    pub highlighter: Option<&'a mut dyn Highlighter>,
    /// Enables the find bar and match highlighting.
    pub find: Option<&'a mut FindState>,
    pub diagnostics: &'a [Diagnostic],
    /// Typing an opening bracket or quote inserts its partner.
    pub auto_close: bool,
    /// Marks the bracket at the caret and its partner.
    pub match_brackets: bool,
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
            find: None,
            diagnostics: &[],
            auto_close: true,
            match_brackets: true,
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
        /// Alt+click: keep the other selections.
        add: bool,
    },
    SelectAll,
    CollapseSelections,
    Undo,
    Redo,
    Clipboard {
        cut: bool,
    },
    Command(EditorCommand),
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MarkKind {
    Match,
    CurrentMatch,
    Bracket,
    Diagnostic(Severity, String),
}

/// A decorated byte range of one declared line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AreaMark {
    pub line: usize,
    pub from: usize,
    pub to: usize,
    pub kind: MarkKind,
}

/// Splits a char range into per-line marks inside the declared lines.
pub(crate) fn marks_for(
    document: &TextDocument,
    range: Range<usize>,
    kind: &MarkKind,
    lines: Range<usize>,
    out: &mut Vec<AreaMark>,
) {
    let (start, end) = (document.position(range.start), document.position(range.end));
    for line in start.0.max(lines.start)..=end.0.min(lines.end.saturating_sub(1)) {
        let from = if line == start.0 { start.1 } else { 0 };
        let to = if line == end.0 {
            end.1
        } else {
            document.line(line).len()
        };
        if to > from {
            out.push(AreaMark {
                line,
                from,
                to,
                kind: kind.clone(),
            });
        }
    }
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
