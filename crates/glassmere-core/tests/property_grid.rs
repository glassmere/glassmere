mod support;

use glassmere_core::app::Action as Message;
use glassmere_core::{
    Action, Button, Event, Key, NodeId, NumberOptions, PropertyGridOptions, PropertyGridState,
    PropertyMsg, Role, TextEdit, Ui,
};
use glassmere_scene::{Color, Point, Size};
use support::{Driven, act};

const ROOT: Size = Size::new(640.0, 900.0);

struct Shape {
    width: f64,
    visible: bool,
    name: String,
    align: usize,
    fill: Color,
    top: f64,
}

/// What the application owns: the shape, the grid's own state, and the
/// paths of the values that changed, in order.
struct Settings {
    shape: Shape,
    grid: PropertyGridState,
    changed: Vec<String>,
    values: [f64; 12],
    clicks: u32,
}

fn property_msg(message: PropertyMsg) -> Message<Settings> {
    act(move |s: &mut Settings| {
        s.grid.apply(&message);
        match &message {
            PropertyMsg::Number { path, value } => {
                match path.as_str() {
                    "Layout/Width" => s.shape.width = *value,
                    "Layout/Margins/Top" => s.shape.top = *value,
                    _ => {}
                }
                s.changed.push(path.clone());
            }
            PropertyMsg::Bool { path, value } => {
                s.shape.visible = *value;
                s.changed.push(path.clone());
            }
            PropertyMsg::Text { path, value } => {
                s.shape.name.clone_from(value);
                s.changed.push(path.clone());
            }
            PropertyMsg::Choice { path, value } => {
                s.shape.align = *value;
                s.changed.push(path.clone());
            }
            PropertyMsg::Color { path, value } => {
                s.shape.fill = *value;
                s.changed.push(path.clone());
            }
            PropertyMsg::Filter(_) | PropertyMsg::Group { .. } | PropertyMsg::NameShare(_) => {}
        }
    })
}

fn settings_view(state: &Settings, ui: &mut Ui<'_, Message<Settings>>) {
    ui.keyed("grid", |ui| {
        ui.property_grid(
            &state.grid,
            PropertyGridOptions::new("Shape"),
            property_msg,
            |grid| {
                grid.group("Layout", |grid| {
                    grid.default(100.0).number(
                        "Width",
                        state.shape.width,
                        NumberOptions::default().range(0.0, 500.0).suffix(" px"),
                    );
                    grid.choice("Align", state.shape.align, &["Start", "Center", "End"]);
                    grid.group("Margins", |grid| {
                        grid.number("Top", state.shape.top, NumberOptions::default());
                    });
                });
                grid.group("Appearance", |grid| {
                    grid.default(true).bool("Visible", state.shape.visible);
                    grid.text("Name", &state.shape.name);
                    grid.color("Fill", state.shape.fill);
                    grid.custom("Clicks", |ui, label| {
                        ui.keyed("button", |ui| {
                            ui.button(
                                Button::new(label).on_click(act(|s: &mut Settings| s.clicks += 1)),
                            )
                        });
                    });
                    grid.readonly("Kind", "Rectangle");
                });
            },
        )
    });
}

fn settings() -> Settings {
    Settings {
        shape: Shape {
            width: 120.0,
            visible: true,
            name: "Card".into(),
            align: 1,
            fill: Color::rgb(0x1a, 0x9c, 0x8f),
            top: 4.0,
        },
        grid: PropertyGridState::default(),
        changed: Vec::new(),
        values: [0.0; 12],
        clicks: 0,
    }
}

struct App {
    driven: Driven<Settings>,
    /// How many entries of `Settings::changed` `changed` has returned.
    seen: usize,
}

impl App {
    fn new() -> Self {
        Self {
            driven: Driven::new(ROOT, settings(), settings_view),
            seen: 0,
        }
    }

    fn frame(&mut self) {
        self.driven.frame();
    }

    /// The paths changed since the last call: what the view would have
    /// reported as `changed`. Reads without `state_mut`, which would make the
    /// next frame declare and reset an open draft.
    fn changed(&mut self) -> Vec<String> {
        let state = self.driven.state();
        let new = state.changed[self.seen..].to_vec();
        self.seen = state.changed.len();
        new
    }

    fn find(&self, path: &[&str]) -> Option<NodeId> {
        let mut full = vec!["grid", "$rows"];
        full.extend(path);
        self.driven.context.find(&full)
    }

    fn click(&mut self, id: NodeId) {
        let bounds = self.driven.context.node(id).unwrap().bounds;
        self.driven.context.event(Event::PointerMoved(Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }));
        self.driven.context.event(Event::PointerDown);
        self.driven.context.event(Event::PointerUp);
    }

    fn retype(&mut self, id: NodeId, text: &str) {
        self.driven.context.action(id, Action::Focus);
        self.driven.context.event(Event::Text(TextEdit::SelectAll));
        self.driven
            .context
            .event(Event::Text(TextEdit::Insert(text.into())));
    }
}

#[test]
fn rows_bind_values_line_up_with_the_header_and_report_changes_by_path() {
    let mut app = App::new();
    // Every editor carries the property's name for assistive technology.
    let width = app
        .find(&["group:Layout", "row:Width", "$editor", "$value", "$field"])
        .unwrap();
    let node = app.driven.context.node(width).unwrap();
    assert_eq!(
        (node.semantics.role, node.semantics.label.as_str()),
        (Role::SpinButton, "Width")
    );
    let visible = app
        .find(&["group:Appearance", "row:Visible", "$editor", "$value"])
        .unwrap();
    let node = app.driven.context.node(visible).unwrap();
    assert_eq!(
        (node.semantics.role, node.semantics.label.as_str()),
        (Role::Checkbox, "Visible")
    );
    assert_eq!(node.checked, Some(true));

    // The value column starts where the header's second pane does, at every depth.
    let header = app
        .driven
        .context
        .find(&["grid", "$columns", "$second"])
        .unwrap();
    let header = app.driven.context.node(header).unwrap().bounds.x;
    for path in [
        vec!["group:Layout", "row:Width", "$editor"],
        vec!["group:Layout", "group:Margins", "row:Top", "$editor"],
        vec!["group:Appearance", "row:Kind", "$editor"],
    ] {
        let editor = app
            .driven
            .context
            .node(app.find(&path).unwrap())
            .unwrap()
            .bounds;
        assert!(
            (editor.x - header).abs() < 0.5,
            "{path:?}: {} vs {header}",
            editor.x
        );
    }

    // A number commits on Enter and reports its path.
    app.retype(width, "250");
    app.frame();
    assert!(app.changed().is_empty());
    app.driven.context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
    app.frame();
    assert_eq!(app.changed(), ["Layout/Width"]);
    assert_eq!(app.driven.state().shape.width, 250.0);

    app.click(visible);
    app.frame();
    assert_eq!(app.changed(), ["Appearance/Visible"]);
    assert!(!app.driven.state().shape.visible);

    let name = app
        .find(&["group:Appearance", "row:Name", "$editor", "$value"])
        .unwrap();
    app.retype(name, "Badge");
    app.frame();
    assert_eq!(app.changed(), ["Appearance/Name"]);
    assert_eq!(app.driven.state().shape.name, "Badge");

    // A custom row declares the application's own widgets with its messages.
    let clicks = app
        .find(&["group:Appearance", "row:Clicks", "$editor", "button"])
        .unwrap();
    assert_eq!(
        app.driven.context.node(clicks).unwrap().semantics.label,
        "Clicks"
    );
    app.click(clicks);
    app.frame();
    assert_eq!(app.driven.state().clicks, 1);
    assert!(app.changed().is_empty(), "a custom row reports no path");
}

#[test]
fn reset_restores_the_default_and_is_enabled_only_while_the_value_differs() {
    let mut app = App::new();
    let reset = app.find(&["group:Layout", "row:Width", "$reset"]).unwrap();
    let node = app.driven.context.node(reset).unwrap();
    assert!(!node.semantics.disabled, "120 differs from the default 100");
    assert_eq!(node.description.as_deref(), Some("Reset Width"));
    app.click(reset);
    app.frame();
    assert_eq!(app.changed(), ["Layout/Width"]);
    assert_eq!(app.driven.state().shape.width, 100.0);
    app.frame();
    assert!(app.driven.context.node(reset).unwrap().semantics.disabled);
    // Rows without a default have no button.
    assert!(app.find(&["group:Layout", "row:Align", "$reset"]).is_none());
    // Visible is at its default, so its button is there but switched off.
    let reset = app
        .find(&["group:Appearance", "row:Visible", "$reset"])
        .unwrap();
    assert!(app.driven.context.node(reset).unwrap().semantics.disabled);
}

#[test]
fn groups_collapse_and_the_filter_opens_matches_and_hides_the_rest() {
    let mut app = App::new();
    let header = app.find(&["group:Layout", "$header"]).unwrap();
    assert_eq!(
        app.driven
            .context
            .node(header)
            .unwrap()
            .description
            .as_deref(),
        Some("Collapse Layout")
    );
    app.click(header);
    app.frame();
    app.frame();
    assert!(app.driven.state().grid.collapsed.contains("Layout"));
    assert!(app.find(&["group:Layout", "row:Width"]).is_none());
    assert!(app.find(&["group:Appearance", "row:Name"]).is_some());

    // A filter looks inside collapsed groups and hides groups with no match.
    app.driven.state_mut().grid.filter = "top".into();
    app.frame();
    assert!(
        app.find(&["group:Layout", "group:Margins", "row:Top"])
            .is_some()
    );
    assert!(app.find(&["group:Layout", "row:Width"]).is_none());
    assert!(app.find(&["group:Appearance"]).is_none());
    // A group's own name shows all of its rows.
    app.driven.state_mut().grid.filter = " APPEAR ".into();
    app.frame();
    assert!(app.find(&["group:Layout"]).is_none());
    for row in [
        "row:Visible",
        "row:Name",
        "row:Fill",
        "row:Clicks",
        "row:Kind",
    ] {
        assert!(app.find(&["group:Appearance", row]).is_some(), "{row}");
    }
    // Clearing the filter brings the collapsed state back.
    app.driven.state_mut().grid.filter.clear();
    app.frame();
    assert!(app.find(&["group:Layout", "row:Width"]).is_none());
    assert!(app.find(&["group:Layout", "$header"]).is_some());
}

#[test]
#[should_panic(expected = "property `Width` holds a f64 but its default is a i32")]
fn a_default_of_the_wrong_type_is_a_programming_error_with_a_clear_message() {
    fn view(_: &Settings, ui: &mut Ui<'_, Message<Settings>>) {
        ui.keyed("grid", |ui| {
            ui.property_grid(
                &PropertyGridState::default(),
                PropertyGridOptions::default(),
                property_msg,
                |grid| {
                    grid.default(100)
                        .number("Width", 1.0, NumberOptions::default());
                },
            )
        });
    }
    let _ = Driven::new(ROOT, settings(), view);
}

#[test]
fn a_scrolling_grid_keeps_its_rows_clear_of_the_scrollbar_and_under_the_header() {
    fn view(state: &Settings, ui: &mut Ui<'_, Message<Settings>>) {
        let options = PropertyGridOptions::new("Many").height(200.0);
        ui.keyed("grid", |ui| {
            ui.property_grid(&state.grid, options, property_msg, |grid| {
                for (index, value) in state.values.iter().enumerate() {
                    grid.number(&format!("Value {index}"), *value, NumberOptions::default());
                }
            })
        });
    }
    let mut driven = Driven::new(ROOT, settings(), view);
    // The first declaration learns the width, the second uses it.
    driven.state_mut();
    driven.frame();
    let context = &driven.context;
    let bounds = |path: &[&str]| context.node(context.find(path).unwrap()).unwrap().bounds;
    let scroll = bounds(&["grid", "$scroll"]);
    assert_eq!(scroll.height, 200.0);
    assert!(
        context
            .node(context.find(&["grid", "$scroll"]).unwrap())
            .unwrap()
            .scroll
            .unwrap()
            .maximum
            > 0.0
    );
    let row = bounds(&["grid", "$scroll", "$rows", "row:Value 0"]);
    assert!(row.x + row.width <= scroll.x + scroll.width - 4.0);
    let editor = bounds(&["grid", "$scroll", "$rows", "row:Value 0", "$editor"]);
    let header = bounds(&["grid", "$columns", "$second"]);
    assert!((editor.x - header.x).abs() < 0.5);
    assert_eq!(bounds(&["grid", "$filter"]).width, row.width);
}

#[test]
fn the_header_divider_sizes_the_name_column() {
    let mut app = App::new();
    let editor = |app: &App| {
        let id = app.find(&["group:Layout", "row:Width", "$editor"]).unwrap();
        app.driven.context.node(id).unwrap().bounds.x
    };
    let divider = app
        .driven
        .context
        .find(&["grid", "$columns", "$divider"])
        .unwrap();
    let before = editor(&app);
    app.driven.context.action(divider, Action::Focus);
    for _ in 0..5 {
        app.driven.context.event(Event::KeyDown {
            key: Key::ArrowRight,
            shift: false,
            repeat: false,
        });
    }
    app.frame();
    app.frame();
    assert!(app.driven.state().grid.name_share > 0.4);
    assert!(editor(&app) > before + 5.0);
}

#[test]
fn colours_follow_whole_hex_values_and_drop_drafts_when_the_field_is_left() {
    let mut app = App::new();
    let hex = app
        .find(&["group:Appearance", "row:Fill", "$editor", "$value", "$hex"])
        .unwrap();
    let shown = |app: &App| app.driven.context.node(hex).unwrap().value;
    assert_eq!(shown(&app).as_deref(), Some("#1a9c8f"));
    // The node shows the draft as it is typed, before any declaration.
    app.retype(hex, "#ff80");
    assert_eq!(shown(&app).as_deref(), Some("#ff80"));
    app.frame();
    assert!(app.changed().is_empty());
    assert_eq!(app.driven.state().shape.fill, Color::rgb(0x1a, 0x9c, 0x8f));
    // The draft outlives the declarations a message-model app runs.
    app.driven.state_mut();
    app.frame();
    assert_eq!(shown(&app).as_deref(), Some("#ff80"));
    app.driven
        .context
        .event(Event::Text(TextEdit::Insert("00".into())));
    app.frame();
    assert_eq!(app.changed(), ["Appearance/Fill"]);
    assert_eq!(app.driven.state().shape.fill, Color::rgb(0xff, 0x80, 0x00));
    // Alpha is the fourth pair; a draft that is no colour goes when focus does.
    app.retype(hex, "FF800080");
    app.frame();
    assert_eq!(app.driven.state().shape.fill, Color(0xff, 0x80, 0x00, 0x80));
    app.retype(hex, "orange");
    app.frame();
    app.driven.context.event(Event::KeyDown {
        key: Key::Tab,
        shift: false,
        repeat: false,
    });
    app.frame();
    app.frame();
    assert_eq!(shown(&app).as_deref(), Some("#ff800080"));
}

/// Rows are 44 px with 4 px between them, so row `n` starts at `n * PITCH`.
const PITCH: f32 = 48.0;
const VIEWPORT: f32 = 300.0;

/// What a grid of `items` rows and headers is tall to scroll through.
fn scrollable(items: usize) -> f32 {
    items as f32 * PITCH - 4.0 - VIEWPORT
}

fn tall_view(state: &Settings, ui: &mut Ui<'_, Message<Settings>>) {
    ui.keyed("grid", |ui| {
        let options = PropertyGridOptions::new("Tall").height(VIEWPORT);
        ui.property_grid(&state.grid, options, property_msg, |grid| {
            for index in 0..10_000 {
                grid.number(
                    &format!("Value {index}"),
                    index as f64,
                    NumberOptions::default(),
                );
            }
        })
    });
}

fn grouped_view(state: &Settings, ui: &mut Ui<'_, Message<Settings>>) {
    ui.keyed("grid", |ui| {
        let options = PropertyGridOptions::new("Groups").height(VIEWPORT);
        ui.property_grid(&state.grid, options, property_msg, |grid| {
            for group in 0..100 {
                grid.group(&format!("Group {group}"), |grid| {
                    for row in 0..100 {
                        grid.number(&format!("Row {row}"), 0.0, NumberOptions::default());
                    }
                });
            }
        })
    });
}

fn scroll_to(driven: &mut Driven<Settings>, offset: f32) {
    let scroll = driven.context.find(&["grid", "$scroll"]).unwrap();
    driven.context.action(scroll, Action::ScrollTo(offset));
    driven.frame();
}

fn bounds(driven: &Driven<Settings>, path: &[&str]) -> glassmere_scene::Rect {
    let id = driven.context.find(path).unwrap();
    driven.context.node(id).unwrap().bounds
}

#[test]
fn ten_thousand_properties_declare_only_a_screenful_of_nodes() {
    let mut driven = Driven::new(ROOT, settings(), tall_view);
    // The first declaration learns the width, the second uses it.
    driven.frame();
    let scroll = driven.context.find(&["grid", "$scroll"]).unwrap();
    let maximum =
        |driven: &Driven<Settings>| driven.context.node(scroll).unwrap().scroll.unwrap().maximum;
    // The scrollbar is as long as if every row were there.
    assert_eq!(maximum(&driven), scrollable(10_000));
    let has = |driven: &Driven<Settings>, index: usize| {
        let row = format!("row:Value {index}");
        driven
            .context
            .find(&["grid", "$scroll", "$rows", &row])
            .is_some()
    };
    assert!(has(&driven, 0) && has(&driven, 8));
    assert!(!has(&driven, 20));
    assert!(
        driven.context.node_count() < 150,
        "{}",
        driven.context.node_count()
    );

    // Scrolled to the middle, only the rows there exist, and row 5000 is
    // exactly where its place in the full list puts it.
    scroll_to(&mut driven, 5000.0 * PITCH);
    assert!(has(&driven, 5000) && !has(&driven, 0) && !has(&driven, 4990));
    assert!(
        driven.context.node_count() < 150,
        "{}",
        driven.context.node_count()
    );
    let top = bounds(&driven, &["grid", "$scroll"]).y;
    let row = bounds(&driven, &["grid", "$scroll", "$rows", "row:Value 5000"]);
    assert_eq!((row.y, row.height), (top, 44.0));
    let field = driven
        .context
        .find(&[
            "grid",
            "$scroll",
            "$rows",
            "row:Value 5000",
            "$editor",
            "$value",
            "$field",
        ])
        .unwrap();
    assert_eq!(
        driven.context.node(field).unwrap().value.as_deref(),
        Some("5000")
    );

    // The end of the list is reachable, flush with the bottom.
    scroll_to(&mut driven, f32::MAX);
    assert!(has(&driven, 9999) && !has(&driven, 9980));
    let last = bounds(&driven, &["grid", "$scroll", "$rows", "row:Value 9999"]);
    let view = bounds(&driven, &["grid", "$scroll"]);
    assert_eq!(last.y + last.height, view.y + view.height);
}

#[test]
fn groups_scroll_as_if_every_row_were_there_and_declare_only_those_in_view() {
    let mut driven = Driven::new(ROOT, settings(), grouped_view);
    driven.frame();
    let maximum = |driven: &Driven<Settings>| {
        let scroll = driven.context.find(&["grid", "$scroll"]).unwrap();
        driven.context.node(scroll).unwrap().scroll.unwrap().maximum
    };
    // A header is a row too: a hundred groups of a hundred rows and a header.
    assert_eq!(maximum(&driven), scrollable(100 * 101));
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 0", "$header"])
            .is_some()
    );
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 40"])
            .is_none()
    );

    // Item 5100 is row 49 of group 50. The group's header and its first rows
    // are out of view, so it starts with a spacer for them and the groups
    // before it are gone; what follows the view is one spacer at the end.
    scroll_to(&mut driven, 5100.0 * PITCH);
    let group = ["grid", "$scroll", "$rows", "group:Group 50"];
    let at = |tail: &[&str]| {
        let path: Vec<&str> = group.iter().chain(tail).copied().collect();
        driven.context.find(&path)
    };
    assert!(at(&["$header"]).is_none());
    assert!(at(&["row:Row 49"]).is_some() && at(&["row:Row 40"]).is_none());
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 49"])
            .is_none()
    );
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 51"])
            .is_none()
    );
    let top = bounds(&driven, &["grid", "$scroll"]).y;
    let row = bounds(
        &driven,
        &["grid", "$scroll", "$rows", "group:Group 50", "row:Row 49"],
    );
    assert_eq!(row.y, top);
    let before = bounds(
        &driven,
        &["grid", "$scroll", "$rows", "group:Group 50", "$before"],
    );
    assert_eq!(before.height, 5098.0 * PITCH - 4.0);
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "$after"])
            .is_some()
    );
    assert!(
        driven.context.node_count() < 150,
        "{}",
        driven.context.node_count()
    );

    // A header is declared when it is in view, with the rows under it.
    scroll_to(&mut driven, 5050.0 * PITCH);
    let header = bounds(
        &driven,
        &["grid", "$scroll", "$rows", "group:Group 50", "$header"],
    );
    assert_eq!(header.y, top);
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 50", "row:Row 0"])
            .is_some()
    );
}

#[test]
fn collapsing_and_filtering_change_what_the_scrollbar_covers_and_a_scrolled_grid_follows() {
    let mut driven = Driven::new(ROOT, settings(), grouped_view);
    driven.frame();
    let scroll = driven.context.find(&["grid", "$scroll"]).unwrap();
    let node = |driven: &Driven<Settings>| driven.context.node(scroll).unwrap().scroll.unwrap();

    // A collapsed group is its header alone: the one after it comes next.
    driven.state_mut().grid.collapsed.insert("Group 3".into());
    driven.frame();
    assert_eq!(node(&driven).maximum, scrollable(100 * 101 - 100));
    scroll_to(&mut driven, 303.0 * PITCH);
    let rows = ["grid", "$scroll", "$rows"];
    let find = |driven: &Driven<Settings>, tail: &[&str]| {
        let path: Vec<&str> = rows.iter().chain(tail).copied().collect();
        driven.context.find(&path)
    };
    assert!(find(&driven, &["group:Group 3", "$header"]).is_some());
    assert!(find(&driven, &["group:Group 3", "row:Row 0"]).is_none());
    assert!(find(&driven, &["group:Group 4", "$header"]).is_some());
    let header = bounds(
        &driven,
        &["grid", "$scroll", "$rows", "group:Group 3", "$header"],
    );
    let next = bounds(
        &driven,
        &["grid", "$scroll", "$rows", "group:Group 4", "$header"],
    );
    assert_eq!(next.y - header.y, PITCH);
    driven.state_mut().grid.collapsed.clear();

    // The filter keeps one row of each group: a hundred groups of two. A grid
    // scrolled far past the end of that declares nothing for a frame, then
    // shows the end.
    scroll_to(&mut driven, 5000.0 * PITCH);
    driven.state_mut().grid.filter = "row 99".into();
    driven.frame();
    driven.frame();
    assert_eq!(node(&driven).maximum, scrollable(200));
    assert_eq!(node(&driven).offset, scrollable(200));
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 99", "row:Row 99"])
            .is_some()
    );
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 0"])
            .is_none()
    );
    scroll_to(&mut driven, 0.0);
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 0", "row:Row 99"])
            .is_some()
    );
    assert!(
        driven
            .context
            .find(&["grid", "$scroll", "$rows", "group:Group 0", "row:Row 98"])
            .is_none()
    );
}

#[test]
fn tab_walks_the_rows_of_a_scrolled_grid_and_keeps_the_focus_in_view() {
    let mut driven = Driven::new(ROOT, settings(), tall_view);
    driven.frame();
    let field = |index: usize| {
        let row = format!("row:Value {index}");
        driven.context.find(&[
            "grid", "$scroll", "$rows", &row, "$editor", "$value", "$field",
        ])
    };
    driven.context.action(field(0).unwrap(), Action::Focus);
    let press = |driven: &mut Driven<Settings>, shift: bool| {
        driven.context.event(Event::KeyDown {
            key: Key::Tab,
            shift,
            repeat: false,
        });
        driven.frame();
    };
    let focused_row = |driven: &Driven<Settings>| {
        let node = driven
            .context
            .node(driven.context.focused().unwrap())
            .unwrap();
        (node.semantics.label.clone(), node.bounds)
    };
    let view = bounds(&driven, &["grid", "$scroll"]);
    for step in 1..=60 {
        press(&mut driven, false);
        let (label, bounds) = focused_row(&driven);
        assert!(label.starts_with("Value "), "step {step}: {label}");
        assert!(
            bounds.y >= view.y && bounds.y + bounds.height <= view.y + view.height,
            "step {step}: {label} at {bounds:?} outside {view:?}"
        );
    }
    let (label, _) = focused_row(&driven);
    let reached: usize = label["Value ".len()..].parse().unwrap();
    assert!(reached >= 20, "reached only {label}");
    let scroll = driven.context.find(&["grid", "$scroll"]).unwrap();
    assert!(driven.context.node(scroll).unwrap().scroll.unwrap().offset > 0.0);
    assert!(driven.context.node_count() < 150);

    // And back up to the first.
    for _ in 0..60 {
        press(&mut driven, true);
    }
    let (label, _) = focused_row(&driven);
    assert_eq!(label, "Value 0");
    assert_eq!(
        driven.context.node(scroll).unwrap().scroll.unwrap().offset,
        0.0
    );
}
