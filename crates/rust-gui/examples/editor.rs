use rust_gui::{
    AppOptions, CodeEditorOptions, Color, HighlightSpan, Highlighter, Length, RichRun, RichText,
    Size, Style, TextDocument, TextWrap, run,
};

const SAMPLE: &str = r#"use std::collections::HashMap;

/// Counts words, ignoring case.
fn count_words(text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let text = "the quick brown fox jumps over the lazy dog";
    let mut counts: Vec<_> = count_words(text).into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (word, count) in counts {
        println!("{word}: {count}");
    }
}
"#;

/// A few lines of lexing, enough to show the `Highlighter` contract.
struct Keywords;

impl Highlighter for Keywords {
    fn highlight(&mut self, document: &TextDocument, line: usize) -> Vec<HighlightSpan> {
        let text = document.line(line);
        let span = |range, color, italic| HighlightSpan {
            range,
            color,
            weight: None,
            italic,
        };
        if let Some(start) = text.find("//") {
            return vec![span(start..text.len(), Color(120, 140, 130, 255), true)];
        }
        let mut spans = Vec::new();
        let mut word_start = None;
        let mut string_start = None;
        for (index, ch) in text.char_indices().chain([(text.len(), ' ')]) {
            if let Some(start) = string_start {
                if ch == '"' {
                    spans.push(span(start..index + 1, Color(230, 180, 110, 255), false));
                    string_start = None;
                }
                continue;
            }
            if ch.is_alphanumeric() || ch == '_' {
                word_start.get_or_insert(index);
                continue;
            }
            if let Some(start) = word_start.take()
                && matches!(
                    &text[start..index],
                    "fn" | "let" | "mut" | "for" | "in" | "use" | "struct" | "impl" | "if" | "else"
                )
            {
                spans.push(span(start..index, Color(200, 130, 240, 255), false));
            }
            if ch == '"' {
                string_start = Some(index);
            }
        }
        spans
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let mut document = TextDocument::from_text(SAMPLE);
    let mut highlighter = Keywords;
    let mut followed = String::from("none");
    run(
        AppOptions {
            title: "Rust GUI / Editor".into(),
            size: Size::new(900.0, 700.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
            let theme = ui.theme().clone();
            let page = Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0))
                .padding(20.0)
                .gap(12.0);
            ui.column("page", page, |ui| {
                let intro = RichText::new()
                    .text("A ")
                    .bold("rich text")
                    .text(" block mixes ")
                    .italic("styles")
                    .text(", ")
                    .code("inline code")
                    .text(", ")
                    .run(RichRun::new("decorations").underline())
                    .text(" and links such as ")
                    .link("the milestone report", "docs/milestone-7.md")
                    .text(" or ")
                    .link("the roadmap", "docs/roadmap.md")
                    .text(
                        ". It wraps as one paragraph; Tab to it and use the arrow keys and Enter.",
                    );
                if let Some(target) = ui.rich_text(
                    "intro",
                    &intro,
                    Style::default()
                        .width(Length::Percent(1.0))
                        .wrap(TextWrap::Word),
                ) {
                    followed = target;
                }
                let response = ui.code_editor(
                    "code",
                    &mut document,
                    CodeEditorOptions {
                        label: "Rust source".into(),
                        style: Style::default()
                            .width(Length::Percent(1.0))
                            .grow(1.0)
                            .shrink(1.0)
                            .padding(8.0)
                            .radius(8.0),
                        highlighter: Some(&mut highlighter),
                        ..CodeEditorOptions::default()
                    },
                );
                let _ = response;
                let (line, byte) = document.position(document.primary().cursor);
                ui.label_styled(
                    "status",
                    format!(
                        "Ln {}, byte {}  ·  {} lines  ·  {}  ·  last link: {followed}",
                        line + 1,
                        byte,
                        document.len_lines(),
                        if document.is_modified() {
                            "modified"
                        } else {
                            "saved"
                        },
                    ),
                    Style::default()
                        .font_size(13.0)
                        .foreground(theme.text_muted),
                );
            });
        },
    )
}
