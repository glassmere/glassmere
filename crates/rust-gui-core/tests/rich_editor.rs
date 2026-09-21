use rust_gui_core::{
    Block, BlockKind, Context, EditorCommand, Event, Key, Length, MonospaceMeasurer, RichDocument,
    RichEditorOptions, RichEditorResponse, RichPosition, Role, Style, TextEdit,
};
use rust_gui_scene::{Point, Primitive, Rect, Size};
use std::time::Duration;

const ROOT: Size = Size::new(640.0, 480.0);
const ADVANCE: f32 = 16.0 * 0.6;
const LINE: f32 = 16.0 * 1.4;
const PADDING: f32 = 12.0;
/// 376 px of content hold 39 monospace characters per visual line.
const COLUMNS: usize = 39;

struct App {
    document: RichDocument,
    toolbar: bool,
    read_only: bool,
    last: RichEditorResponse,
}

fn app(document: RichDocument) -> App {
    App {
        document,
        toolbar: false,
        read_only: false,
        last: RichEditorResponse::default(),
    }
}

fn frame(context: &mut Context, app: &mut App) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        app.last = ui.rich_text_editor(
            "editor",
            &mut app.document,
            RichEditorOptions {
                label: "Notes".into(),
                style: Style::default()
                    .width(Length::Px(400.0))
                    .height(Length::Px(300.0))
                    .padding(PADDING),
                toolbar: app.toolbar,
                read_only: app.read_only,
                ..RichEditorOptions::default()
            },
        );
    });
}

fn started(document: RichDocument) -> (Context, App) {
    let mut context = Context::new();
    let mut app = app(document);
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    (context, app)
}

fn at(block: usize, offset: usize) -> RichPosition {
    RichPosition { block, offset }
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

/// A point in the cell at visual `(row, column)` of an editor without a
/// toolbar whose blocks are all single-line paragraphs of body text.
fn cell(row: usize, column: usize) -> Point {
    Point {
        x: PADDING + column as f32 * ADVANCE + 1.0,
        y: PADDING + row as f32 * (LINE + 10.0) + 4.0,
    }
}

fn labelled(context: &Context, role: Role, label: &str) -> Rect {
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

#[test]
fn typing_marks_enter_and_undo_flow_through_queued_operations() {
    let (mut context, mut app) = started(RichDocument::new());
    assert!(!context.accepts_text());
    click(&mut context, cell(0, 0));
    assert!(context.accepts_text() && context.editor_focused());
    insert(&mut context, "Hello ");
    context.event(Event::Editor(EditorCommand::ToggleBold));
    insert(&mut context, "world");
    key(&mut context, Key::Enter, false);
    insert(&mut context, "next");
    assert_eq!(app.document.plain_text(), "");
    frame(&mut context, &mut app);
    assert!(app.last.changed);
    assert_eq!(app.document.plain_text(), "Hello world\nnext");
    let first = &app.document.blocks()[0];
    assert_eq!(first.runs.len(), 2);
    assert!(first.runs[1].marks.bold && first.runs[1].text == "world");
    // Bold carries over the line break until it is toggled off.
    assert!(app.document.blocks()[1].runs[0].marks.bold);

    // The scene has one rich block per paragraph, with a bold span.
    frame(&mut context, &mut app);
    let blocks: Vec<_> = context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::RichText { spans, .. } => Some(spans.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0][1].weight, Some(700));

    // Selecting with the keyboard and toggling italic, then undoing it.
    key(&mut context, Key::ArrowLeft, true);
    key(&mut context, Key::ArrowLeft, true);
    context.event(Event::Editor(EditorCommand::ToggleItalic));
    frame(&mut context, &mut app);
    assert!(app.document.blocks()[1].runs[1].marks.italic);
    context.event(Event::Text(TextEdit::Undo));
    frame(&mut context, &mut app);
    assert_eq!(app.document.blocks()[1].runs.len(), 1);

    let node = context.node(context.focused().unwrap()).unwrap();
    assert_eq!(node.semantics.role, Role::TextInput);
    assert_eq!(node.semantics.label, "Notes");
    assert_eq!(node.value.as_deref(), Some("next"));
}

#[test]
fn vertical_and_line_keys_follow_the_wrapped_lines_with_a_sticky_column() {
    let long = "x".repeat(COLUMNS * 2 + 10);
    let (mut context, mut app) = started(RichDocument::from_blocks(vec![
        Block::new(BlockKind::Paragraph, &long),
        Block::new(BlockKind::Paragraph, "short"),
        Block::new(BlockKind::Paragraph, &long),
    ]));
    click(&mut context, cell(0, 0));
    frame(&mut context, &mut app);
    app.document.set_cursor(at(0, 20), false);
    frame(&mut context, &mut app);
    // Down within the paragraph: same column, next visual line.
    key(&mut context, Key::ArrowDown, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.cursor(), at(0, COLUMNS + 20));
    // End goes to the true end of the wrapped line. The same offset starts the
    // next line; the caret's affinity keeps it on this one, so Home returns here.
    key(&mut context, Key::End, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.cursor(), at(0, COLUMNS * 2));
    let caret = context.ime_cursor_rect().unwrap();
    assert!((caret.y - (PADDING + LINE)).abs() < 0.5, "{caret:?}");
    key(&mut context, Key::Home, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.cursor(), at(0, COLUMNS));
    // Through the short paragraph and back out to column 20.
    app.document.set_cursor(at(0, COLUMNS * 2 + 5), false);
    frame(&mut context, &mut app);
    context.event(Event::Text(TextEdit::Move {
        direction: rust_gui_core::TextMove::Right,
        select: false,
        word: false,
    }));
    frame(&mut context, &mut app);
    for _ in 0..3 {
        key(&mut context, Key::ArrowLeft, false);
    }
    frame(&mut context, &mut app);
    assert_eq!(app.document.cursor(), at(0, COLUMNS * 2 + 3));
    app.document.set_cursor(at(0, COLUMNS + 20), false);
    frame(&mut context, &mut app);
    key(&mut context, Key::ArrowDown, false);
    frame(&mut context, &mut app);
    // The last visual line has ten characters: the column clamps there.
    assert_eq!(app.document.cursor(), at(0, COLUMNS * 2 + 10));
    key(&mut context, Key::ArrowDown, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.cursor(), at(1, 5));
    key(&mut context, Key::ArrowDown, true);
    frame(&mut context, &mut app);
    // The sticky column returns once the line is long enough again.
    assert_eq!(app.document.cursor(), at(2, 20));
    assert_eq!(app.document.anchor(), at(1, 5));
    // Visual moves resolve against the last shaped frame, one step per declaration.
    key(&mut context, Key::ArrowUp, false);
    frame(&mut context, &mut app);
    key(&mut context, Key::ArrowUp, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.cursor().block, 0);
}

#[test]
fn pointer_places_drags_and_multi_clicks_across_blocks() {
    let (mut context, mut app) = started(RichDocument::from_blocks(vec![
        Block::new(BlockKind::Paragraph, "first paragraph"),
        Block::new(BlockKind::Paragraph, "second one"),
    ]));
    click(&mut context, cell(1, 3));
    frame(&mut context, &mut app);
    assert_eq!(app.document.cursor(), at(1, 3));
    context.event(Event::PointerMoved(cell(0, 6)));
    context.event(Event::PointerDown);
    context.event(Event::PointerMoved(cell(1, 6)));
    context.event(Event::PointerUp);
    frame(&mut context, &mut app);
    assert_eq!(app.document.selected_text(), "paragraph\nsecond");
    // Selection quads are painted in both blocks.
    frame(&mut context, &mut app);
    let highlight = context.theme().highlight;
    let selected = context
        .scene()
        .primitives
        .iter()
        .filter(
            |primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == highlight),
        )
        .count();
    assert_eq!(selected, 9 + 6);

    context.set_now(Duration::from_secs(3));
    click(&mut context, cell(0, 8));
    context.set_now(Duration::from_millis(3100));
    click(&mut context, cell(0, 8));
    frame(&mut context, &mut app);
    assert_eq!(app.document.selected_text(), "paragraph");
    context.set_now(Duration::from_millis(3200));
    click(&mut context, cell(0, 8));
    frame(&mut context, &mut app);
    assert_eq!(app.document.selected_text(), "first paragraph");
}

#[test]
fn the_toolbar_formats_shows_state_and_returns_focus_to_the_text() {
    let (mut context, mut app) = started(RichDocument::from_blocks(vec![Block::new(
        BlockKind::Paragraph,
        "make me bold",
    )]));
    app.toolbar = true;
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    let area = labelled(&context, Role::TextInput, "Notes");
    click(
        &mut context,
        Point {
            x: area.x + PADDING + 1.0,
            y: area.y + PADDING + 4.0,
        },
    );
    frame(&mut context, &mut app);
    let editor = context.focused();
    app.document.select_all();
    frame(&mut context, &mut app);

    let press = |context: &mut Context, app: &mut App, label: &str| {
        let bounds = labelled(context, Role::Button, label);
        click(
            context,
            Point {
                x: bounds.x + bounds.width / 2.0,
                y: bounds.y + bounds.height / 2.0,
            },
        );
        frame(context, app);
        frame(context, app);
    };
    press(&mut context, &mut app, "B");
    assert!(app.document.blocks()[0].runs[0].marks.bold);
    // Focus came back, so typing continues without another click.
    assert_eq!(context.focused(), editor);
    press(&mut context, &mut app, "H2");
    assert_eq!(app.document.block_kind(), BlockKind::Heading(2));
    assert_eq!(context.focused(), editor);
    // Active buttons take the accent colour.
    let accent = context.theme().accent;
    let active = context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == accent))
        .count();
    assert!(active >= 2, "{active}");
    press(&mut context, &mut app, "H2");
    assert_eq!(app.document.block_kind(), BlockKind::Paragraph);
}

#[test]
fn clipboard_read_only_composition_and_scrolling_behave_like_the_code_editor() {
    let blocks: Vec<Block> = (0..60)
        .map(|n| Block::new(BlockKind::Paragraph, &format!("paragraph {n}")))
        .collect();
    let (mut context, mut app) = started(RichDocument::from_blocks(blocks));
    click(&mut context, cell(0, 0));
    key(&mut context, Key::ArrowRight, true);
    key(&mut context, Key::ArrowRight, true);
    assert!(context.request_clipboard(false));
    frame(&mut context, &mut app);
    assert_eq!(context.take_clipboard().as_deref(), Some("pa"));

    // Preedit shows inline and leaves the document alone until the commit.
    context.event(Event::Text(TextEdit::Preedit {
        text: "にほ".into(),
        selection: None,
    }));
    assert!(context.composing_text());
    frame(&mut context, &mut app);
    assert_eq!(app.document.blocks()[0].text(), "paragraph 0");
    context.event(Event::Text(TextEdit::Commit("日本".into())));
    frame(&mut context, &mut app);
    assert_eq!(app.document.blocks()[0].text(), "日本ragraph 0");
    assert!(context.ime_cursor_rect().is_some());

    // Moving to the end scrolls the last paragraph into view.
    context.event(Event::Text(TextEdit::Move {
        direction: rust_gui_core::TextMove::DocumentEnd,
        select: false,
        word: false,
    }));
    frame(&mut context, &mut app);
    frame(&mut context, &mut app);
    let shown = |context: &Context, needle: &str| {
        context.scene().primitives.iter().any(|primitive| {
            matches!(primitive, Primitive::RichText { spans, .. }
                if spans.iter().map(|span| span.text.as_str()).collect::<String>() == needle)
        })
    };
    assert!(shown(&context, "paragraph 59") && !shown(&context, "paragraph 1"));
    let caret = context.ime_cursor_rect().unwrap();
    assert!(caret.y + caret.height <= 300.0 - PADDING + 0.5);

    app.read_only = true;
    frame(&mut context, &mut app);
    assert!(!context.accepts_text());
    context.request_clipboard(true);
    key(&mut context, Key::Backspace, false);
    frame(&mut context, &mut app);
    assert_eq!(app.document.blocks()[59].text(), "paragraph 59");
}
