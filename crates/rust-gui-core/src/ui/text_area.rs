use super::*;
use crate::text_area::{AreaLine, AreaOp, AreaSelection, Granularity, line_spans};
use crate::{CodeEditorOptions, CodeEditorResponse, Length, TextDocument};

/// Lines declared beyond the viewport on each side, so small scrolls and the
/// first frame after a resize already have their text.
const OVERSCAN: usize = 8;

fn apply(document: &mut TextDocument, op: AreaOp, read_only: bool, clipboard: &mut Option<String>) {
    match op {
        AreaOp::Move { motion, select } => document.move_cursors(motion, select),
        AreaOp::Pointer {
            line,
            byte,
            select,
            granularity,
        } => {
            let offset = document.offset(line, byte);
            match granularity {
                Granularity::Char => {
                    let anchor = if select {
                        document.primary().anchor
                    } else {
                        offset
                    };
                    document.set_selection(anchor, offset);
                }
                Granularity::Word => {
                    let word = document.word_at(offset);
                    document.set_selection(word.start, word.end);
                }
                Granularity::Line => {
                    let range = document.line_range(line);
                    document.set_selection(range.start, range.end);
                }
            }
        }
        AreaOp::SelectAll => document.select_all(),
        AreaOp::CollapseSelections => document.collapse_selections(),
        AreaOp::Clipboard { cut } => {
            *clipboard = Some(if cut && !read_only {
                document.cut()
            } else {
                document.copy_text()
            });
        }
        _ if read_only => {}
        AreaOp::Insert(text) => {
            document.insert(&text);
        }
        AreaOp::Backspace { word } => {
            document.backspace(word);
        }
        AreaOp::Delete { word } => {
            document.delete(word);
        }
        AreaOp::Newline => {
            document.newline();
        }
        AreaOp::Indent => {
            document.indent();
        }
        AreaOp::Outdent => {
            document.outdent();
        }
        AreaOp::Undo => {
            document.undo();
        }
        AreaOp::Redo => {
            document.redo();
        }
    }
}

impl Ui<'_> {
    /// A monospace, non-wrapping editor over an app-owned document. Only the
    /// visible lines are declared, shaped and painted. Input is queued as
    /// operations and applied here, so the document is never touched between
    /// declarations. Tab indents; Escape followed by Tab leaves the editor.
    pub fn code_editor(
        &mut self,
        key: impl Into<String>,
        document: &mut TextDocument,
        mut options: CodeEditorOptions<'_>,
    ) -> CodeEditorResponse {
        let key = key.into();
        let view = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::TextArea)
            .and_then(|id| self.context.area_view(id));
        let revision = document.revision();
        let before = document.primary();
        let mut applied = false;
        let mut clipboard = None;
        if let Some(ControlUpdate::TextOps(ops)) = self.take_update(&key, Kind::TextArea) {
            for op in ops {
                applied = true;
                apply(document, op, options.read_only, &mut clipboard);
            }
        }
        if let Some(text) = clipboard {
            *self.clipboard = Some(text);
        }

        let font_size = if options.font_size.is_finite() {
            options.font_size.clamp(6.0, 96.0)
        } else {
            14.0
        };
        let line_height = font_size * 1.4;
        let total = document.len_lines();
        let primary = document.position(document.primary().cursor);
        // Before the first layout, guess the viewport from the style.
        let viewport = view.map_or(
            match options.style.height {
                Length::Px(height) => height,
                _ => 800.0,
            },
            |view| view.viewport,
        );
        let mut scroll_y = view.map_or(0.0, |view| view.scroll_y);
        let moved = view.is_none_or(|view| view.primary != Some(primary));
        if (applied || moved) && view.is_some() {
            let top = primary.0 as f32 * line_height;
            scroll_y = scroll_y.min(top).max(top + line_height - viewport);
        }
        scroll_y = scroll_y.clamp(0.0, (total as f32 * line_height - viewport).max(0.0));

        let first = ((scroll_y / line_height) as usize).saturating_sub(OVERSCAN);
        let last = (first + (viewport / line_height).ceil() as usize + 2 * OVERSCAN + 1).min(total);
        let lines = (first..last)
            .map(|line| {
                let text = document.line(line);
                let highlights = options
                    .highlighter
                    .as_mut()
                    .map_or_else(Vec::new, |highlighter| {
                        highlighter.highlight(document, line)
                    });
                AreaLine {
                    spans: line_spans(&text, &highlights),
                    text,
                }
            })
            .collect();
        let selections = document
            .selections()
            .iter()
            .filter_map(|selection| {
                let start = document.position(selection.range().start);
                let end = document.position(selection.range().end);
                (end.0 >= first && start.0 < last).then(|| AreaSelection {
                    start,
                    end,
                    cursor: document.position(selection.cursor),
                    primary: *selection == document.primary(),
                })
            })
            .collect();
        self.push(Element {
            key: WidgetKey::Named(key),
            kind: Kind::TextArea,
            text: String::new(),
            style: options.style,
            enabled: self.enabled,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control: Control::TextArea {
                label: options.label,
                lines,
                first_line: first,
                total_lines: total,
                selections,
                selection_count: document.selections().len(),
                primary,
                font_size,
                line_numbers: options.line_numbers,
                read_only: options.read_only,
                scroll_y,
            },
        });
        CodeEditorResponse {
            changed: document.revision() != revision,
            selection_changed: document.primary() != before,
        }
    }
}
