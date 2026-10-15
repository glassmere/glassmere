//! Options, response and queued operations of the WYSIWYG rich text editor.
use crate::{BlockKind, Length, Mark, Marks, RichDocument, RichMotion, RichPosition, Style};
use rust_gui_scene::{Color, FontFamily, TextSpan};
use std::ops::Range;

pub struct RichEditorOptions {
    /// Accessible name.
    pub label: String,
    pub style: Style,
    /// Body text size; headings scale from it.
    pub font_size: f32,
    /// Selection, copy and navigation still work.
    pub read_only: bool,
    /// Show the formatting toolbar above the text.
    pub toolbar: bool,
}

impl Default for RichEditorOptions {
    fn default() -> Self {
        Self {
            label: "Rich text".into(),
            style: Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Px(320.0))
                .padding(12.0)
                .radius(8.0),
            font_size: 16.0,
            read_only: false,
            toolbar: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RichEditorResponse {
    /// The document's content changed during this declaration.
    pub changed: bool,
    pub selection_changed: bool,
    /// Target of the link at the cursor, for an "open link" affordance.
    pub link_at_cursor: Option<String>,
}

/// How far a click in a rich editor selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RichGranularity {
    Char,
    Word,
    Block,
}

/// One edit to a rich editor's document, as a message carries it.
#[derive(Clone, Debug, PartialEq)]
pub enum RichOp {
    Insert(String),
    Backspace {
        word: bool,
    },
    Delete {
        word: bool,
    },
    Newline,
    Move {
        motion: RichMotion,
        select: bool,
    },
    /// A position resolved from the shaped text: clicks and visual-line moves.
    SetCursor {
        position: RichPosition,
        select: bool,
        granularity: RichGranularity,
    },
    SelectAll,
    Undo,
    Redo,
    Clipboard {
        cut: bool,
    },
    Toggle(Mark),
    /// The block kind of the paragraph at the cursor, from the toolbar.
    SetBlock(BlockKind),
}

impl RichOp {
    /// Whether the op changes the document. A read-only editor drops these.
    pub(crate) fn is_edit(&self) -> bool {
        matches!(
            self,
            Self::Insert(_)
                | Self::Backspace { .. }
                | Self::Delete { .. }
                | Self::Newline
                | Self::Undo
                | Self::Redo
                | Self::Toggle(_)
                | Self::SetBlock(_)
        )
    }

    pub(crate) fn apply(
        self,
        document: &mut RichDocument,
        read_only: bool,
        clipboard: &mut Option<String>,
    ) {
        match self {
            Self::Clipboard { cut } => {
                *clipboard = Some(if cut && !read_only {
                    document.cut()
                } else {
                    document.selected_text()
                });
            }
            op if read_only && op.is_edit() => {}
            op => document.apply(op),
        }
    }
}

impl RichDocument {
    /// Applies one operation to the document. A cut removes the selection;
    /// the text a copy or cut puts on the clipboard is `selected_text()`,
    /// read before applying.
    pub fn apply(&mut self, op: RichOp) {
        match op {
            RichOp::Move { motion, select } => self.move_cursor(motion, select),
            RichOp::SetCursor {
                position,
                select,
                granularity,
            } => match granularity {
                RichGranularity::Char => self.set_cursor(position, select),
                RichGranularity::Word => self.select_word(position),
                RichGranularity::Block => self.select_block(position.block),
            },
            RichOp::SelectAll => self.select_all(),
            RichOp::Clipboard { cut } => {
                if cut {
                    self.cut();
                }
            }
            RichOp::Insert(text) => self.insert_text(&text),
            RichOp::Backspace { word } => self.backspace(word),
            RichOp::Delete { word } => self.delete(word),
            RichOp::Newline => self.newline(),
            RichOp::Undo => {
                self.undo();
            }
            RichOp::Redo => {
                self.redo();
            }
            RichOp::Toggle(mark) => self.toggle_mark(mark),
            RichOp::SetBlock(kind) => self.set_block_kind(kind),
        }
    }
}

/// A decorated byte range of a block: rules and inline code backgrounds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RichRule {
    pub range: Range<usize>,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link: bool,
}

/// One declared block, ready to shape and paint.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RichBlockView {
    pub kind: BlockKind,
    /// Position in a numbered list, from one.
    pub number: usize,
    pub text: String,
    pub spans: Vec<TextSpan>,
    pub rules: Vec<RichRule>,
}

impl RichBlockView {
    pub(crate) fn font_scale(kind: BlockKind) -> f32 {
        match kind {
            BlockKind::Heading(1) => 1.8,
            BlockKind::Heading(2) => 1.45,
            BlockKind::Heading(_) => 1.2,
            _ => 1.0,
        }
    }

    pub(crate) fn indent(kind: BlockKind) -> f32 {
        match kind {
            BlockKind::Bullet | BlockKind::Numbered => 28.0,
            BlockKind::Quote => 16.0,
            BlockKind::Code => 10.0,
            _ => 0.0,
        }
    }
}

pub(crate) fn views(document: &RichDocument, link: Color) -> Vec<RichBlockView> {
    let mut number = 0;
    document
        .blocks()
        .iter()
        .map(|block| {
            number = if block.kind == BlockKind::Numbered {
                number + 1
            } else {
                0
            };
            let mut at = 0;
            let mut rules = Vec::new();
            let spans = block
                .runs
                .iter()
                .map(|run| {
                    let Marks {
                        bold,
                        italic,
                        underline,
                        strikethrough,
                        code,
                        link: target,
                    } = &run.marks;
                    let range = at..at + run.text.len();
                    at = range.end;
                    if *underline || *strikethrough || *code || target.is_some() {
                        rules.push(RichRule {
                            range,
                            underline: *underline,
                            strikethrough: *strikethrough,
                            code: *code,
                            link: target.is_some(),
                        });
                    }
                    TextSpan {
                        text: run.text.clone(),
                        color: target.is_some().then_some(link),
                        weight: bold.then_some(700),
                        italic: italic.then_some(true),
                        family: code.then_some(FontFamily::Monospace),
                    }
                })
                .collect();
            RichBlockView {
                kind: block.kind,
                number,
                text: block.text(),
                spans,
                rules,
            }
        })
        .collect()
}
