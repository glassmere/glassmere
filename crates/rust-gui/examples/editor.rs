use rust_gui::syntax::SyntectHighlighter;
use rust_gui::{
    App, AppOptions, CodeEditorOptions, Diagnostic, Effects, FindState, Highlighter, Label, Length,
    RichRun, RichText, Severity, Size, Style, TextDocument, TextInput, TextOp, TextWrap, Ui, run,
};
use std::cell::RefCell;

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

#[derive(Clone)]
enum Msg {
    /// An edit, a caret move or a find bar change in the code editor.
    Edit(TextOp),
    /// A link of the rich text block was followed.
    Link(String),
    Mixed(String),
}

struct Editor {
    document: TextDocument,
    find: FindState,
    diagnostics: Vec<Diagnostic>,
    /// One highlighter per window theme; the view reads the one that fits,
    /// and the editor fills it as it declares.
    dark: RefCell<SyntectHighlighter>,
    light: RefCell<SyntectHighlighter>,
    followed: String,
    mixed: String,
}

impl App for Editor {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Edit(op) => {
                let revision = self.document.revision();
                self.find.apply(&mut self.document, op, effects);
                if self.document.revision() != revision {
                    self.diagnostics = lint(&self.document);
                }
            }
            Msg::Link(target) => self.followed = target,
            Msg::Mixed(text) => self.mixed = text,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        let theme = ui.theme().clone();
        let window = theme.window;
        let dark = u32::from(window.0) + u32::from(window.1) + u32::from(window.2) < 384;
        let highlighter: &RefCell<dyn Highlighter> = if dark { &self.dark } else { &self.light };
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
            ui.rich_text(
                &intro,
                Style::default()
                    .width(Length::Percent(1.0))
                    .wrap(TextWrap::Word),
                Msg::Link,
            );
            // Latin, Hebrew and Arabic in one field: arrows move on screen, and the
            // caret carries a small flag showing the direction of the text it is in.
            ui.text_input(
                TextInput::new("Bidirectional text")
                    .placeholder("")
                    .style(Style {
                        width: Length::Px(420.0),
                        height: Length::Px(40.0),
                        padding: 10.0,
                        ..Style::button()
                    }),
                &self.mixed,
                Msg::Mixed,
            );
            ui.code_editor(
                &self.document,
                CodeEditorOptions {
                    label: "Rust source".into(),
                    style: Style::default()
                        .width(Length::Percent(1.0))
                        .grow(1.0)
                        .shrink(1.0)
                        .padding(8.0)
                        .radius(8.0),
                    highlighter: Some(highlighter),
                    find: Some(&self.find),
                    diagnostics: &self.diagnostics,
                    ..CodeEditorOptions::default()
                },
                Msg::Edit,
            );
            let (line, byte) = self.document.position(self.document.primary().cursor);
            ui.label(
                Label::new(format!(
                    "Ln {}, byte {}  ·  {} cursors  ·  {} problems  ·  {}  ·  last link: {}",
                    line + 1,
                    byte,
                    self.document.selections().len(),
                    self.diagnostics.len(),
                    if self.document.is_modified() {
                        "modified"
                    } else {
                        "saved"
                    },
                    self.followed,
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
    let open = std::env::args().any(|arg| arg == "--open");
    let document = TextDocument::from_text(SAMPLE);
    let mut find = FindState::default();
    if open {
        find.open = true;
        find.query = "counts".into();
        find.refresh(&document);
    }
    let diagnostics = lint(&document);
    run(
        AppOptions {
            title: "Rust GUI / Editor".into(),
            size: Size::new(900.0, 700.0),
            frame_limit: smoke.then_some(3),
            ..AppOptions::default()
        },
        Editor {
            document,
            find,
            diagnostics,
            dark: RefCell::new(SyntectHighlighter::new("rs", true)),
            light: RefCell::new(SyntectHighlighter::new("rs", false)),
            followed: String::from("none"),
            mixed: String::from(
                "name: \u{5e9}\u{5dc}\u{5d5}\u{5dd} \u{5e2}\u{5d5}\u{5dc}\u{5dd} (\u{633}\u{644}\u{627}\u{645}) ok",
            ),
        },
    )?;
    Ok(())
}
