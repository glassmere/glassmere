use seaglass_core::{HighlightSpan, Highlighter, TextDocument};
use seaglass_syntax::SyntectHighlighter;

fn frame(
    highlighter: &mut SyntectHighlighter,
    document: &TextDocument,
    line: usize,
) -> Vec<HighlightSpan> {
    highlighter.begin(document);
    highlighter.highlight(document, line)
}

/// The colour covering byte `at`.
fn color_at(spans: &[HighlightSpan], at: usize) -> Option<seaglass_scene::Color> {
    spans
        .iter()
        .find(|span| span.range.contains(&at))
        .map(|span| span.color)
}

#[test]
fn keywords_strings_and_comments_get_different_colours_and_state_crosses_lines() {
    let document = TextDocument::from_text(
        "fn main() {\n    let text = \"hi\"; /* open\n    still a comment */ let x = 1;\n}\n",
    );
    let mut highlighter = SyntectHighlighter::new("rs", true);
    assert_eq!(highlighter.language(), "Rust");
    let first = frame(&mut highlighter, &document, 0);
    let keyword = color_at(&first, 0).unwrap();
    let name = color_at(&first, 3).unwrap();
    assert_ne!(keyword, name);
    let second = frame(&mut highlighter, &document, 1);
    let string = color_at(&second, 16).unwrap();
    let comment = color_at(&second, 22).unwrap();
    assert_ne!(string, comment);
    assert_eq!(color_at(&second, 4), Some(keyword));
    // The block comment opened on line 1 still colours the start of line 2.
    let third = frame(&mut highlighter, &document, 2);
    assert_eq!(color_at(&third, 6), Some(comment));
    assert_eq!(color_at(&third, 23), Some(keyword));
    // Spans ascend, never overlap and stay inside the line.
    for spans in [&first, &second, &third] {
        assert!(
            spans
                .windows(2)
                .all(|pair| pair[0].range.end <= pair[1].range.start)
        );
    }
    assert!(third.last().unwrap().range.end <= document.line(2).len());
}

#[test]
fn edits_rehighlight_later_lines_and_undo_restores_them() {
    let mut document = TextDocument::from_text("let a = 1;\nlet b = 2;\nlet c = 3;\n");
    let mut highlighter = SyntectHighlighter::new("Rust", true);
    let before = frame(&mut highlighter, &document, 2);
    document.set_selection(0, 0);
    document.insert("/*");
    let inside = frame(&mut highlighter, &document, 2);
    assert_ne!(before, inside);
    assert_eq!(inside.len(), 1, "one comment span: {inside:?}");
    document.undo();
    assert_eq!(frame(&mut highlighter, &document, 2), before);
}

#[test]
fn far_jumps_catch_up_over_several_declarations_instead_of_stalling_one() {
    let text: String = (0..6000)
        .map(|n| format!("let value_{n} = {n};\n"))
        .collect();
    let document = TextDocument::from_text(&text);
    let mut highlighter = SyntectHighlighter::new("rs", false);
    assert!(frame(&mut highlighter, &document, 5900).is_empty());
    assert!(highlighter.pending());
    let mut declarations = 1;
    let spans = loop {
        declarations += 1;
        let spans = frame(&mut highlighter, &document, 5900);
        if !highlighter.pending() {
            break spans;
        }
        assert!(declarations < 20);
    };
    assert!(declarations >= 3, "{declarations}");
    assert!(!spans.is_empty());
    // Nearby lines now come from checkpoints without pending work.
    assert!(!frame(&mut highlighter, &document, 5800).is_empty());
    assert!(!highlighter.pending());
}

#[test]
fn unknown_languages_are_plain_text_and_themes_are_listed() {
    let document = TextDocument::from_text("just words\n");
    let mut highlighter = SyntectHighlighter::new("no-such-language", true);
    assert_eq!(highlighter.language(), "Plain Text");
    let spans = frame(&mut highlighter, &document, 0);
    assert!(spans.len() <= 1);
    assert!(SyntectHighlighter::themes().contains(&"InspiredGitHub"));
    assert!(SyntectHighlighter::languages().contains(&"JSON"));
    assert!(SyntectHighlighter::with_theme("rs", "missing").is_none());
}
