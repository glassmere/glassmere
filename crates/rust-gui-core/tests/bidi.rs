use rust_gui_core::{
    Block, BlockKind, CaretMovement, CodeEditorOptions, Context, Event, Key, Length,
    MonospaceMeasurer, RichDocument, RichEditorOptions, RichPosition, Style, TextDocument,
    TextEdit, TextMove,
};
use rust_gui_scene::{Affinity, Point, Size, TextMeasurer, TextStyle};

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

fn click(context: &mut Context, point: Point) {
    context.event(Event::PointerMoved(point));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
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
            wrap: rust_gui_core::TextWrap::Word,
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

fn field_frame(context: &mut Context, value: &mut String) {
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.text_input("field", "Field", value);
    });
}

fn caret_x(context: &Context) -> f32 {
    context.ime_cursor_rect().unwrap().x
}

#[test]
fn text_inputs_move_visually_by_default_and_logically_on_request() {
    let mut value = MIXED.to_owned();
    let mut context = Context::new();
    field_frame(&mut context, &mut value);
    click(&mut context, Point { x: 20.0, y: 20.0 });
    context.event(Event::Text(TextEdit::Move {
        direction: TextMove::Start,
        select: false,
        word: false,
    }));
    field_frame(&mut context, &mut value);
    let mut positions = vec![caret_x(&context)];
    for _ in 0..11 {
        key(&mut context, Key::ArrowRight, false);
        field_frame(&mut context, &mut value);
        positions.push(caret_x(&context));
    }
    // Every press moved the caret one cell to the right.
    assert!(
        positions
            .windows(2)
            .all(|pair| (pair[1] - pair[0] - 9.6).abs() < 0.01),
        "{positions:?}"
    );
    // At the end another Right stays put.
    key(&mut context, Key::ArrowRight, false);
    field_frame(&mut context, &mut value);
    assert_eq!(caret_x(&context), *positions.last().unwrap());

    // Shift+Left moves the caret visually; the selection is the logical range
    // between the anchor and wherever the caret now is in the text.
    for _ in 0..3 {
        key(&mut context, Key::ArrowLeft, true);
        field_frame(&mut context, &mut value);
    }
    assert_eq!(context.selected_text(), Some("def"));
    // The next step enters the Hebrew run at its right end, which is its
    // first letter: everything after that letter is now selected.
    key(&mut context, Key::ArrowLeft, true);
    field_frame(&mut context, &mut value);
    assert_eq!(context.selected_text(), Some("\u{5d1}\u{5d2} def"));

    // Logical movement walks the text order, so the caret jumps on screen.
    context.set_caret_movement(CaretMovement::Logical);
    context.event(Event::Text(TextEdit::Move {
        direction: TextMove::Start,
        select: false,
        word: false,
    }));
    let mut logical = Vec::new();
    for _ in 0..6 {
        key(&mut context, Key::ArrowRight, false);
        field_frame(&mut context, &mut value);
        logical.push(caret_x(&context));
    }
    assert!(
        logical.windows(2).any(|pair| pair[1] < pair[0]),
        "{logical:?}"
    );

    // Clicking either side of the boundary gives the same offset, drawn where
    // the click was.
    context.set_caret_movement(CaretMovement::Visual);
    let origin = positions[0];
    click(
        &mut context,
        Point {
            x: origin + 3.9 * 9.6,
            y: 20.0,
        },
    );
    field_frame(&mut context, &mut value);
    let left_side = caret_x(&context);
    click(
        &mut context,
        Point {
            x: origin + 7.9 * 9.6,
            y: 20.0,
        },
    );
    field_frame(&mut context, &mut value);
    let right_side = caret_x(&context);
    assert!((right_side - left_side - 4.0 * 9.6).abs() < 0.01);
    // Typing Hebrew at the boundary keeps the caret with the typed letter.
    context.event(Event::Text(TextEdit::Insert("\u{5d3}".into())));
    field_frame(&mut context, &mut value);
    field_frame(&mut context, &mut value);
    assert_eq!(value, "abc \u{5d3}\u{5d0}\u{5d1}\u{5d2} def");
}

#[test]
fn the_code_editor_steps_visually_on_mixed_lines_and_leaves_them_at_the_visual_end() {
    let mut document = TextDocument::from_text(&format!("let s = \"{MIXED}\";\nnext"));
    let mut context = Context::new();
    let frame = |context: &mut Context, document: &mut TextDocument| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            ui.code_editor(
                "code",
                document,
                CodeEditorOptions {
                    line_numbers: false,
                    style: Style::default()
                        .width(Length::Px(600.0))
                        .height(Length::Px(200.0))
                        .padding(8.0),
                    ..CodeEditorOptions::default()
                },
            );
        });
    };
    frame(&mut context, &mut document);
    frame(&mut context, &mut document);
    click(&mut context, Point { x: 9.0, y: 12.0 });
    frame(&mut context, &mut document);
    let mut positions = vec![caret_x(&context)];
    for _ in 0..22 {
        key(&mut context, Key::ArrowRight, false);
        frame(&mut context, &mut document);
        positions.push(caret_x(&context));
    }
    // 22 cells, each press one cell to the right, across the Hebrew run.
    assert!(
        positions.windows(2).all(|pair| pair[1] > pair[0]),
        "{positions:?}"
    );
    // One more leaves the line for the start of the next.
    key(&mut context, Key::ArrowRight, false);
    frame(&mut context, &mut document);
    assert_eq!(document.position(document.primary().cursor), (1, 0));
}

#[test]
fn the_rich_editor_follows_direction_in_blocks_and_flags_the_caret() {
    let mut document = RichDocument::from_blocks(vec![
        Block::new(BlockKind::Paragraph, MIXED),
        Block::new(BlockKind::Paragraph, "\u{5e9}\u{5dc}\u{5d5}\u{5dd} abc"),
    ]);
    let mut context = Context::new();
    let frame = |context: &mut Context, document: &mut RichDocument| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            ui.rich_text_editor(
                "rich",
                document,
                RichEditorOptions {
                    toolbar: false,
                    style: Style::default()
                        .width(Length::Px(400.0))
                        .height(Length::Px(200.0))
                        .padding(12.0),
                    ..RichEditorOptions::default()
                },
            );
        });
    };
    frame(&mut context, &mut document);
    frame(&mut context, &mut document);
    click(&mut context, Point { x: 13.0, y: 16.0 });
    frame(&mut context, &mut document);
    let mut positions = vec![caret_x(&context)];
    for _ in 0..11 {
        key(&mut context, Key::ArrowRight, false);
        frame(&mut context, &mut document);
        positions.push(caret_x(&context));
    }
    assert!(
        positions.windows(2).all(|pair| pair[1] > pair[0]),
        "{positions:?}"
    );
    assert_eq!(
        document.cursor(),
        RichPosition {
            block: 0,
            offset: 14
        }
    );

    // In the right-to-left paragraph the text starts at the right: Home goes
    // there, and Left moves forward through the Hebrew word.
    document.set_cursor(
        RichPosition {
            block: 1,
            offset: 4,
        },
        false,
    );
    frame(&mut context, &mut document);
    key(&mut context, Key::Home, false);
    frame(&mut context, &mut document);
    assert_eq!(document.cursor().offset, 0);
    let home = caret_x(&context);
    key(&mut context, Key::ArrowLeft, false);
    frame(&mut context, &mut document);
    assert_eq!(document.cursor().offset, 2);
    assert!(caret_x(&context) < home);
    // A flag marks the caret's direction in mixed text: a 4 by 2 quad.
    frame(&mut context, &mut document);
    let flags = context
        .scene()
        .primitives
        .iter()
        .filter(|primitive| {
            matches!(primitive, rust_gui_scene::Primitive::Quad { rect, .. }
                if rect.width == 4.0 && rect.height == 2.0)
        })
        .count();
    assert_eq!(flags, 1);
}
