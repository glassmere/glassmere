use rust_gui_core::{
    Action, Context, Event, Key, MonospaceMeasurer, NodeId, NumberOptions, PropertyGridOptions,
    PropertyGridResponse, PropertyGridState, Role, TextEdit,
};
use rust_gui_scene::{Color, Point, Size};

const ROOT: Size = Size::new(640.0, 900.0);

struct Shape {
    width: f64,
    visible: bool,
    name: String,
    align: usize,
    fill: Color,
    top: f64,
    clicks: u32,
}

struct App {
    context: Context,
    state: PropertyGridState,
    shape: Shape,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            context: Context::new(),
            state: PropertyGridState::default(),
            shape: Shape {
                width: 120.0,
                visible: true,
                name: "Card".into(),
                align: 1,
                fill: Color::rgb(0x1a, 0x9c, 0x8f),
                top: 4.0,
                clicks: 0,
            },
        };
        app.frame();
        app
    }

    fn frame(&mut self) -> PropertyGridResponse {
        let (state, shape) = (&mut self.state, &mut self.shape);
        let mut response = PropertyGridResponse::default();
        self.context.run(ROOT, &mut MonospaceMeasurer, |ui| {
            response = ui.property_grid("grid", state, PropertyGridOptions::new("Shape"), |grid| {
                grid.group("Layout", |grid| {
                    grid.default(100.0).number(
                        "Width",
                        &mut shape.width,
                        NumberOptions::default().range(0.0, 500.0).suffix(" px"),
                    );
                    grid.choice("Align", &mut shape.align, &["Start", "Center", "End"]);
                    grid.group("Margins", |grid| {
                        grid.number("Top", &mut shape.top, NumberOptions::default());
                    });
                });
                grid.group("Appearance", |grid| {
                    grid.default(true).bool("Visible", &mut shape.visible);
                    grid.text("Name", &mut shape.name);
                    grid.color("Fill", &mut shape.fill);
                    grid.readonly("Kind", "Rectangle");
                    grid.custom("Clicks", |ui, label| {
                        let clicked = ui.button_keyed("$value", format!("{label}: add")).clicked();
                        shape.clicks += u32::from(clicked);
                        clicked
                    });
                });
            });
        });
        response
    }

    fn find(&self, path: &[&str]) -> Option<NodeId> {
        let mut full = vec!["grid", "$rows"];
        full.extend(path);
        self.context.find(&full)
    }

    fn click(&mut self, id: NodeId) {
        let bounds = self.context.node(id).unwrap().bounds;
        self.context.event(Event::PointerMoved(Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }));
        self.context.event(Event::PointerDown);
        self.context.event(Event::PointerUp);
    }

    fn retype(&mut self, id: NodeId, text: &str) {
        self.context.action(id, Action::Focus);
        self.context.event(Event::Text(TextEdit::SelectAll));
        self.context
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
    let node = app.context.node(width).unwrap();
    assert_eq!(
        (node.semantics.role, node.semantics.label.as_str()),
        (Role::SpinButton, "Width")
    );
    let visible = app
        .find(&["group:Appearance", "row:Visible", "$editor", "$value"])
        .unwrap();
    let node = app.context.node(visible).unwrap();
    assert_eq!(
        (node.semantics.role, node.semantics.label.as_str()),
        (Role::Checkbox, "Visible")
    );
    assert_eq!(node.checked, Some(true));

    // The value column starts where the header's second pane does, at every depth.
    let header = app.context.find(&["grid", "$columns", "$second"]).unwrap();
    let header = app.context.node(header).unwrap().bounds.x;
    for path in [
        vec!["group:Layout", "row:Width", "$editor"],
        vec!["group:Layout", "group:Margins", "row:Top", "$editor"],
        vec!["group:Appearance", "row:Kind", "$editor"],
    ] {
        let editor = app.context.node(app.find(&path).unwrap()).unwrap().bounds;
        assert!(
            (editor.x - header).abs() < 0.5,
            "{path:?}: {} vs {header}",
            editor.x
        );
    }

    // A number commits on Enter and reports its path.
    app.retype(width, "250");
    assert!(!app.frame().any());
    app.context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
    assert_eq!(app.frame().changed, ["Layout/Width"]);
    assert_eq!(app.shape.width, 250.0);

    app.click(visible);
    assert_eq!(app.frame().changed, ["Appearance/Visible"]);
    assert!(!app.shape.visible);

    let name = app
        .find(&["group:Appearance", "row:Name", "$editor", "$value"])
        .unwrap();
    app.retype(name, "Badge");
    assert_eq!(app.frame().changed, ["Appearance/Name"]);
    assert_eq!(app.shape.name, "Badge");

    let custom = app
        .find(&["group:Appearance", "row:Clicks", "$editor", "$value"])
        .unwrap();
    app.click(custom);
    assert_eq!(app.frame().changed, ["Appearance/Clicks"]);
    assert_eq!(app.shape.clicks, 1);
}

#[test]
fn reset_restores_the_default_and_is_enabled_only_while_the_value_differs() {
    let mut app = App::new();
    let reset = app.find(&["group:Layout", "row:Width", "$reset"]).unwrap();
    let node = app.context.node(reset).unwrap();
    assert!(!node.semantics.disabled, "120 differs from the default 100");
    assert_eq!(node.description.as_deref(), Some("Reset Width"));
    app.click(reset);
    assert_eq!(app.frame().changed, ["Layout/Width"]);
    assert_eq!(app.shape.width, 100.0);
    app.frame();
    assert!(app.context.node(reset).unwrap().semantics.disabled);
    // Rows without a default have no button.
    assert!(app.find(&["group:Layout", "row:Align", "$reset"]).is_none());
    // Visible is at its default, so its button is there but switched off.
    let reset = app
        .find(&["group:Appearance", "row:Visible", "$reset"])
        .unwrap();
    assert!(app.context.node(reset).unwrap().semantics.disabled);
}

#[test]
fn colours_follow_whole_hex_values_and_drop_drafts_when_the_field_is_left() {
    let mut app = App::new();
    let hex = app
        .find(&["group:Appearance", "row:Fill", "$editor", "$value", "$hex"])
        .unwrap();
    assert_eq!(
        app.context.node(hex).unwrap().value.as_deref(),
        Some("#1a9c8f")
    );
    app.retype(hex, "#ff80");
    assert!(!app.frame().any());
    assert_eq!(app.shape.fill, Color::rgb(0x1a, 0x9c, 0x8f));
    assert_eq!(
        app.context.node(hex).unwrap().value.as_deref(),
        Some("#ff80")
    );
    app.context
        .event(Event::Text(TextEdit::Insert("00".into())));
    assert_eq!(app.frame().changed, ["Appearance/Fill"]);
    assert_eq!(app.shape.fill, Color::rgb(0xff, 0x80, 0x00));
    // Alpha is the fourth pair; a draft that is no colour goes when focus does.
    app.retype(hex, "FF800080");
    app.frame();
    assert_eq!(app.shape.fill, Color(0xff, 0x80, 0x00, 0x80));
    app.retype(hex, "orange");
    app.frame();
    app.context.event(Event::KeyDown {
        key: Key::Tab,
        shift: false,
        repeat: false,
    });
    app.frame();
    app.frame();
    assert_eq!(
        app.context.node(hex).unwrap().value.as_deref(),
        Some("#ff800080")
    );
}

#[test]
fn groups_collapse_and_the_filter_opens_matches_and_hides_the_rest() {
    let mut app = App::new();
    let header = app.find(&["group:Layout", "$header"]).unwrap();
    assert_eq!(
        app.context.node(header).unwrap().description.as_deref(),
        Some("Collapse Layout")
    );
    app.click(header);
    app.frame();
    app.frame();
    assert!(app.state.collapsed.contains("Layout"));
    assert!(app.find(&["group:Layout", "row:Width"]).is_none());
    assert!(app.find(&["group:Appearance", "row:Name"]).is_some());

    // A filter looks inside collapsed groups and hides groups with no match.
    app.state.filter = "top".into();
    app.frame();
    assert!(
        app.find(&["group:Layout", "group:Margins", "row:Top"])
            .is_some()
    );
    assert!(app.find(&["group:Layout", "row:Width"]).is_none());
    assert!(app.find(&["group:Appearance"]).is_none());
    // A group's own name shows all of its rows.
    app.state.filter = " APPEAR ".into();
    app.frame();
    assert!(app.find(&["group:Layout"]).is_none());
    for row in [
        "row:Visible",
        "row:Name",
        "row:Fill",
        "row:Kind",
        "row:Clicks",
    ] {
        assert!(app.find(&["group:Appearance", row]).is_some(), "{row}");
    }
    // Clearing the filter brings the collapsed state back.
    app.state.filter.clear();
    app.frame();
    assert!(app.find(&["group:Layout", "row:Width"]).is_none());
    assert!(app.find(&["group:Layout", "$header"]).is_some());
}

#[test]
fn the_header_divider_sizes_the_name_column() {
    let mut app = App::new();
    let divider = app.context.find(&["grid", "$columns", "$divider"]).unwrap();
    let editor = |app: &App| {
        let id = app.find(&["group:Layout", "row:Width", "$editor"]).unwrap();
        app.context.node(id).unwrap().bounds.x
    };
    let before = editor(&app);
    app.context.action(divider, Action::Focus);
    for _ in 0..5 {
        app.context.event(Event::KeyDown {
            key: Key::ArrowRight,
            shift: false,
            repeat: false,
        });
    }
    app.frame();
    app.frame();
    assert!(app.state.name_share > 0.4);
    assert!(editor(&app) > before + 5.0);
}

#[test]
#[should_panic(expected = "property `Width` holds a f64 but its default is a i32")]
fn a_default_of_the_wrong_type_is_a_programming_error_with_a_clear_message() {
    let mut context = Context::new();
    let mut state = PropertyGridState::default();
    let mut width = 1.0;
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        ui.property_grid("grid", &mut state, PropertyGridOptions::default(), |grid| {
            grid.default(100)
                .number("Width", &mut width, NumberOptions::default());
        });
    });
}
