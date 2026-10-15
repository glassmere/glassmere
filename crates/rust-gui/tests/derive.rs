#![cfg(feature = "derive")]
use rust_gui::{
    Action, Color, Context, Effects, Event, Key, MonospaceMeasurer, NodeId, Point, Properties,
    PropertyGridOptions, PropertyGridResponse, PropertyGridState, PropertyMsg, Role, Size,
    TextEdit, Ui,
};
use rust_gui_testing::Harness;

#[derive(Properties)]
struct Shadow {
    #[property(min = 0.0, max = 40.0, suffix = " px")]
    blur: f32,
    color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    Top,
    Bottom,
}

fn edit_anchor(ui: &mut Ui<'_>, label: &str, anchor: &mut Anchor) -> bool {
    let clicked = ui
        .keyed("$value", |ui| ui.button(format!("{label}: {anchor:?}")))
        .clicked();
    if clicked {
        *anchor = match anchor {
            Anchor::Top => Anchor::Bottom,
            Anchor::Bottom => Anchor::Top,
        };
    }
    clicked
}

#[derive(Properties)]
struct Card {
    #[property(group = "Content")]
    title: String,
    #[property(choices("Start", "Center", "End"), default = 1)]
    align: usize,
    #[property(
        group = "Layout",
        min = 80.0,
        max = 420.0,
        step = 10.0,
        suffix = " px",
        default = 260.0
    )]
    width: f64,
    #[property(name = "Corner radius", max = 60)]
    corner_radius: u8,
    #[property(flatten)]
    shadow: Shadow,
    #[property(group = "", default = true)]
    visible: bool,
    #[property(readonly)]
    revision: u32,
    #[property(with = edit_anchor)]
    anchor: Anchor,
    #[property(skip)]
    #[allow(dead_code)]
    cache: Vec<u8>,
}

struct App {
    context: Context,
    state: PropertyGridState,
    card: Card,
}

impl App {
    fn frame(&mut self) -> PropertyGridResponse {
        let (state, card) = (&mut self.state, &mut self.card);
        let mut response = PropertyGridResponse::default();
        self.context
            .run(Size::new(640.0, 1000.0), &mut MonospaceMeasurer, |ui| {
                response = ui.keyed("grid", |ui| {
                    ui.property_grid(state, PropertyGridOptions::new("Card"), |grid| {
                        card.properties(grid);
                    })
                });
            });
        response
    }

    fn find(&self, path: &[&str]) -> Option<NodeId> {
        let mut full = vec!["grid", "$rows"];
        full.extend(path);
        self.context.find(&full)
    }

    fn commit(&mut self, id: NodeId, text: &str) -> PropertyGridResponse {
        self.context.action(id, Action::Focus);
        self.context.event(Event::Text(TextEdit::SelectAll));
        self.context
            .event(Event::Text(TextEdit::Insert(text.into())));
        self.context.event(Event::KeyDown {
            key: Key::Enter,
            shift: false,
            repeat: false,
        });
        self.frame()
    }

    fn click(&mut self, id: NodeId) -> PropertyGridResponse {
        let bounds = self.context.node(id).unwrap().bounds;
        self.context.event(Event::PointerMoved(Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }));
        self.context.event(Event::PointerDown);
        self.context.event(Event::PointerUp);
        self.frame()
    }
}

#[test]
fn a_derived_struct_declares_grouped_typed_rows_bound_to_its_fields() {
    let mut app = App {
        context: Context::new(),
        state: PropertyGridState::default(),
        card: Card {
            title: "Hello".into(),
            align: 0,
            width: 300.0,
            corner_radius: 16,
            shadow: Shadow {
                blur: 8.0,
                color: Color::rgb(0, 0, 0),
            },
            visible: true,
            revision: 7,
            anchor: Anchor::Top,
            cache: Vec::new(),
        },
    };
    app.frame();

    // `group` stays in force for the following fields; names come from the fields.
    for path in [
        vec!["group:Content", "row:Title"],
        vec!["group:Content", "row:Align"],
        vec!["group:Layout", "row:Width"],
        vec!["group:Layout", "row:Corner radius"],
        vec!["group:Layout", "group:Shadow", "row:Blur"],
        vec!["group:Layout", "group:Shadow", "row:Color"],
        vec!["row:Visible"],
        vec!["row:Revision"],
        vec!["row:Anchor"],
    ] {
        assert!(app.find(&path).is_some(), "{path:?}");
    }
    assert!(app.find(&["row:Cache"]).is_none());

    // A float with its range, step, unit and default.
    let width = app
        .find(&["group:Layout", "row:Width", "$editor", "$value", "$field"])
        .unwrap();
    let node = app.context.node(width).unwrap();
    assert_eq!(node.semantics.role, Role::SpinButton);
    assert_eq!(node.value.as_deref(), Some("300 px"));
    let numeric = node.numeric.unwrap();
    assert_eq!(
        (numeric.min, numeric.max, numeric.step),
        (80.0, 420.0, 10.0)
    );
    assert_eq!(app.commit(width, "1000").changed, ["Layout/Width"]);
    assert_eq!(app.card.width, 420.0);
    let reset = app.find(&["group:Layout", "row:Width", "$reset"]).unwrap();
    app.click(reset);
    assert_eq!(app.card.width, 260.0);

    // An integer keeps its type: whole steps, its own minimum, the given maximum.
    let radius = app
        .find(&[
            "group:Layout",
            "row:Corner radius",
            "$editor",
            "$value",
            "$field",
        ])
        .unwrap();
    let numeric = app.context.node(radius).unwrap().numeric.unwrap();
    assert_eq!((numeric.min, numeric.max, numeric.step), (0.0, 60.0, 1.0));
    app.commit(radius, "12.7");
    assert_eq!(app.card.corner_radius, 13);
    app.commit(radius, "-5");
    assert_eq!(app.card.corner_radius, 0);

    // A nested struct is a nested group and reports nested paths.
    let blur = app
        .find(&[
            "group:Layout",
            "group:Shadow",
            "row:Blur",
            "$editor",
            "$value",
            "$field",
        ])
        .unwrap();
    assert_eq!(app.commit(blur, "12").changed, ["Layout/Shadow/Blur"]);
    assert_eq!(app.card.shadow.blur, 12.0);

    // Choices with a default, a read-only value and a custom editor.
    let reset = app.find(&["group:Content", "row:Align", "$reset"]).unwrap();
    assert_eq!(app.click(reset).changed, ["Content/Align"]);
    assert_eq!(app.card.align, 1);
    let revision = app.find(&["row:Revision", "$editor", "$value"]).unwrap();
    assert_eq!(app.context.node(revision).unwrap().semantics.label, "7");
    let anchor = app.find(&["row:Anchor", "$editor", "$value"]).unwrap();
    assert_eq!(app.click(anchor).changed, ["Anchor"]);
    assert_eq!(app.card.anchor, Anchor::Bottom);
}

#[derive(Properties)]
#[allow(dead_code)]
struct Lamp {
    #[property(group = "Light", min = 0.0, max = 10.0, step = 0.5)]
    brightness: f32,
    color: Color,
    #[property(choices("Warm", "Cool"))]
    tone: usize,
    #[property(name = "Label")]
    text: String,
    visible: bool,
    #[property(readonly)]
    id: u32,
    #[property(skip)]
    cache: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Grid(PropertyMsg),
    Lamp(LampMsg),
}

struct LampApp {
    state: PropertyGridState,
    lamp: Lamp,
    messages: Vec<Msg>,
}

impl rust_gui::App for LampApp {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match &message {
            Msg::Grid(grid) => self.state.apply(grid),
            Msg::Lamp(lamp) => self.lamp.apply(lamp.clone()),
        }
        self.messages.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.property_grid_msg(
            &self.state,
            PropertyGridOptions::new("Lamp"),
            Msg::Grid,
            |grid| self.lamp.properties_msg(grid, Msg::Lamp),
        );
    }
}

fn lamp_app() -> Harness<LampApp> {
    Harness::app(
        Size::new(640.0, 1000.0),
        LampApp {
            state: PropertyGridState::default(),
            lamp: Lamp {
                brightness: 2.0,
                color: Color::rgb(255, 255, 255),
                tone: 0,
                text: "Desk".into(),
                visible: true,
                id: 9,
                cache: Vec::new(),
            },
            messages: Vec::new(),
        },
    )
}

/// The first node with this key whose path contains `within`.
fn find_keyed_node(harness: &Harness<LampApp>, key: &str, within: &str) -> NodeId {
    let context = harness.context();
    let mut pending = vec![context.root()];
    while let Some(id) = pending.pop() {
        if context
            .details(id)
            .is_some_and(|details| details.key == key)
            && harness.path(id).contains(within)
        {
            return id;
        }
        if let Some(node) = context.node(id) {
            pending.extend(node.children);
        }
    }
    panic!("no node keyed {key:?}");
}

#[test]
fn a_derived_number_edit_sends_its_message_and_apply_stores_it() {
    let mut harness = lamp_app();
    let field = find_keyed_node(&harness, "$field", "row:Brightness");
    harness.focus(field);
    // The field shows "2.0" with the caret at the end; select it so the draft is "4".
    harness.event(Event::Text(TextEdit::SelectAll));
    harness.type_text("4");
    let stats = harness.press(Key::Enter);
    assert_eq!(stats.messages, 1);
    assert_eq!(
        harness.state().messages.last(),
        Some(&Msg::Lamp(LampMsg::Brightness(4.0)))
    );
    assert_eq!(harness.state().lamp.brightness, 4.0);
}

#[test]
fn a_derived_checkbox_click_sends_the_opposite_value() {
    let mut harness = lamp_app();
    let visible = harness.state().lamp.visible;
    let checkbox = find_keyed_node(&harness, "$value", "row:Visible");
    let stats = harness.click(checkbox);
    assert_eq!(stats.messages, 1);
    assert_eq!(
        harness.state().messages.last(),
        Some(&Msg::Lamp(LampMsg::Visible(!visible)))
    );
    assert_eq!(harness.state().lamp.visible, !visible);
}
