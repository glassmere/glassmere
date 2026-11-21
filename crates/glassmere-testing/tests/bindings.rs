//! Fields bound to the state: a widget writes the value it reports without
//! a message variant or an arm in `update` of its own.
use glassmere_core::app::{Action, bind, change};
use glassmere_core::{App, Button, Component, Effects, NumberOptions, Style, Ui};
use glassmere_scene::Size;
use glassmere_testing::Harness;

#[derive(Clone)]
enum Msg {
    /// Every bound field and every change that needs no name.
    Set(Action<Form>),
    /// What has consequences stays a message.
    Submit,
    Address(AddressMsg),
}

impl From<Action<Form>> for Msg {
    fn from(change: Action<Form>) -> Self {
        Self::Set(change)
    }
}

#[derive(Default)]
struct Form {
    name: String,
    subscribed: bool,
    seats: f64,
    resets: u32,
    submitted: Vec<String>,
    address: Address,
}

impl App for Form {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Set(change) => change(self),
            Msg::Submit => self.submitted.push(self.name.clone()),
            Msg::Address(message) => self.address.update(message, &mut effects.map(Msg::Address)),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.text_input(
            "Name",
            &self.name,
            bind(|form: &mut Self, name| form.name = name),
        );
        ui.checkbox(
            "Subscribe",
            self.subscribed,
            bind(|form: &mut Self, on| form.subscribed = on),
        );
        ui.number_input(
            "Seats",
            self.seats,
            NumberOptions::default(),
            bind(|form: &mut Self, seats| form.seats = seats),
        );
        ui.button(Button::new("Reset").on_click(change(|form: &mut Self| {
            form.name.clear();
            form.resets += 1;
        })));
        ui.button(Button::new("Submit").on_click(Msg::Submit));
        ui.component(Style::default(), &self.address, Msg::Address);
    }
}

/// A component binds its own fields the same way, through its own message.
#[derive(Clone)]
struct AddressMsg(Action<Address>);

impl From<Action<Address>> for AddressMsg {
    fn from(change: Action<Address>) -> Self {
        Self(change)
    }
}

#[derive(Default)]
struct Address {
    city: String,
}

impl Component for Address {
    type Message = AddressMsg;

    fn update(&mut self, message: AddressMsg, _: &mut Effects<'_, AddressMsg>) {
        (message.0)(self);
    }

    fn view(&self, ui: &mut Ui<'_, AddressMsg>) {
        ui.text_input(
            "City",
            &self.city,
            bind(|address: &mut Self, city| address.city = city),
        );
    }
}

#[test]
fn bound_fields_write_the_state_without_messages_of_their_own() {
    let mut harness = Harness::app(Size::new(600.0, 600.0), Form::default());
    let name = harness.by_label("Name");
    harness.focus(name);
    harness.type_text("Ada");
    assert_eq!(harness.state().name, "Ada");

    harness.click(harness.by_label("Subscribe"));
    assert!(harness.state().subscribed);
    harness.click(harness.by_label("Subscribe"));
    assert!(!harness.state().subscribed);

    let seats = harness.by_label("Seats");
    harness.focus(seats);
    harness.press(glassmere_core::Key::ArrowUp);
    harness.press(glassmere_core::Key::ArrowUp);
    assert_eq!(harness.state().seats, 2.0);

    // A named message beside them reads what they wrote.
    harness.click(harness.by_label("Submit"));
    assert_eq!(harness.state().submitted, ["Ada"]);

    // A change that needs no name, on a button.
    harness.click(harness.by_label("Reset"));
    assert_eq!(
        (harness.state().name.as_str(), harness.state().resets),
        ("", 1)
    );
    harness.click(harness.by_label("Reset"));
    assert_eq!(harness.state().resets, 2);

    // A component's bound field changes the component, through the parent.
    let city = harness.by_label("City");
    harness.focus(city);
    harness.type_text("Brno");
    assert_eq!(harness.state().address.city, "Brno");
    assert_eq!(harness.state().name, "");
}

#[test]
fn a_bound_field_works_where_the_message_is_the_change_itself() {
    // `run_with` applications send an `Action` as it is.
    let on_name: Box<dyn Fn(String) -> Action<Form>> =
        Box::new(bind(|form: &mut Form, name| form.name = name));
    let mut form = Form::default();
    on_name("Grace".into())(&mut form);
    assert_eq!(form.name, "Grace");
    let reset: Action<Form> = change(|form: &mut Form| form.name.clear());
    reset(&mut form);
    assert_eq!(form.name, "");
}
