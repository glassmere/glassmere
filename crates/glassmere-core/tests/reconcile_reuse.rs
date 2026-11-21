//! A node that is declared again takes what the declaration holds and drops
//! what it held, at the point it takes the new value. What the application
//! can see of that, the sizes its vector images report and when they are
//! dropped, happens in that order, and what the nodes show is what a context
//! that was declared once shows.
use glassmere_core::{
    BackgroundImage, Button, CodeEditorOptions, Context, CustomWidget, DockLayout, DockNode,
    DockOptions, DockPanel, DockTarget, DockZone, ImageOptions, ItemId, Label, Length, ListModel,
    ListState, MonospaceMeasurer, PopoverOptions, Semantics, Style, Tab, TextDocument, TextInput,
    Tooltip, Ui, VirtualListOptions, WidgetBuilder,
};
use glassmere_scene::{Color, Image, Primitive, Rect, Size, VectorImage};
use std::cell::Cell;
use std::sync::{Arc, Mutex};

const ROOT: Size = Size::new(300.0, 300.0);

type Log = Arc<Mutex<Vec<String>>>;

/// A vector image that says when it is asked its size and when it is dropped.
struct Probe {
    name: String,
    log: Log,
}

impl VectorImage for Probe {
    fn size(&self) -> Size {
        self.log.lock().unwrap().push(format!("size:{}", self.name));
        Size::new(10.0, 10.0)
    }

    fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
        vec![255; (width * height * 4) as usize]
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.log.lock().unwrap().push(format!("drop:{}", self.name));
    }
}

fn probe(log: &Log, name: &str) -> Image {
    Image::from_vector(Probe {
        name: name.to_owned(),
        log: log.clone(),
    })
}

/// A box with no room, so that nothing it holds reaches the scene.
fn nothing() -> Style {
    Style::default()
        .width(Length::Px(0.0))
        .height(Length::Px(0.0))
}

fn run(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
    context.run(ROOT, &mut MonospaceMeasurer, view);
}

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.lock().unwrap())
}

/// A parent whose background, a child image and a custom widget each hold an
/// image, then a later sibling image. The names carry the frame.
fn images(ui: &mut Ui<'_>, log: &Log, frame: u32) {
    let named = |letter: &str| probe(log, &format!("{letter}{frame}"));
    let (a, b, b2, c, d) = (named("A"), named("B"), named("X"), named("C"), named("D"));
    ui.keyed("outer", |ui| {
        ui.column(nothing().background_image(BackgroundImage::new(&a)), |ui| {
            ui.keyed("b", |ui| {
                ui.image(&b, ImageOptions::new("").style(nothing()))
            });
            ui.keyed("b2", |ui| {
                ui.image(&b2, ImageOptions::new("").style(nothing()))
            });
            ui.keyed("c", |ui| {
                ui.widget(
                    nothing().opacity(0.0),
                    WidgetBuilder::new(|_| CustomWidget {
                        primitives: vec![Primitive::Image {
                            rect: Rect::default(),
                            image: c.clone(),
                            source: Rect::default(),
                            tint: None,
                            radius: 0.0,
                        }],
                        ..CustomWidget::default()
                    }),
                )
            });
        })
    });
    ui.keyed("d", |ui| {
        ui.image(&d, ImageOptions::new("").style(nothing()))
    });
}

#[test]
fn old_images_are_dropped_where_the_node_takes_the_new_ones() {
    let log = Log::default();
    let mut context = Context::new();
    run(&mut context, |ui| images(ui, &log, 1));
    let first = take(&log);
    run(&mut context, |ui| images(ui, &log, 2));
    let second = take(&log);
    run(&mut context, |ui| images(ui, &log, 3));
    let third = take(&log);
    assert_eq!(
        first,
        &[
            "size:B1", "size:X1", "size:D1", "size:A1", "size:B1", "size:X1", "size:D1"
        ]
    );
    assert_eq!(
        second,
        &[
            "drop:A1", "size:B2", "drop:B1", "size:X2", "drop:X1", "size:D2", "drop:D1", "size:A2",
            "size:B2", "size:X2", "size:D2", "drop:C1",
        ]
    );
    assert_eq!(
        third,
        &[
            "drop:A2", "size:B3", "drop:B2", "size:X3", "drop:X2", "size:D3", "drop:D2", "size:A3",
            "size:B3", "size:X3", "size:D3", "drop:C2",
        ]
    );
}

fn tooltips(ui: &mut Ui<'_>, log: &Log, frame: u32) {
    let named = |letter: &str| probe(log, &format!("{letter}{frame}"));
    let (e, f, g) = (named("E"), named("F"), named("G"));
    let bubble = |image: &Image| {
        Tooltip::new("help").style(nothing().background_image(BackgroundImage::new(image)))
    };
    ui.keyed("outer", |ui| {
        ui.column(Style::default(), |ui| {
            ui.keyed("inner", |ui| {
                ui.button(Button::new("inner").style(Style::button()));
            });
            ui.tooltip_with(bubble(&f));
        });
    });
    ui.tooltip_with(bubble(&e));
    ui.keyed("later", |ui| {
        ui.button(Button::new("later").style(Style::button()));
    });
    ui.tooltip_with(bubble(&g));
}

#[test]
fn a_tooltip_that_is_replaced_drops_its_image_at_the_same_place() {
    let log = Log::default();
    let mut context = Context::new();
    run(&mut context, |ui| tooltips(ui, &log, 1));
    let first = take(&log);
    run(&mut context, |ui| tooltips(ui, &log, 2));
    let second = take(&log);
    run(&mut context, |ui| tooltips(ui, &log, 3));
    let third = take(&log);
    assert_eq!(first, &[] as &[&str]);
    assert_eq!(second, &["drop:E1", "drop:F1", "drop:G1"]);
    assert_eq!(third, &["drop:E2", "drop:F2", "drop:G2"]);
}

/// One declaration of the spare-pool test: `count` rows, each with its own
/// text, style, tooltip and enabled state, picked by `salt`.
fn rows(ui: &mut Ui<'_>, count: usize, salt: usize) {
    ui.keyed("list", |ui| {
        ui.column(Style::default().gap(2.0), |ui| {
            for index in 0..count {
                ui.keyed(format!("row{index}"), |ui| {
                    ui.row(
                        Style::default().padding(((index + salt) % 3) as f32),
                        |ui| {
                            ui.keyed("name", |ui| {
                                ui.label(Label::new(format!("row {index} of {salt}")).style(
                                    Style::default().font_size(12.0 + ((index * salt) % 4) as f32),
                                ))
                            });
                            ui.keyed("go", |ui| {
                                ui.button(
                                    Button::new(format!("go {}", index + salt))
                                        .enabled(index % 2 == salt % 2)
                                        .style(Style::button()),
                                )
                            });
                            if (index + salt) % 2 == 1 {
                                ui.tooltip(format!("tip {index} {salt}"));
                            }
                        },
                    )
                });
            }
        })
    });
}

/// What a context shows of its nodes: the scene, and for every row the
/// bounds and the semantics.
fn shown(context: &Context, count: usize) -> Vec<String> {
    let mut shown = vec![format!("{:?}", context.scene().primitives)];
    shown.push(context.node_count().to_string());
    for index in 0..count {
        for leaf in ["name", "go"] {
            let id = context
                .find(&["list", &format!("row{index}"), leaf])
                .unwrap();
            let node = context.node(id).unwrap();
            shown.push(format!("{:?} {:?}", node.bounds, node.semantics));
        }
    }
    shown
}

#[test]
fn declarations_that_grow_shrink_and_change_show_what_a_fresh_context_shows() {
    let mut context = Context::new();
    // What the tree does with the elements of each frame: how many nodes it
    // makes, keeps and drops.
    let mut counts = Vec::new();
    for (count, salt) in [(6, 1), (2, 2), (6, 3), (6, 3), (3, 4)] {
        run(&mut context, |ui| rows(ui, count, salt));
        let stats = context.stats();
        counts.push((stats.created, stats.reused, stats.removed));
        let mut fresh = Context::new();
        run(&mut fresh, |ui| rows(ui, count, salt));
        assert_eq!(shown(&context, count), shown(&fresh, count));
    }
    assert_eq!(
        counts,
        [(19, 0, 0), (0, 7, 12), (12, 7, 0), (0, 19, 0), (0, 10, 9)]
    );
}

struct Rows(usize);

impl ListModel for Rows {
    fn len(&self) -> usize {
        self.0
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        ((id.0 as usize) < self.0).then_some(id.0 as usize)
    }
    fn label(&self, index: usize) -> String {
        format!("Row {index}")
    }
}

/// A form with a node of every kind whose fields a declaration changes, in
/// the two states `step` picks: text, style, tooltip, enabled state, custom
/// widget, a field's value, a selectable text and the selected tab.
fn form(ui: &mut Ui<'_>, step: usize) {
    let on = step == 1;
    ui.keyed("title", |ui| {
        ui.label(
            Label::new(if on { "Second title" } else { "First" })
                .style(Style::default().font_size(if on { 20.0 } else { 14.0 })),
        );
        ui.tooltip(if on {
            "about the second"
        } else {
            "about the first"
        });
    });
    ui.keyed("go", |ui| {
        ui.button(
            Button::new(if on { "Stop" } else { "Go" })
                .enabled(!on)
                .style(Style::button().padding(if on { 4.0 } else { 9.0 })),
        );
        ui.tooltip_with(Tooltip::new("go").style(Style::default().padding(step as f32)));
    });
    ui.keyed("name", |ui| {
        ui.text_input(
            TextInput::new("Name"),
            if on { "Eve" } else { "Ada" },
            |_| (),
        );
    });
    ui.keyed("words", |ui| {
        ui.label(Label::new(if on { "pick these" } else { "pick those words" }).selectable());
    });
    ui.keyed("custom", |ui| {
        ui.widget(
            Style::default().width(Length::Px(40.0 + step as f32 * 10.0)),
            WidgetBuilder::new(move |_| CustomWidget {
                size: Size::new(40.0 + step as f32 * 10.0, 20.0 + step as f32),
                semantics: Semantics {
                    value: Some(format!("dial {step}")),
                    ..Semantics::default()
                },
                primitives: vec![Primitive::Quad {
                    rect: Rect {
                        width: 8.0 + step as f32,
                        height: 8.0,
                        ..Rect::default()
                    },
                    color: Color(10, 20, 30, 255),
                    radius: 0.0,
                }],
                ..CustomWidget::default()
            }),
        );
    });
    let tabs: Vec<Tab> = (0..12)
        .map(|index| Tab::new(format!("Tab {index}")))
        .collect();
    ui.keyed("tabs", |ui| {
        ui.tabs(if on { 11 } else { 0 }, &tabs, |_| (), |_, _| {});
    });
}

/// Everything a context shows: the scene, and every node without its id and
/// the ids it holds, which a context that declared other views before has
/// numbered differently.
fn everything(context: &Context) -> Vec<String> {
    let mut shown: Vec<String> = context
        .scene()
        .primitives
        .iter()
        .map(|primitive| format!("{primitive:?}"))
        .collect();
    for node in context.snapshots() {
        shown.push(format!(
            "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {}",
            node.bounds,
            node.clip,
            node.semantics,
            node.scroll,
            node.value,
            node.checked,
            node.selected,
            node.description,
            node.expanded,
            node.text_selection,
            node.collection,
            node.children.len(),
        ));
    }
    shown
}

#[test]
fn a_form_that_changes_every_field_of_its_nodes_shows_what_a_fresh_context_shows() {
    let mut context = Context::new();
    for step in [0, 1, 0, 1, 1] {
        run(&mut context, |ui| form(ui, step));
        let mut fresh = Context::new();
        run(&mut fresh, |ui| form(ui, step));
        assert_eq!(everything(&context), everything(&fresh), "step {step}");
    }
}

/// What a list and a text area scroll to is carried by the node's scroll
/// offset, which a fresh context has not got to yet, so it is read where it
/// has to be: the revealed row and the caret's line are in their view.
fn scrolled_view(ui: &mut Ui<'_>, selected: Option<u64>, notes: &TextDocument) {
    ui.keyed("list", |ui| {
        let mut state = ListState::default();
        state.select(selected.map(ItemId));
        ui.virtual_list(
            &Rows(100),
            &state,
            VirtualListOptions {
                height: 120.0,
                row_height: 30.0,
                ..Default::default()
            },
            |_| (),
        );
    });
    ui.keyed("notes", |ui| {
        ui.text_area(
            notes,
            CodeEditorOptions {
                style: Style::default()
                    .width(Length::Px(200.0))
                    .height(Length::Px(100.0)),
                ..CodeEditorOptions::default()
            },
            |_| (),
        );
    });
}

/// The bounds of the node with this label.
fn bounds_of(context: &Context, label: &str) -> Rect {
    context
        .snapshots()
        .into_iter()
        .find(|node| node.semantics.label == label)
        .unwrap_or_else(|| panic!("no node labelled {label}"))
        .bounds
}

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.y >= outer.y - 0.5 && inner.y + inner.height <= outer.y + outer.height + 0.5
}

#[test]
fn a_list_and_a_text_area_take_the_offset_their_declaration_scrolls_to() {
    let text = (0..100)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut notes = TextDocument::from_text(&text);
    let mut context = Context::new();
    run(&mut context, |ui| scrolled_view(ui, None, &notes));
    let list = context.find(&["list"]).unwrap();
    assert_eq!(context.node(list).unwrap().scroll.unwrap().offset, 0.0);
    let area = bounds_of(&context, "Items");
    assert!(inside(bounds_of(&context, "Row 0"), area));

    let start = text.find("line 80").unwrap();
    notes.set_selection(start, start);
    run(&mut context, |ui| scrolled_view(ui, Some(60), &notes));
    let offset = context.node(list).unwrap().scroll.unwrap().offset;
    assert!(offset > 0.0, "the list did not scroll");
    assert!(inside(bounds_of(&context, "Row 60"), area));

    let notes_bounds = context
        .node(context.find(&["notes"]).unwrap())
        .unwrap()
        .bounds;
    let line = context
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::RichText { rect, spans, .. }
                if spans.iter().any(|span| span.text == "line 80") =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .expect("the caret's line is painted");
    assert!(
        inside(line, notes_bounds),
        "{line:?} is outside {notes_bounds:?}"
    );
}

/// A memoized subtree that is kept while its key stays and declared again
/// when the key changes.
fn memoized(ui: &mut Ui<'_>, key: u32, step: usize, runs: &Cell<usize>) {
    ui.keyed("memo", |ui| {
        ui.memo(&key, Style::default(), |ui| {
            runs.set(runs.get() + 1);
            form(ui, step)
        })
    });
}

#[test]
fn a_memo_keeps_what_it_held_and_declares_what_its_key_changed() {
    let runs = Cell::new(0);
    let mut context = Context::new();
    // The content of a kept subtree is not asked for, so it stays what the
    // last declaration made it.
    for (key, step, shown, ran) in [
        (1, 0, 0, 1),
        (1, 1, 0, 1),
        (2, 1, 1, 2),
        (2, 0, 1, 2),
        (3, 0, 0, 3),
    ] {
        run(&mut context, |ui| memoized(ui, key, step, &runs));
        let mut fresh = Context::new();
        run(&mut fresh, |ui| memoized(ui, key, shown, &Cell::new(0)));
        assert_eq!(
            everything(&context),
            everything(&fresh),
            "key {key} step {step}"
        );
        assert_eq!(runs.get(), ran, "key {key} step {step}");
    }
}

fn docked(ui: &mut Ui<'_>, layout: &DockLayout, step: usize) {
    let panels = [
        DockPanel::new("files", "Files"),
        DockPanel::new("editor", "Editor"),
        DockPanel::new("output", "Output"),
    ];
    ui.keyed("dock", |ui| {
        ui.dock(
            layout,
            &panels,
            DockOptions::default(),
            |_| (),
            |ui, id| {
                if id != "editor" {
                    ui.label(format!("content:{id}"));
                    return;
                }
                ui.column(Style::default().gap(step as f32 * 3.0), |ui| {
                    ui.label(
                        Label::new(format!("editor {step}"))
                            .style(Style::default().font_size(12.0 + step as f32 * 4.0)),
                    );
                    ui.text_input(
                        TextInput::new("Notes"),
                        if step == 1 { "Eve" } else { "Ada" },
                        |_| (),
                    );
                    ui.tooltip(format!("tip {step}"));
                });
            },
        )
    });
}

fn node_labelled(context: &Context, label: &str) -> glassmere_core::NodeId {
    context
        .snapshots()
        .into_iter()
        .find(|node| node.semantics.label == label)
        .unwrap_or_else(|| panic!("no node labelled {label}"))
        .id
}

#[test]
fn a_panel_moved_to_another_group_takes_a_changed_style_and_control_with_it() {
    let before = DockLayout::new(DockNode::row(
        0.3,
        DockNode::tabs(["files"]),
        DockNode::column(0.5, DockNode::tabs(["editor"]), DockNode::tabs(["output"])),
    ));
    let mut after = before.clone();
    assert!(after.move_panel(
        "editor",
        DockTarget::Group {
            anchor: "files".into(),
            zone: DockZone::Center,
        },
    ));
    let mut context = Context::new();
    run(&mut context, |ui| docked(ui, &before, 0));
    let input = node_labelled(&context, "Notes");
    run(&mut context, |ui| docked(ui, &after, 1));
    assert_eq!(
        node_labelled(&context, "Notes"),
        input,
        "the panel's nodes were claimed in their new place"
    );
    let mut fresh = Context::new();
    run(&mut fresh, |ui| docked(ui, &after, 1));
    assert_eq!(everything(&context), everything(&fresh));
}

/// A button that anchors a popover while it is open, between two others.
fn anchored(ui: &mut Ui<'_>, open: bool) {
    ui.keyed("bar", |ui| {
        ui.row(Style::default(), |ui| {
            ui.keyed("share", |ui| {
                ui.button("Share");
                ui.popover(
                    open,
                    PopoverOptions::default(),
                    |_| (),
                    |ui| {
                        ui.keyed("note", |ui| ui.label("Copied to the clipboard"));
                    },
                );
            });
            ui.keyed("more", |ui| ui.button("More"));
        });
    });
}

#[test]
fn an_anchor_keeps_the_size_its_popover_gives_it_while_it_opens_and_closes() {
    let mut context = Context::new();
    for open in [false, true, true, false, true] {
        run(&mut context, |ui| anchored(ui, open));
        let mut fresh = Context::new();
        run(&mut fresh, |ui| anchored(ui, open));
        // Focus returns to the anchor when the popover closes, which a
        // fresh context has not got to, so the sizes are what is compared.
        let sizes = |context: &Context| -> Vec<Rect> {
            context.snapshots().iter().map(|node| node.bounds).collect()
        };
        assert_eq!(sizes(&context), sizes(&fresh), "open {open}");
    }
}
