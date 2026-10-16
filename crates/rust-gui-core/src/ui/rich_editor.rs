use super::*;
use crate::app::Handler;
use crate::rich_editor::{RichOp, views};
use crate::{
    Align, BlockKind, Length, Mark, RichDocument, RichEditorOptions, RichEditorResponse, Theme,
};
use std::rc::Rc;

/// The mark buttons of the toolbar.
const MARKS: [(&str, &str, Mark); 5] = [
    ("B", "Bold (Ctrl+B)", Mark::Bold),
    ("I", "Italic (Ctrl+I)", Mark::Italic),
    ("U", "Underline (Ctrl+U)", Mark::Underline),
    ("S", "Strikethrough (Ctrl+Shift+X)", Mark::Strikethrough),
    ("<>", "Inline code (Ctrl+E)", Mark::Code),
];

/// The block-kind buttons of the toolbar.
const BLOCKS: [(&str, &str, BlockKind); 7] = [
    ("H1", "Heading 1", BlockKind::Heading(1)),
    ("H2", "Heading 2", BlockKind::Heading(2)),
    ("H3", "Heading 3", BlockKind::Heading(3)),
    ("•", "Bulleted list", BlockKind::Bullet),
    ("1.", "Numbered list", BlockKind::Numbered),
    ("”", "Quote", BlockKind::Quote),
    ("{ }", "Code block", BlockKind::Code),
];

fn toolbar_row() -> Style {
    Style::default().gap(4.0).align_items(Align::Center)
}

fn toolbar_style(theme: &Theme, active: bool) -> Style {
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
}

impl<'a, M: 'static> Ui<'a, M> {
    /// A WYSIWYG editor over an app-owned `RichDocument`: wrapped paragraphs,
    /// headings, lists, quotes and code blocks with bold, italic, underline,
    /// strikethrough, inline code and links. Each edit, mark or block change
    /// arrives as one `RichOp` through `on`, in order, and `update` applies it
    /// with `RichDocument::apply_with`, which also puts a copy or cut on the
    /// clipboard. Ctrl+B, Ctrl+I and Ctrl+U toggle marks; the optional toolbar
    /// sends its buttons' ops the same way, and hands the focus back to the
    /// text after each press.
    #[track_caller]
    pub fn rich_editor_msg(
        &mut self,
        document: &RichDocument,
        options: RichEditorOptions,
        on: impl Fn(RichOp) -> M + 'static,
    ) -> RichEditorResponse {
        let mut outer = options.style.clone();
        outer.padding = 0.0;
        outer.radius = 0.0;
        outer.background = None;
        outer.gap = 6.0;
        let on: Rc<dyn Fn(RichOp) -> M> = Rc::new(on);
        let read_only = options.read_only;
        let key = self.next_key();
        self.group(key, Kind::CodeEditor, outer, |ui| {
            if options.toolbar && !read_only {
                ui.rich_toolbar_msg(document, &*on);
            }
            ui.rich_editor_declare(document, options, Some(rich_handler(on, read_only)));
        });
        RichEditorResponse {
            link_at_cursor: document.marks().link,
            ..RichEditorResponse::default()
        }
    }

    /// Declares the editor's node with what the document shows now, and the
    /// handler its edits go to, if any.
    #[track_caller]
    fn rich_editor_declare(
        &mut self,
        document: &RichDocument,
        options: RichEditorOptions,
        handler: Option<Handler<M>>,
    ) {
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
            handler: None,
            seq: 0,
            key: WidgetKey::Named("$area".into()),
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
        if let Some(handler) = handler {
            self.set_handler(handler);
        }
    }

    /// The toolbar of [`Ui::rich_editor_msg`]: each button sends its op.
    #[track_caller]
    fn rich_toolbar_msg(&mut self, document: &RichDocument, on: &dyn Fn(RichOp) -> M) {
        let theme = self.theme().clone();
        let marks = document.marks();
        let kind = document.block_kind();
        self.row_at("$toolbar", toolbar_row(), |ui| {
            for (label, tip, mark) in MARKS {
                ui.button_at(
                    format!("$mark-{label}"),
                    label,
                    true,
                    toolbar_style(&theme, marks.has(mark)),
                );
                ui.set_handler(Handler::Refocus(on(RichOp::Toggle(mark))));
                ui.tooltip(tip);
            }
            ui.label_at("$gap", "", Style::default().width(Length::Px(8.0)));
            for (label, tip, block) in BLOCKS {
                ui.button_at(
                    format!("$block-{tip}"),
                    label,
                    true,
                    toolbar_style(&theme, kind == block),
                );
                ui.set_handler(Handler::Refocus(on(RichOp::SetBlock(block))));
                ui.tooltip(tip);
            }
        });
    }
}

/// The handler of an editor that sends its edits. A read-only editor drops
/// the edits and turns a cut into a copy, as the kernel does.
fn rich_handler<M: 'static>(on: Rc<dyn Fn(RichOp) -> M>, read_only: bool) -> Handler<M> {
    Handler::Each(Box::new(move |update, out| {
        let ControlUpdate::RichOps(ops) = update else {
            return;
        };
        for op in ops {
            let op = match op {
                RichOp::Clipboard { cut } => RichOp::Clipboard {
                    cut: cut && !read_only,
                },
                op if read_only && op.is_edit() => continue,
                op => op,
            };
            out.push(on(op));
        }
    }))
}
