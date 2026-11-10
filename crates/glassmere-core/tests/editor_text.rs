mod support;

use glassmere_core::app::Action as Message;
use glassmere_core::{
    Action, Block, BlockKind, CodeEditorOptions, Context, EditorPoint, EditorText, Event, Length,
    MonospaceMeasurer, NodeId, RichDocument, RichEditorOptions, RichOp, RichPosition, Role, Style,
    TextDocument, TextInput, TextOp, Ui,
};
use glassmere_scene::Size;
use support::{Driven, on};

const ROOT: Size = Size::new(640.0, 480.0);
/// 376 px of content hold 39 monospace characters per visual line.
const COLUMNS: usize = 39;

/// The editor itself, which a rich editor declares inside a frame.
fn editor_named(context: &Context, label: &str) -> NodeId {
    let mut stack = vec![context.root()];
    while let Some(id) = stack.pop() {
        let node = context.node(id).unwrap();
        if node.semantics.role == Role::MultilineTextInput && node.semantics.label == label {
            return id;
        }
        stack.extend(node.children.iter().rev());
    }
    panic!("no editor labelled {label}");
}

/// The ends of the selection of `text`, which must have one.
fn selection_of(text: &EditorText) -> (EditorPoint, EditorPoint) {
    let selection = text.selection.unwrap();
    (selection.anchor, selection.focus)
}

fn point(paragraph: usize, byte: usize) -> EditorPoint {
    EditorPoint { paragraph, byte }
}

struct Notes {
    document: TextDocument,
}

fn notes(state: &Notes, ui: &mut Ui<'_, Message<Notes>>) {
    ui.keyed("editor", |ui| {
        ui.text_area(
            &state.document,
            CodeEditorOptions {
                label: "Notes".into(),
                style: Style::default()
                    .width(Length::Px(400.0))
                    .height(Length::Px(200.0)),
                ..CodeEditorOptions::default()
            },
            on(|s: &mut Notes, op: TextOp| s.document.apply(op)),
        );
    });
}

#[test]
fn a_text_area_reads_its_lines_and_moves_its_caret_for_assistive_technology() {
    let mut app = Driven::new(
        ROOT,
        Notes {
            document: TextDocument::from_text("one\ntwö\nthree"),
        },
        notes,
    );
    app.frame();
    let editor = app.context.find(&["editor"]).unwrap();
    let node = app.context.node(editor).unwrap();
    assert_eq!(node.semantics.role, Role::MultilineTextInput);
    let text = app.context.editor_text(editor).unwrap();
    let lines: Vec<_> = text.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(lines, ["one\n", "twö\n", "three"]);
    assert_eq!(
        text.lines
            .iter()
            .map(|line| line.paragraph)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    // The line break counts as a character, of width nothing, at the end.
    let second = &text.lines[1];
    assert_eq!(
        second.characters.iter().map(|c| c.len).collect::<Vec<_>>(),
        [1, 1, 2, 1]
    );
    assert_eq!(second.characters[3].width, 0.0);
    assert!(text.lines[1].bounds.y > text.lines[0].bounds.y);
    assert_eq!(second.word_starts, [0]);

    // Select from the middle of the second line into the third.
    assert!(app.context.action(
        editor,
        Action::SetSelection {
            anchor: point(1, 1),
            focus: point(2, 3),
        },
    ));
    app.frame();
    app.frame();
    assert_eq!(app.state().document.selected_text(), "wö\nthr");
    let text = app.context.editor_text(editor).unwrap();
    assert_eq!(selection_of(&text), (point(1, 1), point(2, 3)));

    // A caret is a selection that starts and ends in the same place.
    assert!(app.context.action(
        editor,
        Action::SetSelection {
            anchor: point(0, 2),
            focus: point(0, 2),
        },
    ));
    app.frame();
    app.frame();
    let text = app.context.editor_text(editor).unwrap();
    assert_eq!(selection_of(&text), (point(0, 2), point(0, 2)));
    assert_eq!(app.state().document.selected_text(), "");
}

struct Pages {
    document: RichDocument,
}

fn pages(state: &Pages, ui: &mut Ui<'_, Message<Pages>>) {
    ui.keyed("editor", |ui| {
        ui.rich_editor(
            &state.document,
            RichEditorOptions {
                label: "Pages".into(),
                style: Style::default()
                    .width(Length::Px(400.0))
                    .height(Length::Px(300.0))
                    .padding(12.0),
                toolbar: false,
                ..RichEditorOptions::default()
            },
            on(|s: &mut Pages, op: RichOp| s.document.apply(op)),
        );
    });
}

#[test]
fn a_rich_editor_splits_a_wrapped_block_into_lines_and_selects_across_blocks() {
    let first = "word ".repeat(20);
    let mut app = Driven::new(
        ROOT,
        Pages {
            document: RichDocument::from_blocks(vec![
                Block::new(BlockKind::Paragraph, &first),
                Block::new(BlockKind::Paragraph, "second"),
            ]),
        },
        pages,
    );
    app.frame();
    let editor = editor_named(&app.context, "Pages");
    let node = app.context.node(editor).unwrap();
    assert_eq!(node.semantics.role, Role::MultilineTextInput);
    let text = app.context.editor_text(editor).unwrap();
    let wrapped: Vec<_> = text
        .lines
        .iter()
        .filter(|line| line.paragraph == 0)
        .collect();
    assert!(wrapped.len() >= 3, "{} lines", wrapped.len());
    assert!(wrapped.iter().all(|line| line.text.len() <= COLUMNS + 1));
    // Together the lines are the block, with its break at the end, and each
    // says where in the block it starts.
    let joined: String = wrapped.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(joined, format!("{first}\n"));
    let mut start = 0;
    for line in &wrapped {
        assert_eq!(line.start, start);
        start += line.text.len();
    }
    let last = text.lines.last().unwrap();
    assert_eq!((last.paragraph, last.text.as_str()), (1, "second"));

    assert!(app.context.action(
        editor,
        Action::SetSelection {
            anchor: point(0, 45),
            focus: point(1, 2),
        },
    ));
    app.frame();
    app.frame();
    assert_eq!(
        app.state().document.selection(),
        (
            RichPosition {
                block: 0,
                offset: 45
            },
            RichPosition {
                block: 1,
                offset: 2
            }
        )
    );
    let text = app.context.editor_text(editor).unwrap();
    assert_eq!(selection_of(&text), (point(0, 45), point(1, 2)));
}

#[test]
fn a_password_has_no_text_for_assistive_technology_and_a_field_can_be_selected_in() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("secret", |ui| {
            ui.text_input(TextInput::new("Secret").password(), "hunter2", |_| ())
        });
        ui.keyed("name", |ui| ui.text_input("Name", "héllo", |_| ()));
    };
    context.run(ROOT, &mut MonospaceMeasurer, view);
    context.run(ROOT, &mut MonospaceMeasurer, view);
    let secret = context.find(&["secret"]).unwrap();
    let name = context.find(&["name"]).unwrap();
    assert_eq!(
        context.node(secret).unwrap().semantics.role,
        Role::PasswordInput
    );
    assert!(context.editor_text(secret).is_none());
    assert!(!context.action(
        secret,
        Action::SetSelection {
            anchor: point(0, 1),
            focus: point(0, 3),
        },
    ));

    let text = context.editor_text(name).unwrap();
    assert_eq!(text.lines.len(), 1);
    assert_eq!(text.lines[0].text, "héllo");
    // The caret starts at the end.
    assert_eq!(selection_of(&text), (point(0, 6), point(0, 6)));
    assert!(context.action(
        name,
        Action::SetSelection {
            anchor: point(0, 1),
            focus: point(0, 3),
        },
    ));
    context.run(ROOT, &mut MonospaceMeasurer, view);
    let node = context.node(name).unwrap();
    let selection = node.text_selection.unwrap();
    assert_eq!((selection.anchor, selection.cursor), (1, 3));
    let text = context.editor_text(name).unwrap();
    assert_eq!(selection_of(&text), (point(0, 1), point(0, 3)));
}

/// A document of `count` lines in an editor of ten lines.
fn long_notes(count: usize) -> Driven<Notes> {
    let text: Vec<_> = (0..count).map(|n| format!("line {n}")).collect();
    Driven::new(
        ROOT,
        Notes {
            document: TextDocument::from_text(&text.join("\n")),
        },
        notes,
    )
}

#[test]
fn a_selection_end_outside_the_declared_lines_is_not_reported_or_accepted() {
    let mut app = long_notes(1000);
    app.frame();
    let editor = app.context.find(&["editor"]).unwrap();
    let text = app.context.editor_text(editor).unwrap();
    assert_eq!(selection_of(&text), (point(0, 0), point(0, 0)));

    app.context
        .event(Event::PointerMoved(glassmere_scene::Point {
            x: 100.0,
            y: 100.0,
        }));
    app.context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 20.0 * 100.0,
    });
    app.frame();
    let text = app.context.editor_text(editor).unwrap();
    let first = text.lines[0].paragraph;
    assert!(first > 10, "{first}");
    // The caret is still on the first line, which is not declared: the lines
    // are reported without a selection instead of one at the window's edge.
    assert_eq!(text.selection, None);
    assert!(!app.context.action(
        editor,
        Action::SetSelection {
            anchor: point(0, 0),
            focus: point(first, 2),
        },
    ));
    assert_eq!(app.state().document.selected_text(), "");

    assert!(app.context.action(
        editor,
        Action::SetSelection {
            anchor: point(first, 1),
            focus: point(first + 1, 2),
        },
    ));
    app.frame();
    app.frame();
    assert_eq!(
        app.state().document.selected_text(),
        format!("ine {first}\nli")
    );
}

#[test]
fn a_selection_cannot_be_set_while_text_is_being_composed() {
    let mut app = long_notes(3);
    app.frame();
    let editor = app.context.find(&["editor"]).unwrap();
    assert!(app.context.action(editor, Action::Focus));
    app.context
        .event(Event::Text(glassmere_core::TextEdit::Preedit {
            text: "にほ".into(),
            selection: None,
        }));
    app.frame();
    assert!(app.context.composing_text());
    assert!(!app.context.action(
        editor,
        Action::SetSelection {
            anchor: point(1, 0),
            focus: point(1, 2),
        },
    ));

    let mut app = Driven::new(
        ROOT,
        Pages {
            document: RichDocument::from_blocks(vec![
                Block::new(BlockKind::Paragraph, "first"),
                Block::new(BlockKind::Paragraph, "second"),
            ]),
        },
        pages,
    );
    app.frame();
    let editor = editor_named(&app.context, "Pages");
    assert!(app.context.action(editor, Action::Focus));
    app.context
        .event(Event::Text(glassmere_core::TextEdit::Preedit {
            text: "にほ".into(),
            selection: None,
        }));
    app.frame();
    assert!(app.context.composing_text());
    assert!(!app.context.action(
        editor,
        Action::SetSelection {
            anchor: point(1, 0),
            focus: point(1, 2),
        },
    ));
}

#[test]
fn the_start_of_a_wrapped_line_in_the_middle_of_a_word_is_not_a_word_start() {
    let mut app = Driven::new(
        ROOT,
        Pages {
            document: RichDocument::from_blocks(vec![Block::new(
                BlockKind::Paragraph,
                &"x".repeat(100),
            )]),
        },
        pages,
    );
    app.frame();
    let editor = editor_named(&app.context, "Pages");
    let text = app.context.editor_text(editor).unwrap();
    assert!(text.lines.len() >= 3, "{} lines", text.lines.len());
    assert_eq!(text.lines[0].word_starts, [0]);
    assert!(
        text.lines[1..]
            .iter()
            .all(|line| line.word_starts.is_empty())
    );

    // Where a line starts in the middle of a word, only the words after the
    // rest of it start; the others are as in the whole block.
    let mut app = Driven::new(
        ROOT,
        Pages {
            document: RichDocument::from_blocks(vec![Block::new(
                BlockKind::Paragraph,
                &"word ".repeat(20),
            )]),
        },
        pages,
    );
    app.frame();
    let editor = editor_named(&app.context, "Pages");
    let text = app.context.editor_text(editor).unwrap();
    let starts = |line: &glassmere_core::EditorLine| -> Vec<usize> {
        line.word_starts
            .iter()
            .map(|start| start + line.start)
            .collect()
    };
    assert_eq!(text.lines[0].word_starts[0], 0);
    for line in &text.lines {
        // A word start in the block is a multiple of five.
        assert!(starts(line).iter().all(|start| start % 5 == 0), "{line:?}");
    }
    // The second line starts with the space that ends a word, then a word.
    assert_eq!(text.lines[1].word_starts[0], 1);
}

#[test]
fn an_empty_document_and_an_empty_line_are_lines_of_their_own() {
    let mut app = Driven::new(
        ROOT,
        Notes {
            document: TextDocument::from_text(""),
        },
        notes,
    );
    app.frame();
    let editor = app.context.find(&["editor"]).unwrap();
    let text = app.context.editor_text(editor).unwrap();
    assert_eq!(text.lines.len(), 1);
    assert_eq!(text.lines[0].text, "");
    assert!(text.lines[0].characters.is_empty());
    assert_eq!(selection_of(&text), (point(0, 0), point(0, 0)));

    let mut app = Driven::new(
        ROOT,
        Notes {
            document: TextDocument::from_text("a\n\nb"),
        },
        notes,
    );
    app.frame();
    let text = app.context.editor_text(editor).unwrap();
    let lines: Vec<_> = text.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(lines, ["a\n", "\n", "b"]);
    assert_eq!(text.lines[1].characters.len(), 1);
}

#[test]
fn a_grapheme_longer_than_a_length_can_say_is_split_into_characters() {
    let mut context = Context::new();
    // One cluster of 401 bytes.
    let long = format!("e{}!", "\u{301}".repeat(200));
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("name", |ui| ui.text_input("Name", &long, |_| ()));
    };
    context.run(ROOT, &mut MonospaceMeasurer, view);
    context.run(ROOT, &mut MonospaceMeasurer, view);
    let name = context.find(&["name"]).unwrap();
    let text = context.editor_text(name).unwrap();
    let lengths: Vec<usize> = text.lines[0].characters.iter().map(|c| c.len).collect();
    assert!(lengths.iter().all(|&len| (1..=255).contains(&len)));
    assert_eq!(lengths.iter().sum::<usize>(), long.len());
    assert!(lengths.len() >= 3);
    assert_eq!(*lengths.last().unwrap(), 1);
}
