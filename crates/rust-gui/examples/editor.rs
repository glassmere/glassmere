use rust_gui::syntax::SyntectHighlighter;
use rust_gui::{
    AppOptions, CodeEditorOptions, Diagnostic, FindState, Label, Length, RichRun, RichText,
    Severity, Size, Style, TextDocument, TextInput, TextWrap, run,
};

const SAMPLE: &str = r#"use std::collections::HashMap;

/// Counts words, ignoring case.
// TODO: strip punctuation before counting.
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
        println!("{word}: {count}"); // FIXME: align the columns.
    }
}
"#;

/// A stand-in for a language server: every TODO warns, every FIXME is an error.
fn lint(document: &TextDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (marker, severity, message) in [
        ("TODO", Severity::Warning, "unfinished work"),
        ("FIXME", Severity::Error, "known defect"),
    ] {
        for range in document.find_all(marker, true, true) {
            diagnostics.push(Diagnostic {
                range,
                severity,
                message: format!("{marker}: {message}"),
            });
        }
    }
    diagnostics
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let open = std::env::args().any(|arg| arg == "--open");
    let mut document = TextDocument::from_text(SAMPLE);
    let mut find = FindState::default();
    if open {
        find.open = true;
        find.query = "counts".into();
    }
    let mut diagnostics = lint(&document);
    // Rebuilt when the window theme flips between dark and light.
    let mut highlighter: Option<(bool, SyntectHighlighter)> = None;
    let mut followed = String::from("none");
    // Latin, Hebrew and Arabic in one field: arrows move on screen, and the
    // caret carries a small flag showing the direction of the text it is in.
    let mut mixed = String::from(
        "name: \u{5e9}\u{5dc}\u{5d5}\u{5dd} \u{5e2}\u{5d5}\u{5dc}\u{5dd} (\u{633}\u{644}\u{627}\u{645}) ok",
    );
    run(
        AppOptions {
            title: "Rust GUI / Editor".into(),
            size: Size::new(900.0, 700.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        move |ui| {
            let theme = ui.theme().clone();
            let window = theme.window;
            let dark = u32::from(window.0) + u32::from(window.1) + u32::from(window.2) < 384;
            if highlighter.as_ref().is_none_or(|(was, _)| *was != dark) {
                highlighter = Some((dark, SyntectHighlighter::new("rs", dark)));
            }
            let page = Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0))
                .padding(20.0)
                .gap(12.0);
            ui.column(page, |ui| {
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
                        ". In the editor: Ctrl+F and Ctrl+H search and replace, F3 steps, Ctrl+D \
                         selects the next occurrence, Alt+click and Ctrl+Alt+Up/Down add cursors.",
                    );
                if let Some(target) = ui.rich_text(&intro, Style::default()
                        .width(Length::Percent(1.0))
                        .wrap(TextWrap::Word)) {
                    followed = target;
                }
                ui.text_input(TextInput::new("Bidirectional text").placeholder("").style(Style {
                        width: Length::Px(420.0),
                        height: Length::Px(40.0),
                        padding: 10.0,
                        ..Style::button()
                    }), &mut mixed);
                let response = ui.code_editor(&mut document, CodeEditorOptions {
                        label: "Rust source".into(),
                        style: Style::default()
                            .width(Length::Percent(1.0))
                            .grow(1.0)
                            .shrink(1.0)
                            .padding(8.0)
                            .radius(8.0),
                        highlighter: highlighter
                            .as_mut()
                            .map(|(_, highlighter)| highlighter as _),
                        find: Some(&mut find),
                        diagnostics: &diagnostics,
                        ..CodeEditorOptions::default()
                    });
                if response.changed {
                    diagnostics = lint(&document);
                }
                let (line, byte) = document.position(document.primary().cursor);
                ui.label(Label::new(format!(
                        "Ln {}, byte {}  ·  {} cursors  ·  {} problems  ·  {}  ·  last link: {followed}",
                        line + 1,
                        byte,
                        document.selections().len(),
                        diagnostics.len(),
                        if document.is_modified() {
                            "modified"
                        } else {
                            "saved"
                        },
                    )).style(Style::default()
                        .font_size(13.0)
                        .foreground(theme.text_muted)));
            });
        },
    )
}
