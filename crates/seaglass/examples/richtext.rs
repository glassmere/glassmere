use seaglass::syntax::SyntectHighlighter;
use seaglass::{
    App, AppOptions, Axis, CodeEditorOptions, Effects, Label, Length, RichDocument,
    RichEditorOptions, RichOp, Size, SplitOptions, Style, TextDocument, TextOp, Ui, run,
};
use std::cell::RefCell;

const SAMPLE: &str = "# Release notes

This editor is **WYSIWYG**: *italic*, <u>underline</u>, ~~strikethrough~~, `inline code` \
and [links](https://example.test/docs) are edited in place, and long paragraphs wrap to the \
width of the pane while the caret follows the visual lines.

## Shortcuts

- **Ctrl+B**, **Ctrl+I** and **Ctrl+U** toggle marks for the selection or for what you type next
- Enter on an empty list item leaves the list
- Backspace at the start of a heading turns it back into a paragraph

1. Edit on the left
2. Watch the Markdown on the right

> Documents load from and save to a Markdown subset.

```
let document = RichDocument::from_markdown(text);
std::fs::write(path, document.to_markdown())?;
```
";

#[derive(Clone)]
enum Msg {
    /// An edit, mark or block change made in the rich editor.
    Rich(RichOp),
    /// A caret move, selection or copy in the Markdown view.
    Markdown(TextOp),
    Split(f32),
}

struct RichTextApp {
    document: RichDocument,
    /// The Markdown view, kept in step with `document` by `update`.
    markdown: TextDocument,
    /// The highlighters of the Markdown view, light then dark, one for each
    /// window theme. The view reads the one that fits; the editor fills it
    /// as it declares.
    highlighters: [RefCell<SyntectHighlighter>; 2],
    share: f32,
}

impl App for RichTextApp {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Rich(op) => {
                self.document.apply(op);
                let markdown = self.document.to_markdown();
                if markdown != self.markdown.text() {
                    self.markdown.set_text(&markdown);
                }
            }
            Msg::Markdown(op) => self.markdown.apply(op),
            Msg::Split(share) => self.share = share,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let theme = ui.theme().clone();
        let window = theme.window;
        let dark = u32::from(window.0) + u32::from(window.1) + u32::from(window.2) < 384;
        let page = Style::default()
            .width(Length::Percent(1.0))
            .height(Length::Percent(1.0))
            .padding(16.0)
            .gap(8.0);
        let fill = || {
            Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0))
                .padding(12.0)
                .radius(8.0)
        };
        ui.column(page, |ui| {
            let mut link = None;
            ui.split(
                self.share,
                Axis::Horizontal,
                SplitOptions {
                    min_first: 320.0,
                    min_second: 240.0,
                    style: Style::default()
                        .width(Length::Percent(1.0))
                        .grow(1.0)
                        .shrink(1.0),
                    ..SplitOptions::default()
                },
                Msg::Split,
                |ui| {
                    let response = ui.rich_editor(
                        &self.document,
                        RichEditorOptions {
                            label: "Release notes".into(),
                            style: fill(),
                            ..RichEditorOptions::default()
                        },
                        Msg::Rich,
                    );
                    link = response.link_at_cursor;
                },
                |ui| {
                    ui.code_editor(
                        &self.markdown,
                        CodeEditorOptions {
                            label: "Markdown output".into(),
                            style: fill(),
                            read_only: true,
                            line_numbers: false,
                            highlighter: Some(&self.highlighters[usize::from(dark)]),
                            ..CodeEditorOptions::default()
                        },
                        Msg::Markdown,
                    );
                },
            );
            ui.label(
                Label::new(format!(
                    "{} blocks  ·  link at cursor: {}",
                    self.document.blocks().len(),
                    link.as_deref().unwrap_or("none"),
                ))
                .style(
                    Style::default()
                        .font_size(13.0)
                        .foreground(theme.text_muted),
                ),
            );
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    // `--rtl` mirrors the window as a right-to-left locale would.
    let direction = if std::env::args().any(|arg| arg == "--rtl") {
        seaglass::DirectionChoice::Rtl
    } else {
        seaglass::DirectionChoice::Auto
    };
    let document = RichDocument::from_markdown(SAMPLE);
    let markdown = TextDocument::from_text(&document.to_markdown());
    run(
        AppOptions {
            title: "Seaglass / Rich text".into(),
            size: Size::new(1100.0, 720.0),
            frame_limit: smoke.then_some(3),
            direction,
            ..AppOptions::default()
        },
        RichTextApp {
            document,
            markdown,
            highlighters: [
                RefCell::new(SyntectHighlighter::new("md", false)),
                RefCell::new(SyntectHighlighter::new("md", true)),
            ],
            share: 0.55,
        },
    )?;
    Ok(())
}
