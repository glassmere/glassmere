use rust_gui_scene::{Rect, TextGeometry, TextMeasurer, TextStyle, TextWrap};
use std::{collections::VecDeque, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMove {
    Left,
    Right,
    Start,
    End,
}

#[derive(Clone, Debug)]
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
}

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

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.range()]
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

    pub fn composition_range(&self) -> Option<Range<usize>> {
        self.composition.as_ref().map(|composition| {
            let start = self.selection.range().start;
            start..start + composition.text.len()
        })
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

    pub fn apply(&mut self, edit: TextEdit) -> bool {
        let before = (self.text.clone(), self.selection, self.composition.clone());
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
                let cursor = match direction {
                    TextMove::Start => 0,
                    TextMove::End => self.text.len(),
                    TextMove::Left if !select && !range.is_empty() => range.start,
                    TextMove::Right if !select && !range.is_empty() => range.end,
                    TextMove::Left => self.adjacent(false, word),
                    TextMove::Right => self.adjacent(true, word),
                };
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
        before != (self.text.clone(), self.selection, self.composition.clone())
    }

    pub fn prepare(
        &mut self,
        measurer: &mut impl TextMeasurer,
        mut style: TextStyle,
        width: f32,
    ) -> bool {
        style.wrap = TextWrap::None;
        let text = self.display();
        let key = (text, style);
        let changed = self.geometry_key.as_ref() != Some(&key);
        if changed {
            self.geometry = measurer.geometry(&key.0, &key.1, None);
            self.geometry_key = Some(key);
        }
        let old_scroll = self.scroll_x;
        if let Some(cursor) = self.display_cursor() {
            let x = self.geometry.caret(cursor).x;
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
        changed || self.scroll_x != old_scroll
    }

    pub fn caret(&self) -> Option<Rect> {
        self.display_cursor()
            .map(|cursor| self.geometry.caret(cursor))
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
        assert_eq!(editor.selected_text(), "hello");
    }
}
