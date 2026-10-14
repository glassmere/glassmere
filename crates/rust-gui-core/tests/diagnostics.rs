use rust_gui_core::Label;
use rust_gui_core::{
    Context, DiagnosticKind, HUGE_CHILDREN, Length, MonospaceMeasurer, RELAYOUT_FRAMES,
    REPAINT_FRAMES, Style, Tab, TextWrap, UNSTABLE_ID_FRAMES, Ui,
};
use rust_gui_scene::{Color, Size};

const ROOT: Size = Size::new(800.0, 600.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

fn kinds(context: &Context) -> Vec<DiagnosticKind> {
    context
        .diagnostics()
        .iter()
        .map(|entry| entry.kind)
        .collect()
}

#[test]
fn keys_that_change_every_frame_are_reported_once_with_their_container() {
    let mut context = Context::new();
    assert!(context.diagnostics_enabled());
    for round in 0..u32::from(UNSTABLE_ID_FRAMES) + 5 {
        frame(&mut context, |ui| {
            ui.keyed("stable", |ui| {
                ui.column(Style::default(), |ui| {
                    for item in 0..4 {
                        ui.keyed(format!("row-{item}"), |ui| {
                            ui.label(format!("stable {item} at {round}"))
                        });
                    }
                })
            });
            ui.keyed("churning", |ui| {
                ui.column(Style::default(), |ui| {
                    for item in 0..4 {
                        // The frame number in the key makes every row a new widget.
                        ui.keyed(format!("row-{item}-{round}"), |ui| ui.label("unstable"));
                    }
                })
            });
        });
    }
    let entries = context.diagnostics();
    assert_eq!(kinds(&context), [DiagnosticKind::UnstableIds]);
    assert_eq!(entries[0].path, "churning");
    assert_eq!(entries[0].node, context.find(&["churning"]));
    assert!(entries[0].message.contains("4 children"));

    // A list that really changes (one row added per frame) is not churn.
    let mut context = Context::new();
    for round in 0..30 {
        frame(&mut context, |ui| {
            for item in 0..=round {
                ui.keyed(format!("row-{item}"), |ui| ui.label("grows"));
            }
        });
    }
    assert!(context.diagnostics().is_empty());
}

#[test]
fn layout_that_never_moves_anything_and_wholesale_repaints_are_reported() {
    let mut context = Context::new();
    for round in 0..RELAYOUT_FRAMES + 5 {
        frame(&mut context, |ui| {
            // A wrapped label re-measures whenever its text changes, even
            // though these two strings always measure the same.
            ui.keyed("ticker", |ui| {
                ui.label(
                    Label::new(if round % 2 == 0 { "tick" } else { "tock" }).style(
                        Style::default()
                            .width(Length::Px(200.0))
                            .wrap(TextWrap::Word),
                    ),
                )
            });
        });
    }
    let entries = context.diagnostics();
    assert_eq!(kinds(&context), [DiagnosticKind::Relayout]);
    assert_eq!(entries[0].path, "ticker");

    let mut context = Context::new();
    for round in 0..REPAINT_FRAMES + 5 {
        frame(&mut context, |ui| {
            for item in 0..30 {
                ui.keyed(format!("cell-{item}"), |ui| {
                    ui.label(Label::new("cell").style(Style::default().foreground(Color(
                        200,
                        (round % 200) as u8,
                        0,
                        255,
                    ))))
                });
            }
        });
    }
    assert_eq!(kinds(&context), [DiagnosticKind::Repaint]);
    assert!(context.diagnostics()[0].node.is_none());

    // A tree that settles is quiet.
    let mut context = Context::new();
    for _ in 0..REPAINT_FRAMES + 5 {
        frame(&mut context, |ui| {
            for item in 0..30 {
                ui.keyed(format!("cell-{item}"), |ui| ui.label("cell"));
            }
        });
    }
    assert!(context.diagnostics().is_empty());
}

#[test]
fn controls_without_a_name_are_reported_and_tooltips_or_labels_fix_them() {
    let mut context = Context::new();
    let mut value = String::new();
    let mut selected = 0;
    frame(&mut context, |ui| {
        ui.keyed("empty", |ui| ui.button(""));
        ui.keyed("symbol", |ui| ui.button("→"));
        ui.keyed("described", |ui| ui.button("→"));
        ui.tooltip("Next page");
        ui.keyed("named", |ui| ui.button("Next"));
        ui.keyed("field", |ui| ui.text_input("", &mut value));
        // Built-in widgets name their own symbol buttons.
        ui.keyed("tabs", |ui| {
            ui.tabs(&mut selected, &[Tab::closable("notes.md")], |ui, _| {
                ui.label("panel");
            })
        });
    });
    let mut reported: Vec<(String, bool)> = context
        .diagnostics()
        .iter()
        .map(|entry| {
            assert_eq!(entry.kind, DiagnosticKind::Inaccessible);
            (entry.path.clone(), entry.message.contains("symbols"))
        })
        .collect();
    reported.sort();
    assert_eq!(
        reported,
        [
            ("empty".to_owned(), false),
            ("field".to_owned(), false),
            ("symbol".to_owned(), true),
        ]
    );
    // Unchanged controls are not reported again on later frames.
    let count = context.diagnostics()[0].count;
    frame(&mut context, |ui| {
        ui.keyed("empty", |ui| ui.button(""));
    });
    assert!(
        context
            .diagnostics()
            .iter()
            .all(|entry| entry.count == count)
    );
}

#[test]
fn oversized_layers_and_huge_containers_are_reported_and_checks_can_be_turned_off() {
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("faded", |ui| {
            ui.container(
                Style::default()
                    .width(Length::Px(3000.0))
                    .height(Length::Px(2000.0))
                    .opacity(0.5),
                |ui| ui.label("content"),
            )
        });
        ui.keyed("small", |ui| {
            ui.container(
                Style::default()
                    .width(Length::Px(300.0))
                    .height(Length::Px(200.0))
                    .opacity(0.5),
                |ui| ui.label("content"),
            )
        });
        ui.keyed("everything", |ui| {
            ui.column(Style::default(), |ui| {
                for item in 0..=HUGE_CHILDREN {
                    ui.keyed(format!("row-{item}"), |ui| ui.label("row"));
                }
            })
        });
    };
    let mut context = Context::new();
    frame(&mut context, view);
    let mut found: Vec<(DiagnosticKind, String)> = context
        .diagnostics()
        .iter()
        .map(|entry| (entry.kind, entry.path.clone()))
        .collect();
    found.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(
        found,
        [
            (DiagnosticKind::HugeCollection, "everything".to_owned()),
            (DiagnosticKind::OversizedLayer, "faded".to_owned()),
        ]
    );
    assert!(
        context.diagnostics()[0].message.contains("501")
            || context.diagnostics()[1].message.contains("501")
    );
    context.clear_diagnostics();
    assert!(context.diagnostics().is_empty());

    let mut quiet = Context::new();
    quiet.set_diagnostics(false);
    frame(&mut quiet, view);
    assert!(quiet.diagnostics().is_empty());
}

/// Files that could not be used, and font families that are not there.
#[test]
fn resources_that_fail_and_fonts_that_are_missing_are_reported() {
    let mut context = Context::new();
    context.set_diagnostics(true);
    let path = std::path::Path::new("logo.png");
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.keyed("logo", |ui| {
            ui.image_file(path, rust_gui_core::ImageOptions::default())
        });
        ui.keyed("title", |ui| {
            ui.label(Label::new("Title").style(Style {
                font_family: Some(rust_gui_core::FontFamily::Named("Nonesuch".into())),
                ..Style::default()
            }))
        });
    });
    // The host tells the framework what it could not decode.
    context.set_image_file(path, Err("not a PNG".into()));
    let resource = context
        .diagnostics()
        .iter()
        .find(|entry| entry.kind == rust_gui_core::DiagnosticKind::Resource)
        .expect("the failed image");
    assert!(resource.message.contains("logo.png") && resource.message.contains("not a PNG"));
    // The same failure again is one entry, not two.
    context.set_image_file(path, Err("not a PNG".into()));
    assert_eq!(
        context
            .diagnostics()
            .iter()
            .filter(|entry| entry.kind == rust_gui_core::DiagnosticKind::Resource)
            .count(),
        1
    );

    // The families a style named are offered to the host, which knows which
    // fonts are installed.
    assert_eq!(context.requested_families(), ["Nonesuch"]);
    context.report_font("font families not installed: Nonesuch");
    assert!(
        context
            .diagnostics()
            .iter()
            .any(|entry| entry.kind == rust_gui_core::DiagnosticKind::Font
                && entry.message.contains("Nonesuch"))
    );
}
