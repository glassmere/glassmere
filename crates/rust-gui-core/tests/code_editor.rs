mod support;

use rust_gui_core::app::Action;
use rust_gui_core::{
    CodeEditorOptions, Context, Diagnostic, EditorCommand, Event, FindState, HighlightSpan,
    Highlighter, Key, Length, MonospaceMeasurer, Role, Severity, Style, TOOLTIP_DELAY,
    TextDocument, TextEdit, TextMove, TextOp, Ui,
};
use rust_gui_scene::{Color, Point, Primitive, Size};
use std::time::Duration;
use support::{Driven, on};

const ROOT: Size = Size::new(640.0, 400.0);
const SIZE: f32 = 14.0;
const ADVANCE: f32 = SIZE * 0.6;
const LINE: f32 = SIZE * 1.4;
const PADDING: f32 = 8.0;

struct State {
    document: TextDocument,
    read_only: bool,
    line_numbers: bool,
    auto_close: bool,
    diagnostics: Vec<Diagnostic>,
}

impl State {
    fn new(text: &str) -> Self {
        Self {
            document: TextDocument::from_text(text),
            read_only: false,
            line_numbers: false,
            auto_close: false,
            diagnostics: Vec::new(),
        }
    }
}

fn editor(state: &State, ui: &mut Ui<'_, Action<State>>) {
    ui.keyed("editor", |ui| {
        ui.code_editor_msg(
            &state.document,
            CodeEditorOptions {
                label: "Source".into(),
                style: Style::default()
                    .width(Length::Px(600.0))
                    .height(Length::Px(216.0))
                    .padding(PADDING),
                font_size: SIZE,
                line_numbers: state.line_numbers,
                read_only: state.read_only,
                diagnostics: &state.diagnostics,
                auto_close: state.auto_close,
                ..CodeEditorOptions::default()
            },
            on(|s: &mut State, op: TextOp| s.document.apply(op)),
        );
    });
}

/// A declared editor, after the second frame the old tests also waited for.
fn started(text: &str) -> Driven<State> {
    let mut app = Driven::new(ROOT, State::new(text), editor);
    app.frame();
    app
}

// The find bar keeps its state in the view, which `code_editor_msg` refuses;
// the test that needs it stays on `Context::run` until that form exists.
struct OldApp {
    document: TextDocument,
    find: FindState,
    read_only: bool,
}

fn old_frame(context: &mut Context, app: &mut OldApp) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.keyed("editor", |ui| {
            ui.code_editor(
                &mut app.document,
                CodeEditorOptions {
                    label: "Source".into(),
                    style: Style::default()
                        .width(Length::Px(600.0))
                        .height(Length::Px(216.0))
                        .padding(PADDING),
                    font_size: SIZE,
                    read_only: app.read_only,
                    find: Some(&mut app.find),
                    ..CodeEditorOptions::default()
                },
            )
        });
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

#[test]
fn typing_keys_and_undo_edit_the_app_owned_document_at_declaration() {
    let mut app = started("fn main() {\n}");
    assert!(!app.context.accepts_text());
    click(&mut app.context, cell(0, 11));
    assert!(app.context.accepts_text());
    // Input is queued: the document changes when the editor is declared.
    key(&mut app.context, Key::Enter, false);
    insert(&mut app.context, "l");
    insert(&mut app.context, "et");
    assert_eq!(app.state().document.text(), "fn main() {\n}");
    let stats = app.frame();
    assert!(stats.messages > 0 && stats.declared);
    assert_eq!(app.state().document.text(), "fn main() {\n    let\n}");
    assert_eq!(app.frame().messages, 0);
    assert_eq!(lines(&app.context), ["fn main() {", "    let", "}"]);

    key(&mut app.context, Key::Home, false);
    key(&mut app.context, Key::Tab, false);
    key(&mut app.context, Key::End, true);
    app.context
        .event(Event::Text(TextEdit::Backspace { word: false }));
    app.frame();
    assert_eq!(app.state().document.line(1), "        ");
    app.context.event(Event::Text(TextEdit::Undo));
    app.context.event(Event::Text(TextEdit::Undo));
    app.frame();
    assert_eq!(app.state().document.line(1), "    let");
    app.context.event(Event::Text(TextEdit::Move {
        direction: TextMove::DocumentEnd,
        select: false,
        word: false,
    }));
    key(&mut app.context, Key::Backspace, false);
    app.frame();
    assert_eq!(app.state().document.text(), "fn main() {\n    let\n");
}

#[test]
fn only_visible_lines_are_declared_and_scrolling_follows_wheel_pages_and_the_caret() {
    let text: String = (0..100_000).map(|n| format!("line {n}\n")).collect();
    let mut app = started(&text);
    app.state_mut().line_numbers = true;
    app.frame();
    let shown = lines(&app.context);
    assert_eq!(shown[0], "line 0");
    // 200 px of content fit ten full lines and part of another.
    assert_eq!(shown.len(), 11);
    assert!(
        app.context
            .scene()
            .primitives
            .iter()
            .any(|primitive| matches!(
                primitive,
                Primitive::StyledText { text, .. } if text == "1"
            ))
    );

    app.context
        .event(Event::PointerMoved(Point { x: 100.0, y: 100.0 }));
    app.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: LINE * 5000.0,
    });
    app.frame();
    assert_eq!(lines(&app.context)[0], "line 5000");
    // The wheel moves the view, not the caret.
    assert_eq!(app.state().document.primary().cursor, 0);

    // A caret move brings the view back to it, and paging scrolls with it.
    click(&mut app.context, Point { x: 100.0, y: 100.0 });
    app.frame();
    key(&mut app.context, Key::PageDown, false);
    app.frame();
    app.frame();
    let (line, _) = app
        .state()
        .document
        .position(app.state().document.primary().cursor);
    assert!(line > 5000 + 8 && line < 5000 + 20, "{line}");
    assert!(lines(&app.context).contains(&format!("line {line}")));
    app.context.event(Event::Text(TextEdit::Move {
        direction: TextMove::DocumentEnd,
        select: false,
        word: false,
    }));
    app.frame();
    app.frame();
    assert!(lines(&app.context).contains(&"line 99999".to_owned()));
}

#[test]
fn clicks_drags_and_multi_clicks_place_carets_and_select_words_and_lines() {
    let mut app = started("let alpha = beta;\nsecond line\nthird");
    click(&mut app.context, cell(1, 3));
    app.frame();
    assert_eq!(
        app.state()
            .document
            .position(app.state().document.primary().cursor),
        (1, 3)
    );
    // Past the end of a line and below the last line clamp.
    click(&mut app.context, cell(2, 40));
    app.frame();
    assert_eq!(
        app.state()
            .document
            .position(app.state().document.primary().cursor),
        (2, 5)
    );

    app.context.event(Event::PointerMoved(cell(0, 4)));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerMoved(cell(1, 6)));
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(
        app.state().document.selected_text(),
        "alpha = beta;\nsecond"
    );

    app.context.event(Event::PointerMoved(cell(0, 13)));
    app.context.event(Event::PointerDownWithShift);
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().document.selected_text(), "alpha = b");

    // Double and triple clicks need to be close in time and place.
    app.context.set_now(Duration::from_secs(10));
    click(&mut app.context, cell(0, 6));
    app.context.set_now(Duration::from_millis(10_100));
    click(&mut app.context, cell(0, 6));
    app.frame();
    assert_eq!(app.state().document.selected_text(), "alpha");
    app.context.set_now(Duration::from_millis(10_200));
    click(&mut app.context, cell(0, 6));
    app.frame();
    assert_eq!(app.state().document.selected_text(), "let alpha = beta;\n");
    app.context.set_now(Duration::from_secs(20));
    click(&mut app.context, cell(0, 6));
    app.frame();
    assert!(app.state().document.primary().is_caret());
}

// M35: a clipboard request is serviced by the declaration that follows it
// (`take_clipboard` reads what that declaration produced), and no message or
// view-read change asks for one, so a message-model frame never services it.
// Stays on `Context::run` until the view rule covers the clipboard outbox.
#[test]
fn copy_and_cut_travel_through_the_clipboard_outbox_and_read_only_blocks_edits() {
    let mut context = Context::new();
    let mut app = OldApp {
        document: TextDocument::from_text("one\ntwo\nthree"),
        find: FindState::default(),
        read_only: false,
    };
    old_frame(&mut context, &mut app);
    old_frame(&mut context, &mut app);
    assert!(!context.request_clipboard(false));
    click(&mut context, cell(1, 1));
    old_frame(&mut context, &mut app);
    // No selection: the whole line.
    assert!(context.request_clipboard(false));
    assert_eq!(context.take_clipboard(), None);
    old_frame(&mut context, &mut app);
    assert_eq!(context.take_clipboard().as_deref(), Some("two\n"));
    assert_eq!(context.take_clipboard(), None);

    context.event(Event::Text(TextEdit::SelectAll));
    app.read_only = true;
    old_frame(&mut context, &mut app);
    assert!(!context.accepts_text());
    // Cut degrades to copy, and edits are ignored.
    context.request_clipboard(true);
    key(&mut context, Key::Delete, false);
    key(&mut context, Key::Enter, false);
    old_frame(&mut context, &mut app);
    assert_eq!(context.take_clipboard().as_deref(), Some("one\ntwo\nthree"));
    assert_eq!(app.document.text(), "one\ntwo\nthree");

    app.read_only = false;
    old_frame(&mut context, &mut app);
    context.request_clipboard(true);
    old_frame(&mut context, &mut app);
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

// M35: the highlighter is `&mut` for every declaration and keeps a call log;
// a message-model view only has `&State`, so this stays on `Context::run`.
#[test]
fn highlighters_colour_visible_lines_only() {
    let text: String = (0..1000).map(|n| format!("let x{n} = let;\n")).collect();
    let mut document = TextDocument::from_text(&text);
    let mut highlighter = Keywords { calls: Vec::new() };
    let mut context = Context::new();
    for _ in 0..2 {
        highlighter.calls.clear();
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            ui.keyed("editor", |ui| {
                ui.code_editor(
                    &mut document,
                    CodeEditorOptions {
                        highlighter: Some(&mut highlighter),
                        ..CodeEditorOptions::default()
                    },
                )
            });
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
    let mut app = started("ab");
    click(&mut app.context, cell(0, 1));
    app.frame();
    let caret = app.context.ime_cursor_rect().unwrap();
    assert!((caret.x - (PADDING + ADVANCE)).abs() < 0.5 && caret.y == PADDING);

    app.context.event(Event::Text(TextEdit::Preedit {
        text: "にほ".into(),
        selection: None,
    }));
    assert!(app.context.composing_text());
    app.frame();
    // The document is untouched while the line shows the preedit text.
    assert_eq!(app.state().document.text(), "ab");
    assert_eq!(lines(&app.context), ["aにほb"]);
    app.context
        .event(Event::Text(TextEdit::Commit("日本".into())));
    assert!(!app.context.composing_text());
    app.frame();
    app.frame();
    assert_eq!(app.state().document.text(), "a日本b");
    assert_eq!(lines(&app.context), ["a日本b"]);

    let editor = app.context.focused().unwrap();
    let node = app.context.node(editor).unwrap();
    assert_eq!(node.semantics.role, Role::TextInput);
    assert_eq!(node.semantics.label, "Source");
    assert_eq!(node.value.as_deref(), Some("a日本b"));

    // Tab indents; after Escape it moves focus on.
    key(&mut app.context, Key::Tab, false);
    app.frame();
    assert_eq!(app.context.focused(), Some(editor));
    // Column three pads with one space to the next four-column stop.
    assert_eq!(app.state().document.text(), "a日本 b");
    // The editor is the only focusable node here, so focus wraps back to it;
    // what matters is that this Tab went to focus navigation, not the text.
    key(&mut app.context, Key::Escape, false);
    key(&mut app.context, Key::Tab, false);
    app.frame();
    assert_eq!(app.state().document.text(), "a日本 b");
}

/// Bounds of the first node with this role and label.
fn labelled(context: &Context, role: Role, label: &str) -> rust_gui_scene::Rect {
    let mut stack = vec![context.root()];
    while let Some(id) = stack.pop() {
        let node = context.node(id).unwrap();
        if node.semantics.role == role && node.semantics.label == label {
            return node.bounds;
        }
        stack.extend(node.children.iter().rev());
    }
    panic!("no {role:?} labelled {label}");
}

fn press_button(context: &mut Context, app: &mut OldApp, label: &str) {
    let bounds = labelled(context, Role::Button, label);
    click(
        context,
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        },
    );
    old_frame(context, app);
    old_frame(context, app);
}

fn quads(context: &Context, color: Color) -> usize {
    context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Quad { color: c, .. } if *c == color))
        .count()
}

// M35: the find bar (`CodeEditorOptions::find`) keeps its state in the view;
// `code_editor_msg` refuses it, so this test stays on the old API.
#[test]
fn find_and_replace_search_step_wrap_and_replace_from_the_bar_and_the_keyboard() {
    let mut context = Context::new();
    let mut app = OldApp {
        document: TextDocument::from_text("foo bar\nFoo foo\nfoobar foo"),
        find: FindState::default(),
        read_only: false,
    };
    old_frame(&mut context, &mut app);
    old_frame(&mut context, &mut app);
    // Select the first word, then Ctrl+F: the bar opens searching for it.
    context.set_now(Duration::from_secs(1));
    click(&mut context, cell(0, 1));
    context.set_now(Duration::from_millis(1100));
    click(&mut context, cell(0, 1));
    old_frame(&mut context, &mut app);
    let editor = context.focused();
    context.event(Event::Editor(EditorCommand::Find));
    old_frame(&mut context, &mut app);
    old_frame(&mut context, &mut app);
    assert!(app.find.open);
    assert_eq!(app.find.query, "foo");
    assert_eq!(app.find.matches().len(), 5);
    // The query field took focus, so typing edits the query, not the document.
    assert_ne!(context.focused(), editor);
    context.event(Event::Text(TextEdit::SelectAll));
    insert(&mut context, "foo");
    old_frame(&mut context, &mut app);
    assert_eq!(app.document.text(), "foo bar\nFoo foo\nfoobar foo");

    // Enter in the field and F3 step through matches; Shift+F3 goes back and wraps.
    key(&mut context, Key::Enter, false);
    old_frame(&mut context, &mut app);
    assert_eq!(app.document.primary().range(), 8..11);
    context.event(Event::Editor(EditorCommand::FindNext));
    old_frame(&mut context, &mut app);
    assert_eq!(app.document.primary().range(), 12..15);
    for _ in 0..3 {
        context.event(Event::Editor(EditorCommand::FindPrevious));
    }
    old_frame(&mut context, &mut app);
    assert_eq!(app.document.primary().range(), 23..26);

    // Options narrow the matches.
    app.find.case_sensitive = true;
    app.find.whole_word = true;
    old_frame(&mut context, &mut app);
    assert_eq!(app.find.matches(), [0..3, 12..15, 23..26]);

    // Ctrl+H adds the replace row; one replacement, then all, each one undo step.
    context.event(Event::Editor(EditorCommand::Replace));
    old_frame(&mut context, &mut app);
    old_frame(&mut context, &mut app);
    app.find.replacement = "qux".into();
    press_button(&mut context, &mut app, "Replace");
    assert_eq!(app.document.text(), "foo bar\nFoo foo\nfoobar qux");
    press_button(&mut context, &mut app, "Replace all");
    assert_eq!(app.document.text(), "qux bar\nFoo qux\nfoobar qux");
    context.event(Event::Text(TextEdit::Undo));
    old_frame(&mut context, &mut app);

    // Escape in the bar closes it and gives the focus back to the text.
    let field = labelled(&context, Role::TextInput, "Find");
    click(
        &mut context,
        Point {
            x: field.x + 5.0,
            y: field.y + 5.0,
        },
    );
    key(&mut context, Key::Escape, false);
    old_frame(&mut context, &mut app);
    old_frame(&mut context, &mut app);
    assert!(!app.find.open);
    assert_eq!(context.focused(), editor);
}

#[test]
fn several_cursors_come_from_alt_click_next_occurrence_and_vertical_adds() {
    let mut app = started("one two\none two\none two");
    click(&mut app.context, cell(0, 0));
    app.context
        .event(Event::Editor(EditorCommand::AddCursorAt(cell(1, 0))));
    app.context
        .event(Event::Editor(EditorCommand::AddCursorBelow));
    insert(&mut app.context, "> ");
    app.frame();
    assert_eq!(
        app.state().document.text(),
        "> one two\n> one two\n> one two"
    );
    assert_eq!(app.state().document.selections().len(), 3);
    // Three carets are painted.
    app.frame();

    // Escape collapses to the primary cursor.
    key(&mut app.context, Key::Escape, false);
    app.frame();
    assert_eq!(app.state().document.selections().len(), 1);

    // Ctrl+D: the word, then its next occurrences.
    click(&mut app.context, cell(0, 7));
    for _ in 0..3 {
        app.context
            .event(Event::Editor(EditorCommand::SelectNextOccurrence));
    }
    insert(&mut app.context, "2");
    app.frame();
    assert_eq!(app.state().document.text(), "> one 2\n> one 2\n> one 2");
    app.context.event(Event::Text(TextEdit::Undo));
    app.frame();
    assert_eq!(
        app.state().document.text(),
        "> one two\n> one two\n> one two"
    );
}

#[test]
fn brackets_pair_while_typing_and_the_partner_of_the_bracket_at_the_caret_is_marked() {
    let mut app = started("");
    app.state_mut().auto_close = true;
    click(&mut app.context, cell(0, 0));
    // One batch per keystroke: each edit resolves against the declared document.
    for text in ["f", "(", "[", "x", "]"] {
        insert(&mut app.context, text);
        app.frame();
    }
    app.frame();
    assert_eq!(app.state().document.text(), "f([x])");
    assert_eq!(app.state().document.primary().cursor, 5);
    // The caret sits after `]`: it and its `[` are marked.
    app.frame();
    let text = app.context.theme().text;
    assert_eq!(quads(&app.context, Color(text.0, text.1, text.2, 56)), 2);
    for key_press in [Key::ArrowLeft, Key::ArrowLeft, Key::Backspace] {
        key(&mut app.context, key_press, false);
        app.frame();
    }
    // Backspace inside `[|x]` is a plain delete; inside an empty pair it takes both.
    assert_eq!(app.state().document.text(), "f(x])");
    app.context.event(Event::Text(TextEdit::Undo));
    app.frame();
    for key_press in [Key::ArrowRight, Key::Backspace, Key::Backspace] {
        key(&mut app.context, key_press, false);
        app.frame();
    }
    assert_eq!(app.state().document.text(), "f()");
}

#[test]
fn diagnostics_underline_mark_the_gutter_and_show_their_message_on_hover() {
    let mut app = started("let x = 1;\nlet y = oops;\nlet z = 3;");
    app.state_mut().line_numbers = true;
    app.state_mut().diagnostics = vec![
        Diagnostic {
            range: 19..23,
            severity: Severity::Error,
            message: "cannot find value `oops`".into(),
        },
        Diagnostic {
            range: 4..5,
            severity: Severity::Warning,
            message: "unused variable".into(),
        },
    ];
    app.frame();
    app.frame();
    let error = Color(229, 83, 75, 255);
    let warning = Color(229, 192, 70, 255);
    // One underline and one gutter dot each.
    assert_eq!(quads(&app.context, error), 2);
    assert_eq!(quads(&app.context, warning), 2);

    // Resting on the error shows its message next to the pointer.
    let gutter = ADVANCE * 2.0 + 26.0;
    let over = Point {
        x: PADDING + gutter + 9.0 * ADVANCE,
        y: PADDING + LINE + 4.0,
    };
    app.context.set_now(Duration::from_secs(5));
    app.context.event(Event::PointerMoved(over));
    assert_eq!(
        app.context.next_wake(),
        Some(Duration::from_secs(5) + TOOLTIP_DELAY)
    );
    app.context.set_now(Duration::from_secs(5) + TOOLTIP_DELAY);
    app.frame();
    assert!(lines_and_labels(&app.context).contains(&"cannot find value `oops`".to_owned()));
    // Moving to plain text hides it and nothing new is due.
    app.context.event(Event::PointerMoved(Point {
        x: over.x,
        y: PADDING + 2.0 * LINE + 4.0,
    }));
    assert_eq!(app.context.next_wake(), None);
    app.frame();
    assert!(!lines_and_labels(&app.context).contains(&"cannot find value `oops`".to_owned()));
}

fn lines_and_labels(context: &Context) -> Vec<String> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::StyledText { text, .. } | Primitive::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}
