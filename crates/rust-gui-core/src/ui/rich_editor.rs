use super::*;
use crate::rich_editor::views;
use crate::{Align, BlockKind, Length, Mark, RichDocument, RichEditorOptions, RichEditorResponse};

impl Ui<'_> {
    /// A WYSIWYG editor over an app-owned `RichDocument`: wrapped paragraphs,
    /// headings, lists, quotes and code blocks with bold, italic, underline,
    /// strikethrough, inline code and links. Input is queued and applied
    /// here, like `code_editor`. Ctrl+B, Ctrl+I and Ctrl+U toggle marks; the
    /// optional toolbar offers marks and block kinds.
    pub fn rich_text_editor(
        &mut self,
        key: impl Into<String>,
        document: &mut RichDocument,
        options: RichEditorOptions,
    ) -> RichEditorResponse {
        let mut outer = options.style.clone();
        outer.padding = 0.0;
        outer.radius = 0.0;
        outer.background = None;
        outer.gap = 6.0;
        let mut response = RichEditorResponse::default();
        self.group(key.into(), Kind::CodeEditor, outer, |ui| {
            response = ui.rich_editor_body(document, options);
        });
        response
    }

    fn rich_editor_body(
        &mut self,
        document: &mut RichDocument,
        options: RichEditorOptions,
    ) -> RichEditorResponse {
        let key = "$area";
        let revision = document.revision();
        let before = (document.anchor(), document.cursor());
        let mut clipboard = None;
        if let Some(ControlUpdate::RichOps(ops)) = self.take_update(key, Kind::RichEditor) {
            for op in ops {
                op.apply(document, options.read_only, &mut clipboard);
            }
        }
        if let Some(text) = clipboard {
            *self.clipboard = Some(text);
        }
        if options.toolbar && !options.read_only && self.rich_toolbar(document) {
            // Formatting from the toolbar keeps the text ready for typing.
            self.request_focus(key);
        }
        let font_size = if options.font_size.is_finite() {
            options.font_size.clamp(8.0, 72.0)
        } else {
            16.0
        };
        let accent = self.theme().accent;
        let mut style = Style::default()
            .width(Length::Percent(1.0))
            .grow(1.0)
            .shrink(1.0)
            .padding(options.style.padding)
            .radius(options.style.radius);
        style.background = options.style.background;
        self.push(Element {
            key: WidgetKey::Named(key.into()),
            kind: Kind::RichEditor,
            text: String::new(),
            style,
            enabled: self.enabled,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control: Control::RichEditor {
                label: options.label,
                blocks: views(document, accent),
                anchor: document.anchor(),
                cursor: document.cursor(),
                font_size,
                read_only: options.read_only,
            },
        });
        RichEditorResponse {
            changed: document.revision() != revision,
            selection_changed: before != (document.anchor(), document.cursor()),
            link_at_cursor: document.marks().link,
        }
    }

    /// Mark and block buttons reflecting the state at the cursor. Returns
    /// whether one was used.
    fn rich_toolbar(&mut self, document: &mut RichDocument) -> bool {
        let theme = self.theme().clone();
        let marks = document.marks();
        let kind = document.block_kind();
        let mut used = false;
        let button = |active: bool| {
            let mut style = Style {
                padding: 6.0,
                radius: 6.0,
                min_width: Length::Px(34.0),
                ..Style::button()
            };
            if active {
                style.background = Some(theme.accent);
                style.foreground = Some(theme.window);
            }
            style
        };
        self.row(
            "$toolbar",
            Style::default().gap(4.0).align_items(Align::Center),
            |ui| {
                for (label, tip, mark) in [
                    ("B", "Bold (Ctrl+B)", Mark::Bold),
                    ("I", "Italic (Ctrl+I)", Mark::Italic),
                    ("U", "Underline (Ctrl+U)", Mark::Underline),
                    ("S", "Strikethrough (Ctrl+Shift+X)", Mark::Strikethrough),
                    ("<>", "Inline code (Ctrl+E)", Mark::Code),
                ] {
                    if ui
                        .button_styled(
                            format!("$mark-{label}"),
                            label,
                            true,
                            button(marks.has(mark)),
                        )
                        .clicked()
                    {
                        document.toggle_mark(mark);
                        used = true;
                    }
                    ui.tooltip(tip);
                }
                ui.label_styled("$gap", "", Style::default().width(Length::Px(8.0)));
                for (label, tip, block) in [
                    ("H1", "Heading 1", BlockKind::Heading(1)),
                    ("H2", "Heading 2", BlockKind::Heading(2)),
                    ("H3", "Heading 3", BlockKind::Heading(3)),
                    ("•", "Bulleted list", BlockKind::Bullet),
                    ("1.", "Numbered list", BlockKind::Numbered),
                    ("”", "Quote", BlockKind::Quote),
                    ("{ }", "Code block", BlockKind::Code),
                ] {
                    if ui
                        .button_styled(format!("$block-{tip}"), label, true, button(kind == block))
                        .clicked()
                    {
                        document.set_block_kind(block);
                        used = true;
                    }
                    ui.tooltip(tip);
                }
            },
        );
        used
    }
}
