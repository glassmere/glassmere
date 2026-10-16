use super::*;
use crate::app::Handler;
use crate::text_area::{
    AreaLine, AreaMark, AreaOp, AreaSelection, FindEdit, MarkKind, line_spans, marks_for,
};
use crate::{
    Align, Button, CodeEditorOptions, EditorCommand, FindState, FocusScope, Length, Severity,
    TextDocument, TextInput,
};
use std::rc::Rc;

/// Lines declared beyond the viewport on each side, so small scrolls and the
/// first frame after a resize already have their text.
const OVERSCAN: usize = 8;

fn compact() -> Style {
    Style {
        padding: 6.0,
        radius: 6.0,
        ..Style::button()
    }
}

/// The find bar: query, options, match count and, optionally, replacement.
/// Every change is sent as a `TextOp` that `FindState::apply` handles.
fn find_bar<M: 'static>(
    ui: &mut Ui<'_, M>,
    find: &FindState,
    document: &TextDocument,
    read_only: bool,
    on: &Rc<dyn Fn(AreaOp) -> M>,
) {
    let input = |width| Style {
        width: Length::Px(width),
        height: Length::Px(32.0),
        ..compact()
    };
    let row = Style::default().gap(6.0).align_items(Align::Center);
    let command = |command: EditorCommand| on(AreaOp::Command(command));
    let edit = |edit: FindEdit| on(AreaOp::Find(edit));
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
                let typed = on.clone();
                ui.at("$query").text_input(
                    TextInput::new("Find")
                        .placeholder("Find")
                        .style(input(220.0))
                        .on_submit(command(EditorCommand::FindNext)),
                    &find.query,
                    move |query| typed(AreaOp::Find(FindEdit::Query(query))),
                );
                ui.at("$previous").button(
                    Button::new("Previous")
                        .style(compact())
                        .on_click(command(EditorCommand::FindPrevious)),
                );
                ui.at("$next").button(
                    Button::new("Next")
                        .style(compact())
                        .on_click(command(EditorCommand::FindNext)),
                );
                ui.at("$case")
                    .checkbox("Match case", find.case_sensitive, |on| {
                        edit(FindEdit::CaseSensitive(on))
                    });
                ui.enabled(!find.regex, |ui| {
                    ui.at("$word")
                        .checkbox("Whole word", find.whole_word, |on| {
                            edit(FindEdit::WholeWord(on))
                        });
                });
                ui.at("$regex")
                    .checkbox("Regular expression", find.regex, |on| {
                        edit(FindEdit::Regex(on))
                    });
                let (matches, error) = find.found(document);
                let current = document.primary().range();
                let position = matches.iter().position(|range| *range == current);
                let muted = ui.theme().text_muted;
                let count = match (matches.len(), position) {
                    (0, _) if find.query.is_empty() => String::new(),
                    (0, _) => "No results".into(),
                    (total, Some(index)) => format!("{} of {total}", index + 1),
                    (total, None) => format!("{total} results"),
                };
                // A pattern that cannot be used says so where the count would be.
                // No token means trouble, so the accent stands in for one.
                let (text, colour) = match error {
                    Some(error) => (error.into_owned(), ui.theme().accent),
                    None => (count, muted),
                };
                ui.label_at(
                    "$count",
                    text,
                    Style::default().font_size(13.0).foreground(colour),
                );
                ui.at("$close").button(
                    Button::new("×")
                        .style(compact())
                        .on_click(command(EditorCommand::CloseFind)),
                );
                ui.tooltip(ui.context.tr("rust-gui-find-close", &[]));
            });
            if find.replace {
                ui.row_at("$replace", row.clone(), |ui| {
                    let typed = on.clone();
                    ui.at("$replacement").text_input(
                        TextInput::new("Replace with")
                            .placeholder("Replace")
                            .style(input(220.0)),
                        &find.replacement,
                        move |text| typed(AreaOp::Find(FindEdit::Replacement(text))),
                    );
                    ui.at("$one").button(
                        Button::new("Replace")
                            .enabled(!read_only)
                            .style(compact())
                            .on_click(edit(FindEdit::ReplaceOne)),
                    );
                    ui.at("$all").button(
                        Button::new("Replace all")
                            .enabled(!read_only)
                            .style(compact())
                            .on_click(edit(FindEdit::ReplaceAll)),
                    );
                });
            }
        },
    );
}

impl<'a, M: 'static> Ui<'a, M> {
    /// Declares the editor's node with what the document shows now.
    /// `handler` is what the node sends when the user edits it.
    #[track_caller]
    fn editor_view(
        &mut self,
        key: WidgetKey,
        document: &TextDocument,
        options: CodeEditorOptions<'_>,
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
        // The caret is scrolled into view when it is not where the last layout
        // saw it, which is what an edit or a find step does to it.
        let moved = view.is_none_or(|view| view.primary != Some(primary));
        if moved && view.is_some() {
            let top = primary.0 as f32 * line_height;
            scroll_y = scroll_y.min(top).max(top + line_height - viewport);
        }
        scroll_y = scroll_y.clamp(0.0, (total as f32 * line_height - viewport).max(0.0));

        let first = ((scroll_y / line_height) as usize).saturating_sub(OVERSCAN);
        let last = (first + (viewport / line_height).ceil() as usize + 2 * OVERSCAN + 1).min(total);
        let mut highlighter = options.highlighter.map(|cell| cell.borrow_mut());
        if let Some(highlighter) = highlighter.as_mut() {
            highlighter.begin(document);
        }
        let lines = (first..last)
            .map(|line| {
                let text = document.line(line);
                let highlights = highlighter.as_mut().map_or_else(Vec::new, |highlighter| {
                    highlighter.highlight(document, line)
                });
                AreaLine {
                    spans: line_spans(&text, &highlights),
                    text,
                }
            })
            .collect();
        if highlighter
            .as_ref()
            .is_some_and(|highlighter| highlighter.pending())
        {
            self.request_redraw();
        }
        drop(highlighter);

        // Decorations inside the declared window of lines.
        let window = document.line_range(first).start
            ..document.line_range(last.saturating_sub(1).max(first)).end;
        let inside =
            |range: &std::ops::Range<usize>| range.start <= window.end && range.end >= window.start;
        let mut marks: Vec<AreaMark> = Vec::new();
        if let Some(find) = options.find
            && find.open
        {
            let (matches, _) = find.found(document);
            let current = document.primary().range();
            let from = matches.partition_point(|range| range.end < window.start);
            for range in matches[from..]
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
    /// [`Ui::code_editor`]. Each edit arrives as one `TextOp` through
    /// `on`, in order; `update` applies it with `TextDocument::apply_with`, and
    /// the view declares the document as it is then.
    #[track_caller]
    pub fn text_area(
        &mut self,
        document: &TextDocument,
        options: CodeEditorOptions<'_>,
        on: impl Fn(AreaOp) -> M + 'static,
    ) {
        if options.find.is_some() {
            self.misuse("a text area has no find bar; use code_editor for it".into());
            return;
        }
        let handler = text_handler(Rc::new(on), options.read_only, options.auto_close);
        let key = self.next_key();
        self.editor_view(key, document, options, Some(handler));
    }

    /// A code editor over a document the view only reads: each edit arrives
    /// as one `TextOp` through `on`, in order, and `update` applies it with
    /// `TextDocument::apply_with`. Undo, the caret, the selection and IME
    /// composition stay in the document and the editor, so they survive the
    /// declaration that follows. A copy or cut is a `TextOp::Clipboard`, which
    /// `apply_with` puts on the clipboard in the batch that carries it.
    ///
    /// With `options.find`, Ctrl+F and Ctrl+H open a find bar above the text.
    /// The application owns the `FindState` and applies every `TextOp` with
    /// `FindState::apply` instead, which also handles the bar's own edits;
    /// the view reads it. The highlighter, if any, lives in a `RefCell`.
    /// Tab indents; Escape followed by Tab leaves the editor.
    #[track_caller]
    pub fn code_editor(
        &mut self,
        document: &TextDocument,
        options: CodeEditorOptions<'_>,
        on: impl Fn(AreaOp) -> M + 'static,
    ) {
        let mut outer = options.style.clone();
        outer.padding = 0.0;
        outer.radius = 0.0;
        outer.background = None;
        outer.gap = 6.0;
        let on: Rc<dyn Fn(AreaOp) -> M> = Rc::new(on);
        let mut handler = Some(text_handler(
            on.clone(),
            options.read_only,
            options.auto_close,
        ));
        let key = self.next_key();
        self.group(key, Kind::CodeEditor, outer, |ui| {
            if let Some(find) = options.find
                && find.open
            {
                find_bar(ui, find, document, options.read_only, &on);
            }
            ui.editor_view(
                WidgetKey::Named("$area".into()),
                document,
                options,
                handler.take(),
            );
        });
    }
}

/// The handler of an editor that sends its edits. A read-only editor drops
/// the edits and turns a cut into a copy, as the kernel does; auto-close
/// turns a typed bracket or quote, and a backspace, into their paired ops.
fn text_handler<M: 'static>(
    on: Rc<dyn Fn(AreaOp) -> M>,
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
