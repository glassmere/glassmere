use rust_gui::syntax::SyntectHighlighter;
use rust_gui::{
    AppOptions, Axis, CodeEditorOptions, Length, RichDocument, RichEditorOptions, Size,
    SplitOptions, Style, TextDocument, run,
};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut document = RichDocument::from_markdown(SAMPLE);
    let mut markdown = TextDocument::from_text(&document.to_markdown());
    let mut highlighter: Option<(bool, SyntectHighlighter)> = None;
    let mut share = 0.55f32;
    run(
        AppOptions {
            title: "Rust GUI / Rich text".into(),
            size: Size::new(1100.0, 720.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
            let theme = ui.theme().clone();
            let window = theme.window;
            let dark = u32::from(window.0) + u32::from(window.1) + u32::from(window.2) < 384;
            if highlighter.as_ref().is_none_or(|(was, _)| *was != dark) {
                highlighter = Some((dark, SyntectHighlighter::new("md", dark)));
            }
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
            ui.column("page", page, |ui| {
                let mut changed = false;
                let mut link = None;
                ui.split(
                    "panes",
                    &mut share,
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
                    |ui| {
                        let response = ui.rich_text_editor(
                            "rich",
                            &mut document,
                            RichEditorOptions {
                                label: "Release notes".into(),
                                style: fill(),
                                ..RichEditorOptions::default()
                            },
                        );
                        changed = response.changed;
                        link = response.link_at_cursor;
                    },
                    |ui| {
                        ui.code_editor(
                            "markdown",
                            &mut markdown,
                            CodeEditorOptions {
                                label: "Markdown output".into(),
                                style: fill(),
                                read_only: true,
                                line_numbers: false,
                                highlighter: highlighter
                                    .as_mut()
                                    .map(|(_, highlighter)| highlighter as _),
                                ..CodeEditorOptions::default()
                            },
                        );
                    },
                );
                if changed {
                    markdown.set_text(&document.to_markdown());
                    ui.request_redraw();
                }
                ui.label_styled(
                    "status",
                    format!(
                        "{} blocks  ·  link at cursor: {}",
                        document.blocks().len(),
                        link.as_deref().unwrap_or("none"),
                    ),
                    Style::default()
                        .font_size(13.0)
                        .foreground(theme.text_muted),
                );
            });
        },
    )
}
