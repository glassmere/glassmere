use super::*;
use crate::text_area::{
    AreaLine, AreaMark, AreaOp, AreaSelection, Granularity, MarkKind, line_spans, marks_for,
};
use crate::{
    Align, CodeEditorOptions, CodeEditorResponse, EditorCommand, FindState, FocusScope, Length,
    Severity, TextDocument,
};

/// Lines declared beyond the viewport on each side, so small scrolls and the
/// first frame after a resize already have their text.
const OVERSCAN: usize = 8;

struct Applied<'a, 'b> {
    document: &'a mut TextDocument,
    find: Option<&'a mut FindState>,
    read_only: bool,
    auto_close: bool,
    clipboard: &'b mut Option<String>,
}

impl Applied<'_, '_> {
    fn command(&mut self, command: EditorCommand) {
        match command {
            EditorCommand::SelectNextOccurrence => {
                self.document.select_next_occurrence();
            }
            EditorCommand::AddCursorAbove => {
                self.document.add_cursor_vertically(false);
            }
            EditorCommand::AddCursorBelow => {
                self.document.add_cursor_vertically(true);
            }
            // Resolved into a pointer operation when the event arrives, or meant
            // for rich text editors.
            EditorCommand::AddCursorAt(_)
            | EditorCommand::ToggleBold
            | EditorCommand::ToggleItalic
            | EditorCommand::ToggleUnderline
            | EditorCommand::ToggleStrikethrough
            | EditorCommand::ToggleCode => {}
            search => {
                let Some(find) = self.find.as_deref_mut() else {
                    return;
                };
                match search {
                    EditorCommand::Find | EditorCommand::Replace => {
                        find.open = true;
                        find.replace |= search == EditorCommand::Replace;
                        // Search for the selection, as editors do.
                        let selected = self.document.selected_text();
                        if !selected.is_empty() && !selected.contains('\n') {
                            find.query = selected;
                        }
                    }
                    EditorCommand::CloseFind => find.open = false,
                    EditorCommand::FindNext => {
                        find.step(self.document, true);
                    }
                    _ => {
                        find.step(self.document, false);
                    }
                }
            }
        }
    }

    fn op(&mut self, op: AreaOp) {
        let document = &mut *self.document;
        match op {
            AreaOp::Move { motion, select } => document.move_cursors(motion, select),
            AreaOp::Pointer {
                line,
                byte,
                select,
                granularity,
                add,
            } => {
                let offset = document.offset(line, byte);
                let (anchor, cursor) = match granularity {
                    Granularity::Char if select => (document.primary().anchor, offset),
                    Granularity::Char => (offset, offset),
                    Granularity::Word => {
                        let word = document.word_at(offset);
                        (word.start, word.end)
                    }
                    Granularity::Line => {
                        let range = document.line_range(line);
                        (range.start, range.end)
                    }
                };
                if add {
                    document.add_selection(anchor, cursor);
                } else {
                    document.set_selection(anchor, cursor);
                }
            }
            AreaOp::SelectAll => document.select_all(),
            AreaOp::CollapseSelections => document.collapse_selections(),
            AreaOp::Clipboard { cut } => {
                *self.clipboard = Some(if cut && !self.read_only {
                    document.cut()
                } else {
                    document.copy_text()
                });
            }
            AreaOp::Command(command) => self.command(command),
            _ if self.read_only => {}
            AreaOp::Insert(text) => {
                if self.auto_close {
                    document.insert_paired(&text);
                } else {
                    document.insert(&text);
                }
            }
            AreaOp::Backspace { word: false } if self.auto_close => {
                document.backspace_paired();
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
}

fn compact() -> Style {
    Style {
        padding: 6.0,
        radius: 6.0,
        ..Style::button()
    }
}

/// The find bar: query, options, match count and, optionally, replacement.
fn find_bar(ui: &mut Ui<'_>, find: &mut FindState, document: &mut TextDocument, read_only: bool) {
    let input = |width| Style {
        width: Length::Px(width),
        height: Length::Px(32.0),
        ..compact()
    };
    let row = Style::default().gap(6.0).align_items(Align::Center);
    ui.focus_scope(
        "$find",
        FocusScope {
            restore: true,
            autofocus: true,
            ..FocusScope::default()
        },
        Style::default().gap(6.0),
        |ui| {
            ui.row("$search", row.clone(), |ui| {
                let query =
                    ui.text_input_styled("$query", "Find", &mut find.query, "Find", input(220.0));
                if query.submitted {
                    find.step(document, true);
                }
                if ui
                    .button_styled("$previous", "Previous", true, compact())
                    .clicked()
                {
                    find.step(document, false);
                }
                if ui.button_styled("$next", "Next", true, compact()).clicked() {
                    find.step(document, true);
                }
                ui.checkbox("$case", "Match case", &mut find.case_sensitive);
                ui.checkbox("$word", "Whole word", &mut find.whole_word);
                find.refresh(document);
                let current = document.primary().range();
                let position = find.matches().iter().position(|range| *range == current);
                let muted = ui.theme().text_muted;
                ui.label_styled(
                    "$count",
                    match (find.matches().len(), position) {
                        (0, _) if find.query.is_empty() => String::new(),
                        (0, _) => "No results".into(),
                        (total, Some(index)) => format!("{} of {total}", index + 1),
                        (total, None) => format!("{total} results"),
                    },
                    Style::default().font_size(13.0).foreground(muted),
                );
                if ui.button_styled("$close", "×", true, compact()).clicked() {
                    find.open = false;
                }
                ui.tooltip("Close the find bar");
            });
            if find.replace {
                ui.row("$replace", row.clone(), |ui| {
                    ui.text_input_styled(
                        "$replacement",
                        "Replace with",
                        &mut find.replacement,
                        "Replace",
                        input(220.0),
                    );
                    if ui
                        .button_styled("$one", "Replace", !read_only, compact())
                        .clicked()
                    {
                        find.replace_one(document);
                    }
                    if ui
                        .button_styled("$all", "Replace all", !read_only, compact())
                        .clicked()
                    {
                        find.replace_all(document);
                    }
                });
            }
        },
    );
}

impl Ui<'_> {
    /// A monospace, non-wrapping editor over an app-owned document. Only the
    /// visible lines are declared, shaped and painted. Input is queued as
    /// operations and applied here, so the document is never touched between
    /// declarations. Tab indents; Escape followed by Tab leaves the editor.
    /// With `options.find`, Ctrl+F and Ctrl+H open a find bar above the text.
    pub fn code_editor(
        &mut self,
        key: impl Into<String>,
        document: &mut TextDocument,
        options: CodeEditorOptions<'_>,
    ) -> CodeEditorResponse {
        let mut outer = options.style.clone();
        outer.padding = 0.0;
        outer.radius = 0.0;
        outer.background = None;
        outer.gap = 6.0;
        let mut response = CodeEditorResponse::default();
        self.group(key.into(), Kind::CodeEditor, outer, |ui| {
            response = ui.editor_body(document, options);
        });
        response
    }

    fn editor_body(
        &mut self,
        document: &mut TextDocument,
        mut options: CodeEditorOptions<'_>,
    ) -> CodeEditorResponse {
        let key = "$area";
        let id = self
            .context
            .child(self.parent, &WidgetKey::Named(key.into()), Kind::TextArea);
        let view = id.and_then(|id| self.context.area_view(id));
        let revision = document.revision();
        let before = document.primary();
        let mut applied = false;
        let mut clipboard = None;
        if let Some(ControlUpdate::TextOps(ops)) = self.take_update(key, Kind::TextArea) {
            let mut target = Applied {
                document: &mut *document,
                find: options.find.as_deref_mut(),
                read_only: options.read_only,
                auto_close: options.auto_close,
                clipboard: &mut clipboard,
            };
            for op in ops {
                applied = true;
                target.op(op);
            }
        }
        if let Some(text) = clipboard {
            *self.clipboard = Some(text);
        }
        if let Some(find) = options.find.as_deref_mut()
            && find.open
        {
            find_bar(self, find, document, options.read_only);
        }
        applied |= document.primary() != before;

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
        if let Some(highlighter) = options.highlighter.as_mut() {
            highlighter.begin(document);
        }
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
        if options
            .highlighter
            .as_ref()
            .is_some_and(|highlighter| highlighter.pending())
        {
            self.request_redraw();
        }

        // Decorations inside the declared window of lines.
        let window = document.line_range(first).start
            ..document.line_range(last.saturating_sub(1).max(first)).end;
        let inside =
            |range: &std::ops::Range<usize>| range.start <= window.end && range.end >= window.start;
        let mut marks: Vec<AreaMark> = Vec::new();
        if let Some(find) = options.find.as_deref_mut()
            && find.open
        {
            find.refresh(document);
            let current = document.primary().range();
            let from = find
                .matches()
                .partition_point(|range| range.end < window.start);
            for range in find.matches()[from..]
                .iter()
                .take_while(|range| range.start <= window.end)
            {
                let kind = if *range == current {
                    MarkKind::CurrentMatch
                } else {
                    MarkKind::Match
                };
                marks_for(document, range.clone(), &kind, first..last, &mut marks);
            }
        }
        if options.match_brackets
            && document.primary().is_caret()
            && let Some((here, partner)) = document.matching_bracket(document.primary().cursor)
        {
            for at in [here, partner] {
                marks_for(
                    document,
                    at..at + 1,
                    &MarkKind::Bracket,
                    first..last,
                    &mut marks,
                );
            }
        }
        let mut gutter_marks: Vec<(usize, Severity)> = Vec::new();
        for diagnostic in options.diagnostics.iter().filter(|d| inside(&d.range)) {
            let len = document.len_chars();
            let mut range = diagnostic.range.start.min(len)..diagnostic.range.end.min(len);
            // An empty range still underlines one character.
            if range.is_empty() {
                range.end = (range.start + 1).min(len);
            }
            let kind = MarkKind::Diagnostic(diagnostic.severity, diagnostic.message.clone());
            marks_for(document, range.clone(), &kind, first..last, &mut marks);
            let line = document.position(range.start).0;
            match gutter_marks.iter_mut().find(|mark| mark.0 == line) {
                Some(mark) => mark.1 = mark.1.max(diagnostic.severity),
                None => gutter_marks.push((line, diagnostic.severity)),
            }
        }

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
        let mut style = Style::default()
            .width(Length::Percent(1.0))
            .grow(1.0)
            .shrink(1.0)
            .padding(options.style.padding)
            .radius(options.style.radius);
        style.background = options.style.background;
        // Source code reads left to right whatever the UI direction is.
        style.direction = Some(crate::Direction::Ltr);
        self.push(Element {
            key: WidgetKey::Named(key.into()),
            kind: Kind::TextArea,
            text: String::new(),
            style,
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
                marks,
                gutter_marks,
            },
        });
        CodeEditorResponse {
            changed: document.revision() != revision,
            selection_changed: document.primary() != before,
        }
    }
}
