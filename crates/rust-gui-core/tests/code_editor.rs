use rust_gui_core::{
    CodeEditorOptions, CodeEditorResponse, Context, Event, HighlightSpan, Highlighter, Key, Length,
    MonospaceMeasurer, Role, Style, TextDocument, TextEdit, TextMove,
};
use rust_gui_scene::{Color, Point, Primitive, Size};
use std::time::Duration;

const ROOT: Size = Size::new(640.0, 400.0);
const SIZE: f32 = 14.0;
const ADVANCE: f32 = SIZE * 0.6;
const LINE: f32 = SIZE * 1.4;
const PADDING: f32 = 8.0;

struct App {
    document: TextDocument,
    read_only: bool,
    line_numbers: bool,
    last: CodeEditorResponse,
}

fn app(text: &str) -> App {
    App {
        document: TextDocument::from_text(text),
        read_only: false,
        line_numbers: false,
        last: CodeEditorResponse::default(),
    }
}

fn frame(context: &mut Context, app: &mut App) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        app.last = ui.code_editor(
            "editor",
            &mut app.document,
            CodeEditorOptions {
                label: "Source".into(),
                style: Style::default()
                    .width(Length::Px(600.0))
                    .height(Length::Px(216.0))
                    .padding(PADDING),
                font_size: SIZE,
                line_numbers: app.line_numbers,
                read_only: app.read_only,
                highlighter: None,
            },
        );
    });
}

/// A point inside the cell at `(line, column)` of an editor without a gutter
/// that is scrolled to the top.
fn cell(line: usize, column: usize) -> Point {
    Point {
        x: PADDING + column as f32 * ADVANCE + 1.0,
        y: PADDING + line as f32 * LINE + 4.0,
    }
}

fn click(context: &mut Context, point: Point) {
    context.event(Event::PointerMoved(point));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

fn key(context: &mut Context, key: Key, shift: bool) {
    context.event(Event::KeyDown {
        key,
        shift,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

fn insert(context: &mut Context, text: &str) {
    context.event(Event::Text(TextEdit::Insert(text.into())));
}

/// Texts of the rich lines in the scene, top to bottom.
fn lines(context: &Context) -> Vec<String> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::RichText { spans, .. } => {
                Some(spans.iter().map(|span| span.text.as_str()).collect())
            }
            _ => None,
        })
        .collect()
}

fn started(text: &str) -> (Context, App) {
    let mut context = Context::new();
    let mut app = app(text);
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    (context, app)
}

#[test]
fn typing_keys_and_undo_edit_the_app_owned_document_at_declaration() {
    let (mut context, mut app) = started("fn main() {\n}");
    assert!(!context.accepts_text());
    click(&mut context, cell(0, 11));
    assert!(context.accepts_text());
    // Input is queued: the document changes when the editor is declared.
    key(&mut context, Key::Enter, false);
    insert(&mut context, "l");
    insert(&mut context, "et");
    assert_eq!(app.document.text(), "fn main() {\n}");
    frame(&mut context, &mut app);
    assert!(app.last.changed && app.last.selection_changed);
    assert_eq!(app.document.text(), "fn main() {\n    let\n}");
    frame(&mut context, &mut app);
    assert!(!app.last.changed);
    assert_eq!(lines(&context), ["fn main() {", "    let", "}"]);

    key(&mut context, Key::Home, false);
    key(&mut context, Key::Tab, false);
    key(&mut context, Key::End, true);
    context.event(Event::Text(TextEdit::Backspace { word: false }));
    frame(&mut context, &mut app);
    assert_eq!(app.document.line(1), "        ");
    context.event(Event::Text(TextEdit::Undo));
    context.event(Event::Text(TextEdit::Undo));
    frame(&mut context, &mut app);
    assert_eq!(app.document.line(1), "    let");
    context.event(Event::Text(TextEdit::Move {
        direction: TextMove::DocumentEnd,
        select: false,
        word: false,
    }));
    key(&mut context, Key::Backspace, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.text(), "fn main() {\n    let\n");
}

#[test]
fn only_visible_lines_are_declared_and_scrolling_follows_wheel_pages_and_the_caret() {
    let text: String = (0..100_000).map(|n| format!("line {n}\n")).collect();
    let (mut context, mut app) = started(&text);
    app.line_numbers = true;
    frame(&mut context, &mut app);
    let shown = lines(&context);
    assert_eq!(shown[0], "line 0");
    // 200 px of content fit ten full lines and part of another.
    assert_eq!(shown.len(), 11);
    assert!(context.scene().primitives.iter().any(|primitive| matches!(
        primitive,
        Primitive::StyledText { text, .. } if text == "1"
    )));

    context.event(Event::PointerMoved(Point { x: 100.0, y: 100.0 }));
    context.event(Event::Scroll {
        delta_y: LINE * 5000.0,
    });
    frame(&mut context, &mut app);
    assert_eq!(lines(&context)[0], "line 5000");
    // The wheel moves the view, not the caret.
    assert_eq!(app.document.primary().cursor, 0);

    // A caret move brings the view back to it, and paging scrolls with it.
    click(&mut context, Point { x: 100.0, y: 100.0 });
    frame(&mut context, &mut app);
    key(&mut context, Key::PageDown, false);
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    let (line, _) = app.document.position(app.document.primary().cursor);
    assert!(line > 5000 + 8 && line < 5000 + 20, "{line}");
    assert!(lines(&context).contains(&format!("line {line}")));
    context.event(Event::Text(TextEdit::Move {
        direction: TextMove::DocumentEnd,
        select: false,
        word: false,
    }));
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    assert!(lines(&context).contains(&"line 99999".to_owned()));
}

#[test]
fn clicks_drags_and_multi_clicks_place_carets_and_select_words_and_lines() {
    let (mut context, mut app) = started("let alpha = beta;\nsecond line\nthird");
    click(&mut context, cell(1, 3));
    frame(&mut context, &mut app);
    assert_eq!(app.document.position(app.document.primary().cursor), (1, 3));
    // Past the end of a line and below the last line clamp.
    click(&mut context, cell(2, 40));
    frame(&mut context, &mut app);
    assert_eq!(app.document.position(app.document.primary().cursor), (2, 5));

    context.event(Event::PointerMoved(cell(0, 4)));
    context.event(Event::PointerDown);
    context.event(Event::PointerMoved(cell(1, 6)));
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    assert_eq!(app.document.selected_text(), "alpha = beta;\nsecond");

    context.event(Event::PointerMoved(cell(0, 13)));
    context.event(Event::PointerDownWithShift);
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    assert_eq!(app.document.selected_text(), "alpha = b");

    // Double and triple clicks need to be close in time and place.
    context.set_now(Duration::from_secs(10));
    click(&mut context, cell(0, 6));
    context.set_now(Duration::from_millis(10_100));
    click(&mut context, cell(0, 6));
    frame(&mut context, &mut app);
    assert_eq!(app.document.selected_text(), "alpha");
    context.set_now(Duration::from_millis(10_200));
    click(&mut context, cell(0, 6));
    frame(&mut context, &mut app);
    assert_eq!(app.document.selected_text(), "let alpha = beta;\n");
    context.set_now(Duration::from_secs(20));
    click(&mut context, cell(0, 6));
    frame(&mut context, &mut app);
    assert!(app.document.primary().is_caret());
}

#[test]
fn copy_and_cut_travel_through_the_clipboard_outbox_and_read_only_blocks_edits() {
    let (mut context, mut app) = started("one\ntwo\nthree");
    assert!(!context.request_clipboard(false));
    click(&mut context, cell(1, 1));
    frame(&mut context, &mut app);
    // No selection: the whole line.
    assert!(context.request_clipboard(false));
    assert_eq!(context.take_clipboard(), None);
    frame(&mut context, &mut app);
    assert_eq!(context.take_clipboard().as_deref(), Some("two\n"));
    assert_eq!(context.take_clipboard(), None);

    context.event(Event::Text(TextEdit::SelectAll));
    app.read_only = true;
    frame(&mut context, &mut app);
    assert!(!context.accepts_text());
    // Cut degrades to copy, and edits are ignored.
    context.request_clipboard(true);
    key(&mut context, Key::Delete, false);
    key(&mut context, Key::Enter, false);
    frame(&mut context, &mut app);
    assert_eq!(context.take_clipboard().as_deref(), Some("one\ntwo\nthree"));
    assert_eq!(app.document.text(), "one\ntwo\nthree");

    app.read_only = false;
    frame(&mut context, &mut app);
    context.request_clipboard(true);
    frame(&mut context, &mut app);
    assert_eq!(context.take_clipboard().as_deref(), Some("one\ntwo\nthree"));
    assert_eq!(app.document.text(), "");
}

struct Keywords {
    calls: Vec<usize>,
}

impl Highlighter for Keywords {
    fn highlight(&mut self, document: &TextDocument, line: usize) -> Vec<HighlightSpan> {
        self.calls.push(line);
        let text = document.line(line);
        text.match_indices("let")
            .map(|(start, word)| HighlightSpan {
                range: start..start + word.len(),
                color: Color(200, 80, 80, 255),
                weight: Some(700),
                italic: false,
            })
            .collect()
    }
}

#[test]
fn highlighters_colour_visible_lines_only() {
    let text: String = (0..1000).map(|n| format!("let x{n} = let;\n")).collect();
    let mut document = TextDocument::from_text(&text);
    let mut highlighter = Keywords { calls: Vec::new() };
    let mut context = Context::new();
    for _ in 0..2 {
        highlighter.calls.clear();
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            ui.code_editor(
                "editor",
                &mut document,
                CodeEditorOptions {
                    highlighter: Some(&mut highlighter),
                    ..CodeEditorOptions::default()
                },
            );
        });
    }
    assert!(highlighter.calls.len() < 50, "{}", highlighter.calls.len());
    assert!(highlighter.calls.windows(2).all(|pair| pair[0] < pair[1]));
    let Some(Primitive::RichText { spans, .. }) = context
        .scene()
        .primitives
        .iter()
        .find(|primitive| matches!(primitive, Primitive::RichText { .. }))
    else {
        panic!("no rich line");
    };
    let shape: Vec<_> = spans
        .iter()
        .map(|span| (span.text.as_str(), span.color.is_some(), span.weight))
        .collect();
    assert_eq!(
        shape,
        [
            ("let", true, Some(700)),
            (" x0 = ", false, None),
            ("let", true, Some(700)),
            (";", false, None),
        ]
    );
}

#[test]
fn composition_shows_inline_and_escape_then_tab_leaves_the_editor() {
    let (mut context, mut app) = started("ab");
    click(&mut context, cell(0, 1));
    frame(&mut context, &mut app);
    let caret = context.ime_cursor_rect().unwrap();
    assert!((caret.x - (PADDING + ADVANCE)).abs() < 0.5 && caret.y == PADDING);

    context.event(Event::Text(TextEdit::Preedit {
        text: "にほ".into(),
        selection: None,
    }));
    assert!(context.composing_text());
    frame(&mut context, &mut app);
    // The document is untouched while the line shows the preedit text.
    assert_eq!(app.document.text(), "ab");
    assert_eq!(lines(&context), ["aにほb"]);
    context.event(Event::Text(TextEdit::Commit("日本".into())));
    assert!(!context.composing_text());
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    assert_eq!(app.document.text(), "a日本b");
    assert_eq!(lines(&context), ["a日本b"]);

    let editor = context.focused().unwrap();
    let node = context.node(editor).unwrap();
    assert_eq!(node.semantics.role, Role::TextInput);
    assert_eq!(node.semantics.label, "Source");
    assert_eq!(node.value.as_deref(), Some("a日本b"));

    // Tab indents; after Escape it moves focus on.
    key(&mut context, Key::Tab, false);
    frame(&mut context, &mut app);
    assert_eq!(context.focused(), Some(editor));
    // Column three pads with one space to the next four-column stop.
    assert_eq!(app.document.text(), "a日本 b");
    // The editor is the only focusable node here, so focus wraps back to it;
    // what matters is that this Tab went to focus navigation, not the text.
    key(&mut context, Key::Escape, false);
    key(&mut context, Key::Tab, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.text(), "a日本 b");
}
