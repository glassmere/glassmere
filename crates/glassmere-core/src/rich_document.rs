//! The app-owned model of the WYSIWYG editor: blocks of marked runs, one
//! selection, snapshot undo and a Markdown subset for loading and saving.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mark {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
}

impl Marks {
    pub fn has(&self, mark: Mark) -> bool {
        match mark {
            Mark::Bold => self.bold,
            Mark::Italic => self.italic,
            Mark::Underline => self.underline,
            Mark::Strikethrough => self.strikethrough,
            Mark::Code => self.code,
        }
    }
    fn set(&mut self, mark: Mark, on: bool) {
        match mark {
            Mark::Bold => self.bold = on,
            Mark::Italic => self.italic = on,
            Mark::Underline => self.underline = on,
            Mark::Strikethrough => self.strikethrough = on,
            Mark::Code => self.code = on,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineRun {
    pub text: String,
    pub marks: Marks,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlockKind {
    #[default]
    Paragraph,
    /// Levels 1 to 3.
    Heading(u8),
    Bullet,
    Numbered,
    Quote,
    /// Monospace; keeps line breaks inside the block.
    Code,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub runs: Vec<InlineRun>,
}

/// A place in the document: a block and a byte offset into its plain text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct RichPosition {
    pub block: usize,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RichMotion {
    Left,
    Right,
    WordLeft,
    WordRight,
    BlockStart,
    BlockEnd,
    DocumentStart,
    DocumentEnd,
}

impl Block {
    pub fn new(kind: BlockKind, text: &str) -> Self {
        Self {
            kind,
            runs: vec![InlineRun {
                text: text.to_owned(),
                marks: Marks::default(),
            }],
        }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.runs.iter().map(|run| run.text.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Splits runs so that `offset` falls between two runs; returns the index
    /// of the run that starts there.
    fn split_at(&mut self, offset: usize) -> usize {
        let mut at = 0;
        for index in 0..self.runs.len() {
            let len = self.runs[index].text.len();
            if offset == at {
                return index;
            }
            if offset < at + len {
                let tail = self.runs[index].text.split_off(offset - at);
                let marks = self.runs[index].marks.clone();
                self.runs.insert(index + 1, InlineRun { text: tail, marks });
                return index + 1;
            }
            at += len;
        }
        self.runs.len()
    }

    /// Merges equal neighbours and drops empty runs.
    fn normalize(&mut self) {
        self.runs.retain(|run| !run.text.is_empty());
        let mut merged: Vec<InlineRun> = Vec::with_capacity(self.runs.len());
        for run in self.runs.drain(..) {
            match merged.last_mut() {
                Some(last) if last.marks == run.marks => last.text.push_str(&run.text),
                _ => merged.push(run),
            }
        }
        self.runs = merged;
    }

    /// Marks of the character before `offset`, which typing continues.
    fn marks_before(&self, offset: usize) -> Marks {
        let mut at = 0;
        for run in &self.runs {
            at += run.text.len();
            if offset <= at && !run.text.is_empty() {
                return run.marks.clone();
            }
        }
        self.runs
            .last()
            .map(|run| run.marks.clone())
            .unwrap_or_default()
    }

    fn insert(&mut self, offset: usize, text: &str, marks: Marks) {
        let index = self.split_at(offset);
        self.runs.insert(
            index,
            InlineRun {
                text: text.to_owned(),
                marks,
            },
        );
        self.normalize();
    }

    fn remove(&mut self, range: Range<usize>) {
        // Start first: splitting at the end afterwards cannot move `start`.
        let start = self.split_at(range.start);
        let end = self.split_at(range.end);
        self.runs.drain(start..end);
        self.normalize();
    }

    fn each(&mut self, range: Range<usize>, mut change: impl FnMut(&mut Marks)) {
        let start = self.split_at(range.start);
        let end = self.split_at(range.end);
        for run in &mut self.runs[start..end] {
            change(&mut run.marks);
        }
        self.normalize();
    }

    fn all(&self, range: Range<usize>, test: impl Fn(&Marks) -> bool) -> bool {
        let mut at = 0;
        self.runs.iter().all(|run| {
            let span = at..at + run.text.len();
            at = span.end;
            span.end <= range.start
                || span.start >= range.end
                || span.is_empty()
                || test(&run.marks)
        })
    }

    /// Runs after `offset`, removed from this block.
    fn split_off(&mut self, offset: usize) -> Vec<InlineRun> {
        let index = self.split_at(offset);
        let tail = self.runs.split_off(index);
        self.normalize();
        tail
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    blocks: Vec<Block>,
    anchor: RichPosition,
    cursor: RichPosition,
}

const HISTORY: usize = 300;

/// The part of block `index` inside the selection `start..end`.
fn block_range(
    blocks: &[Block],
    index: usize,
    start: RichPosition,
    end: RichPosition,
) -> Range<usize> {
    let from = if index == start.block {
        start.offset
    } else {
        0
    };
    let to = if index == end.block {
        end.offset
    } else {
        blocks[index].len()
    };
    from..to
}

#[derive(Clone, Debug)]
pub struct RichDocument {
    blocks: Vec<Block>,
    anchor: RichPosition,
    cursor: RichPosition,
    /// Marks the next typed text takes when toggled at a caret.
    typing: Option<Marks>,
    // Whole-document snapshots; switch to block-level edits if
    // documents grow past a few thousand blocks.
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    coalesce: bool,
    revision: u64,
}

impl Default for RichDocument {
    fn default() -> Self {
        Self::from_blocks(Vec::new())
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

impl RichDocument {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_blocks(mut blocks: Vec<Block>) -> Self {
        if blocks.is_empty() {
            blocks.push(Block::default());
        }
        for block in &mut blocks {
            block.normalize();
        }
        Self {
            blocks,
            anchor: RichPosition::default(),
            cursor: RichPosition::default(),
            typing: None,
            undo: Vec::new(),
            redo: Vec::new(),
            coalesce: false,
            revision: 0,
        }
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn anchor(&self) -> RichPosition {
        self.anchor
    }
    pub fn cursor(&self) -> RichPosition {
        self.cursor
    }
    pub fn selection(&self) -> (RichPosition, RichPosition) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }
    pub fn has_selection(&self) -> bool {
        self.anchor != self.cursor
    }

    fn clamp(&self, position: RichPosition) -> RichPosition {
        let block = position.block.min(self.blocks.len() - 1);
        let text = self.blocks[block].text();
        let mut offset = position.offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        RichPosition { block, offset }
    }

    pub fn set_cursor(&mut self, position: RichPosition, select: bool) {
        self.cursor = self.clamp(position);
        if !select {
            self.anchor = self.cursor;
        }
        self.typing = None;
        self.coalesce = false;
    }

    pub fn select_all(&mut self) {
        self.anchor = RichPosition::default();
        self.cursor = self.end();
        self.typing = None;
    }

    fn end(&self) -> RichPosition {
        let block = self.blocks.len() - 1;
        RichPosition {
            block,
            offset: self.blocks[block].len(),
        }
    }

    pub fn select_word(&mut self, position: RichPosition) {
        let position = self.clamp(position);
        let text = self.blocks[position.block].text();
        let Some((start, word)) = text.split_word_bound_indices().find(|(start, word)| {
            *start <= position.offset && position.offset <= start + word.len()
        }) else {
            return self.set_cursor(position, false);
        };
        self.anchor = RichPosition {
            block: position.block,
            offset: start,
        };
        self.cursor = RichPosition {
            block: position.block,
            offset: start + word.len(),
        };
    }

    pub fn select_block(&mut self, block: usize) {
        let block = block.min(self.blocks.len() - 1);
        self.anchor = RichPosition { block, offset: 0 };
        self.cursor = RichPosition {
            block,
            offset: self.blocks[block].len(),
        };
    }

    fn moved(&self, from: RichPosition, motion: RichMotion) -> RichPosition {
        let text = self.blocks[from.block].text();
        let previous_end = |block: usize| RichPosition {
            block: block - 1,
            offset: self.blocks[block - 1].len(),
        };
        match motion {
            RichMotion::Left if from.offset == 0 => {
                if from.block == 0 {
                    from
                } else {
                    previous_end(from.block)
                }
            }
            RichMotion::Left => RichPosition {
                offset: text[..from.offset]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(index, _)| index),
                ..from
            },
            RichMotion::Right if from.offset >= text.len() => {
                if from.block + 1 < self.blocks.len() {
                    RichPosition {
                        block: from.block + 1,
                        offset: 0,
                    }
                } else {
                    from
                }
            }
            RichMotion::Right => RichPosition {
                offset: from.offset
                    + text[from.offset..]
                        .graphemes(true)
                        .next()
                        .map_or(0, str::len),
                ..from
            },
            RichMotion::WordLeft if from.offset == 0 => self.moved(from, RichMotion::Left),
            RichMotion::WordLeft => {
                let mut chars = text[..from.offset].char_indices().rev().peekable();
                while chars.peek().is_some_and(|(_, ch)| word_class(*ch) == 0) {
                    chars.next();
                }
                let class = chars.peek().map(|(_, ch)| word_class(*ch));
                let mut offset = chars.peek().map_or(0, |(index, ch)| index + ch.len_utf8());
                while let Some((index, ch)) = chars.peek().copied() {
                    if Some(word_class(ch)) != class {
                        break;
                    }
                    offset = index;
                    chars.next();
                }
                RichPosition { offset, ..from }
            }
            RichMotion::WordRight if from.offset >= text.len() => {
                self.moved(from, RichMotion::Right)
            }
            RichMotion::WordRight => {
                let mut chars = text[from.offset..].char_indices().peekable();
                let class = chars.peek().map(|(_, ch)| word_class(*ch));
                while chars
                    .peek()
                    .is_some_and(|(_, ch)| Some(word_class(*ch)) == class && class != Some(0))
                {
                    chars.next();
                }
                while chars.peek().is_some_and(|(_, ch)| word_class(*ch) == 0) {
                    chars.next();
                }
                let offset = chars
                    .peek()
                    .map_or(text.len(), |(index, _)| from.offset + index);
                RichPosition { offset, ..from }
            }
            RichMotion::BlockStart => RichPosition { offset: 0, ..from },
            RichMotion::BlockEnd => RichPosition {
                offset: text.len(),
                ..from
            },
            RichMotion::DocumentStart => RichPosition::default(),
            RichMotion::DocumentEnd => self.end(),
        }
    }

    pub fn move_cursor(&mut self, motion: RichMotion, select: bool) {
        let target = if !select && self.has_selection() {
            // A selection collapses to its edge in the direction of travel.
            match motion {
                RichMotion::Left => self.selection().0,
                RichMotion::Right => self.selection().1,
                _ => self.moved(self.cursor, motion),
            }
        } else {
            self.moved(self.cursor, motion)
        };
        self.set_cursor(target, select);
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            blocks: self.blocks.clone(),
            anchor: self.anchor,
            cursor: self.cursor,
        }
    }

    /// Records the state before a change. Consecutive typing is one step.
    fn record(&mut self, typing: bool) {
        if !(typing && self.coalesce) {
            self.undo.push(self.snapshot());
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
        }
        self.coalesce = typing;
        self.redo.clear();
        self.revision += 1;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.blocks = snapshot.blocks;
        self.anchor = snapshot.anchor;
        self.cursor = snapshot.cursor;
        self.typing = None;
        self.coalesce = false;
        self.revision += 1;
    }

    /// Removes the selection without recording history.
    fn remove_selection(&mut self) {
        let (start, end) = self.selection();
        if start == end {
            return;
        }
        if start.block == end.block {
            self.blocks[start.block].remove(start.offset..end.offset);
        } else {
            let tail = self.blocks[end.block].split_off(end.offset);
            let first = &mut self.blocks[start.block];
            let len = first.len();
            first.remove(start.offset..len);
            first.runs.extend(tail);
            first.normalize();
            self.blocks.drain(start.block + 1..=end.block);
        }
        self.anchor = start;
        self.cursor = start;
    }

    /// Replaces the selection with plain text; line breaks start new blocks
    /// (or stay inside a code block).
    pub fn insert_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let typing = text.chars().count() == 1 && text != "\n" && !self.has_selection();
        self.record(typing);
        let marks = self.typing.clone();
        self.remove_selection();
        let code = self.blocks[self.cursor.block].kind == BlockKind::Code;
        let mut lines = text.split('\n');
        let insert = |document: &mut Self, piece: &str| {
            let position = document.cursor;
            let block = &mut document.blocks[position.block];
            let marks = marks
                .clone()
                .unwrap_or_else(|| block.marks_before(position.offset));
            block.insert(position.offset, piece, marks);
            document.cursor.offset += piece.len();
        };
        if code {
            insert(self, &text);
        } else {
            if let Some(first) = lines.next() {
                insert(self, first);
            }
            for line in lines {
                self.split_block();
                insert(self, line);
            }
        }
        self.anchor = self.cursor;
        self.typing = marks;
    }

    fn split_block(&mut self) {
        let position = self.cursor;
        let block = &mut self.blocks[position.block];
        let at_end = position.offset >= block.len();
        let tail = block.split_off(position.offset);
        // A heading is followed by body text, not by another heading.
        let kind = match block.kind {
            BlockKind::Heading(_) if at_end => BlockKind::Paragraph,
            kind => kind,
        };
        self.blocks
            .insert(position.block + 1, Block { kind, runs: tail });
        self.cursor = RichPosition {
            block: position.block + 1,
            offset: 0,
        };
        self.anchor = self.cursor;
    }

    /// Enter: splits the block. An empty list item or quote becomes a
    /// paragraph instead, and a second Enter at the end of a code block
    /// leaves it.
    pub fn newline(&mut self) {
        // Formatting continues on the new line, as in word processors.
        let marks = Some(self.marks()).filter(|marks| *marks != Marks::default());
        self.record(false);
        self.remove_selection();
        let position = self.cursor;
        let block = &mut self.blocks[position.block];
        match block.kind {
            BlockKind::Bullet | BlockKind::Numbered | BlockKind::Quote if block.is_empty() => {
                block.kind = BlockKind::Paragraph;
            }
            BlockKind::Code => {
                let text = block.text();
                if position.offset == text.len() && text.ends_with('\n') {
                    block.remove(text.len() - 1..text.len());
                    self.blocks.insert(position.block + 1, Block::default());
                    self.cursor = RichPosition {
                        block: position.block + 1,
                        offset: 0,
                    };
                } else {
                    block.insert(position.offset, "\n", Marks::default());
                    self.cursor.offset += 1;
                }
            }
            _ => self.split_block(),
        }
        self.anchor = self.cursor;
        self.typing = marks.map(|marks| Marks {
            link: None,
            ..marks
        });
    }

    fn delete_towards(&mut self, motion: RichMotion) {
        if !self.has_selection() {
            let target = self.moved(self.cursor, motion);
            if target == self.cursor {
                return;
            }
            self.anchor = target;
        }
        self.record(false);
        self.remove_selection();
    }

    /// At the start of a styled block, Backspace first turns it into a
    /// paragraph; at the start of a paragraph it joins the previous block.
    pub fn backspace(&mut self, word: bool) {
        let at_start = !self.has_selection() && self.cursor.offset == 0;
        let block = &mut self.blocks[self.cursor.block];
        if at_start && block.kind != BlockKind::Paragraph {
            self.record(false);
            self.blocks[self.cursor.block].kind = BlockKind::Paragraph;
            return;
        }
        self.delete_towards(if word {
            RichMotion::WordLeft
        } else {
            RichMotion::Left
        });
    }

    pub fn delete(&mut self, word: bool) {
        self.delete_towards(if word {
            RichMotion::WordRight
        } else {
            RichMotion::Right
        });
    }

    fn each_selected(&mut self, mut change: impl FnMut(&mut Block, Range<usize>)) {
        let (start, end) = self.selection();
        for index in start.block..=end.block {
            let range = block_range(&self.blocks, index, start, end);
            change(&mut self.blocks[index], range);
        }
    }

    /// Marks at the caret (what typing would produce) or of the selection.
    pub fn marks(&self) -> Marks {
        if let Some(marks) = &self.typing {
            return marks.clone();
        }
        let (start, _) = self.selection();
        let probe = if self.has_selection() {
            start.offset + 1
        } else {
            start.offset
        };
        self.blocks[start.block].marks_before(probe)
    }

    /// With a selection: removes the mark when all of it has it, otherwise
    /// adds it. At a caret: toggles it for the text typed next.
    pub fn toggle_mark(&mut self, mark: Mark) {
        if !self.has_selection() {
            let mut marks = self.marks();
            let on = !marks.has(mark);
            marks.set(mark, on);
            self.typing = Some(marks);
            return;
        }
        let (start, end) = self.selection();
        let all = (start.block..=end.block).all(|index| {
            let range = block_range(&self.blocks, index, start, end);
            self.blocks[index].all(range, |marks| marks.has(mark))
        });
        self.record(false);
        self.each_selected(|block, range| block.each(range, |marks| marks.set(mark, !all)));
    }

    /// Links or unlinks the selection.
    pub fn set_link(&mut self, target: Option<&str>) {
        if !self.has_selection() {
            return;
        }
        self.record(false);
        let target = target.map(str::to_owned);
        self.each_selected(|block, range| block.each(range, |marks| marks.link = target.clone()));
    }

    pub fn block_kind(&self) -> BlockKind {
        self.blocks[self.cursor.block].kind
    }

    /// Sets the kind of every selected block; the kind they all already have
    /// toggles back to paragraphs.
    pub fn set_block_kind(&mut self, kind: BlockKind) {
        let (start, end) = self.selection();
        let blocks = start.block..=end.block;
        let all = self.blocks[blocks.clone()]
            .iter()
            .all(|block| block.kind == kind);
        self.record(false);
        for block in &mut self.blocks[blocks] {
            block.kind = if all { BlockKind::Paragraph } else { kind };
        }
    }

    /// Selected text; blocks join with line breaks.
    pub fn selected_text(&self) -> String {
        let (start, end) = self.selection();
        (start.block..=end.block)
            .map(|index| {
                let range = block_range(&self.blocks, index, start, end);
                self.blocks[index].text()[range].to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn cut(&mut self) -> String {
        let text = self.selected_text();
        if self.has_selection() {
            self.record(false);
            self.remove_selection();
        }
        text
    }

    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .map(Block::text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// CommonMark-style text: `#` headings, `-` and `1.` lists, `>` quotes,
    /// fenced code, `**bold**`, `*italic*`, `` `code` ``, `~~strike~~`,
    /// `<u>underline</u>` and `[text](target)` links.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let mut number = 0;
        for (index, block) in self.blocks.iter().enumerate() {
            number = if block.kind == BlockKind::Numbered {
                number + 1
            } else {
                0
            };
            let list = |kind| matches!(kind, BlockKind::Bullet | BlockKind::Numbered);
            if index > 0 && !(list(block.kind) && self.blocks[index - 1].kind == block.kind) {
                out.push('\n');
            }
            if block.kind == BlockKind::Code {
                out.push_str("```\n");
                out.push_str(&block.text());
                out.push_str("\n```\n");
                continue;
            }
            match block.kind {
                BlockKind::Heading(level) => {
                    out.push_str(&"#".repeat(level.clamp(1, 3) as usize));
                    out.push(' ');
                }
                BlockKind::Bullet => out.push_str("- "),
                BlockKind::Numbered => out.push_str(&format!("{number}. ")),
                BlockKind::Quote => out.push_str("> "),
                _ => {}
            }
            // Marks open and close as a stack in a fixed order, so neighbouring
            // runs share their common marks instead of colliding delimiters.
            let mut open: Vec<Level> = Vec::new();
            for run in &block.runs {
                let wanted = Level::of(&run.marks);
                let common = open.iter().zip(&wanted).take_while(|(a, b)| a == b).count();
                for level in open.drain(common..).rev() {
                    out.push_str(&level.close());
                }
                for level in &wanted[common..] {
                    out.push_str(level.open());
                    open.push(level.clone());
                }
                if run.marks.code {
                    out.push('`');
                    out.push_str(&run.text);
                    out.push('`');
                } else {
                    for ch in run.text.chars() {
                        if matches!(ch, '\\' | '*' | '`' | '[' | ']' | '~' | '<') {
                            out.push('\\');
                        }
                        out.push(ch);
                    }
                }
            }
            for level in open.drain(..).rev() {
                out.push_str(&level.close());
            }
            out.push('\n');
        }
        out
    }

    /// Reads what `to_markdown` writes; anything else stays literal text.
    pub fn from_markdown(text: &str) -> Self {
        let mut blocks = Vec::new();
        let mut code: Option<Vec<&str>> = None;
        let normalized = text.replace("\r\n", "\n");
        for line in normalized.lines() {
            if let Some(lines) = &mut code {
                if line.trim_end() == "```" {
                    blocks.push(Block::new(BlockKind::Code, &lines.join("\n")));
                    code = None;
                } else {
                    lines.push(line);
                }
                continue;
            }
            if line.trim_start().starts_with("```") {
                code = Some(Vec::new());
                continue;
            }
            if line.trim().is_empty() {
                continue;
            }
            let numbered = line.split_once(". ").filter(|(digits, _)| {
                !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
            });
            let (kind, rest) = if let Some(rest) = line.strip_prefix("### ") {
                (BlockKind::Heading(3), rest)
            } else if let Some(rest) = line.strip_prefix("## ") {
                (BlockKind::Heading(2), rest)
            } else if let Some(rest) = line.strip_prefix("# ") {
                (BlockKind::Heading(1), rest)
            } else if let Some(rest) = line.strip_prefix("- ").or(line.strip_prefix("* ")) {
                (BlockKind::Bullet, rest)
            } else if let Some(rest) = line.strip_prefix("> ") {
                (BlockKind::Quote, rest)
            } else if let Some((_, rest)) = numbered {
                (BlockKind::Numbered, rest)
            } else {
                (BlockKind::Paragraph, line)
            };
            let mut runs = Vec::new();
            inline(rest, &Marks::default(), &mut runs);
            blocks.push(Block { kind, runs });
        }
        if let Some(lines) = code {
            blocks.push(Block::new(BlockKind::Code, &lines.join("\n")));
        }
        Self::from_blocks(blocks)
    }
}

/// One nesting level of inline Markdown, outermost first.
#[derive(Clone, Debug, PartialEq)]
enum Level {
    Link(String),
    Bold,
    Italic,
    Underline,
    Strikethrough,
}

impl Level {
    fn of(marks: &Marks) -> Vec<Self> {
        let mut levels = Vec::new();
        if let Some(target) = &marks.link {
            levels.push(Self::Link(target.clone()));
        }
        for (on, level) in [
            (marks.bold, Self::Bold),
            (marks.italic, Self::Italic),
            (marks.underline, Self::Underline),
            (marks.strikethrough, Self::Strikethrough),
        ] {
            if on {
                levels.push(level);
            }
        }
        levels
    }
    fn open(&self) -> &'static str {
        match self {
            Self::Link(_) => "[",
            Self::Bold => "**",
            Self::Italic => "*",
            Self::Underline => "<u>",
            Self::Strikethrough => "~~",
        }
    }
    fn close(&self) -> String {
        match self {
            Self::Link(target) => format!("]({target})"),
            Self::Underline => "</u>".into(),
            other => other.open().into(),
        }
    }
}

/// An opening delimiter, its closer and the mark it adds.
type Delimiter = (&'static str, &'static str, fn(&mut Marks));

/// Parses inline Markdown into runs carrying `marks` plus what they add.
fn inline(text: &str, marks: &Marks, out: &mut Vec<InlineRun>) {
    let mut plain = String::new();
    let mut rest = text;
    let flush = |plain: &mut String, out: &mut Vec<InlineRun>| {
        if !plain.is_empty() {
            out.push(InlineRun {
                text: std::mem::take(plain),
                marks: marks.clone(),
            });
        }
    };
    while let Some(ch) = rest.chars().next() {
        if ch == '\\' && rest.len() > 1 {
            let escaped = rest[1..].chars().next().unwrap();
            plain.push(escaped);
            rest = &rest[1 + escaped.len_utf8()..];
            continue;
        }
        let delimited: [Delimiter; 5] = [
            ("**", "**", |marks| marks.bold = true),
            ("~~", "~~", |marks| marks.strikethrough = true),
            ("<u>", "</u>", |marks| marks.underline = true),
            ("*", "*", |marks| marks.italic = true),
            ("`", "`", |marks| marks.code = true),
        ];
        let mut matched = false;
        for (open, close, apply) in delimited {
            let Some(body) = rest.strip_prefix(open) else {
                continue;
            };
            let Some(mut end) = body.find(close).filter(|end| *end > 0) else {
                continue;
            };
            // In `**bold *both***` the first `*` of the run closes the italic.
            if open == "**" && body[end + 2..].starts_with('*') && body[..end].contains('*') {
                end += 1;
            }
            flush(&mut plain, out);
            let mut inner = marks.clone();
            apply(&mut inner);
            if inner.code {
                out.push(InlineRun {
                    text: body[..end].to_owned(),
                    marks: inner,
                });
            } else {
                inline(&body[..end], &inner, out);
            }
            rest = &body[end + close.len()..];
            matched = true;
            break;
        }
        if matched {
            continue;
        }
        if ch == '['
            && let Some(label_end) = rest.find("](")
            && let Some(target_end) = rest[label_end..].find(')')
        {
            flush(&mut plain, out);
            let mut inner = marks.clone();
            inner.link = Some(rest[label_end + 2..label_end + target_end].to_owned());
            inline(&rest[1..label_end], &inner, out);
            rest = &rest[label_end + target_end + 1..];
            continue;
        }
        plain.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    flush(&mut plain, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(block: usize, offset: usize) -> RichPosition {
        RichPosition { block, offset }
    }

    fn shape(document: &RichDocument) -> Vec<(BlockKind, String)> {
        document
            .blocks()
            .iter()
            .map(|block| (block.kind, block.text()))
            .collect()
    }

    #[test]
    fn typing_marks_selection_marks_and_links_split_and_merge_runs() {
        let mut document = RichDocument::new();
        document.insert_text("plain ");
        document.toggle_mark(Mark::Bold);
        document.insert_text("bold");
        document.toggle_mark(Mark::Bold);
        document.insert_text(" tail");
        let runs: Vec<_> = document.blocks()[0]
            .runs
            .iter()
            .map(|run| (run.text.as_str(), run.marks.bold))
            .collect();
        assert_eq!(runs, [("plain ", false), ("bold", true), (" tail", false)]);

        // Adding a mark over a mixed range covers all of it; again removes it.
        document.set_cursor(at(0, 3), false);
        document.set_cursor(at(0, 12), true);
        document.toggle_mark(Mark::Bold);
        assert!(document.blocks()[0].all(3..12, |marks| marks.bold));
        assert!(document.marks().bold);
        document.toggle_mark(Mark::Bold);
        assert_eq!(document.blocks()[0].runs.len(), 1);
        document.toggle_mark(Mark::Italic);
        document.set_link(Some("https://example.test"));
        let linked = &document.blocks()[0].runs[1];
        assert_eq!(linked.text, "in bold t");
        assert!(linked.marks.italic && linked.marks.link.is_some());
        document.undo();
        document.undo();
        assert_eq!(document.blocks()[0].runs.len(), 1);
    }

    #[test]
    fn enter_backspace_and_cross_block_deletes_keep_structure_sane() {
        let mut document = RichDocument::new();
        document.set_block_kind(BlockKind::Heading(1));
        document.insert_text("Title");
        document.newline();
        document.insert_text("Body one\nBody two");
        document.set_block_kind(BlockKind::Bullet);
        document.newline();
        document.insert_text("second");
        document.newline();
        // Enter on the empty item leaves the list.
        document.newline();
        assert_eq!(
            shape(&document),
            [
                (BlockKind::Heading(1), "Title".into()),
                (BlockKind::Paragraph, "Body one".into()),
                (BlockKind::Bullet, "Body two".into()),
                (BlockKind::Bullet, "second".into()),
                (BlockKind::Paragraph, String::new()),
            ]
        );
        // Backspace at a paragraph start joins; at a list item start it unlists first.
        document.backspace(false);
        assert_eq!(document.blocks().len(), 4);
        assert_eq!(document.cursor(), at(3, 6));
        document.set_cursor(at(2, 0), false);
        document.backspace(false);
        assert_eq!(document.blocks()[2].kind, BlockKind::Paragraph);
        document.backspace(false);
        assert_eq!(document.blocks()[1].text(), "Body oneBody two");
        // A selection across blocks collapses them into one.
        document.set_cursor(at(0, 2), false);
        document.set_cursor(at(2, 3), true);
        assert_eq!(document.selected_text(), "tle\nBody oneBody two\nsec");
        document.insert_text("X");
        assert_eq!(shape(&document), [(BlockKind::Heading(1), "TiXond".into())]);
        document.undo();
        assert_eq!(document.blocks().len(), 3);
    }

    #[test]
    fn code_blocks_keep_line_breaks_and_a_second_enter_leaves_them() {
        let mut document = RichDocument::new();
        document.set_block_kind(BlockKind::Code);
        document.insert_text("let a = 1;");
        document.newline();
        document.insert_text("let b = 2;");
        assert_eq!(document.blocks()[0].text(), "let a = 1;\nlet b = 2;");
        document.newline();
        document.newline();
        assert_eq!(
            shape(&document),
            [
                (BlockKind::Code, "let a = 1;\nlet b = 2;".into()),
                (BlockKind::Paragraph, String::new()),
            ]
        );
    }

    #[test]
    fn movement_covers_graphemes_words_and_block_boundaries() {
        let mut document = RichDocument::from_blocks(vec![
            Block::new(BlockKind::Paragraph, "one e\u{301}👩‍💻"),
            Block::new(BlockKind::Paragraph, "two words"),
        ]);
        document.set_cursor(at(0, 4), false);
        document.move_cursor(RichMotion::Right, false);
        assert_eq!(document.cursor(), at(0, 7));
        document.move_cursor(RichMotion::Right, false);
        document.move_cursor(RichMotion::Right, false);
        assert_eq!(document.cursor(), at(1, 0));
        document.move_cursor(RichMotion::WordRight, true);
        assert_eq!(document.selected_text(), "two ");
        document.move_cursor(RichMotion::Left, false);
        assert_eq!(document.cursor(), at(1, 0));
        document.move_cursor(RichMotion::WordLeft, false);
        assert_eq!(document.cursor().block, 0);
        document.select_word(at(1, 5));
        assert_eq!(document.selected_text(), "words");
        document.move_cursor(RichMotion::DocumentEnd, false);
        assert_eq!(document.cursor(), at(1, 9));
    }

    #[test]
    fn markdown_round_trips_blocks_marks_links_and_escapes() {
        let source = "# Title\n\nSome **bold *both*** and `code` with ~~gone~~, <u>under</u> and [a link](https://example.test/x).\n\n- first\n- second \\*literal\\*\n\n1. one\n2. two\n\n> quoted\n\n```\nfn main() {}\n\nlet x = *p;\n```\n";
        let document = RichDocument::from_markdown(source);
        let kinds: Vec<_> = document.blocks().iter().map(|block| block.kind).collect();
        assert_eq!(
            kinds,
            [
                BlockKind::Heading(1),
                BlockKind::Paragraph,
                BlockKind::Bullet,
                BlockKind::Bullet,
                BlockKind::Numbered,
                BlockKind::Numbered,
                BlockKind::Quote,
                BlockKind::Code,
            ]
        );
        let paragraph = &document.blocks()[1];
        assert_eq!(
            paragraph.text(),
            "Some bold both and code with gone, under and a link."
        );
        let both = paragraph
            .runs
            .iter()
            .find(|run| run.text == "both")
            .unwrap();
        assert!(both.marks.bold && both.marks.italic);
        let link = paragraph
            .runs
            .iter()
            .find(|run| run.text == "a link")
            .unwrap();
        assert_eq!(link.marks.link.as_deref(), Some("https://example.test/x"));
        assert_eq!(document.blocks()[3].text(), "second *literal*");
        assert_eq!(document.blocks()[7].text(), "fn main() {}\n\nlet x = *p;");
        // Writing and reading again is stable.
        let written = document.to_markdown();
        assert_eq!(
            RichDocument::from_markdown(&written).blocks(),
            document.blocks()
        );
        assert_eq!(RichDocument::from_markdown(&written).to_markdown(), written);
    }
}
