mod support;

use seaglass_core::app::Action;
use seaglass_core::{
    App, AppDriver, Block, BlockKind, Context, EditorCommand, Effects, Event, Key, Length,
    MonospaceMeasurer, RichDocument, RichEditorOptions, RichOp, RichPosition, Role, Style,
    TextEdit, TextMove, Ui,
};
use seaglass_scene::{Point, Primitive, Size};
use std::time::Duration;
use support::{Driven, click, labelled, on};

const ROOT: Size = Size::new(640.0, 480.0);
const ADVANCE: f32 = 16.0 * 0.6;
const LINE: f32 = 16.0 * 1.4;
const PADDING: f32 = 12.0;
/// 376 px of content hold 39 monospace characters per visual line.
const COLUMNS: usize = 39;

struct State {
    document: RichDocument,
    toolbar: bool,
    read_only: bool,
}

fn editor(state: &State, ui: &mut Ui<'_, Action<State>>) {
    ui.keyed("editor", |ui| {
        ui.rich_editor(
            &state.document,
            RichEditorOptions {
                label: "Notes".into(),
                style: Style::default()
                    .width(Length::Px(400.0))
                    .height(Length::Px(300.0))
                    .padding(PADDING),
                toolbar: state.toolbar,
                read_only: state.read_only,
                ..RichEditorOptions::default()
            },
            on(|s: &mut State, op: RichOp| s.document.apply(op)),
        );
    });
}

fn started(document: RichDocument) -> Driven<State> {
    let mut app = Driven::new(
        ROOT,
        State {
            document,
            toolbar: false,
            read_only: false,
        },
        editor,
    );
    app.frame();
    app
}

fn at(block: usize, offset: usize) -> RichPosition {
    RichPosition { block, offset }
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

#[test]
fn typing_marks_enter_and_undo_flow_through_queued_operations() {
    let mut app = started(RichDocument::new());
    assert!(!app.context.accepts_text());
    click(&mut app.context, cell(0, 0));
    assert!(app.context.accepts_text() && app.context.editor_focused());
    insert(&mut app.context, "Hello ");
    app.context.event(Event::Editor(EditorCommand::ToggleBold));
    insert(&mut app.context, "world");
    key(&mut app.context, Key::Enter, false);
    insert(&mut app.context, "next");
    assert_eq!(app.state().document.plain_text(), "");
    let stats = app.frame();
    assert!(stats.messages > 0);
    assert_eq!(app.state().document.plain_text(), "Hello world\nnext");
    let first = &app.state().document.blocks()[0];
    assert_eq!(first.runs.len(), 2);
    assert!(first.runs[1].marks.bold && first.runs[1].text == "world");
    // Bold carries over the line break until it is toggled off.
    assert!(app.state().document.blocks()[1].runs[0].marks.bold);

    // The scene has one rich block per paragraph, with a bold span.
    app.frame();
    let blocks: Vec<_> = app
        .context
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
    key(&mut app.context, Key::ArrowLeft, true);
    key(&mut app.context, Key::ArrowLeft, true);
    app.context
        .event(Event::Editor(EditorCommand::ToggleItalic));
    app.frame();
    assert!(app.state().document.blocks()[1].runs[1].marks.italic);
    app.context.event(Event::Text(TextEdit::Undo));
    app.frame();
    assert_eq!(app.state().document.blocks()[1].runs.len(), 1);

    let node = app.context.node(app.context.focused().unwrap()).unwrap();
    assert_eq!(node.semantics.role, Role::TextInput);
    assert_eq!(node.semantics.label, "Notes");
    assert_eq!(node.value.as_deref(), Some("next"));
}

#[test]
fn vertical_and_line_keys_follow_the_wrapped_lines_with_a_sticky_column() {
    let long = "x".repeat(COLUMNS * 2 + 10);
    let mut app = started(RichDocument::from_blocks(vec![
        Block::new(BlockKind::Paragraph, &long),
        Block::new(BlockKind::Paragraph, "short"),
        Block::new(BlockKind::Paragraph, &long),
    ]));
    click(&mut app.context, cell(0, 0));
    app.frame();
    app.state_mut().document.set_cursor(at(0, 20), false);
    app.frame();
    // Down within the paragraph: same column, next visual line.
    key(&mut app.context, Key::ArrowDown, false);
    app.frame();
    assert_eq!(app.state().document.cursor(), at(0, COLUMNS + 20));
    // End goes to the true end of the wrapped line. The same offset starts the
    // next line; the caret's affinity keeps it on this one, so Home returns here.
    key(&mut app.context, Key::End, false);
    app.frame();
    assert_eq!(app.state().document.cursor(), at(0, COLUMNS * 2));
    let caret = app.context.ime_cursor_rect().unwrap();
    assert!((caret.y - (PADDING + LINE)).abs() < 0.5, "{caret:?}");
    key(&mut app.context, Key::Home, false);
    app.frame();
    assert_eq!(app.state().document.cursor(), at(0, COLUMNS));
    // Through the short paragraph and back out to column 20.
    app.state_mut()
        .document
        .set_cursor(at(0, COLUMNS * 2 + 5), false);
    app.frame();
    app.context.event(Event::Text(TextEdit::Move {
        direction: TextMove::Right,
        select: false,
        word: false,
    }));
    app.frame();
    for _ in 0..3 {
        key(&mut app.context, Key::ArrowLeft, false);
    }
    app.frame();
    assert_eq!(app.state().document.cursor(), at(0, COLUMNS * 2 + 3));
    app.state_mut()
        .document
        .set_cursor(at(0, COLUMNS + 20), false);
    app.frame();
    key(&mut app.context, Key::ArrowDown, false);
    app.frame();
    // The last visual line has ten characters: the column clamps there.
    assert_eq!(app.state().document.cursor(), at(0, COLUMNS * 2 + 10));
    key(&mut app.context, Key::ArrowDown, false);
    app.frame();
    assert_eq!(app.state().document.cursor(), at(1, 5));
    key(&mut app.context, Key::ArrowDown, true);
    app.frame();
    // The sticky column returns once the line is long enough again.
    assert_eq!(app.state().document.cursor(), at(2, 20));
    assert_eq!(app.state().document.anchor(), at(1, 5));
    // Visual moves resolve against the last shaped frame, one step per declaration.
    key(&mut app.context, Key::ArrowUp, false);
    app.frame();
    key(&mut app.context, Key::ArrowUp, false);
    app.frame();
    assert_eq!(app.state().document.cursor().block, 0);
}

#[test]
fn pointer_places_drags_and_multi_clicks_across_blocks() {
    let mut app = started(RichDocument::from_blocks(vec![
        Block::new(BlockKind::Paragraph, "first paragraph"),
        Block::new(BlockKind::Paragraph, "second one"),
    ]));
    click(&mut app.context, cell(1, 3));
    app.frame();
    assert_eq!(app.state().document.cursor(), at(1, 3));
    app.context.event(Event::PointerMoved(cell(0, 6)));
    app.context.event(Event::PointerDown);
    app.context.event(Event::PointerMoved(cell(1, 6)));
    app.context.event(Event::PointerUp);
    app.frame();
    assert_eq!(app.state().document.selected_text(), "paragraph\nsecond");
    // Selection quads are painted in both blocks.
    app.frame();
    let highlight = app.context.theme().highlight;
    let selected = app
        .context
        .scene()
        .primitives
        .iter()
        .filter(
            |primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == highlight),
        )
        .count();
    assert_eq!(selected, 9 + 6);

    app.context.set_now(Duration::from_secs(3));
    click(&mut app.context, cell(0, 8));
    app.context.set_now(Duration::from_millis(3100));
    click(&mut app.context, cell(0, 8));
    app.frame();
    assert_eq!(app.state().document.selected_text(), "paragraph");
    app.context.set_now(Duration::from_millis(3200));
    click(&mut app.context, cell(0, 8));
    app.frame();
    assert_eq!(app.state().document.selected_text(), "first paragraph");
}

#[test]
fn the_toolbar_formats_shows_state_and_returns_focus_to_the_text() {
    let mut app = started(RichDocument::from_blocks(vec![Block::new(
        BlockKind::Paragraph,
        "make me bold",
    )]));
    app.state_mut().toolbar = true;
    app.frame();
    let area = labelled(&app.context, Role::TextInput, "Notes");
    click(
        &mut app.context,
        Point {
            x: area.x + PADDING + 1.0,
            y: area.y + PADDING + 4.0,
        },
    );
    app.frame();
    let editor = app.context.focused();
    app.state_mut().document.select_all();
    app.frame();

    let press = |app: &mut Driven<State>, label: &str| {
        let bounds = labelled(&app.context, Role::Button, label);
        click(
            &mut app.context,
            Point {
                x: bounds.x + bounds.width / 2.0,
                y: bounds.y + bounds.height / 2.0,
            },
        );
        app.frame();
        app.frame();
    };
    press(&mut app, "B");
    assert!(app.state().document.blocks()[0].runs[0].marks.bold);
    // Focus came back, so typing continues without another click.
    assert_eq!(app.context.focused(), editor);
    press(&mut app, "H2");
    assert_eq!(app.state().document.block_kind(), BlockKind::Heading(2));
    assert_eq!(app.context.focused(), editor);
    // Active buttons take the accent colour.
    let accent = app.context.theme().accent;
    let active = app
        .context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Quad { color, .. } if *color == accent))
        .count();
    assert!(active >= 2, "{active}");
    press(&mut app, "H2");
    assert_eq!(app.state().document.block_kind(), BlockKind::Paragraph);
}

/// A real `App`: `update` applies each op with `RichDocument::apply_with`,
/// which needs the `Effects` of the batch for the clipboard.
struct Notes {
    document: RichDocument,
    read_only: bool,
}

impl App for Notes {
    type Message = RichOp;

    fn update(&mut self, op: RichOp, effects: &mut Effects<'_, RichOp>) {
        self.document.apply_with(op, effects);
    }

    fn view(&self, ui: &mut Ui<'_, RichOp>) {
        ui.keyed("editor", |ui| {
            ui.rich_editor(
                &self.document,
                RichEditorOptions {
                    label: "Notes".into(),
                    style: Style::default()
                        .width(Length::Px(400.0))
                        .height(Length::Px(300.0))
                        .padding(PADDING),
                    toolbar: false,
                    read_only: self.read_only,
                    ..RichEditorOptions::default()
                },
                |op| op,
            )
        });
    }
}

fn frame(driver: &mut AppDriver<RichOp>, notes: &mut Notes, context: &mut Context) {
    driver.frame(notes, context, ROOT, &mut MonospaceMeasurer);
}

#[test]
fn clipboard_read_only_composition_and_scrolling_behave_like_the_code_editor() {
    let blocks: Vec<Block> = (0..60)
        .map(|n| Block::new(BlockKind::Paragraph, &format!("paragraph {n}")))
        .collect();
    let mut notes = Notes {
        document: RichDocument::from_blocks(blocks),
        read_only: false,
    };
    let mut driver = AppDriver::new(|| true);
    let mut context = Context::new();
    frame(&mut driver, &mut notes, &mut context);
    frame(&mut driver, &mut notes, &mut context);
    click(&mut context, cell(0, 0));
    key(&mut context, Key::ArrowRight, true);
    key(&mut context, Key::ArrowRight, true);
    assert!(context.request_clipboard(false));
    // The batch that carries the request services it.
    frame(&mut driver, &mut notes, &mut context);
    assert_eq!(context.take_clip().unwrap().text, "pa");

    // Preedit shows inline and leaves the document alone until the commit.
    context.event(Event::Text(TextEdit::Preedit {
        text: "にほ".into(),
        selection: None,
    }));
    assert!(context.composing_text());
    frame(&mut driver, &mut notes, &mut context);
    assert_eq!(notes.document.blocks()[0].text(), "paragraph 0");
    context.event(Event::Text(TextEdit::Commit("日本".into())));
    frame(&mut driver, &mut notes, &mut context);
    assert_eq!(notes.document.blocks()[0].text(), "日本ragraph 0");
    assert!(context.ime_cursor_rect().is_some());

    // Moving to the end scrolls the last paragraph into view.
    context.event(Event::Text(TextEdit::Move {
        direction: TextMove::DocumentEnd,
        select: false,
        word: false,
    }));
    frame(&mut driver, &mut notes, &mut context);
    frame(&mut driver, &mut notes, &mut context);
    let shown = |context: &Context, needle: &str| {
        context.scene().primitives.iter().any(|primitive| {
            matches!(primitive, Primitive::RichText { spans, .. }
                if spans.iter().map(|span| span.text.as_str()).collect::<String>() == needle)
        })
    };
    assert!(shown(&context, "paragraph 59") && !shown(&context, "paragraph 1"));
    let caret = context.ime_cursor_rect().unwrap();
    assert!(caret.y + caret.height <= 300.0 - PADDING + 0.5);

    notes.read_only = true;
    driver.invalidate();
    frame(&mut driver, &mut notes, &mut context);
    assert!(!context.accepts_text());
    context.request_clipboard(true);
    key(&mut context, Key::Backspace, false);
    frame(&mut driver, &mut notes, &mut context);
    assert_eq!(notes.document.blocks()[59].text(), "paragraph 59");
}
