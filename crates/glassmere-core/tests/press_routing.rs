//! Where a press goes when text areas and rich editors come and go.
mod support;

use glassmere_core::app::Action;
use glassmere_core::{
    Button, CodeEditorOptions, EditorCommand, Event, Label, Length, RichDocument,
    RichEditorOptions, RichOp, Style, TextDocument, TextOp, Ui,
};
use glassmere_scene::{Point, Size};
use support::{Driven, act, click, on};

const ROOT: Size = Size::new(640.0, 600.0);

fn sized(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Editor,
    Label,
    Absent,
}

struct Page {
    area: Slot,
    second: Slot,
    rich: Slot,
    first: TextDocument,
    other: TextDocument,
    notes: RichDocument,
    clicks: u32,
}

fn page(first: &str, other: &str) -> Page {
    Page {
        area: Slot::Editor,
        second: Slot::Absent,
        rich: Slot::Absent,
        first: TextDocument::from_text(first),
        other: TextDocument::from_text(other),
        notes: RichDocument::from_markdown("alpha beta gamma"),
        clicks: 0,
    }
}

fn area_options() -> CodeEditorOptions<'static> {
    CodeEditorOptions {
        label: "Source".into(),
        style: sized(600.0, 216.0).padding(PADDING),
        font_size: 14.0,
        line_numbers: false,
        ..CodeEditorOptions::default()
    }
}

fn view(state: &Page, ui: &mut Ui<'_, Action<Page>>) {
    match state.area {
        Slot::Absent => {}
        Slot::Label => ui.keyed("area", |ui| {
            ui.label(Label::new("not an editor").style(sized(600.0, 216.0)))
        }),
        Slot::Editor => ui.keyed("area", |ui| {
            ui.code_editor(
                &state.first,
                area_options(),
                on(|s: &mut Page, op: TextOp| s.first.apply(op)),
            )
        }),
    }
    if state.second == Slot::Editor {
        ui.keyed("second", |ui| {
            ui.code_editor(
                &state.other,
                area_options(),
                on(|s: &mut Page, op: TextOp| s.other.apply(op)),
            )
        });
    }
    match state.rich {
        Slot::Absent => {}
        Slot::Label => ui.keyed("rich", |ui| {
            ui.label(Label::new("not an editor").style(sized(600.0, 216.0)))
        }),
        Slot::Editor => ui.keyed("rich", |ui| {
            ui.rich_editor(
                &state.notes,
                RichEditorOptions {
                    label: "Notes".into(),
                    toolbar: false,
                    style: sized(400.0, 300.0).padding(12.0),
                    ..RichEditorOptions::default()
                },
                on(|s: &mut Page, op: RichOp| s.notes.apply(op)),
            );
        }),
    }
    ui.keyed("go", |ui| {
        ui.button(
            Button::new("Go")
                .enabled(true)
                .style(
                    Style::button()
                        .width(Length::Px(600.0))
                        .height(Length::Px(216.0)),
                )
                .on_click(act(|s: &mut Page| s.clicks += 1)),
        )
    });
}

fn started(state: Page) -> Driven<Page> {
    let mut driven = Driven::new(ROOT, state, view);
    driven.frame();
    driven.frame();
    driven
}

const PADDING: f32 = 8.0;
const ADVANCE: f32 = 14.0 * 0.6;
const LINE: f32 = 14.0 * 1.4;

/// A point inside the cell at `column` of the first line of a text area at the
/// top of the page.
fn cell(column: usize) -> Point {
    Point {
        x: PADDING + column as f32 * ADVANCE + 1.0,
        y: PADDING + 4.0,
    }
}

/// The same for a rich editor of 12 px padding and 16 px type.
fn rich_cell(column: usize) -> Point {
    Point {
        x: 12.0 + column as f32 * 16.0 * 0.6 + 1.0,
        y: 12.0 + 4.0,
    }
}

fn caret(document: &TextDocument) -> (usize, usize) {
    document.position(document.primary().cursor)
}

#[test]
fn a_text_area_that_leaves_and_returns_takes_presses_only_while_it_is_there() {
    let mut driven = started(page("hello world\nsecond", ""));
    let at = cell(3);
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(caret(&driven.state().first), (0, 3));

    // Gone, the press falls to the button that took its place.
    driven.state_mut().area = Slot::Absent;
    driven.frame();
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(driven.state().clicks, 1);
    assert_eq!(caret(&driven.state().first), (0, 3));

    // The same key as a label is no editor either.
    driven.state_mut().area = Slot::Label;
    driven.frame();
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(driven.state().clicks, 1);
    assert!(!driven.context.editor_focused());
    assert_eq!(caret(&driven.state().first), (0, 3));

    // And again an editor, which takes the press.
    driven.state_mut().area = Slot::Editor;
    driven.frame();
    let at = cell(6);
    click(&mut driven.context, at);
    driven.frame();
    assert!(driven.context.editor_focused());
    assert_eq!(caret(&driven.state().first), (0, 6));
    assert_eq!(driven.state().clicks, 1);
}

#[test]
fn the_text_area_left_takes_presses_when_another_has_gone() {
    let mut driven = started(page("one", "two words here"));
    driven.state_mut().second = Slot::Editor;
    driven.frame();
    driven.state_mut().area = Slot::Absent;
    driven.frame();
    // The second area now sits where the first was.
    let at = cell(4);
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(caret(&driven.state().other), (0, 4));
    assert_eq!(driven.state().clicks, 0);

    // A press with shift extends, and a double click takes the word.
    let far = cell(8);
    driven.context.event(Event::PointerMoved(far));
    driven.context.event(Event::PointerDownWithShift);
    driven.context.event(Event::PointerUp);
    driven.frame();
    assert_eq!(driven.state().other.selected_text(), "word");
    click(&mut driven.context, at);
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(driven.state().other.selected_text(), "words");
}

#[test]
fn adding_a_cursor_needs_a_text_area_under_the_point() {
    let mut driven = started(page("one two\nthree", ""));
    driven.state_mut().area = Slot::Absent;
    driven.frame();
    let there = cell(0);
    assert!(
        !driven
            .context
            .event(Event::Editor(EditorCommand::AddCursorAt(there)))
    );
    driven.frame();
    assert_eq!(driven.state().clicks, 0);

    driven.state_mut().area = Slot::Editor;
    driven.frame();
    let at = cell(0);
    click(&mut driven.context, at);
    let line = Point {
        x: at.x,
        y: at.y + LINE,
    };
    assert!(
        driven
            .context
            .event(Event::Editor(EditorCommand::AddCursorAt(line)))
    );
    driven.frame();
    assert_eq!(driven.state().first.selections().len(), 2);
}

#[test]
fn a_rich_editor_that_leaves_and_returns_takes_presses_only_while_it_is_there() {
    let mut driven = started(page("", ""));
    driven.state_mut().area = Slot::Absent;
    driven.state_mut().rich = Slot::Editor;
    driven.frame();
    let at = rich_cell(5);
    click(&mut driven.context, at);
    driven.frame();
    assert!(driven.context.editor_focused());
    assert_eq!(driven.state().notes.cursor().offset, 5);

    driven.state_mut().rich = Slot::Absent;
    driven.frame();
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(driven.state().clicks, 1);
    assert_eq!(driven.state().notes.cursor().offset, 5);

    driven.state_mut().rich = Slot::Label;
    driven.frame();
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(driven.state().clicks, 1);

    driven.state_mut().rich = Slot::Editor;
    driven.frame();
    let at = rich_cell(2);
    click(&mut driven.context, at);
    driven.frame();
    assert_eq!(driven.state().notes.cursor().offset, 2);
    assert_eq!(driven.state().clicks, 1);
}
