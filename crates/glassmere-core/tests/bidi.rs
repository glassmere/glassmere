mod support;

use glassmere_core::app::Action;
use glassmere_core::{
    Block, BlockKind, CaretMovement, CodeEditorOptions, Context, Direction, Event, Key, Length,
    MonospaceMeasurer, RichDocument, RichEditorOptions, RichOp, RichPosition, Role, Style,
    TextDocument, TextEdit, TextMove, TextOp, Ui,
};

use glassmere_scene::{Affinity, Point, Size, TextMeasurer, TextStyle};
use support::{Driven, click, labelled, on};

const ROOT: Size = Size::new(640.0, 400.0);
/// Latin, a Hebrew word, Latin: `abc אבג def`. The Hebrew run reads right to left.
const MIXED: &str = "abc \u{5d0}\u{5d1}\u{5d2} def";

fn key(context: &mut Context, key: Key, shift: bool) {
    context.event(Event::KeyDown {
        key,
        shift,
        repeat: false,
    });
    context.event(Event::KeyUp(key));
}

#[test]
fn geometry_places_one_offset_on_both_sides_of_a_direction_boundary() {
    let style = TextStyle::default();
    let advance = 16.0 * 0.6;
    let geometry = MonospaceMeasurer.geometry(MIXED, &style, None);
    let slot = |x: f32| (x / advance).round() as usize;
    // Offset 4 sits after the space (left of the Hebrew run) and before alef
    // (the run's right end).
    assert_eq!(slot(geometry.caret_at(4, Affinity::Upstream).x), 4);
    assert_eq!(slot(geometry.caret_at(4, Affinity::Downstream).x), 8);
    // Stepping right visits every screen position once, left to right.
    let mut at = (0, Affinity::Downstream);
    let mut visited = vec![slot(geometry.caret_at(at.0, at.1).x)];
    let mut offsets = vec![at.0];
    while let Some(next) = geometry.visual_step(at.0, at.1, true) {
        at = next;
        visited.push(slot(geometry.caret_at(at.0, at.1).x));
        offsets.push(at.0);
    }
    assert_eq!(visited, (0..=11).collect::<Vec<_>>());
    assert_eq!(offsets, [0, 1, 2, 3, 4, 10, 8, 6, 4, 12, 13, 14]);
    // And back again.
    while let Some(next) = geometry.visual_step(at.0, at.1, false) {
        at = next;
    }
    assert_eq!(at, (0, Affinity::Downstream));
    // A click lands on the side of the character it hit.
    let y = 5.0;
    assert_eq!(
        geometry.hit_affinity(Point {
            x: 3.9 * advance,
            y
        }),
        (4, Affinity::Upstream)
    );
    assert_eq!(
        geometry.hit_affinity(Point {
            x: 7.9 * advance,
            y
        }),
        (4, Affinity::Downstream)
    );
    assert_eq!(
        geometry.mixed_direction(6, Affinity::Downstream),
        Some(true)
    );
    assert_eq!(
        geometry.mixed_direction(1, Affinity::Downstream),
        Some(false)
    );

    // A right-to-left paragraph starts at its right end.
    let rtl = MonospaceMeasurer.geometry("\u{5d0}\u{5d1} ab", &style, None);
    assert!(rtl.lines[0].rtl);
    assert_eq!(slot(rtl.caret_at(0, Affinity::Downstream).x), 5);
    assert_eq!(rtl.line_edge(3, Affinity::Downstream, false).0, 0);
    // Wrapped lines: the shared offset belongs to either line by affinity.
    let wrapped = MonospaceMeasurer.geometry(
        "abcdefgh",
        &TextStyle {
            wrap: glassmere_core::TextWrap::Word,
            ..style.clone()
        },
        Some(advance * 4.0),
    );
    assert_eq!(wrapped.lines.len(), 2);
    assert_eq!(
        wrapped.line_edge(2, Affinity::Downstream, true),
        (4, Affinity::Upstream)
    );
    assert_eq!(wrapped.caret_at(4, Affinity::Upstream).y, 0.0);
    assert!(wrapped.caret_at(4, Affinity::Downstream).y > 0.0);
    assert!(
        MonospaceMeasurer
            .geometry("plain", &style, None)
            .mixed_direction(2, Affinity::Downstream)
            .is_none()
    );
}

struct Field {
    value: String,
}

fn field(state: &Field, ui: &mut Ui<'_, Action<Field>>) {
    ui.keyed("field", |ui| {
        ui.text_input(
            "Field",
            &state.value,
            on(|s: &mut Field, text: String| s.value = text),
        );
    });
}

fn caret_x(context: &Context) -> f32 {
    context.ime_cursor_rect().unwrap().x
}

#[test]
fn text_inputs_move_visually_by_default_and_logically_on_request() {
    let mut app = Driven::new(
        ROOT,
        Field {
            value: MIXED.to_owned(),
        },
        field,
    );
    click(&mut app.context, Point { x: 20.0, y: 20.0 });
    app.context.event(Event::Text(TextEdit::Move {
        direction: TextMove::Start,
        select: false,
        word: false,
    }));
    app.frame();
    let mut positions = vec![caret_x(&app.context)];
    for _ in 0..11 {
        key(&mut app.context, Key::ArrowRight, false);
        app.frame();
        positions.push(caret_x(&app.context));
    }
    // Every press moved the caret one cell to the right.
    assert!(
        positions
            .windows(2)
            .all(|pair| (pair[1] - pair[0] - 9.6).abs() < 0.01),
        "{positions:?}"
    );
    // At the end another Right stays put.
    key(&mut app.context, Key::ArrowRight, false);
    app.frame();
    assert_eq!(caret_x(&app.context), *positions.last().unwrap());

    // Shift+Left moves the caret visually; the selection is the logical range
    // between the anchor and wherever the caret now is in the text.
    for _ in 0..3 {
        key(&mut app.context, Key::ArrowLeft, true);
        app.frame();
    }
    assert_eq!(app.context.selected_text(), Some("def"));
    // The next step enters the Hebrew run at its right end, which is its
    // first letter: everything after that letter is now selected.
    key(&mut app.context, Key::ArrowLeft, true);
    app.frame();
    assert_eq!(app.context.selected_text(), Some("\u{5d1}\u{5d2} def"));

    // Logical movement walks the text order, so the caret jumps on screen.
    app.context.set_caret_movement(CaretMovement::Logical);
    app.context.event(Event::Text(TextEdit::Move {
        direction: TextMove::Start,
        select: false,
        word: false,
    }));
    let mut logical = Vec::new();
    for _ in 0..6 {
        key(&mut app.context, Key::ArrowRight, false);
        app.frame();
        logical.push(caret_x(&app.context));
    }
    assert!(
        logical.windows(2).any(|pair| pair[1] < pair[0]),
        "{logical:?}"
    );

    // Clicking either side of the boundary gives the same offset, drawn where
    // the click was.
    app.context.set_caret_movement(CaretMovement::Visual);
    let origin = positions[0];
    click(
        &mut app.context,
        Point {
            x: origin + 3.9 * 9.6,
            y: 20.0,
        },
    );
    app.frame();
    let left_side = caret_x(&app.context);
    click(
        &mut app.context,
        Point {
            x: origin + 7.9 * 9.6,
            y: 20.0,
        },
    );
    app.frame();
    let right_side = caret_x(&app.context);
    assert!((right_side - left_side - 4.0 * 9.6).abs() < 0.01);
    // Typing Hebrew at the boundary keeps the caret with the typed letter.
    app.context
        .event(Event::Text(TextEdit::Insert("\u{5d3}".into())));
    app.frame();
    app.frame();
    assert_eq!(app.state().value, "abc \u{5d3}\u{5d0}\u{5d1}\u{5d2} def");
}

fn code(document: &TextDocument, ui: &mut Ui<'_, Action<TextDocument>>) {
    ui.keyed("code", |ui| {
        ui.code_editor(
            document,
            CodeEditorOptions {
                line_numbers: false,
                style: Style::default()
                    .width(Length::Px(600.0))
                    .height(Length::Px(200.0))
                    .padding(8.0),
                ..CodeEditorOptions::default()
            },
            on(|s: &mut TextDocument, op: TextOp| s.apply(op)),
        );
    });
}

#[test]
fn the_code_editor_steps_visually_on_mixed_lines_and_leaves_them_at_the_visual_end() {
    let mut app = Driven::new(
        ROOT,
        TextDocument::from_text(&format!("let s = \"{MIXED}\";\nnext")),
        code,
    );
    app.frame();
    click(&mut app.context, Point { x: 9.0, y: 12.0 });
    app.frame();
    let mut positions = vec![caret_x(&app.context)];
    for _ in 0..22 {
        key(&mut app.context, Key::ArrowRight, false);
        app.frame();
        positions.push(caret_x(&app.context));
    }
    // 22 cells, each press one cell to the right, across the Hebrew run.
    assert!(
        positions.windows(2).all(|pair| pair[1] > pair[0]),
        "{positions:?}"
    );
    // One more leaves the line for the start of the next.
    key(&mut app.context, Key::ArrowRight, false);
    app.frame();
    assert_eq!(app.state().position(app.state().primary().cursor), (1, 0));
}

fn rich(document: &RichDocument, ui: &mut Ui<'_, Action<RichDocument>>) {
    ui.keyed("rich", |ui| {
        ui.rich_editor(
            document,
            RichEditorOptions {
                toolbar: false,
                style: Style::default()
                    .width(Length::Px(400.0))
                    .height(Length::Px(200.0))
                    .padding(12.0),
                ..RichEditorOptions::default()
            },
            on(|s: &mut RichDocument, op: RichOp| s.apply(op)),
        );
    });
}

#[test]
fn the_rich_editor_follows_direction_in_blocks_and_flags_the_caret() {
    let mut app = Driven::new(
        ROOT,
        RichDocument::from_blocks(vec![
            Block::new(BlockKind::Paragraph, MIXED),
            Block::new(BlockKind::Paragraph, "\u{5e9}\u{5dc}\u{5d5}\u{5dd} abc"),
        ]),
        rich,
    );
    app.frame();
    click(&mut app.context, Point { x: 13.0, y: 16.0 });
    app.frame();
    let mut positions = vec![caret_x(&app.context)];
    for _ in 0..11 {
        key(&mut app.context, Key::ArrowRight, false);
        app.frame();
        positions.push(caret_x(&app.context));
    }
    assert!(
        positions.windows(2).all(|pair| pair[1] > pair[0]),
        "{positions:?}"
    );
    assert_eq!(
        app.state().cursor(),
        RichPosition {
            block: 0,
            offset: 14
        }
    );

    // In the right-to-left paragraph the text starts at the right: Home goes
    // there, and Left moves forward through the Hebrew word.
    app.state_mut().set_cursor(
        RichPosition {
            block: 1,
            offset: 4,
        },
        false,
    );
    app.frame();
    key(&mut app.context, Key::Home, false);
    app.frame();
    assert_eq!(app.state().cursor().offset, 0);
    let home = caret_x(&app.context);
    key(&mut app.context, Key::ArrowLeft, false);
    app.frame();
    assert_eq!(app.state().cursor().offset, 2);
    assert!(caret_x(&app.context) < home);
    // A flag marks the caret's direction in mixed text: a 4 by 2 quad.
    app.frame();
    let flags = app
        .context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| {
            matches!(primitive, glassmere_scene::Primitive::Quad { rect, .. }
                if rect.width == 4.0 && rect.height == 2.0)
        })
        .count();
    assert_eq!(flags, 1);
}

/// Latin, then Hebrew. In a free paragraph the Latin comes first on the left;
/// forced right to left, the Hebrew word is the one on the left.
const LATIN_FIRST: &str = "abc \u{5d0}\u{5d1}\u{5d2}";

fn forced(direction: Option<Direction>) -> Style {
    Style {
        base_direction: direction,
        ..Style::default()
    }
}

/// Puts the caret at each of `offsets` in turn and clicks where it was
/// drawn. The caret must come back to the same place, and at the offsets in
/// `inside`, which are not at an edge of a run, as the same offset.
fn round_trip<S: 'static>(
    app: &mut Driven<S>,
    offsets: &[usize],
    inside: &[usize],
    set: impl Fn(&mut S, usize),
    get: impl Fn(&S) -> usize,
) -> Vec<f32> {
    let mut xs = Vec::new();
    for &offset in offsets {
        set(app.state_mut(), offset);
        app.frame();
        let caret = app.context.ime_cursor_rect().unwrap();
        xs.push(caret.x);
        click(
            &mut app.context,
            Point {
                x: caret.x + 0.5,
                y: caret.y + caret.height / 2.0,
            },
        );
        app.frame();
        assert!(
            (caret_x(&app.context) - caret.x).abs() < 0.01,
            "offset {offset}"
        );
        if inside.contains(&offset) {
            assert_eq!(get(app.state()), offset);
        }
    }
    xs
}

fn code_with(
    direction: Option<Direction>,
) -> impl Fn(&TextDocument, &mut Ui<'_, Action<TextDocument>>) {
    move |document, ui| {
        ui.keyed("code", |ui| {
            ui.code_editor(
                document,
                CodeEditorOptions {
                    line_numbers: false,
                    style: forced(direction)
                        .width(Length::Px(600.0))
                        .height(Length::Px(200.0))
                        .padding(8.0),
                    ..CodeEditorOptions::default()
                },
                on(|s: &mut TextDocument, op: TextOp| s.apply(op)),
            );
        });
    }
}

#[test]
fn the_code_editor_takes_a_forced_base_direction_for_every_line() {
    let text = format!("{LATIN_FIRST}\nplain");
    let mut positions = Vec::new();
    for direction in [None, Some(Direction::Rtl)] {
        let mut app = Driven::new(ROOT, TextDocument::from_text(&text), code_with(direction));
        app.frame();
        click(&mut app.context, Point { x: 9.0, y: 12.0 });
        app.frame();
        let xs = round_trip(
            &mut app,
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &[1, 2, 5, 6],
            |document, offset| document.set_selection(offset, offset),
            |document| document.primary().cursor,
        );
        positions.push(xs);

        // The runs of text a screen reader gets follow the direction too.
        let editor = app.context.find(&["code", "$area"]).unwrap();
        let lines = app.context.editor_text(editor).unwrap().lines;
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.rtl == direction.is_some()));
        for line in &lines {
            let total: usize = line.characters.iter().map(|c| c.len).sum();
            assert_eq!(total, line.text.len());
        }
    }
    // Free: Latin then Hebrew, left to right. Forced: the first run is on the
    // right and the Hebrew word to the left of it.
    assert!(positions[0][0] < positions[0][6]);
    assert!(positions[1][0] > positions[1][6]);
}

fn rich_with(
    direction: Option<Direction>,
) -> impl Fn(&RichDocument, &mut Ui<'_, Action<RichDocument>>) {
    move |document, ui| {
        ui.keyed("rich", |ui| {
            ui.rich_editor(
                document,
                RichEditorOptions {
                    toolbar: false,
                    style: forced(direction)
                        .width(Length::Px(400.0))
                        .height(Length::Px(200.0))
                        .padding(12.0),
                    ..RichEditorOptions::default()
                },
                on(|s: &mut RichDocument, op: RichOp| s.apply(op)),
            );
        });
    }
}

#[test]
fn the_rich_editor_takes_a_forced_base_direction_for_every_block() {
    let document = || {
        RichDocument::from_blocks(vec![
            Block::new(BlockKind::Paragraph, LATIN_FIRST),
            Block::new(BlockKind::Paragraph, "plain"),
        ])
    };
    let mut positions = Vec::new();
    for direction in [None, Some(Direction::Rtl)] {
        let mut app = Driven::new(ROOT, document(), rich_with(direction));
        app.frame();
        click(&mut app.context, Point { x: 13.0, y: 16.0 });
        app.frame();
        let xs = round_trip(
            &mut app,
            &[0, 1, 2, 3, 4, 6, 8, 10],
            &[1, 2, 6, 8],
            |document, offset| document.set_cursor(RichPosition { block: 0, offset }, false),
            |document| document.cursor().offset,
        );
        positions.push(xs);
        let editor = app.context.find(&["rich", "$area"]).unwrap();
        let lines = app.context.editor_text(editor).unwrap().lines;
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.rtl == direction.is_some()));
        for line in &lines {
            let total: usize = line.characters.iter().map(|c| c.len).sum();
            assert_eq!(total, line.text.len());
        }
        // Blocks shaped before the style changed are shaped again.
        if direction.is_some() {
            app.set_view(rich_with(None));
            app.frame();
            let lines = app.context.editor_text(editor).unwrap().lines;
            assert!(lines.iter().all(|line| !line.rtl));
        }
    }
    assert!(positions[0][0] < positions[0][5]);
    assert!(positions[1][0] > positions[1][5]);
    // Free, the text starts at the left of the 400 px editor; forced, it
    // is aligned to the right end.
    assert!(positions[0].iter().all(|x| *x < 100.0), "{positions:?}");
    assert!(positions[1].iter().all(|x| *x > 300.0), "{positions:?}");
}

fn field_with(direction: Option<Direction>) -> impl Fn(&Field, &mut Ui<'_, Action<Field>>) {
    move |state, ui| {
        ui.column(forced(direction), |ui| {
            ui.keyed("field", |ui| {
                ui.text_input(
                    "Field",
                    &state.value,
                    on(|s: &mut Field, text: String| s.value = text),
                );
            });
        });
    }
}

#[test]
fn a_text_input_takes_a_forced_base_direction_and_aligns_to_it() {
    let mut text_start = Vec::new();
    for direction in [None, Some(Direction::Rtl)] {
        let mut app = Driven::new(
            ROOT,
            Field {
                value: LATIN_FIRST.to_owned(),
            },
            field_with(direction),
        );
        app.frame();
        let field = labelled(&app.context, Role::TextInput, "Field");
        let y = field.y + field.height / 2.0;
        // Clicking along the field, and again where the caret was drawn,
        // leaves the caret where it is.
        let mut carets = Vec::new();
        let mut x = field.x + 4.0;
        while x < field.x + field.width - 4.0 {
            click(&mut app.context, Point { x, y });
            app.frame();
            let caret = caret_x(&app.context);
            click(&mut app.context, Point { x: caret + 0.5, y });
            app.frame();
            assert!((caret_x(&app.context) - caret).abs() < 0.01, "{x}");
            carets.push(caret);
            x += 5.0;
        }
        text_start.push(carets.iter().copied().fold(f32::MAX, f32::min));
    }
    // A right-to-left field puts its text at the right end.
    assert!(text_start[1] > text_start[0] + 50.0, "{text_start:?}");
}

#[test]
fn a_line_that_starts_with_hebrew_can_be_forced_to_read_left_to_right() {
    let mut app = Driven::new(
        ROOT,
        TextDocument::from_text("\u{5d0}\u{5d1}\u{5d2} abc"),
        code_with(Some(Direction::Ltr)),
    );
    app.frame();
    click(&mut app.context, Point { x: 9.0, y: 12.0 });
    app.frame();
    let editor = app.context.find(&["code", "$area"]).unwrap();
    let lines = app.context.editor_text(editor).unwrap().lines;
    assert!(!lines[0].rtl);
    // The start of the paragraph is its left end, though it starts with Hebrew.
    app.state_mut().set_selection(0, 0);
    app.frame();
    let start = caret_x(&app.context);
    app.state_mut().set_selection(7, 7);
    app.frame();
    assert!(caret_x(&app.context) > start);
}
