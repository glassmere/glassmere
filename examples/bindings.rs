//! A form whose fields are bound to the state: no message variant per
//! field and no arm in `update` for it. Only what has consequences beyond
//! the field, here Submit, is a message with a name.
use glassmere::app::{Action, bind, change};
use glassmere::{App, AppOptions, Button, Effects, Label, NumberOptions, Style, Ui, run};

#[derive(Clone)]
enum Msg {
    /// A change to the form, from a bound field or an unnamed button.
    Set(Action<Form>),
    Submit,
}

impl From<Action<Form>> for Msg {
    fn from(change: Action<Form>) -> Self {
        Self::Set(change)
    }
}

#[derive(Default)]
struct Form {
    name: String,
    email: String,
    seats: f64,
    newsletter: bool,
    submitted: Option<String>,
}

impl App for Form {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Set(change) => change(self),
            Msg::Submit => {
                self.submitted = Some(format!(
                    "{} <{}>, {} seat(s){}",
                    self.name,
                    self.email,
                    self.seats,
                    if self.newsletter { ", newsletter" } else { "" }
                ));
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.column(Style::default().padding(24.0).gap(12.0), |ui| {
            ui.label("Name");
            ui.text_input(
                "Name",
                &self.name,
                bind(|form: &mut Self, name| form.name = name),
            );
            ui.label("Email");
            ui.text_input(
                "Email",
                &self.email,
                bind(|form: &mut Self, email| form.email = email),
            );
            ui.label("Seats");
            ui.number_input(
                "Seats",
                self.seats,
                NumberOptions::default().range(1.0, 8.0),
                bind(|form: &mut Self, seats| form.seats = seats),
            );
            ui.checkbox(
                "Send me the newsletter",
                self.newsletter,
                bind(|form: &mut Self, on| form.newsletter = on),
            );
            ui.row(Style::default().gap(8.0), |ui| {
                ui.button(Button::new("Submit").on_click(Msg::Submit));
                ui.button(Button::new("Clear").on_click(change(|form: &mut Self| {
                    *form = Form {
                        seats: 1.0,
                        ..Form::default()
                    };
                })));
            });
            if let Some(submitted) = &self.submitted {
                ui.label(Label::new(format!("Submitted: {submitted}")));
            }
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(
        AppOptions {
            title: "Bindings".into(),
            ..AppOptions::default()
        },
        Form {
            seats: 1.0,
            ..Form::default()
        },
    )?;
    Ok(())
}
