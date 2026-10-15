use super::*;
use crate::app::Handler;
use crate::text_area::{
    AreaLine, AreaMark, AreaOp, AreaSelection, MarkKind, line_spans, marks_for,
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
            EditorCommand::SelectNextOccurrence
            | EditorCommand::AddCursorAbove
            | EditorCommand::AddCursorBelow => {
                self.document.apply(AreaOp::Command(command));
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
        match op {
            AreaOp::Clipboard { cut } => {
                *self.clipboard = Some(if cut && !self.read_only {
                    self.document.cut()
                } else {
                    self.document.copy_text()
                });
            }
            AreaOp::Command(command) => self.command(command),
            op if self.read_only && op.is_edit() => {}
            AreaOp::Insert(text) if self.auto_close => {
                self.document.insert_paired(&text);
            }
            AreaOp::Backspace { word: false } if self.auto_close => {
                self.document.backspace_paired();
            }
            op => self.document.apply(op),
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
fn find_bar<M: 'static>(
    ui: &mut Ui<'_, M>,
    find: &mut FindState,
    document: &mut TextDocument,
    read_only: bool,
) {
    let input = |width| Style {
        width: Length::Px(width),
        height: Length::Px(32.0),
        ..compact()
    };
    let row = Style::default().gap(6.0).align_items(Align::Center);
    ui.focus_scope_at(
        "$find",
        FocusScope {
            restore: true,
            autofocus: true,
            ..FocusScope::default()
        },
        Style::default().gap(6.0),
        |ui| {
            ui.row_at("$search", row.clone(), |ui| {
                let query =
                    ui.text_input_at("$query", "Find", &mut find.query, "Find", input(220.0));
                if query.submitted {
                    find.step(document, true);
                }
                if ui
                    .button_at("$previous", "Previous", true, compact())
                    .clicked()
                {
                    find.step(document, false);
                }
                if ui.button_at("$next", "Next", true, compact()).clicked() {
                    find.step(document, true);
                }
                ui.checkbox_at("$case", "Match case", &mut find.case_sensitive);
                ui.enabled(!find.regex, |ui| {
                    ui.checkbox_at("$word", "Whole word", &mut find.whole_word);
                });
                ui.checkbox_at("$regex", "Regular expression", &mut find.regex);
                find.refresh(document);
                let current = document.primary().range();
                let position = find.matches().iter().position(|range| *range == current);
                let muted = ui.theme().text_muted;
                let count = match (find.matches().len(), position) {
                    (0, _) if find.query.is_empty() => String::new(),
                    (0, _) => "No results".into(),
                    (total, Some(index)) => format!("{} of {total}", index + 1),
                    (total, None) => format!("{total} results"),
                };
                // A pattern that cannot be used says so where the count would be.
                // No token means trouble, so the accent stands in for one.
                let (text, colour) = match find.error() {
                    Some(error) => (error.to_owned(), ui.theme().accent),
                    None => (count, muted),
                };
                ui.label_at(
                    "$count",
                    text,
                    Style::default().font_size(13.0).foreground(colour),
                );
                if ui.button_at("$close", "×", true, compact()).clicked() {
                    find.open = false;
                }
                ui.tooltip(ui.context.tr("rust-gui-find-close", &[]));
            });
            if find.replace {
                ui.row_at("$replace", row.clone(), |ui| {
                    ui.text_input_at(
                        "$replacement",
                        "Replace with",
                        &mut find.replacement,
                        "Replace",
                        input(220.0),
                    );
                    if ui
                        .button_at("$one", "Replace", !read_only, compact())
                        .clicked()
                    {
                        find.replace_one(document);
                    }
                    if ui
                        .button_at("$all", "Replace all", !read_only, compact())
                        .clicked()
                    {
                        find.replace_all(document);
                    }
                });
            }
        },
    );
}

impl<'a, M: 'static> Ui<'a, M> {
    /// A monospace, non-wrapping editor over an app-owned document. Only the
    /// visible lines are declared, shaped and painted. Input is queued as
    /// operations and applied here, so the document is never touched between
    /// declarations. Tab indents; Escape followed by Tab leaves the editor.
    /// With `options.find`, Ctrl+F and Ctrl+H open a find bar above the text.
    #[track_caller]
    pub fn code_editor(
        &mut self,
        document: &mut TextDocument,
        options: CodeEditorOptions<'_>,
    ) -> CodeEditorResponse {
        let mut outer = options.style.clone();
        outer.padding = 0.0;
        outer.radius = 0.0;
        outer.background = None;
        outer.gap = 6.0;
        let mut response = CodeEditorResponse::default();
        let key = self.next_key();
        self.group(key, Kind::CodeEditor, outer, |ui| {
            response = ui.editor_body(document, options);
        });
        response
    }

    #[track_caller]
    fn editor_body(
        &mut self,
        document: &mut TextDocument,
        mut options: CodeEditorOptions<'_>,
    ) -> CodeEditorResponse {
        let key = "$area";
        let revision = document.revision();
        let before = document.primary();
        let mut applied = false;
        let mut clipboard = None;
        if let Some(ControlUpdate::TextOps(ops)) =
            self.take_update(&WidgetKey::named(key), Kind::TextArea)
        {
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
        self.editor_view(
            WidgetKey::Named(key.into()),
            document,
            options,
            applied,
            None,
        );
        CodeEditorResponse {
            changed: document.revision() != revision,
            selection_changed: document.primary() != before,
        }
    }

    /// Declares the editor's node with what the document shows now. `applied`
    /// says the caret moved this batch, so it is scrolled into view. `handler`
    /// is what the node sends when the user edits it.
    #[track_caller]
    fn editor_view(
        &mut self,
        key: WidgetKey,
        document: &TextDocument,
        mut options: CodeEditorOptions<'_>,
        applied: bool,
        handler: Option<Handler<M>>,
    ) {
        let id = self.context.child(self.parent, &key, Kind::TextArea);
        let view = id.and_then(|id| self.context.area_view(id));

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
            handler: None,
            seq: 0,
            key,
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
        if let Some(handler) = handler {
            self.set_handler(handler);
        }
    }
}

impl<'a, M: 'static> Ui<'a, M> {
    /// A multi-line editor over an app-owned document, without the frame of
    /// [`Ui::code_editor_msg`]. Each edit arrives as one `TextOp` through
    /// `on`, in order; `update` applies it with `TextDocument::apply`, and the
    /// view declares the document as it is then. The response reports
    /// nothing: changes come through `update`.
    #[track_caller]
    pub fn text_area_msg(
        &mut self,
        document: &TextDocument,
        options: CodeEditorOptions<'_>,
        on: impl Fn(AreaOp) -> M + 'static,
    ) -> CodeEditorResponse {
        if let Some(message) = message_misuse(&options) {
            self.misuse(message.into());
            return CodeEditorResponse::default();
        }
        let handler = text_handler(on, options.read_only, options.auto_close);
        let key = self.next_key();
        self.editor_view(key, document, options, false, Some(handler));
        CodeEditorResponse::default()
    }

    /// [`Ui::code_editor`] over a document the view only reads: each edit
    /// arrives as one `TextOp` through `on`, in order, and `update` applies it
    /// with `TextDocument::apply`. Undo, the caret, the selection and IME
    /// composition stay in the document and the editor, so they survive the
    /// declaration that follows. The find bar and auto-close keep state that
    /// only the view can change, so they are misuses here.
    #[track_caller]
    pub fn code_editor_msg(
        &mut self,
        document: &TextDocument,
        options: CodeEditorOptions<'_>,
        on: impl Fn(AreaOp) -> M + 'static,
    ) -> CodeEditorResponse {
        if let Some(message) = message_misuse(&options) {
            self.misuse(message.into());
            return CodeEditorResponse::default();
        }
        let mut outer = options.style.clone();
        outer.padding = 0.0;
        outer.radius = 0.0;
        outer.background = None;
        outer.gap = 6.0;
        let mut handler = Some(text_handler(on, options.read_only, options.auto_close));
        let key = self.next_key();
        self.group(key, Kind::CodeEditor, outer, |ui| {
            ui.editor_view(
                WidgetKey::Named("$area".into()),
                document,
                options,
                false,
                handler.take(),
            );
        });
        CodeEditorResponse::default()
    }
}

/// What the message forms cannot declare, as a misuse message.
fn message_misuse(options: &CodeEditorOptions<'_>) -> Option<&'static str> {
    options
        .find
        .is_some()
        .then_some("the find bar keeps its state in the view; use code_editor for it")
}

/// The handler of an editor that sends its edits. A read-only editor drops
/// the edits and turns a cut into a copy, as the kernel does; auto-close
/// turns a typed bracket or quote, and a backspace, into their paired ops.
fn text_handler<M: 'static>(
    on: impl Fn(AreaOp) -> M + 'static,
    read_only: bool,
    auto_close: bool,
) -> Handler<M> {
    Handler::Each(Box::new(move |update, out| {
        let ControlUpdate::TextOps(ops) = update else {
            return;
        };
        for op in ops {
            let op = match op {
                AreaOp::Clipboard { cut } => AreaOp::Clipboard {
                    cut: cut && !read_only,
                },
                op if read_only && op.is_edit() => continue,
                AreaOp::Insert(text) if auto_close => AreaOp::InsertPaired(text),
                AreaOp::Backspace { word: false } if auto_close => AreaOp::BackspacePaired,
                op => op,
            };
            out.push(on(op));
        }
    }))
}
