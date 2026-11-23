//! A frame that does not declare prepares the controls the last declaration
//! left, and what it shows is what a context that had declared the same view
//! shows.
mod support;

use glassmere_core::app::Action as Message;
use glassmere_core::{
    Action, Align, CodeEditorOptions, Command, Context, Direction, Event, Label, Length, Menu,
    MenuEntry, RichText, Shortcut, Style, TextDocument, TextInput, TextOp, TouchPhase, Ui,
};
use glassmere_scene::{Point, Rect, Size};
use std::time::Duration;
use support::{Driven, act, on};

const ROOT: Size = Size::new(500.0, 400.0);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Bare,
    Full,
}

struct Page {
    mode: Mode,
    name: String,
    notes: TextDocument,
}

impl Page {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            name: "Ada".into(),
            notes: TextDocument::from_text("one\ntwo\nthree"),
        }
    }
}

fn baseline_row(ui: &mut Ui<'_, Message<Page>>) {
    ui.keyed("row", |ui| {
        ui.row(
            Style::default().align_items(Align::Baseline).gap(4.0),
            |ui| {
                ui.keyed("small", |ui| {
                    ui.label(Label::new("small").style(Style::default().font_size(12.0)))
                });
                ui.keyed("large", |ui| {
                    ui.label(Label::new("large").style(Style::default().font_size(32.0)))
                });
                ui.keyed("padded", |ui| {
                    ui.label(
                        Label::new("pad").style(Style::default().font_size(16.0).padding(10.0)),
                    )
                });
            },
        )
    });
}

fn page(state: &Page, ui: &mut Ui<'_, Message<Page>>) {
    match state.mode {
        Mode::Bare => {
            ui.keyed("title", |ui| {
                ui.label(Label::new("Bare"));
            });
            ui.keyed("button", |ui| {
                ui.button("Go");
            });
        }
        Mode::Full => {
            baseline_row(ui);
            ui.keyed("name", |ui| {
                ui.text_input(
                    TextInput::new("Name"),
                    &state.name,
                    on(|s: &mut Page, text: String| s.name = text),
                );
            });
            ui.keyed("title", |ui| {
                ui.label(Label::new("Selectable words in a label").selectable());
            });
            ui.keyed("notes", |ui| {
                ui.text_area(
                    &state.notes,
                    CodeEditorOptions {
                        label: "Notes".into(),
                        style: Style::default()
                            .width(Length::Px(300.0))
                            .height(Length::Px(80.0)),
                        ..CodeEditorOptions::default()
                    },
                    on(|s: &mut Page, op: TextOp| s.notes.apply(op)),
                );
            });
            ui.keyed("rich", |ui| {
                ui.rich_text(
                    &RichText::new().text("Some ").bold("rich").text(" words"),
                    Style::default().width(Length::Px(120.0)),
                    |_: String| act(|_: &mut Page| ()),
                );
            });
        }
    }
}

fn driven(mode: Mode) -> Driven<Page> {
    Driven::new(ROOT, Page::new(mode), page)
}

/// Everything a frame shows: the scene and every node, without the ids,
/// which a context that declared other views before has numbered differently.
fn shown(context: &Context) -> String {
    let mut text = format!("{:#?}", context.scene().primitives);
    for node in context.snapshots() {
        text += &format!(
            "\n{:?} {:?} {:?}",
            node.bounds,
            node.semantics,
            node.children.len()
        );
    }
    text
}

/// Runs `count` batches and requires that none of them declares.
fn settle(driven: &mut Driven<Page>, count: usize) {
    for _ in 0..count {
        assert!(!driven.frame().declared, "the frame declared");
    }
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(context.find(path).unwrap()).unwrap().bounds
}

#[test]
fn the_baseline_row_stays_lined_up_on_frames_that_scroll_without_declaring() {
    // A fling scrolls the view in frames of its own, which lays every box out
    // again and so puts back the offsets that baselines move.
    let mut driven = Driven::new(
        ROOT,
        Page::new(Mode::Full),
        |_: &Page, ui: &mut Ui<'_, Message<Page>>| {
            ui.keyed("view", |ui| {
                ui.scroll(
                    Style::default()
                        .width(Length::Px(300.0))
                        .height(Length::Px(120.0)),
                    |ui| {
                        baseline_row(ui);
                        ui.keyed("tail", |ui| {
                            ui.label(
                                Label::new("").style(
                                    Style::default()
                                        .height(Length::Px(600.0))
                                        .width(Length::Px(100.0)),
                                ),
                            )
                        });
                    },
                )
            });
        },
    );
    let lined_up = |driven: &Driven<Page>| {
        let at = |key: &str| bounds(&driven.context, &["view", "row", key]);
        let (small, large, padded) = (at("small"), at("large"), at("padded"));
        // The synthetic measurer puts the baseline at four fifths of the size,
        // and padding pushes it down with the text.
        let line = large.y + 32.0 * 0.8;
        let same = |value: f32| (value - line).abs() < 0.01;
        assert!(same(small.y + 12.0 * 0.8), "{small:?} against {line}");
        assert!(
            same(padded.y + 10.0 + 16.0 * 0.8),
            "{padded:?} against {line}"
        );
        assert!(small.y > large.y && padded.y > large.y);
    };
    lined_up(&driven);
    let touch = |driven: &mut Driven<Page>, phase, position| {
        driven.context.event(Event::Touch {
            id: 1,
            phase,
            position,
            force: None,
            pen: None,
        });
    };
    let tail = bounds(&driven.context, &["view", "tail"]);
    let start = Point {
        x: tail.x + 20.0,
        y: tail.y + 90.0,
    };
    driven.context.set_now(Duration::ZERO);
    touch(&mut driven, TouchPhase::Started, start);
    for step in 1..=6u64 {
        driven.context.set_now(Duration::from_millis(step * 10));
        let moved = Point {
            x: start.x,
            y: start.y - 10.0 * step as f32,
        };
        touch(&mut driven, TouchPhase::Moved, moved);
    }
    touch(
        &mut driven,
        TouchPhase::Ended,
        Point {
            x: start.x,
            y: start.y - 60.0,
        },
    );
    driven.frame();
    lined_up(&driven);
    assert!(driven.context.animating(), "the fling wants frames");
    let mut flung = 0;
    for frame in 1..=40u64 {
        driven
            .context
            .set_now(Duration::from_millis(60 + frame * 16));
        let before = bounds(&driven.context, &["view", "tail"]).y;
        let stats = driven.frame();
        assert!(!stats.declared, "frame {frame} declared");
        lined_up(&driven);
        flung += usize::from(bounds(&driven.context, &["view", "tail"]).y != before);
    }
    assert!(flung > 10, "the view scrolled in {flung} frames");
}

#[test]
fn what_a_declaration_drops_is_not_prepared_by_the_frames_after_it() {
    let mut driven = driven(Mode::Full);
    settle(&mut driven, 2);
    driven.state_mut().mode = Mode::Bare;
    assert!(driven.frame().declared);
    let bare = shown(&driven.context);
    settle(&mut driven, 3);
    assert_eq!(shown(&driven.context), bare);
    assert_eq!(bare, shown(&self::driven(Mode::Bare).context));
}

#[test]
fn what_a_declaration_adds_is_prepared_by_it_and_by_the_frames_after_it() {
    let mut driven = driven(Mode::Bare);
    settle(&mut driven, 2);
    driven.state_mut().mode = Mode::Full;
    assert!(driven.frame().declared);
    let mut fresh = self::driven(Mode::Full);
    assert_eq!(shown(&driven.context), shown(&fresh.context));
    for context in [&mut driven.context, &mut fresh.context] {
        let name = context.find(&["name"]).unwrap();
        assert!(context.action(name, Action::Focus));
    }
    driven.frame();
    fresh.frame();
    settle(&mut driven, 3);
    assert!(driven.context.ime_cursor_rect().is_some());
    assert_eq!(
        driven.context.ime_cursor_rect(),
        fresh.context.ime_cursor_rect()
    );
    assert_eq!(shown(&driven.context), shown(&fresh.context));
}

#[test]
fn a_label_that_stops_being_selectable_is_left_alone_by_the_frames_after_it() {
    let mut driven = driven(Mode::Full);
    settle(&mut driven, 2);
    // Same key and kind with no editor: the node keeps its place in the tree.
    driven.set_view(|_: &Page, ui: &mut Ui<'_, Message<Page>>| {
        ui.keyed("title", |ui| {
            ui.label(Label::new("Selectable words in a label"));
        });
    });
    assert!(driven.frame().declared);
    let plain = shown(&driven.context);
    settle(&mut driven, 3);
    assert_eq!(shown(&driven.context), plain);
}

#[test]
fn the_label_and_hint_widths_are_measured_once_and_not_by_the_frames_after_them() {
    let mut driven = Driven::new(
        ROOT,
        Page::new(Mode::Bare),
        |state: &Page, ui: &mut Ui<'_, Message<Page>>| {
            ui.keyed("page", |ui| {
                ui.container(Style::default().direction(Direction::Rtl), |ui| {
                    ui.keyed("volume", |ui| {
                        ui.slider(&state.name, 3.0, 0.0..=10.0, 1.0, |_| {
                            act(|_: &mut Page| ())
                        });
                    });
                });
            });
            ui.keyed("file", |ui| {
                ui.command_menu(
                    "File",
                    &Menu::new([
                        MenuEntry::Command(Command::with_id(
                            "open",
                            "Open...",
                            Some(Shortcut::primary('o')),
                        )),
                        MenuEntry::submenu(
                            "Recent",
                            [MenuEntry::Command(Command::with_id("a", "a.txt", None))],
                        ),
                    ]),
                );
            });
        },
    );
    let file = driven.context.find(&["file"]).unwrap();
    // The label changes in the declaration that opens the menu, so both the
    // slider and the rows of the menu are due.
    assert!(driven.context.action(file, Action::Expand));
    driven.state_mut().name = "Gain".into();
    let before = driven.styled_measurements();
    assert!(driven.frame().declared);
    let measured = driven.styled_measurements();
    assert!(measured > before, "the declaration measured nothing");
    settle(&mut driven, 3);
    assert_eq!(driven.styled_measurements(), measured);
}
