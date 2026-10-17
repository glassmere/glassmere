//! Shared vocabulary of the multi-line editors: highlighting, search,
//! diagnostics, options and the operations input queues until the next
//! declaration can apply them to the app-owned document.
use crate::{Length, Motion, Style, TextDocument};
use seaglass_scene::{Color, Point, TextSpan};
use std::borrow::Cow;
use std::cell::RefCell;
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

    fn key(&self, document: &TextDocument) -> (u64, String, bool, bool, bool) {
        (
            document.revision(),
            self.query.clone(),
            self.case_sensitive,
            self.whole_word,
            self.regex,
        )
    }

    fn search(&self, document: &TextDocument) -> (Vec<Range<usize>>, Option<String>) {
        if self.regex {
            match document.find_all_regex(&self.query, self.case_sensitive) {
                Ok(matches) => (matches, None),
                Err(error) => (Vec::new(), Some(error)),
            }
        } else {
            (
                document.find_all(&self.query, self.case_sensitive, self.whole_word),
                None,
            )
        }
    }

    /// Searches again only when the document or the query changed. `apply`
    /// calls this itself; an application that searches without it calls it
    /// before reading `matches`.
    pub fn refresh(&mut self, document: &TextDocument) {
        let key = self.key(document);
        if self.searched.as_ref() != Some(&key) {
            (self.matches, self.error) = self.search(document);
            self.searched = Some(key);
        }
    }

    /// The matches and error `refresh` would leave, without keeping them:
    /// what the find bar reads in a view that only has `&FindState`. The last
    /// search is reused while it is current, which it is unless the document
    /// changed outside `apply`.
    pub(crate) fn found(
        &self,
        document: &TextDocument,
    ) -> (Cow<'_, [Range<usize>]>, Option<Cow<'_, str>>) {
        if self.searched.as_ref() == Some(&self.key(document)) {
            (
                Cow::Borrowed(&self.matches),
                self.error.as_deref().map(Cow::Borrowed),
            )
        } else {
            let (matches, error) = self.search(document);
            (Cow::Owned(matches), error.map(Cow::Owned))
        }
    }

    /// Applies one operation of a code editor that has a find bar: the
    /// editor commands that open, close and step through the search, the bar's
    /// own edits (`TextOp::Find`), and everything else through
    /// [`TextDocument::apply_with`]. Call it from `update` instead of
    /// `TextDocument::apply`; the matches are searched again here, so the view
    /// reads them without writing.
    pub fn apply<M: 'static>(
        &mut self,
        document: &mut TextDocument,
        op: TextOp,
        effects: &mut crate::Effects<'_, M>,
    ) {
        match op {
            TextOp::Command(search @ (EditorCommand::Find | EditorCommand::Replace)) => {
                self.open = true;
                self.replace |= search == EditorCommand::Replace;
                // Search for the selection, as editors do.
                let selected = document.selected_text();
                if !selected.is_empty() && !selected.contains('\n') {
                    self.query = selected;
                }
            }
            TextOp::Command(EditorCommand::CloseFind) => self.open = false,
            TextOp::Command(EditorCommand::FindNext) => {
                self.step(document, true);
            }
            TextOp::Command(EditorCommand::FindPrevious) => {
                self.step(document, false);
            }
            TextOp::Find(edit) => match edit {
                FindEdit::Query(query) => self.query = query,
                FindEdit::Replacement(replacement) => self.replacement = replacement,
                FindEdit::CaseSensitive(on) => self.case_sensitive = on,
                FindEdit::WholeWord(on) => self.whole_word = on,
                FindEdit::Regex(on) => self.regex = on,
                FindEdit::ReplaceOne => {
                    self.replace_one(document);
                }
                FindEdit::ReplaceAll => {
                    self.replace_all(document);
                }
            },
            op => document.apply_with(op, effects),
        }
        if self.open {
            self.refresh(document);
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
    /// The view only has `&self`, so the highlighter lives in a `RefCell` the
    /// application owns; the editor borrows it while it declares.
    pub highlighter: Option<&'a RefCell<dyn Highlighter + 'a>>,
    /// Enables the find bar and match highlighting. The application applies
    /// the bar's messages with `FindState::apply`.
    pub find: Option<&'a FindState>,
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

/// How far a click selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Granularity {
    Char,
    Word,
    Line,
}

/// One edit to a text area's document, as a message carries it.
#[derive(Clone, Debug, PartialEq)]
pub enum TextOp {
    Insert(String),
    /// Typing an opening bracket or quote, with its partner (auto-close).
    InsertPaired(String),
    Backspace {
        word: bool,
    },
    /// Backspace that also removes an empty pair (auto-close).
    BackspacePaired,
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
    /// An edit of the find bar; `FindState::apply` handles it.
    Find(FindEdit),
}

/// What the find bar of a code editor sends when the user changes it. Search
/// steps and closing the bar travel as `EditorCommand`s.
#[derive(Clone, Debug, PartialEq)]
pub enum FindEdit {
    Query(String),
    Replacement(String),
    CaseSensitive(bool),
    WholeWord(bool),
    Regex(bool),
    ReplaceOne,
    ReplaceAll,
}

impl TextOp {
    /// Whether the op changes the text. A read-only editor drops these and
    /// keeps the ones that move, select, copy or search.
    pub(crate) fn is_edit(&self) -> bool {
        matches!(
            self,
            Self::Insert(_)
                | Self::InsertPaired(_)
                | Self::Backspace { .. }
                | Self::BackspacePaired
                | Self::Delete { .. }
                | Self::Newline
                | Self::Indent
                | Self::Outdent
                | Self::Undo
                | Self::Redo
                | Self::Find(FindEdit::ReplaceOne | FindEdit::ReplaceAll)
        )
    }
}

impl TextDocument {
    /// [`TextDocument::apply`] that also puts the text of a copy or cut on the
    /// clipboard, through `effects`, before applying it. A copy or cut is
    /// serviced in the batch that carries it, whether or not a view follows.
    pub fn apply_with<M: 'static>(&mut self, op: TextOp, effects: &mut crate::Effects<'_, M>) {
        if matches!(op, TextOp::Clipboard { .. }) {
            effects.copy(crate::Clip::text(self.copy_text()));
        }
        self.apply(op);
    }

    /// Applies one operation the way a code editor does, without auto-close
    /// and without a find bar, which need state of the editor. A cut removes
    /// the selection; the text a copy or cut puts on the clipboard is
    /// `copy_text()`, read before applying, which `apply_with` does.
    pub fn apply(&mut self, op: TextOp) {
        match op {
            TextOp::Move { motion, select } => self.move_cursors(motion, select),
            TextOp::Pointer {
                line,
                byte,
                select,
                granularity,
                add,
            } => {
                let offset = self.offset(line, byte);
                let (anchor, cursor) = match granularity {
                    Granularity::Char if select => (self.primary().anchor, offset),
                    Granularity::Char => (offset, offset),
                    Granularity::Word => {
                        let word = self.word_at(offset);
                        (word.start, word.end)
                    }
                    Granularity::Line => {
                        let range = self.line_range(line);
                        (range.start, range.end)
                    }
                };
                if add {
                    self.add_selection(anchor, cursor);
                } else {
                    self.set_selection(anchor, cursor);
                }
            }
            TextOp::SelectAll => self.select_all(),
            TextOp::CollapseSelections => self.collapse_selections(),
            TextOp::Clipboard { cut } => {
                if cut {
                    self.cut();
                }
            }
            TextOp::Command(command) => match command {
                EditorCommand::SelectNextOccurrence => {
                    self.select_next_occurrence();
                }
                EditorCommand::AddCursorAbove => {
                    self.add_cursor_vertically(false);
                }
                EditorCommand::AddCursorBelow => {
                    self.add_cursor_vertically(true);
                }
                // Search needs a find bar, and the rest is pointer or rich text.
                _ => {}
            },
            // The find bar's own edits belong to `FindState::apply`.
            TextOp::Find(_) => {}
            TextOp::Insert(text) => {
                self.insert(&text);
            }
            TextOp::InsertPaired(text) => {
                self.insert_paired(&text);
            }
            TextOp::Backspace { word } => {
                self.backspace(word);
            }
            TextOp::BackspacePaired => {
                self.backspace_paired();
            }
            TextOp::Delete { word } => {
                self.delete(word);
            }
            TextOp::Newline => {
                self.newline();
            }
            TextOp::Indent => {
                self.indent();
            }
            TextOp::Outdent => {
                self.outdent();
            }
            TextOp::Undo => {
                self.undo();
            }
            TextOp::Redo => {
                self.redo();
            }
        }
    }
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
