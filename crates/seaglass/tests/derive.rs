#![cfg(feature = "derive")]
use seaglass::{
    Button, Color, Effects, Event, Key, NodeId, Properties, PropertyGridOptions, PropertyGridState,
    PropertyMsg, Role, Size, TextEdit, Ui,
};
use seaglass_testing::Harness;

#[derive(Properties)]
struct Shadow {
    #[property(min = 0.0, max = 40.0, suffix = " px")]
    blur: f32,
    color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Top,
    Bottom,
}

/// An editor of its own for `#[property(with = ..)]`: a button that flips the
/// anchor, sending the new one.
fn edit_anchor<M: 'static>(
    ui: &mut Ui<'_, M>,
    label: &str,
    anchor: &Anchor,
    on: impl Fn(Anchor) -> M + 'static,
) {
    let next = match anchor {
        Anchor::Top => Anchor::Bottom,
        Anchor::Bottom => Anchor::Top,
    };
    ui.keyed("$value", |ui| {
        ui.button(Button::new(format!("{label}: {anchor:?}")).on_click(on(next)));
    });
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

#[derive(Clone, Debug, PartialEq)]
enum CardEvent {
    Grid(PropertyMsg),
    Card(CardMsg),
}

struct CardApp {
    state: PropertyGridState,
    card: Card,
    sent: Vec<CardMsg>,
}

impl seaglass::App for CardApp {
    type Message = CardEvent;

    fn update(&mut self, message: CardEvent, _effects: &mut Effects<'_, CardEvent>) {
        match message {
            CardEvent::Grid(grid) => self.state.apply(&grid),
            CardEvent::Card(card) => {
                self.sent.push(card.clone());
                self.card.apply(card);
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, CardEvent>) {
        ui.keyed("grid", |ui| {
            ui.property_grid(
                &self.state,
                PropertyGridOptions::new("Card"),
                CardEvent::Grid,
                |grid| self.card.properties(grid, CardEvent::Card),
            )
        });
    }
}

fn card_app() -> Harness<CardApp> {
    Harness::app(
        Size::new(640.0, 1000.0),
        CardApp {
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
            sent: Vec::new(),
        },
    )
}

fn find(app: &Harness<CardApp>, path: &[&str]) -> Option<NodeId> {
    let mut full = vec!["grid", "$rows"];
    full.extend(path);
    app.context().find(&full)
}

/// Retypes and commits a number field; the messages that sent.
fn commit(app: &mut Harness<CardApp>, id: NodeId, text: &str) -> Vec<CardMsg> {
    let before = app.state().sent.len();
    app.focus(id);
    app.event(Event::Text(TextEdit::SelectAll));
    app.type_text(text);
    app.press(Key::Enter);
    app.state().sent[before..].to_vec()
}

fn click(app: &mut Harness<CardApp>, id: NodeId) -> Vec<CardMsg> {
    let before = app.state().sent.len();
    app.click(id);
    app.state().sent[before..].to_vec()
}

#[test]
fn a_derived_struct_declares_grouped_typed_rows_bound_to_its_fields() {
    let mut app = card_app();

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
        assert!(find(&app, &path).is_some(), "{path:?}");
    }
    assert!(find(&app, &["row:Cache"]).is_none());

    // A float with its range, step, unit and default.
    let width = find(
        &app,
        &["group:Layout", "row:Width", "$editor", "$value", "$field"],
    )
    .unwrap();
    let node = app.context().node(width).unwrap();
    assert_eq!(node.semantics.role, Role::SpinButton);
    assert_eq!(node.value.as_deref(), Some("300 px"));
    let numeric = node.numeric.unwrap();
    assert_eq!(
        (numeric.min, numeric.max, numeric.step),
        (80.0, 420.0, 10.0)
    );
    assert_eq!(commit(&mut app, width, "1000"), [CardMsg::Width(420.0)]);
    assert_eq!(app.state().card.width, 420.0);
    let reset = find(&app, &["group:Layout", "row:Width", "$reset"]).unwrap();
    assert_eq!(click(&mut app, reset), [CardMsg::Width(260.0)]);
    assert_eq!(app.state().card.width, 260.0);

    // An integer keeps its type: whole steps, its own minimum, the given maximum.
    let radius = find(
        &app,
        &[
            "group:Layout",
            "row:Corner radius",
            "$editor",
            "$value",
            "$field",
        ],
    )
    .unwrap();
    let numeric = app.context().node(radius).unwrap().numeric.unwrap();
    assert_eq!((numeric.min, numeric.max, numeric.step), (0.0, 60.0, 1.0));
    commit(&mut app, radius, "12.7");
    assert_eq!(app.state().card.corner_radius, 13);
    commit(&mut app, radius, "-5");
    assert_eq!(app.state().card.corner_radius, 0);

    // A nested struct is a nested group whose message wraps its own.
    let blur = find(
        &app,
        &[
            "group:Layout",
            "group:Shadow",
            "row:Blur",
            "$editor",
            "$value",
            "$field",
        ],
    )
    .unwrap();
    assert_eq!(
        commit(&mut app, blur, "12"),
        [CardMsg::Shadow(ShadowMsg::Blur(12.0))]
    );
    assert_eq!(app.state().card.shadow.blur, 12.0);

    // Choices with a default, a read-only value and a custom editor.
    let reset = find(&app, &["group:Content", "row:Align", "$reset"]).unwrap();
    assert_eq!(click(&mut app, reset), [CardMsg::Align(1)]);
    assert_eq!(app.state().card.align, 1);
    let revision = find(&app, &["row:Revision", "$editor", "$value"]).unwrap();
    assert_eq!(app.context().node(revision).unwrap().semantics.label, "7");
    let anchor = find(&app, &["row:Anchor", "$editor", "$value"]).unwrap();
    assert_eq!(click(&mut app, anchor), [CardMsg::Anchor(Anchor::Bottom)]);
    assert_eq!(app.state().card.anchor, Anchor::Bottom);
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

impl seaglass::App for LampApp {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match &message {
            Msg::Grid(grid) => self.state.apply(grid),
            Msg::Lamp(lamp) => self.lamp.apply(lamp.clone()),
        }
        self.messages.push(message);
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.property_grid(
            &self.state,
            PropertyGridOptions::new("Lamp"),
            Msg::Grid,
            |grid| self.lamp.properties(grid, Msg::Lamp),
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
