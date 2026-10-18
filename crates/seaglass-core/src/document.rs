//! The app-owned text document behind code and rich text editors: a rope,
//! one or more selections and edit-based undo. Offsets are `char` indexes.
use ropey::Rope;
use std::collections::VecDeque;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

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

#[derive(Clone, Debug)]
struct Edit {
    /// Start in the text before the transaction; edits ascend and never overlap.
    at: usize,
    removed: String,
    inserted: String,
}

#[derive(Clone, Debug)]
struct Transaction {
    edits: Vec<Edit>,
    before: Vec<Selection>,
    after: Vec<Selection>,
    /// Plain typing merges with the typing right before it.
    typing: bool,
}

const HISTORY: usize = 1000;
const CHANGE_LOG: usize = 64;

#[derive(Clone, Debug)]
pub struct TextDocument {
    rope: Rope,
    /// Ascending and non-overlapping; the primary one drives scrolling.
    selections: Vec<Selection>,
    primary: usize,
    /// Display columns vertical moves try to return to, parallel to `selections`.
    goals: Vec<Option<usize>>,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    revision: u64,
    saved: u64,
    changes: VecDeque<(u64, usize)>,
    crlf: bool,
    pub indentation: Indentation,
}

impl Default for TextDocument {
    fn default() -> Self {
        Self::from_text("")
    }
}

fn word_class(ch: char) -> u8 {
    if ch.is_alphanumeric() || ch == '_' {
        2
    } else if ch.is_whitespace() {
        0
    } else {
        1
    }
}

impl TextDocument {
    pub fn new() -> Self {
        Self::default()
    }

    /// Line endings are normalized to `\n` inside the document; `text` writes
    /// `\r\n` back when the source used it.
    pub fn from_text(text: &str) -> Self {
        let crlf = text.contains("\r\n");
        Self {
            rope: Rope::from_str(&normalize(text)),
            selections: vec![Selection::default()],
            primary: 0,
            goals: vec![None],
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            saved: 0,
            changes: VecDeque::new(),
            crlf,
            indentation: Indentation::default(),
        }
    }

    /// Replaces the content, clearing history and selections.
    pub fn set_text(&mut self, text: &str) {
        let indentation = std::mem::take(&mut self.indentation);
        let revision = self.revision + 1;
        *self = Self::from_text(text);
        self.indentation = indentation;
        self.revision = revision;
        self.changes.push_back((revision, 0));
    }

    pub fn text(&self) -> String {
        let text = String::from(&self.rope);
        if self.crlf {
            text.replace('\n', "\r\n")
        } else {
            text
        }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Line content without its line ending.
    pub fn line(&self, line: usize) -> String {
        if line >= self.len_lines() {
            return String::new();
        }
        let mut text = String::from(self.rope.line(line));
        if text.ends_with('\n') {
            text.pop();
        }
        text
    }

    fn line_len(&self, line: usize) -> usize {
        let slice = self.rope.line(line);
        let len = slice.len_chars();
        if len > 0 && slice.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    /// Bumped by every content change, undo and redo included.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn is_modified(&self) -> bool {
        self.revision != self.saved
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.revision;
    }

    /// The first line touched since `revision`, for incremental consumers such
    /// as highlighters. `None` when nothing changed; line zero when the change
    /// log no longer reaches back that far.
    pub fn changed_since(&self, revision: u64) -> Option<usize> {
        if revision >= self.revision {
            return None;
        }
        let oldest = self.changes.front().map_or(self.revision, |entry| entry.0);
        if revision + 1 < oldest {
            return Some(0);
        }
        self.changes
            .iter()
            .filter(|(at, _)| *at > revision)
            .map(|(_, line)| *line)
            .min()
    }

    /// `(line, byte column)` of a char offset.
    pub fn position(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.len_chars());
        let line = self.rope.char_to_line(offset);
        let start = self.rope.line_to_char(line);
        let bytes = self.rope.char_to_byte(offset) - self.rope.char_to_byte(start);
        (line, bytes)
    }

    /// Char offset of a `(line, byte column)`, clamped into the line and onto
    /// a char boundary.
    pub fn offset(&self, line: usize, byte: usize) -> usize {
        let line = line.min(self.len_lines().saturating_sub(1));
        let start = self.rope.line_to_char(line);
        let text = self.line(line);
        let mut byte = byte.min(text.len());
        while !text.is_char_boundary(byte) {
            byte -= 1;
        }
        start + text[..byte].chars().count()
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }
    pub fn primary(&self) -> Selection {
        self.selections[self.primary]
    }

    pub fn set_selection(&mut self, anchor: usize, cursor: usize) {
        let len = self.len_chars();
        self.selections = vec![Selection {
            anchor: anchor.min(len),
            cursor: cursor.min(len),
        }];
        self.primary = 0;
        self.goals = vec![None];
    }

    /// Adds a selection and makes it primary; overlapping selections merge.
    pub fn add_selection(&mut self, anchor: usize, cursor: usize) {
        let len = self.len_chars();
        let added = Selection {
            anchor: anchor.min(len),
            cursor: cursor.min(len),
        };
        self.selections.push(added);
        self.merge_selections(Some(added));
    }

    /// Keeps only the primary selection.
    pub fn collapse_selections(&mut self) {
        let primary = self.primary();
        self.selections = vec![primary];
        self.primary = 0;
        self.goals = vec![None];
    }

    pub fn select_all(&mut self) {
        self.set_selection(0, self.len_chars());
    }

    /// Selected text; several selections join with newlines.
    pub fn selected_text(&self) -> String {
        self.selections
            .iter()
            .filter(|selection| !selection.is_caret())
            .map(|selection| String::from(self.rope.slice(selection.range())))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// What copy puts on the clipboard: the selection, or the caret's whole
    /// line when nothing is selected.
    pub fn copy_text(&self) -> String {
        if self
            .selections
            .iter()
            .any(|selection| !selection.is_caret())
        {
            return self.selected_text();
        }
        let (line, _) = self.position(self.primary().cursor);
        format!("{}\n", self.line(line))
    }

    /// Removes what `copy_text` returned.
    pub fn cut(&mut self) -> String {
        let text = self.copy_text();
        if self.selections.iter().all(|selection| selection.is_caret()) {
            let lines: Vec<usize> = self
                .selections
                .iter()
                .map(|selection| self.position(selection.cursor).0)
                .collect();
            let mut edits: Vec<(Range<usize>, String)> = Vec::new();
            for line in lines {
                let start = self.rope.line_to_char(line);
                let end = self.rope.line_to_char((line + 1).min(self.len_lines()));
                if edits.last().is_none_or(|(range, _)| range.end <= start) {
                    edits.push((start..end.max(start), String::new()));
                }
            }
            self.edit(edits, false);
        } else {
            self.insert("");
        }
        text
    }

    fn merge_selections(&mut self, primary: Option<Selection>) {
        let primary = primary.unwrap_or_else(|| self.primary());
        let mut order: Vec<Selection> = std::mem::take(&mut self.selections);
        order.sort_by_key(|selection| (selection.range().start, selection.range().end));
        let mut merged: Vec<Selection> = Vec::new();
        for selection in order {
            match merged.last_mut() {
                Some(last)
                    if selection.range().start < last.range().end
                        || (selection.range().start == last.range().start) =>
                {
                    let (start, end) = (
                        last.range().start,
                        last.range().end.max(selection.range().end),
                    );
                    let forward = selection.cursor >= selection.anchor;
                    *last = if forward {
                        Selection {
                            anchor: start,
                            cursor: end,
                        }
                    } else {
                        Selection {
                            anchor: end,
                            cursor: start,
                        }
                    };
                }
                _ => merged.push(selection),
            }
        }
        self.primary = merged
            .iter()
            .position(|selection| {
                selection.range().start <= primary.cursor && primary.cursor <= selection.range().end
            })
            .unwrap_or(0);
        self.goals = vec![None; merged.len()];
        self.selections = merged;
    }

    /// Applies ascending, non-overlapping replacements as one undo step and
    /// leaves a caret after each inserted text.
    fn edit(&mut self, edits: Vec<(Range<usize>, String)>, typing: bool) -> bool {
        if edits
            .iter()
            .all(|(range, text)| range.is_empty() && text.is_empty())
        {
            return false;
        }
        let before = self.selections.clone();
        let mut recorded = Vec::with_capacity(edits.len());
        let first_line = self.rope.char_to_line(edits[0].0.start);
        for (range, text) in edits.iter().rev() {
            let removed = String::from(self.rope.slice(range.clone()));
            self.rope.remove(range.clone());
            self.rope.insert(range.start, text);
            recorded.push(Edit {
                at: range.start,
                removed,
                inserted: text.clone(),
            });
        }
        recorded.reverse();
        let mut delta = 0isize;
        let mut after = Vec::with_capacity(recorded.len());
        for edit in &recorded {
            let inserted = edit.inserted.chars().count() as isize;
            let at = (edit.at as isize + delta + inserted) as usize;
            after.push(Selection::caret(at));
            delta += inserted - edit.removed.chars().count() as isize;
        }
        // Whole-line operations can leave fewer carets than there were selections.
        self.selections = after;
        self.primary = self.primary.min(self.selections.len() - 1);
        self.merge_selections(None);
        let merge = typing
            && self.undo.last().is_some_and(|last| {
                last.typing && last.after == before && last.edits.len() == recorded.len()
            })
            && recorded
                .iter()
                .all(|edit| edit.removed.is_empty() && !edit.inserted.contains('\n'));
        if merge {
            let last = self.undo.last_mut().unwrap();
            for (old, new) in last.edits.iter_mut().zip(&recorded) {
                old.inserted.push_str(&new.inserted);
            }
            last.after = self.selections.clone();
        } else {
            self.undo.push(Transaction {
                edits: recorded,
                before,
                after: self.selections.clone(),
                typing,
            });
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.touch(first_line);
        true
    }

    fn touch(&mut self, line: usize) {
        self.revision += 1;
        self.changes.push_back((self.revision, line));
        if self.changes.len() > CHANGE_LOG {
            self.changes.pop_front();
        }
    }

    /// Replaces every selection with `text`.
    pub fn insert(&mut self, text: &str) -> bool {
        let text = normalize(text);
        let edits = self
            .selections
            .iter()
            .map(|selection| (selection.range(), text.clone()))
            .collect();
        let typing = text.chars().count() == 1 && self.selections.iter().all(|s| s.is_caret());
        self.edit(edits, typing)
    }

    /// Typing with bracket and quote pairing: an opener also inserts its closer
    /// (or wraps the selection), and typing a closer that is already there
    /// steps over it. Other text is a plain `insert`.
    pub fn insert_paired(&mut self, text: &str) -> bool {
        const PAIRS: [(char, char); 5] =
            [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('`', '`')];
        let mut chars = text.chars();
        let (Some(typed), None) = (chars.next(), chars.next()) else {
            return self.insert(text);
        };
        let next_is = |document: &Self, selection: &Selection, ch: char| {
            selection.is_caret()
                && selection.cursor < document.len_chars()
                && document.rope.char(selection.cursor) == ch
        };
        let closer = PAIRS.iter().any(|(_, close)| *close == typed);
        if closer
            && self
                .selections
                .iter()
                .all(|selection| next_is(self, selection, typed))
        {
            self.move_cursors(Motion::Right, false);
            return true;
        }
        let Some((_, close)) = PAIRS.iter().find(|(open, _)| *open == typed) else {
            return self.insert(text);
        };
        // A quote right after a word is an apostrophe or a closing quote.
        let after_word = |selection: &Selection| {
            let start = selection.range().start;
            start > 0 && word_class(self.rope.char(start - 1)) == 2
        };
        if typed == *close && self.selections.iter().any(after_word) {
            return self.insert(text);
        }
        let edits: Vec<(Range<usize>, String)> = self
            .selections
            .iter()
            .map(|selection| {
                let inner = String::from(self.rope.slice(selection.range()));
                (selection.range(), format!("{typed}{inner}{close}"))
            })
            .collect();
        let changed = self.edit(edits, false);
        // Carets sit after each closer: step back inside the pair.
        for selection in &mut self.selections {
            *selection = Selection::caret(selection.cursor - 1);
        }
        if let Some(last) = self.undo.last_mut() {
            last.after = self.selections.clone();
        }
        changed
    }

    /// Backspace that removes an empty pair (`(|)`) as a whole.
    pub fn backspace_paired(&mut self) -> bool {
        let inside_pair = |selection: &Selection| {
            let at = selection.cursor;
            selection.is_caret()
                && at > 0
                && at < self.len_chars()
                && matches!(
                    (self.rope.char(at - 1), self.rope.char(at)),
                    ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('`', '`')
                )
        };
        if !self.selections.iter().all(inside_pair) {
            return self.backspace(false);
        }
        let edits = self
            .selections
            .iter()
            .map(|selection| (selection.cursor - 1..selection.cursor + 1, String::new()))
            .collect();
        self.edit(edits, false)
    }

    /// Replaces one range, for find and replace. Selections collapse after it.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) -> bool {
        self.replace_ranges(vec![(range, text.to_owned())])
    }

    /// Replaces ascending, non-overlapping ranges as one undo step.
    pub fn replace_ranges(&mut self, mut edits: Vec<(Range<usize>, String)>) -> bool {
        let len = self.len_chars();
        edits.retain(|(range, _)| range.start <= range.end && range.end <= len);
        edits.sort_by_key(|(range, _)| range.start);
        let mut end = 0;
        edits.retain(|(range, _)| {
            let keep = range.start >= end;
            if keep {
                end = range.end;
            }
            keep
        });
        if edits.is_empty() {
            return false;
        }
        for (_, text) in &mut edits {
            *text = normalize(text);
        }
        self.edit(edits, false)
    }

    pub fn backspace(&mut self, word: bool) -> bool {
        let edits = self.deletions(if word { Motion::WordLeft } else { Motion::Left });
        self.edit(edits, false)
    }

    pub fn delete(&mut self, word: bool) -> bool {
        let edits = self.deletions(if word {
            Motion::WordRight
        } else {
            Motion::Right
        });
        self.edit(edits, false)
    }

    fn deletions(&self, motion: Motion) -> Vec<(Range<usize>, String)> {
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        for selection in &self.selections {
            let range = if selection.is_caret() {
                let target = self.moved(selection.cursor, motion, &mut None);
                target.min(selection.cursor)..target.max(selection.cursor)
            } else {
                selection.range()
            };
            match edits.last_mut() {
                Some((last, _)) if range.start < last.end => last.end = last.end.max(range.end),
                _ => edits.push((range, String::new())),
            }
        }
        edits
    }

    /// Breaks the line, keeping its indentation and adding one unit after an
    /// opening bracket.
    pub fn newline(&mut self) -> bool {
        let mut splits = Vec::new();
        let edits = self
            .selections
            .iter()
            .map(|selection| {
                let range = selection.range();
                let (line, byte) = self.position(range.start);
                let text = self.line(line);
                let head = &text[..byte];
                let mut indent: String = head
                    .chars()
                    .take_while(|ch| *ch == ' ' || *ch == '\t')
                    .collect();
                let opened = head.trim_end().ends_with(['{', '[', '(']);
                let base = indent.clone();
                if opened {
                    indent.push_str(&self.indentation.unit);
                }
                // `{|}` becomes three lines with the caret on the middle one.
                let closes = opened
                    && text[byte.min(text.len())..].starts_with(['}', ']', ')'])
                    && selection.is_caret();
                if closes {
                    splits.push(base.chars().count() + 1);
                    (range, format!("\n{indent}\n{base}"))
                } else {
                    splits.push(0);
                    (range, format!("\n{indent}"))
                }
            })
            .collect();
        let changed = self.edit(edits, false);
        if changed && splits.iter().any(|back| *back > 0) && splits.len() == self.selections.len() {
            for (selection, back) in self.selections.iter_mut().zip(&splits) {
                *selection = Selection::caret(selection.cursor - back);
            }
            if let Some(last) = self.undo.last_mut() {
                last.after = self.selections.clone();
            }
        }
        changed
    }

    fn touched_lines(&self) -> Vec<usize> {
        let mut lines = Vec::new();
        for selection in &self.selections {
            let range = selection.range();
            let first = self.position(range.start).0;
            let (mut last, byte) = self.position(range.end);
            // A selection ending at column zero does not include that line.
            if byte == 0 && last > first {
                last -= 1;
            }
            for line in first..=last {
                if lines.last() != Some(&line) {
                    lines.push(line);
                }
            }
        }
        lines
    }

    fn shift_lines(&mut self, change: impl Fn(&str) -> (usize, String)) -> bool {
        // (chars removed at the line start, text inserted there)
        let before = self.selections.clone();
        let edits: Vec<(Range<usize>, String)> = self
            .touched_lines()
            .into_iter()
            .filter_map(|line| {
                let start = self.rope.line_to_char(line);
                let (removed, inserted) = change(&self.line(line));
                (removed > 0 || !inserted.is_empty()).then(|| (start..start + removed, inserted))
            })
            .collect();
        if edits.is_empty() {
            return false;
        }
        let shifts: Vec<(usize, isize)> = edits
            .iter()
            .map(|(range, text)| {
                (
                    range.start,
                    text.chars().count() as isize - range.len() as isize,
                )
            })
            .collect();
        let changed = self.edit(edits, false);
        // Keep the user's selections instead of carets after each line edit.
        let map = |offset: usize| {
            let mut delta = 0isize;
            for (at, shift) in &shifts {
                // An offset at the line start stays in front of new indentation.
                if *at < offset {
                    delta += (*shift).max(*at as isize - offset as isize);
                }
            }
            (offset as isize + delta).max(0) as usize
        };
        self.selections = before
            .iter()
            .map(|selection| Selection {
                anchor: map(selection.anchor),
                cursor: map(selection.cursor),
            })
            .collect();
        self.merge_selections(None);
        if let Some(last) = self.undo.last_mut() {
            last.after = self.selections.clone();
        }
        changed
    }

    /// Tab: indents the selected lines, or inserts indentation at carets.
    pub fn indent(&mut self) -> bool {
        let multi_line = self.selections.iter().any(|selection| {
            self.position(selection.range().start).0 != self.position(selection.range().end).0
        });
        if multi_line {
            let unit = self.indentation.unit.clone();
            return self.shift_lines(|line| {
                if line.is_empty() {
                    (0, String::new())
                } else {
                    (0, unit.clone())
                }
            });
        }
        let edits = self
            .selections
            .iter()
            .map(|selection| {
                let range = selection.range();
                let text = if self.indentation.unit == "\t" {
                    "\t".to_owned()
                } else {
                    let (line, byte) = self.position(range.start);
                    let column = self.display_column(&self.line(line)[..byte]);
                    let width = self.indentation.unit.chars().count().max(1);
                    " ".repeat(width - column % width)
                };
                (range, text)
            })
            .collect();
        self.edit(edits, false)
    }

    /// Shift+Tab: removes up to one indentation unit from the touched lines.
    pub fn outdent(&mut self) -> bool {
        let width = self.indentation.unit.chars().count().max(1);
        self.shift_lines(|line| {
            let removed = if line.starts_with('\t') {
                1
            } else {
                line.chars().take(width).take_while(|ch| *ch == ' ').count()
            };
            (removed, String::new())
        })
    }

    fn display_column(&self, head: &str) -> usize {
        let tab = self.indentation.tab_width.max(1);
        head.graphemes(true).fold(0, |column, grapheme| {
            if grapheme == "\t" {
                column + tab - column % tab
            } else {
                column + 1
            }
        })
    }

    fn column_offset(&self, line: usize, goal: usize) -> usize {
        let text = self.line(line);
        let tab = self.indentation.tab_width.max(1);
        let (mut column, mut chars) = (0, 0);
        for grapheme in text.graphemes(true) {
            let next = if grapheme == "\t" {
                column + tab - column % tab
            } else {
                column + 1
            };
            if next > goal {
                break;
            }
            column = next;
            chars += grapheme.chars().count();
        }
        self.rope.line_to_char(line) + chars
    }

    /// Where `motion` takes a caret at `from`. `goal` carries the sticky column
    /// across consecutive vertical moves.
    fn moved(&self, from: usize, motion: Motion, goal: &mut Option<usize>) -> usize {
        let len = self.len_chars();
        let (line, byte) = self.position(from);
        let vertical = |lines: isize, goal: &mut Option<usize>| {
            let column = *goal.get_or_insert_with(|| self.display_column(&self.line(line)[..byte]));
            let last = self.len_lines() as isize - 1;
            let target = line as isize + lines;
            if target < 0 {
                return 0;
            }
            if target > last {
                return len;
            }
            self.column_offset(target as usize, column)
        };
        if !matches!(motion, Motion::Up | Motion::Down | Motion::Lines(_)) {
            *goal = None;
        }
        match motion {
            Motion::Left => {
                if byte == 0 {
                    return from.saturating_sub(1);
                }
                let text = self.line(line);
                let previous = text[..byte]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(index, _)| index);
                from - text[previous..byte].chars().count()
            }
            Motion::Right => {
                let text = self.line(line);
                match text[byte..].graphemes(true).next() {
                    Some(grapheme) => from + grapheme.chars().count(),
                    None => (from + 1).min(len),
                }
            }
            Motion::WordLeft => {
                let mut at = from;
                while at > 0 && word_class(self.rope.char(at - 1)) == 0 {
                    at -= 1;
                }
                if at > 0 {
                    let class = word_class(self.rope.char(at - 1));
                    while at > 0 && word_class(self.rope.char(at - 1)) == class {
                        at -= 1;
                    }
                }
                at
            }
            Motion::WordRight => {
                let mut at = from;
                if at < len {
                    let class = word_class(self.rope.char(at));
                    while at < len && class != 0 && word_class(self.rope.char(at)) == class {
                        at += 1;
                    }
                }
                while at < len && word_class(self.rope.char(at)) == 0 {
                    at += 1;
                }
                at
            }
            Motion::LineStart => {
                let text = self.line(line);
                let blank = text
                    .chars()
                    .take_while(|ch| *ch == ' ' || *ch == '\t')
                    .count();
                let start = self.rope.line_to_char(line);
                if from == start + blank {
                    start
                } else {
                    start + blank
                }
            }
            Motion::LineEnd => self.rope.line_to_char(line) + self.line_len(line),
            Motion::Up => vertical(-1, goal),
            Motion::Down => vertical(1, goal),
            Motion::Lines(lines) => vertical(lines, goal),
            Motion::DocumentStart => 0,
            Motion::DocumentEnd => len,
        }
    }

    /// Moves every cursor. Without `select`, a range collapses to its edge in
    /// the direction of a horizontal move, as editors do.
    pub fn move_cursors(&mut self, motion: Motion, select: bool) {
        let mut goals = std::mem::take(&mut self.goals);
        goals.resize(self.selections.len(), None);
        let moved: Vec<Selection> = self
            .selections
            .iter()
            .zip(goals.iter_mut())
            .map(|(selection, goal)| {
                if !select && !selection.is_caret() {
                    match motion {
                        Motion::Left => {
                            *goal = None;
                            return Selection::caret(selection.range().start);
                        }
                        Motion::Right => {
                            *goal = None;
                            return Selection::caret(selection.range().end);
                        }
                        _ => {}
                    }
                }
                let cursor = self.moved(selection.cursor, motion, goal);
                Selection {
                    anchor: if select { selection.anchor } else { cursor },
                    cursor,
                }
            })
            .collect();
        self.selections = moved;
        let primary = self.primary();
        let kept = goals;
        self.merge_selections(Some(primary));
        if self.goals.len() == kept.len() {
            self.goals = kept;
        }
        self.break_typing();
    }

    /// The word (or the run of punctuation or blanks) around `offset`.
    pub fn word_at(&self, offset: usize) -> Range<usize> {
        let len = self.len_chars();
        let offset = offset.min(len);
        let (line, _) = self.position(offset);
        let (start, end) = (
            self.rope.line_to_char(line),
            self.rope.line_to_char(line) + self.line_len(line),
        );
        if start == end {
            return offset..offset;
        }
        let at = offset.min(end - 1);
        let class = word_class(self.rope.char(at));
        let (mut from, mut to) = (at, at + 1);
        while from > start && word_class(self.rope.char(from - 1)) == class {
            from -= 1;
        }
        while to < end && word_class(self.rope.char(to)) == class {
            to += 1;
        }
        from..to
    }

    /// Every match of a regular expression, ascending, or why the pattern
    /// could not be used. Matches never cross a line.
    pub fn find_all_regex(
        &self,
        pattern: &str,
        case_sensitive: bool,
    ) -> Result<Vec<Range<usize>>, String> {
        if pattern.is_empty() {
            return Ok(Vec::new());
        }
        #[cfg(not(feature = "regex"))]
        {
            let _ = case_sensitive;
            Err("regular expressions need the `regex` feature".to_owned())
        }
        #[cfg(feature = "regex")]
        {
            let expression = regex::RegexBuilder::new(pattern)
                .case_insensitive(!case_sensitive)
                .size_limit(1 << 20)
                .build()
                .map_err(|error| {
                    // The first line of the crate's error names the fault.
                    error
                        .to_string()
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("invalid pattern")
                        .trim()
                        .to_owned()
                })?;
            let mut matches = Vec::new();
            let mut start = 0;
            for line in self.rope.lines() {
                let text = line.to_string();
                for found in expression.find_iter(&text) {
                    // An empty match would never move on, and highlights nothing.
                    if !found.is_empty() {
                        matches.push(start + found.start()..start + found.end());
                    }
                }
                start += text.len();
            }
            Ok(matches)
        }
    }

    /// All occurrences of `query`, ascending and non-overlapping, as char
    /// ranges. Matches do not span lines.
    // Compares lowercased chars line by line; move to a streaming
    // searcher if documents reach tens of megabytes.
    pub fn find_all(
        &self,
        query: &str,
        case_sensitive: bool,
        whole_word: bool,
    ) -> Vec<Range<usize>> {
        let fold = |ch: char| {
            if case_sensitive {
                ch
            } else {
                ch.to_lowercase().next().unwrap_or(ch)
            }
        };
        let needle: Vec<char> = query.chars().map(fold).collect();
        if needle.is_empty() || needle.contains(&'\n') {
            return Vec::new();
        }
        let mut matches = Vec::new();
        let mut start = 0;
        for line in self.rope.lines() {
            let chars: Vec<char> = line.chars().map(fold).collect();
            let mut at = 0;
            while at + needle.len() <= chars.len() {
                let word = |index: Option<usize>| {
                    index
                        .and_then(|index| chars.get(index))
                        .is_some_and(|ch| word_class(*ch) == 2)
                };
                if chars[at..at + needle.len()] == needle[..]
                    && !(whole_word && (word(at.checked_sub(1)) || word(Some(at + needle.len()))))
                {
                    matches.push(start + at..start + at + needle.len());
                    at += needle.len();
                } else {
                    at += 1;
                }
            }
            start += chars.len();
        }
        matches
    }

    /// Ctrl+D: a caret selects its word; a selection adds the next occurrence
    /// of its text (wrapping around) as another selection.
    pub fn select_next_occurrence(&mut self) -> bool {
        let primary = self.primary();
        if primary.is_caret() {
            let word = self.word_at(primary.cursor);
            if word.is_empty() {
                return false;
            }
            let others: Vec<Selection> = self
                .selections
                .iter()
                .copied()
                .filter(|selection| *selection != primary)
                .collect();
            self.selections = others;
            self.add_selection(word.start, word.end);
            return true;
        }
        let text = String::from(self.rope.slice(primary.range()));
        let taken: Vec<Range<usize>> = self.selections.iter().map(|s| s.range()).collect();
        let last = taken.iter().map(|range| range.end).max().unwrap_or(0);
        let matches = self.find_all(&text, true, false);
        let free = |range: &&Range<usize>| !taken.contains(range);
        let next = matches
            .iter()
            .filter(free)
            .find(|range| range.start >= last)
            .or_else(|| matches.iter().find(free))
            .cloned();
        match next {
            Some(range) => {
                self.add_selection(range.start, range.end);
                true
            }
            None => false,
        }
    }

    /// Adds a caret one line above the first or below the last selection, in
    /// the same display column.
    pub fn add_cursor_vertically(&mut self, down: bool) -> bool {
        let edge = if down {
            self.selections[self.selections.len() - 1]
        } else {
            self.selections[0]
        };
        let (line, byte) = self.position(edge.cursor);
        let target = if down { line + 1 } else { line.wrapping_sub(1) };
        if target >= self.len_lines() {
            return false;
        }
        let column = self.display_column(&self.line(line)[..byte]);
        let offset = self.column_offset(target, column);
        self.add_selection(offset, offset);
        true
    }

    /// The bracket at or just before `offset` and its partner, when both exist
    /// within a bounded scan. Brackets in strings and comments count too.
    pub fn matching_bracket(&self, offset: usize) -> Option<(usize, usize)> {
        const SCAN: usize = 20_000;
        let len = self.len_chars();
        let bracket = |at: usize| {
            (at < len)
                .then(|| self.rope.char(at))
                .filter(|ch| "()[]{}".contains(*ch))
        };
        let at = if bracket(offset).is_some() {
            offset
        } else {
            offset.checked_sub(1).filter(|at| bracket(*at).is_some())?
        };
        let ch = bracket(at)?;
        let (partner, forward) = match ch {
            '(' => (')', true),
            '[' => (']', true),
            '{' => ('}', true),
            ')' => ('(', false),
            ']' => ('[', false),
            _ => ('{', false),
        };
        let mut depth = 0usize;
        let mut index = at;
        for _ in 0..SCAN {
            let current = self.rope.char(index);
            if current == ch {
                depth += 1;
            } else if current == partner {
                depth -= 1;
                if depth == 0 {
                    return Some((at, index));
                }
            }
            if forward {
                index += 1;
                if index >= len {
                    return None;
                }
            } else {
                index = index.checked_sub(1)?;
            }
        }
        None
    }

    /// The whole line including its line ending.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let line = line.min(self.len_lines().saturating_sub(1));
        self.rope.line_to_char(line)..self.rope.line_to_char((line + 1).min(self.len_lines()))
    }

    fn break_typing(&mut self) {
        if let Some(last) = self.undo.last_mut() {
            last.typing = false;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.undo.pop() else {
            return false;
        };
        // Earlier edits are restored first, so each `at` is valid again.
        for edit in &transaction.edits {
            let inserted = edit.inserted.chars().count();
            self.rope.remove(edit.at..edit.at + inserted);
            self.rope.insert(edit.at, &edit.removed);
        }
        self.restore(&transaction.before);
        let line = self
            .rope
            .char_to_line(transaction.edits[0].at.min(self.len_chars()));
        self.redo.push(transaction);
        self.touch(line);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(mut transaction) = self.redo.pop() else {
            return false;
        };
        for edit in transaction.edits.iter().rev() {
            let removed = edit.removed.chars().count();
            self.rope.remove(edit.at..edit.at + removed);
            self.rope.insert(edit.at, &edit.inserted);
        }
        self.restore(&transaction.after);
        transaction.typing = false;
        let line = self
            .rope
            .char_to_line(transaction.edits[0].at.min(self.len_chars()));
        self.undo.push(transaction);
        self.touch(line);
        true
    }

    fn restore(&mut self, selections: &[Selection]) {
        self.selections = selections.to_vec();
        self.primary = self.primary.min(self.selections.len().saturating_sub(1));
        self.goals = vec![None; self.selections.len()];
    }
}

fn normalize(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carets(document: &TextDocument) -> Vec<usize> {
        document.selections().iter().map(|s| s.cursor).collect()
    }

    #[test]
    fn typing_merges_into_one_undo_step_and_moves_break_it() {
        let mut document = TextDocument::from_text("");
        for ch in ["h", "i", "!"] {
            document.insert(ch);
        }
        document.move_cursors(Motion::Left, false);
        document.insert("?");
        assert_eq!(document.text(), "hi?!");
        document.undo();
        assert_eq!(document.text(), "hi!");
        document.undo();
        assert_eq!(document.text(), "");
        assert!(!document.undo());
        document.redo();
        document.redo();
        assert_eq!(document.text(), "hi?!");
        assert_eq!(carets(&document), [3]);
        // A new edit drops the redo branch.
        document.undo();
        document.insert("x");
        assert!(!document.can_redo());
    }

    #[test]
    fn graphemes_words_lines_and_sticky_columns_move_as_editors_do() {
        let mut document =
            TextDocument::from_text("let e\u{301}👩‍💻 = foo_bar(1);\n\tx\n\nlonger line");
        document.set_selection(4, 4);
        document.move_cursors(Motion::Right, false);
        assert_eq!(carets(&document), [6]); // e + combining accent
        document.move_cursors(Motion::Right, false);
        assert_eq!(carets(&document), [9]); // the ZWJ emoji is three chars
        document.move_cursors(Motion::WordRight, false);
        document.move_cursors(Motion::WordRight, false);
        let (line, byte) = document.position(document.primary().cursor);
        assert_eq!(&document.line(line)[byte..], "foo_bar(1);");
        document.move_cursors(Motion::WordRight, true);
        assert_eq!(document.selected_text(), "foo_bar");
        document.move_cursors(Motion::LineEnd, false);
        let column = document.position(document.primary().cursor).1;
        assert_eq!(column, document.line(0).len());
        // Down through a short and an empty line and back to the long column.
        document.move_cursors(Motion::Down, false);
        assert_eq!(document.position(document.primary().cursor), (1, 2));
        document.move_cursors(Motion::Down, false);
        assert_eq!(document.position(document.primary().cursor), (2, 0));
        document.move_cursors(Motion::Down, false);
        assert_eq!(document.position(document.primary().cursor), (3, 11));
        // Smart home toggles between the first non-blank and column zero.
        document.set_selection(document.offset(1, 2), document.offset(1, 2));
        document.move_cursors(Motion::LineStart, false);
        assert_eq!(document.position(document.primary().cursor), (1, 1));
        document.move_cursors(Motion::LineStart, false);
        assert_eq!(document.position(document.primary().cursor), (1, 0));
        document.move_cursors(Motion::Left, false);
        assert_eq!(document.position(document.primary().cursor).0, 0);
        document.move_cursors(Motion::Lines(100), false);
        assert_eq!(document.primary().cursor, document.len_chars());
    }

    #[test]
    fn several_cursors_edit_together_and_undo_as_one() {
        let mut document = TextDocument::from_text("a1\na2\na3");
        document.set_selection(0, 1);
        document.add_selection(3, 4);
        document.add_selection(6, 7);
        document.insert("bb");
        assert_eq!(document.text(), "bb1\nbb2\nbb3");
        assert_eq!(carets(&document), [2, 6, 10]);
        document.backspace(false);
        assert_eq!(document.text(), "b1\nb2\nb3");
        document.undo();
        document.undo();
        assert_eq!(document.text(), "a1\na2\na3");
        assert_eq!(document.selections().len(), 3);
        // Overlapping selections merge into one.
        document.set_selection(0, 4);
        document.add_selection(2, 6);
        assert_eq!(
            document.selections(),
            [Selection {
                anchor: 0,
                cursor: 6
            }]
        );
    }

    #[test]
    fn newline_indent_outdent_cut_and_crlf_round_trip() {
        let mut document = TextDocument::from_text("fn main() {\r\n}");
        document.set_selection(11, 11);
        document.newline();
        document.insert("x");
        assert_eq!(document.text(), "fn main() {\r\n    x\r\n}");
        // Indent and outdent whole lines keep the selection.
        document.set_selection(0, document.len_chars());
        document.indent();
        assert_eq!(document.text(), "    fn main() {\r\n        x\r\n    }");
        assert_eq!(document.primary().range(), 0..document.len_chars());
        document.outdent();
        document.outdent();
        assert_eq!(document.text(), "fn main() {\r\nx\r\n}");
        // Tab at a caret pads to the next stop.
        document.set_selection(2, 2);
        document.indent();
        assert_eq!(document.line(0), "fn   main() {");
        document.undo();
        // Copy and cut without a selection take the whole line.
        document.set_selection(13, 13);
        assert_eq!(document.copy_text(), "x\n");
        assert_eq!(document.cut(), "x\n");
        assert_eq!(document.text(), "fn main() {\r\n}");
    }

    #[test]
    fn pairs_close_wrap_step_over_and_delete_together_and_enter_opens_blocks() {
        let mut document = TextDocument::from_text("");
        document.insert_paired("f");
        document.insert_paired("(");
        assert_eq!(
            (document.text().as_str(), carets(&document)),
            ("f()", vec![2])
        );
        document.insert_paired("\"");
        document.insert_paired("x");
        document.insert_paired("\"");
        document.insert_paired(")");
        assert_eq!(
            (document.text().as_str(), carets(&document)),
            ("f(\"x\")", vec![6])
        );
        // A quote typed right after a word does not pair.
        document.set_selection(1, 1);
        document.insert_paired("\"");
        assert_eq!(document.text(), "f\"(\"x\")");
        document.undo();
        document.set_selection(6, 6);
        document.insert_paired(" ");
        document.insert_paired("{");
        document.newline();
        document.insert("body");
        assert_eq!(document.text(), "f(\"x\") {\n    body\n}");
        document.undo();
        document.undo();
        document.backspace_paired();
        assert_eq!(document.text(), "f(\"x\") ");
        // A selection is wrapped.
        document.set_selection(2, 5);
        document.insert_paired("[");
        assert_eq!(document.text(), "f([\"x\"]) ");
    }

    #[test]
    fn search_occurrences_vertical_cursors_and_brackets() {
        let mut document = TextDocument::from_text("foo Foo foobar\nfoo(bar[1], {x})\n  foo");
        assert_eq!(document.find_all("foo", true, false).len(), 4);
        assert_eq!(document.find_all("foo", false, false).len(), 5);
        assert_eq!(
            document.find_all("foo", false, true),
            [0..3, 4..7, 15..18, 34..37]
        );
        assert!(document.find_all("", true, false).is_empty());
        // Ctrl+D: word first, then following occurrences, wrapping around.
        document.set_selection(16, 16);
        assert!(document.select_next_occurrence());
        assert_eq!(document.selected_text(), "foo");
        assert!(document.select_next_occurrence());
        assert!(document.select_next_occurrence());
        assert_eq!(
            document
                .selections()
                .iter()
                .map(|s| s.range())
                .collect::<Vec<_>>(),
            [0..3, 15..18, 34..37]
        );
        document.insert("x");
        assert_eq!(document.text(), "x Foo foobar\nx(bar[1], {x})\n  x");
        document.undo();
        // Carets stack vertically in the same column.
        document.set_selection(2, 2);
        assert!(document.add_cursor_vertically(true));
        assert!(document.add_cursor_vertically(true));
        assert!(!document.add_cursor_vertically(true));
        assert_eq!(carets(&document), [2, 17, 34]);
        // Brackets match at or just before the offset, across nesting.
        assert_eq!(document.matching_bracket(18), Some((18, 30)));
        assert_eq!(document.matching_bracket(31), Some((30, 18)));
        assert_eq!(document.matching_bracket(23), Some((22, 24)));
        assert_eq!(document.matching_bracket(5), None);
    }

    #[test]
    fn revisions_report_the_first_changed_line_and_the_saved_state() {
        let mut document = TextDocument::from_text("one\ntwo\nthree");
        assert!(!document.is_modified());
        let start = document.revision();
        document.set_selection(8, 8);
        document.insert("X");
        document.set_selection(4, 4);
        document.insert("Y");
        assert_eq!(document.changed_since(start), Some(1));
        assert_eq!(document.changed_since(start + 1), Some(1));
        assert_eq!(document.changed_since(document.revision()), None);
        assert!(document.is_modified());
        document.mark_saved();
        assert!(!document.is_modified());
        document.undo();
        assert!(document.is_modified());
        assert!(document.replace_ranges(vec![(0..3, "1".into()), (2..5, "skipped".into())]));
        assert_eq!(document.line(0), "1");
        assert_eq!(document.word_at(3), 2..5);
    }
}
