use alloc::borrow::ToOwned;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;
use glassmere_scene::{Affinity, Rect, TextGeometry, TextMeasurer, TextStyle, TextWrap};
use unicode_segmentation::UnicodeSegmentation;

/// What the Left and Right arrow keys mean in bidirectional text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaretMovement {
    /// Left is left on screen, across direction boundaries.
    #[default]
    Visual,
    /// Left is backward in the text, whichever way that looks.
    Logical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// What `prepare` read when it last found everything in place: floats by
/// their bits, so that equal means the same.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Settled {
    selection: TextSelection,
    affinity: Affinity,
    width: u32,
    rtl: bool,
    scroll_x: u32,
    align_x: u32,
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
    /// The width the geometry wraps at, for text that wraps.
    geometry_limit: Option<f32>,
    /// The inputs of the last `prepare` that found the geometry current and
    /// moved nothing. The same inputs move nothing again. Whatever writes
    /// `geometry` outside `prepare` must clear it.
    settled: Option<Settled>,
    /// The text of a label: it keeps its line breaks and wraps at its box
    /// as drawn, instead of being one line that scrolls.
    pub wrapped: bool,
    /// Which neighbour the caret belongs to at a direction boundary.
    pub affinity: Affinity,
    /// Left and Right move on screen instead of in reading order.
    pub visual: bool,
    /// Shift that right-aligns text shorter than a right-to-left field.
    pub align_x: f32,
    /// A password field: every character is drawn as a bullet, and the
    /// geometry, so every offset into it, is of the bullets.
    pub masked: bool,
    /// A password field whose text the user asked to see. It is the field's
    /// own state, like the caret: the application does not hear of it.
    pub revealed: bool,
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

/// Whether `single_line(text)` is `text`: it holds no control character,
/// which `single_line` turns into a space or drops.
fn is_single_line(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

/// `single_line(text)`, copied whole when there is nothing to strip.
pub(crate) fn single_line_owned(text: &str) -> String {
    if is_single_line(text) {
        text.to_owned()
    } else {
        single_line(text)
    }
}

fn boundary(text: &str, byte: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(core::iter::once(text.len()))
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
            geometry_limit: None,
            settled: None,
            wrapped: false,
            affinity: Affinity::Downstream,
            visual: true,
            align_x: 0.0,
            masked: false,
            revealed: false,
        }
    }

    /// An editor over the text of a label, with the selection at its start.
    pub fn block(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            selection: TextSelection::default(),
            wrapped: true,
            ..Self::new("")
        }
    }

    pub fn synchronize(&mut self, text: &str) {
        let stripped;
        let text: &str = if self.wrapped || is_single_line(text) {
            text
        } else {
            stripped = single_line(text);
            &stripped
        };
        if text != self.text {
            self.text = text.to_owned();
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

    /// What `apply(TextEdit::CancelComposition)` does, without the copies of
    /// the text and the composition `apply` makes to tell whether anything
    /// changed. True when the composition or the caret's affinity changed.
    pub fn cancel_composition(&mut self) -> bool {
        let composing = self.composition.take().is_some();
        let moved = self.affinity != Affinity::Downstream;
        self.affinity = Affinity::Downstream;
        composing || moved
    }

    pub fn apply(&mut self, edit: TextEdit) -> bool {
        if matches!(edit, TextEdit::CancelComposition) {
            return self.cancel_composition();
        }
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
            // `cancel_composition` handles this edit before the match.
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
                    (TextMove::Start | TextMove::End, _) if self.wrapped => {
                        let end = direction == TextMove::End;
                        let (byte, side) =
                            self.geometry
                                .line_edge(self.selection.cursor, self.affinity, end);
                        affinity = side;
                        byte
                    }
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

    /// The furthest a field `width` wide scrolls: its text's end, with room
    /// for the caret after it.
    pub(crate) fn scroll_limit(&self, width: f32) -> f32 {
        (self.geometry.size.width - width + 2.0).max(0.0)
    }

    pub fn prepare(
        &mut self,
        measurer: &mut impl TextMeasurer,
        mut style: TextStyle,
        width: f32,
        rtl: bool,
    ) -> bool {
        let limit = if self.wrapped {
            (style.wrap != TextWrap::None).then_some(width)
        } else {
            style.wrap = TextWrap::None;
            None
        };
        // Most frames find the text as it was shaped, so the comparison
        // must not cost an allocation: what is shown is built only when it
        // is not simply the text.
        let same = self.geometry_limit == limit
            && self.geometry_key.as_ref().is_some_and(|(text, known)| {
                *known == style
                    && if self.masked || self.composition.is_some() {
                        *text == self.shown()
                    } else {
                        *text == self.text
                    }
            });
        let changed = !same;
        if changed {
            let key = (self.shown(), style);
            self.geometry = measurer.geometry(&key.0, &key.1, limit);
            self.geometry_key = Some(key);
            self.geometry_limit = limit;
            self.settled = None;
        }
        if self.wrapped {
            return changed;
        }
        // The caret of a field that is masked or composing depends on more
        // than these inputs.
        let plain = !self.masked && self.composition.is_none();
        let inputs = Settled {
            selection: self.selection,
            affinity: self.affinity,
            width: width.to_bits(),
            rtl,
            scroll_x: self.scroll_x.to_bits(),
            align_x: self.align_x.to_bits(),
        };
        if !changed && plain && self.settled == Some(inputs) {
            return false;
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
        self.scroll_x = self.scroll_x.clamp(0.0, self.scroll_limit(width));
        let old_align = self.align_x;
        self.align_x = if rtl {
            (width - 2.0 - self.geometry.size.width).max(0.0)
        } else {
            0.0
        };
        let moved = changed || self.scroll_x != old_scroll || self.align_x != old_align;
        self.settled = (!moved
            && plain
            && self.scroll_x.to_bits() == inputs.scroll_x
            && self.align_x.to_bits() == inputs.align_x)
            .then_some(inputs);
        moved
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
    use alloc::string::ToString;
    use alloc::vec;
    use glassmere_scene::MonospaceMeasurer;

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

    fn style(font_size: f32) -> TextStyle {
        TextStyle {
            font_size,
            ..TextStyle::default()
        }
    }

    /// `prepare` as if nothing were known about the last call.
    fn prepare_cold(editor: &mut Editor, size: f32, width: f32, rtl: bool) -> bool {
        editor.settled = None;
        editor.prepare(&mut MonospaceMeasurer, style(size), width, rtl)
    }

    fn prepare_warm(editor: &mut Editor, size: f32, width: f32, rtl: bool) -> bool {
        editor.prepare(&mut MonospaceMeasurer, style(size), width, rtl)
    }

    const LONG: &str = "the quick brown fox jumps over the lazy dog again and again";

    /// A long text with the caret at its end, scrolled there and settled.
    fn settled_editor() -> Editor {
        let mut editor = Editor::new(LONG);
        assert!(prepare_warm(&mut editor, 16.0, 100.0, false));
        assert!(!prepare_warm(&mut editor, 16.0, 100.0, false));
        assert!(editor.settled.is_some());
        assert!(editor.scroll_x > 0.0);
        editor
    }

    fn assert_idle(editor: &mut Editor, width: f32, rtl: bool) {
        let (scroll, align) = (editor.scroll_x.to_bits(), editor.align_x.to_bits());
        for _ in 0..2 {
            assert!(!prepare_warm(editor, 16.0, width, rtl));
            assert_eq!(editor.scroll_x.to_bits(), scroll);
            assert_eq!(editor.align_x.to_bits(), align);
        }
    }

    #[test]
    fn a_settled_editor_moves_nothing_on_idle_calls() {
        let mut editor = settled_editor();
        assert_idle(&mut editor, 100.0, false);
    }

    #[test]
    fn a_selection_change_is_noticed_after_settling() {
        let mut editor = settled_editor();
        editor.apply(TextEdit::Move {
            direction: TextMove::Start,
            select: false,
            word: false,
        });
        assert!(prepare_warm(&mut editor, 16.0, 100.0, false));
        assert_eq!(editor.scroll_x, 0.0);
        assert_idle(&mut editor, 100.0, false);
    }

    #[test]
    fn a_scroll_written_from_outside_is_noticed_after_settling() {
        let mut editor = settled_editor();
        let at = editor.scroll_x;
        editor.scroll_x = 0.0;
        assert!(prepare_warm(&mut editor, 16.0, 100.0, false));
        assert_eq!(editor.scroll_x, at);
        assert_idle(&mut editor, 100.0, false);
    }

    #[test]
    fn a_narrower_field_is_noticed_after_settling() {
        let mut editor = settled_editor();
        let at = editor.scroll_x;
        assert!(prepare_warm(&mut editor, 16.0, 60.0, false));
        assert!(editor.scroll_x > at);
        assert_idle(&mut editor, 60.0, false);
    }

    #[test]
    fn an_affinity_change_is_noticed_after_settling() {
        let mut editor = Editor::new("abc \u{5d0}\u{5d1}\u{5d2} def");
        editor.apply(TextEdit::Move {
            direction: TextMove::Start,
            select: false,
            word: false,
        });
        editor.apply(TextEdit::Select {
            anchor: 4,
            cursor: 4,
        });
        prepare_warm(&mut editor, 16.0, 40.0, false);
        prepare_warm(&mut editor, 16.0, 40.0, false);
        assert!(editor.settled.is_some());
        let mut cold = Editor::new(&editor.text);
        cold.selection = editor.selection;
        cold.scroll_x = editor.scroll_x;
        cold.affinity = Affinity::Upstream;
        editor.affinity = Affinity::Upstream;
        let moved = prepare_warm(&mut editor, 16.0, 40.0, false);
        assert_eq!(moved, prepare_cold(&mut cold, 16.0, 40.0, false));
        assert_eq!(editor.scroll_x.to_bits(), cold.scroll_x.to_bits());
        assert_idle(&mut editor, 40.0, false);
    }

    #[test]
    fn the_direction_is_noticed_after_settling() {
        let mut editor = Editor::new("short");
        assert!(prepare_warm(&mut editor, 16.0, 300.0, false));
        assert!(!prepare_warm(&mut editor, 16.0, 300.0, false));
        assert!(editor.settled.is_some());
        assert_eq!(editor.align_x, 0.0);
        assert!(prepare_warm(&mut editor, 16.0, 300.0, true));
        assert!(editor.align_x > 0.0);
        assert_idle(&mut editor, 300.0, true);
        assert!(prepare_warm(&mut editor, 16.0, 300.0, false));
        assert_eq!(editor.align_x, 0.0);
    }

    #[test]
    fn a_preedit_equal_to_the_selection_still_moves_the_caret() {
        let tail = LONG.len() - 30..LONG.len();
        let mut warm = settled_editor();
        let mut cold = settled_editor();
        for editor in [&mut warm, &mut cold] {
            editor.apply(TextEdit::Select {
                anchor: tail.start,
                cursor: tail.end,
            });
            prepare_cold(editor, 16.0, 100.0, false);
        }
        prepare_warm(&mut warm, 16.0, 100.0, false);
        assert!(warm.settled.is_some());
        let at = warm.scroll_x;
        for editor in [&mut warm, &mut cold] {
            editor.apply(TextEdit::Preedit {
                text: LONG[tail.clone()].into(),
                selection: Some((0, 0)),
            });
        }
        let moved = prepare_warm(&mut warm, 16.0, 100.0, false);
        assert_eq!(moved, prepare_cold(&mut cold, 16.0, 100.0, false));
        assert!(warm.scroll_x < at);
        assert_eq!(warm.scroll_x.to_bits(), cold.scroll_x.to_bits());
        assert!(warm.settled.is_none());
    }

    #[test]
    fn a_masked_editor_never_settles() {
        let mut editor = Editor::new("");
        editor.masked = true;
        for _ in 0..3 {
            prepare_warm(&mut editor, 16.0, 100.0, false);
            assert!(editor.settled.is_none());
        }
        editor.masked = false;
        prepare_warm(&mut editor, 16.0, 100.0, false);
        prepare_warm(&mut editor, 16.0, 100.0, false);
        assert!(editor.settled.is_some());
        editor.masked = true;
        editor.apply(TextEdit::Insert("secret words".into()));
        prepare_warm(&mut editor, 16.0, 40.0, false);
        assert!(editor.settled.is_none());
    }

    #[test]
    fn a_wrapped_editor_does_not_settle() {
        let mut editor = Editor::block("some words to wrap");
        let wrapped = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        for _ in 0..3 {
            editor.prepare(&mut MonospaceMeasurer, wrapped.clone(), 60.0, false);
            assert!(editor.settled.is_none());
        }
    }

    /// Whatever is done to an editor, preparing it with a memory of the
    /// last call gives what preparing it afresh each time gives.
    #[test]
    fn remembering_the_last_call_never_changes_what_prepare_does() {
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |bound: u64| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) % bound
        };
        let words = [
            "a",
            "abc",
            "\u{5d0}\u{5d1}",
            "long words here",
            "e\u{301}",
            "",
        ];
        for _ in 0..40 {
            let mut warm = Editor::new("start of the text");
            let mut cold = Editor::new("start of the text");
            let (mut size, mut width, mut rtl) = (16.0_f32, 90.0_f32, false);
            for _ in 0..120 {
                let word = words[next(words.len() as u64) as usize];
                let edit = match next(14) {
                    0 => Some(TextEdit::Insert(word.into())),
                    1 => Some(TextEdit::Backspace { word: false }),
                    2..=5 => Some(TextEdit::Move {
                        direction: [
                            TextMove::Left,
                            TextMove::Right,
                            TextMove::Start,
                            TextMove::End,
                        ][next(4) as usize],
                        select: next(2) == 0,
                        word: false,
                    }),
                    6 => Some(TextEdit::SelectAll),
                    7 => Some(TextEdit::Undo),
                    8 => Some(TextEdit::Redo),
                    9 => {
                        let text = if next(2) == 0 {
                            warm.text[warm.selection.range()].to_owned()
                        } else {
                            word.to_owned()
                        };
                        let selection =
                            [None, Some((0, 0)), Some((text.len(), text.len()))][next(3) as usize];
                        Some(TextEdit::Preedit { text, selection })
                    }
                    10 => Some(TextEdit::CancelComposition),
                    _ => None,
                };
                if let Some(edit) = edit {
                    warm.apply(edit.clone());
                    cold.apply(edit);
                }
                match next(10) {
                    0 => {
                        let to = next(60) as f32 * 2.0;
                        warm.scroll_x = to;
                        cold.scroll_x = to;
                    }
                    1 => {
                        warm.affinity = Affinity::Upstream;
                        cold.affinity = Affinity::Upstream;
                    }
                    2 => width = [30.0, 90.0, 400.0][next(3) as usize],
                    3 => rtl = !rtl,
                    4 => {
                        warm.masked = !warm.masked;
                        cold.masked = !cold.masked;
                    }
                    5 => size = [12.0, 16.0][next(2) as usize],
                    _ => {}
                }
                for _ in 0..1 + next(3) {
                    let remembered = prepare_warm(&mut warm, size, width, rtl);
                    let fresh = prepare_cold(&mut cold, size, width, rtl);
                    assert_eq!(remembered, fresh);
                    assert_eq!(warm.scroll_x.to_bits(), cold.scroll_x.to_bits());
                    assert_eq!(warm.align_x.to_bits(), cold.align_x.to_bits());
                }
            }
        }
    }

    /// What `single_line` does, as a plain loop.
    fn stripped(text: &str) -> String {
        let mut out = String::new();
        for ch in text.chars() {
            if matches!(ch, '\n' | '\r' | '\t') {
                out.push(' ');
            } else if !ch.is_control() {
                out.push(ch);
            }
        }
        out
    }

    /// What `cancel_composition` must do, found the way `apply` tells that
    /// anything changed: by comparing copies of what an edit can touch.
    fn cancelled_by_comparing(editor: &mut Editor) -> bool {
        let before = (
            editor.text.clone(),
            editor.selection,
            editor.composition.clone(),
            editor.affinity,
        );
        editor.composition = None;
        editor.affinity = Affinity::Downstream;
        before
            != (
                editor.text.clone(),
                editor.selection,
                editor.composition.clone(),
                editor.affinity,
            )
    }

    /// Everything an editor holds that a cancelled composition leaves alone,
    /// and what it changes.
    #[allow(clippy::type_complexity)]
    fn held(
        editor: &Editor,
    ) -> (
        String,
        TextSelection,
        Option<Composition>,
        Affinity,
        Vec<(String, TextSelection)>,
        Vec<(String, TextSelection)>,
        [u32; 2],
        (bool, bool),
    ) {
        let history = |history: &VecDeque<Snapshot>| {
            history
                .iter()
                .map(|entry| (entry.text.clone(), entry.selection))
                .collect()
        };
        (
            editor.text.clone(),
            editor.selection,
            editor.composition.clone(),
            editor.affinity,
            history(&editor.undo),
            history(&editor.redo),
            [editor.scroll_x.to_bits(), editor.align_x.to_bits()],
            (editor.masked, editor.revealed),
        )
    }

    /// A settled editor over "hello world" with `undo` and `redo` steps of
    /// history, the selection in `bytes`, and the composition and affinity
    /// given.
    fn editor_in_state(
        composition: Option<Option<(usize, usize)>>,
        affinity: Affinity,
        bytes: (usize, usize),
        (undo, redo): (usize, usize),
        masked: bool,
    ) -> Editor {
        let mut editor = Editor::new("hello world");
        for index in 0..undo + redo {
            editor.apply(TextEdit::Insert(index.to_string()));
        }
        for _ in 0..redo {
            editor.apply(TextEdit::Undo);
        }
        editor.selection = TextSelection {
            anchor: bytes.0,
            cursor: bytes.1,
        };
        editor.masked = masked;
        assert!(prepare_warm(&mut editor, 16.0, 60.0, false));
        assert!(!prepare_warm(&mut editor, 16.0, 60.0, false));
        if let Some(selection) = composition {
            editor.apply(TextEdit::Preedit {
                text: "\u{306b}\u{307b}\u{3093}".into(),
                selection,
            });
        }
        editor.affinity = affinity;
        editor
    }

    #[test]
    fn cancelling_a_composition_changes_what_apply_would_and_reports_it_the_same() {
        let compositions = [None, Some(None), Some(Some((3, 3))), Some(Some((0, 9)))];
        let affinities = [Affinity::Downstream, Affinity::Upstream];
        let selections = [(0, 0), (2, 4), (11, 11)];
        let histories = [(0, 0), (2, 0), (0, 1), (2, 1)];
        for composition in compositions {
            for affinity in affinities {
                for bytes in selections {
                    for history in histories {
                        for masked in [false, true] {
                            let build =
                                || editor_in_state(composition, affinity, bytes, history, masked);
                            let (mut cancelled, mut applied, mut compared) =
                                (build(), build(), build());
                            let settled = cancelled.settled;
                            assert_eq!(settled.is_some(), !masked);
                            let expected = cancelled_by_comparing(&mut compared);
                            assert_eq!(
                                expected,
                                composition.is_some() || affinity == Affinity::Upstream
                            );
                            let case = (composition, affinity, bytes, history, masked);
                            assert_eq!(cancelled.cancel_composition(), expected, "{case:?}");
                            assert_eq!(applied.apply(TextEdit::CancelComposition), expected);
                            assert_eq!(held(&cancelled), held(&compared), "{case:?}");
                            assert_eq!(held(&applied), held(&compared), "{case:?}");
                            assert!(cancelled.settled == settled && applied.settled == settled);
                            assert!(!cancelled.composing());
                            assert_eq!(cancelled.affinity, Affinity::Downstream);
                            assert!(!cancelled.cancel_composition(), "{case:?}");
                            assert!(!applied.apply(TextEdit::CancelComposition), "{case:?}");
                        }
                    }
                }
            }
        }
    }

    /// An editor over `text` with a selection, a composition and one step of
    /// undo and of redo to lose.
    fn lived_in(text: &str) -> Editor {
        let mut editor = Editor::new("x");
        editor.apply(TextEdit::SelectAll);
        editor.apply(TextEdit::Insert(text.into()));
        editor.apply(TextEdit::Insert("a".into()));
        editor.apply(TextEdit::Undo);
        editor.apply(TextEdit::Select {
            anchor: 1,
            cursor: 2,
        });
        editor.apply(TextEdit::Preedit {
            text: "\u{306b}".into(),
            selection: None,
        });
        assert_eq!(editor.text, text);
        editor
    }

    fn assert_lived_in(editor: &Editor, text: &str) {
        assert_eq!(editor.text, text);
        assert!(editor.composing());
        assert_eq!(
            editor.selection,
            TextSelection {
                anchor: 1,
                cursor: 2
            }
        );
        assert_eq!((editor.undo.len(), editor.redo.len()), (1, 1));
    }

    #[test]
    fn synchronizing_with_the_text_it_holds_changes_nothing() {
        let mut editor = lived_in("hello there");
        editor.synchronize("hello there");
        assert_lived_in(&editor, "hello there");

        // Text that strips to the held text is the held text.
        for input in [
            "hello\nthere",
            "hello\rthere",
            "hello\tthere",
            "hel\u{7}lo there",
            "hello\u{7f} there",
            "hello \u{85}there",
            "\0hello there\0",
            "hello there\u{9f}",
        ] {
            assert_eq!(stripped(input), "hello there");
            editor.synchronize(input);
            assert_lived_in(&editor, "hello there");
        }
        let mut editor = lived_in("a  b");
        editor.synchronize("a\r\nb");
        assert_lived_in(&editor, "a  b");
    }

    #[test]
    fn synchronizing_with_other_text_resets_the_editor_to_the_stripped_text() {
        for (input, expected) in [
            ("hello", "hello"),
            ("hello there!", "hello there!"),
            ("wor\u{7}ld\n", "world "),
            ("\u{85}", ""),
            ("", ""),
            ("e\u{301}\t\u{301}x", "e\u{301} \u{301}x"),
        ] {
            assert_eq!(stripped(input), expected);
            let mut editor = lived_in("hello there");
            editor.selection = TextSelection {
                anchor: 1,
                cursor: 8,
            };
            editor.synchronize(input);
            assert_eq!(editor.text, expected);
            assert_eq!(
                editor.selection,
                TextSelection {
                    anchor: boundary(expected, 1),
                    cursor: boundary(expected, 8),
                }
            );
            assert!(!editor.composing());
            assert!(editor.undo.is_empty() && editor.redo.is_empty());
        }
    }

    #[test]
    fn a_wrapped_editor_keeps_its_line_breaks_and_compares_them() {
        let mut editor = Editor::block("a\nb");
        editor.apply(TextEdit::Preedit {
            text: "\u{306b}".into(),
            selection: None,
        });
        editor.synchronize("a\nb");
        assert_eq!(editor.text, "a\nb");
        assert!(editor.composing());
        editor.synchronize("a\r\nb");
        assert_eq!(editor.text, "a\r\nb");
        assert!(!editor.composing());
        let mut editor = Editor::block("ab");
        editor.synchronize("a\nb");
        assert_eq!(editor.text, "a\nb");
    }

    #[test]
    fn a_draft_the_application_does_not_accept_is_reverted() {
        let mut editor = Editor::new("abc");
        editor.apply(TextEdit::Insert("d".into()));
        assert_eq!(editor.text, "abcd");
        assert_eq!(editor.undo.len(), 1);
        editor.synchronize("abc");
        assert_eq!(editor.text, "abc");
        assert!(editor.undo.is_empty() && editor.redo.is_empty());
        editor.apply(TextEdit::Undo);
        assert_eq!(editor.text, "abc");
    }

    #[test]
    fn a_text_without_control_characters_is_single_line_and_copied_whole() {
        let lone = |ch: char| ch.to_string();
        let mut chars: Vec<char> = (0..=0x1ff).filter_map(char::from_u32).collect();
        chars.extend(
            [
                0x2028, 0x2029, 0x200b, 0x200c, 0x200d, 0xad, 0xfeff, 0x301, 0x1f469, 0xe000,
            ]
            .into_iter()
            .filter_map(char::from_u32),
        );
        let mut texts = vec![
            String::new(),
            "plain".into(),
            "\u{1f469}\u{200d}\u{1f4bb} e\u{301}".into(),
            "\u{1f1fa}\u{1f1f8}".into(),
        ];
        for ch in chars {
            for text in [
                lone(ch),
                format!("ab{ch}cd"),
                format!("{ch}{ch}"),
                format!("\u{e9}\u{a3}\u{a9}\u{a1}{ch}"),
            ] {
                texts.push(text);
            }
        }
        for text in texts {
            assert_eq!(single_line(&text), stripped(&text), "{text:?}");
            assert_eq!(is_single_line(&text), stripped(&text) == text, "{text:?}");
            assert_eq!(single_line_owned(&text), stripped(&text), "{text:?}");
        }
        assert_eq!(single_line_owned("").capacity(), 0);
    }
}
