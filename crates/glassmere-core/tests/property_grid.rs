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
            _ => {}
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
