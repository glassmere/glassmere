use glassmere_scene::{Affinity, Rect, TextGeometry, TextMeasurer, TextStyle, TextWrap};
use std::{collections::VecDeque, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

/// What the Left and Right arrow keys mean in bidirectional text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaretMovement {
    /// Left is left on screen, across direction boundaries.
    #[default]
    Visual,
    /// Left is backward in the text, whichever way that looks.
    Logical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextMove {
    Left,
    Right,
    /// Line start and end.
    Start,
    End,
    /// Same as `Start` and `End` in single-line inputs.
    DocumentStart,
    DocumentEnd,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum TextEdit {
    Insert(String),
    Backspace {
        word: bool,
    },
    Delete {
        word: bool,
    },
    Move {
        direction: TextMove,
        select: bool,
        word: bool,
    },
    SelectAll,
    Select {
        anchor: usize,
        cursor: usize,
    },
    Undo,
    Redo,
    Preedit {
        text: String,
        selection: Option<(usize, usize)>,
    },
    Commit(String),
    CancelComposition,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub cursor: usize,
}

impl TextSelection {
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
}

#[derive(Clone)]
struct Snapshot {
    text: String,
    selection: TextSelection,
}

#[derive(Clone, Debug, PartialEq)]
struct Composition {
    text: String,
    selection: Option<(usize, usize)>,
}

pub(crate) struct Editor {
    pub text: String,
    pub selection: TextSelection,
    composition: Option<Composition>,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
    pub geometry: TextGeometry,
    pub scroll_x: f32,
    geometry_key: Option<(String, TextStyle)>,
    /// Which neighbour the caret belongs to at a direction boundary.
    pub affinity: Affinity,
    /// Left and Right move on screen instead of in reading order.
    pub visual: bool,
    /// Shift that right-aligns text shorter than a right-to-left field.
    pub align_x: f32,
    /// A password field: every character is drawn as a bullet, and the
    /// geometry, so every offset into it, is of the bullets.
    pub masked: bool,
}

/// What a password field draws for each character of its text.
const BULLET: char = '\u{2022}';

pub(crate) fn single_line(text: &str) -> String {
    text.chars()
        .filter_map(|ch| match ch {
            '\n' | '\r' | '\t' => Some(' '),
            ch if ch.is_control() => None,
            ch => Some(ch),
        })
        .collect()
}

fn boundary(text: &str, byte: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .take_while(|index| *index <= byte)
        .last()
        .unwrap_or(0)
}

impl Editor {
    pub fn new(text: &str) -> Self {
        let text = single_line(text);
        let end = text.len();
        Self {
            text,
            selection: TextSelection {
                anchor: end,
                cursor: end,
            },
            composition: None,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            geometry: TextGeometry::default(),
            scroll_x: 0.0,
            geometry_key: None,
            affinity: Affinity::Downstream,
            visual: true,
            align_x: 0.0,
            masked: false,
        }
    }

    pub fn synchronize(&mut self, text: &str) {
        let text = single_line(text);
        if text != self.text {
            self.text = text;
            self.selection = TextSelection {
                anchor: boundary(&self.text, self.selection.anchor),
                cursor: boundary(&self.text, self.selection.cursor),
            };
            self.composition = None;
            self.undo.clear();
            self.redo.clear();
        }
    }

    /// What a copy takes, which a password field never gives.
    pub fn selected_text(&self) -> Option<&str> {
        (!self.masked).then(|| &self.text[self.selection.range()])
    }
    pub fn composing(&self) -> bool {
        self.composition.is_some()
    }

    pub fn display(&self) -> String {
        let Some(composition) = &self.composition else {
            return self.text.clone();
        };
        let range = self.selection.range();
        format!(
            "{}{}{}",
            &self.text[..range.start],
            composition.text,
            &self.text[range.end..]
        )
    }

    /// The text as drawn and shaped: `display`, or its bullets.
    pub(crate) fn shown(&self) -> String {
        let display = self.display();
        if self.masked {
            display.graphemes(true).map(|_| BULLET).collect()
        } else {
            display
        }
    }

    /// Where an offset into `display` falls in what is shown.
    pub(crate) fn shown_offset(&self, byte: usize) -> usize {
        if self.masked {
            self.display()[..byte].graphemes(true).count() * BULLET.len_utf8()
        } else {
            byte
        }
    }

    pub(crate) fn shown_range(&self, range: Range<usize>) -> Range<usize> {
        self.shown_offset(range.start)..self.shown_offset(range.end)
    }

    /// The offset into `display` of an offset into what is shown.
    pub(crate) fn display_offset(&self, byte: usize) -> usize {
        if !self.masked {
            return byte;
        }
        let display = self.display();
        display
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .nth(byte / BULLET.len_utf8())
            .unwrap_or(display.len())
    }

    pub fn composition_range(&self) -> Option<Range<usize>> {
        self.composition.as_ref().map(|composition| {
            let start = self.selection.range().start;
            start..start + composition.text.len()
        })
    }

    /// What assistive technology reads: the text as shown, its shaping and
    /// the selection in the offsets of that text. A password gives nothing,
    /// and so does a field that has not been shaped for what it shows.
    pub(crate) fn readable(&self) -> Option<(String, &TextGeometry, TextSelection)> {
        let shown = self.shown();
        if self.masked || self.geometry_key.as_ref().is_none_or(|key| key.0 != shown) {
            return None;
        }
        let selection = match self.composition_range() {
            Some(range) => {
                let caret = self.display_cursor().unwrap_or(range.end);
                TextSelection {
                    anchor: caret,
                    cursor: caret,
                }
            }
            None => self.selection,
        };
        Some((shown, &self.geometry, selection))
    }

    pub fn display_cursor(&self) -> Option<usize> {
        match &self.composition {
            Some(composition) => composition.selection.map(|(_, cursor)| {
                self.selection.range().start + boundary(&composition.text, cursor)
            }),
            None => Some(self.selection.cursor),
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            selection: self.selection,
        }
    }

    fn push_history(history: &mut VecDeque<Snapshot>, snapshot: Snapshot) {
        history.push_back(snapshot);
        // Keep editing history bounded even for unusually long single-line fields.
        while history.len() > 128
            || history.iter().map(|entry| entry.text.len()).sum::<usize>() > 1024 * 1024
        {
            history.pop_front();
        }
    }

    fn replace(&mut self, replacement: &str) {
        let replacement = single_line(replacement);
        let range = self.selection.range();
        let mut text = self.text.clone();
        text.replace_range(range.clone(), &replacement);
        if text != self.text {
            let snapshot = self.snapshot();
            Self::push_history(&mut self.undo, snapshot);
            self.redo.clear();
            self.text = text;
        }
        let cursor = boundary(&self.text, range.start + replacement.len());
        self.selection = TextSelection {
            anchor: cursor,
            cursor,
        };
    }

    fn adjacent(&self, right: bool, word: bool) -> usize {
        let cursor = self.selection.cursor;
        // Word steps in a password would tell where its spaces are.
        if word && self.masked {
            return if right { self.text.len() } else { 0 };
        }
        if word {
            let indices: Vec<_> = self
                .text
                .unicode_word_indices()
                .map(|(index, word)| (index, index + word.len()))
                .collect();
            if right {
                indices
                    .iter()
                    .map(|(_, end)| *end)
                    .find(|end| *end > cursor)
                    .unwrap_or(self.text.len())
            } else {
                indices
                    .iter()
                    .map(|(start, _)| *start)
                    .take_while(|start| *start < cursor)
                    .last()
                    .unwrap_or(0)
            }
        } else if right {
            self.text
                .grapheme_indices(true)
                .map(|(byte, _)| byte)
                .find(|byte| *byte > cursor)
                .unwrap_or(self.text.len())
        } else {
            self.text
                .grapheme_indices(true)
                .map(|(byte, _)| byte)
                .take_while(|byte| *byte < cursor)
                .last()
                .unwrap_or(0)
        }
    }

    /// One visual step, when the shaped geometry still matches the text.
    fn visual_step(&self, right: bool) -> Option<Option<(usize, Affinity)>> {
        let current = self
            .geometry_key
            .as_ref()
            .is_some_and(|(text, _)| *text == self.text);
        (self.visual && current).then(|| {
            self.geometry
                .visual_step(self.selection.cursor, self.affinity, right)
        })
    }

    pub fn apply(&mut self, edit: TextEdit) -> bool {
        let before = (
            self.text.clone(),
            self.selection,
            self.composition.clone(),
            self.affinity,
        );
        let moving = matches!(edit, TextEdit::Move { .. });
        let typed = matches!(edit, TextEdit::Insert(_) | TextEdit::Commit(_));
        match edit {
            TextEdit::Preedit { text, selection } => {
                self.composition = (!text.is_empty()).then(|| Composition {
                    text: single_line(&text),
                    selection,
                });
            }
            TextEdit::CancelComposition => self.composition = None,
            TextEdit::Insert(text) | TextEdit::Commit(text) => {
                self.composition = None;
                self.replace(&text);
            }
            TextEdit::SelectAll => {
                self.composition = None;
                self.selection = TextSelection {
                    anchor: 0,
                    cursor: self.text.len(),
                };
            }
            TextEdit::Select { anchor, cursor } => {
                self.composition = None;
                self.selection = TextSelection {
                    anchor: boundary(&self.text, anchor),
                    cursor: boundary(&self.text, cursor),
                };
            }
            TextEdit::Move {
                direction,
                select,
                word,
            } => {
                self.composition = None;
                let range = self.selection.range();
                let right = direction == TextMove::Right;
                let horizontal = matches!(direction, TextMove::Left | TextMove::Right);
                let mut affinity = Affinity::Downstream;
                let cursor = match (direction, self.visual_step(right)) {
                    (TextMove::Start | TextMove::DocumentStart, _) => 0,
                    (TextMove::End | TextMove::DocumentEnd, _) => {
                        affinity = Affinity::Upstream;
                        self.text.len()
                    }
                    // A selection collapses to its edge on the side of the arrow.
                    (_, step) if horizontal && !select && !range.is_empty() => {
                        let x = |byte| self.geometry.caret_at(byte, Affinity::Downstream).x;
                        let flipped = step.is_some() && x(range.start) > x(range.end);
                        if right != flipped {
                            range.end
                        } else {
                            range.start
                        }
                    }
                    (_, Some(step)) => match step {
                        // In an RTL run the visual direction runs against the text.
                        Some((byte, _)) if word => {
                            self.adjacent(byte > self.selection.cursor, true)
                        }
                        Some((byte, side)) => {
                            affinity = side;
                            byte
                        }
                        None => {
                            affinity = self.affinity;
                            self.selection.cursor
                        }
                    },
                    (_, None) => self.adjacent(right, word),
                };
                self.affinity = affinity;
                self.selection.cursor = cursor;
                if !select {
                    self.selection.anchor = cursor;
                }
            }
            TextEdit::Backspace { word } | TextEdit::Delete { word } => {
                self.composition = None;
                if self.selection.range().is_empty() {
                    self.selection.anchor =
                        self.adjacent(matches!(edit, TextEdit::Delete { .. }), word);
                }
                self.replace("");
            }
            TextEdit::Undo => {
                self.composition = None;
                if let Some(snapshot) = self.undo.pop_back() {
                    let current = self.snapshot();
                    Self::push_history(&mut self.redo, current);
                    self.text = snapshot.text;
                    self.selection = snapshot.selection;
                }
            }
            TextEdit::Redo => {
                self.composition = None;
                if let Some(snapshot) = self.redo.pop_back() {
                    let current = self.snapshot();
                    Self::push_history(&mut self.undo, current);
                    self.text = snapshot.text;
                    self.selection = snapshot.selection;
                }
            }
        }
        if !moving {
            // Typed text keeps the caret with the character just entered.
            self.affinity = if typed && self.selection.cursor > 0 {
                Affinity::Upstream
            } else {
                Affinity::Downstream
            };
        }
        before
            != (
                self.text.clone(),
                self.selection,
                self.composition.clone(),
                self.affinity,
            )
    }

    pub fn prepare(
        &mut self,
        measurer: &mut impl TextMeasurer,
        mut style: TextStyle,
        width: f32,
        rtl: bool,
    ) -> bool {
        style.wrap = TextWrap::None;
        let text = self.shown();
        let key = (text, style);
        let changed = self.geometry_key.as_ref() != Some(&key);
        if changed {
            self.geometry = measurer.geometry(&key.0, &key.1, None);
            self.geometry_key = Some(key);
        }
        let old_scroll = self.scroll_x;
        if let Some(caret) = self.caret() {
            let x = caret.x;
            if x < self.scroll_x {
                self.scroll_x = x;
            }
            if x > self.scroll_x + width - 2.0 {
                self.scroll_x = x - width + 2.0;
            }
        }
        self.scroll_x = self
            .scroll_x
            .clamp(0.0, (self.geometry.size.width - width + 2.0).max(0.0));
        let old_align = self.align_x;
        self.align_x = if rtl {
            (width - 2.0 - self.geometry.size.width).max(0.0)
        } else {
            0.0
        };
        changed || self.scroll_x != old_scroll || self.align_x != old_align
    }

    pub fn caret(&self) -> Option<Rect> {
        let affinity = if self.composing() {
            Affinity::Downstream
        } else {
            self.affinity
        };
        self.display_cursor()
            .map(|cursor| self.geometry.caret_at(self.shown_offset(cursor), affinity))
    }

    /// Direction to flag on the caret, when its line mixes directions.
    pub fn caret_direction(&self) -> Option<bool> {
        let cursor = self.shown_offset(self.display_cursor()?);
        self.geometry.mixed_direction(cursor, self.affinity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grapheme_delete_undo_redo_and_selection_preserve_utf8() {
        let mut editor = Editor::new("a👩‍💻e\u{301}");
        editor.apply(TextEdit::Backspace { word: false });
        assert_eq!(editor.text, "a👩‍💻");
        editor.apply(TextEdit::Backspace { word: false });
        assert_eq!(editor.text, "a");
        editor.apply(TextEdit::Undo);
        assert_eq!(editor.text, "a👩‍💻");
        editor.apply(TextEdit::Redo);
        assert_eq!(editor.text, "a");
        editor.apply(TextEdit::SelectAll);
        editor.apply(TextEdit::Insert("日本語".into()));
        assert_eq!(editor.text, "日本語");
    }

    #[test]
    fn preedit_is_transient_and_commit_is_one_undo_unit() {
        let mut editor = Editor::new("hello");
        editor.apply(TextEdit::SelectAll);
        editor.apply(TextEdit::Preedit {
            text: "にほん".into(),
            selection: Some((9, 9)),
        });
        assert_eq!(editor.text, "hello");
        assert_eq!(editor.display(), "にほん");
        editor.apply(TextEdit::Preedit {
            text: String::new(),
            selection: None,
        });
        editor.apply(TextEdit::Commit("日本".into()));
        assert_eq!(editor.text, "日本");
        editor.apply(TextEdit::Undo);
        assert_eq!(editor.text, "hello");
        assert_eq!(editor.selected_text(), Some("hello"));
    }
}
