//! Optional reactive state. Ordinary Rust state is still the way to write an
//! application here; a store earns its place when a value is derived from
//! several others and read in several places. Press F12 and open the State
//! tab to see what each derived value read and which one moved last.
use seaglass::{
    App, AppOptions, Color, Effects, Label, Length, Memo, Signal, Size, Store, Style, TextWrap, Ui,
    run,
};

#[derive(Clone)]
enum Msg {
    Quantity(f64),
    Price(f64),
    Discount(bool),
    /// Sent once at startup under `--smoke`: move one value and say what came out.
    Smoke,
}

struct Pricing {
    store: Store,
    quantity: Signal<f64>,
    price: Signal<f64>,
    discount: Signal<bool>,
    subtotal: Memo<f64>,
    tax: Memo<f64>,
    total: Memo<f64>,
    smoke: bool,
}

impl Pricing {
    fn new(smoke: bool) -> Self {
        let store = Store::new();
        let quantity = store.signal("quantity", 3.0f64);
        let price = store.signal("price", 12.5f64);
        let discount = store.signal("discount", false);
        let (count, each, cut) = (quantity.clone(), price.clone(), discount.clone());
        // Derived once, read three times, recomputed only when one of the three
        // above has moved. Nothing below says which.
        let subtotal = store.memo("subtotal", move || {
            let full = count.get() * each.get();
            if cut.get() { full * 0.9 } else { full }
        });
        let taxed = subtotal.clone();
        let tax = store.memo("tax", move || taxed.get() * 0.2);
        let (net, duty) = (subtotal.clone(), tax.clone());
        let total = store.memo("total", move || net.get() + duty.get());
        Self {
            store,
            quantity,
            price,
            discount,
            subtotal,
            tax,
            total,
            smoke,
        }
    }
}

impl App for Pricing {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        if !self.smoke {
            return;
        }
        // The message is queued before the first frame, so the smoke run
        // changes a value through `update` like any other input.
        let start = effects.channel(1, |()| Msg::Smoke);
        let _ = start.try_send(());
    }

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Quantity(quantity) => self.quantity.set(quantity),
            Msg::Price(price) => self.price.set(price),
            Msg::Discount(discount) => self.discount.set(discount),
            Msg::Smoke => {
                self.quantity.set(10.0);
                println!("TOTAL {:.2}", self.total.get());
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.observe(&self.store);
        let page = Style {
            padding: 24.0,
            gap: 14.0,
            width: Length::Percent(1.0),
            ..Style::default()
        };
        ui.column(page, |ui| {
            ui.label(Label::new("Reactive state").style(Style {
                font_size: Some(28.0),
                font_weight: Some(700),
                ..Style::default()
            }));
            ui.label(
                Label::new(
                    "A subtotal, its tax and the total are derived from the three values below. \
                 Press F12 and open the State tab to see the graph.",
                )
                .style(Style {
                    width: Length::Percent(1.0),
                    wrap: TextWrap::Word,
                    ..Style::default()
                }),
            );
            ui.slider(
                "Quantity",
                self.quantity.get(),
                1.0..=20.0,
                1.0,
                Msg::Quantity,
            );
            ui.slider("Price each", self.price.get(), 1.0..=50.0, 0.5, Msg::Price);
            ui.checkbox("Ten percent off", self.discount.get(), Msg::Discount);
            for (key, label, value) in [
                ("subtotal", "Subtotal", self.subtotal.get()),
                ("tax", "Tax", self.tax.get()),
                ("total", "Total", self.total.get()),
            ] {
                ui.keyed(key, |ui| {
                    ui.label(Label::new(format!("{label}: {value:.2}")).style(Style {
                        foreground: Some(Color::rgb(103, 232, 206)),
                        font_size: Some(if key == "total" { 20.0 } else { 15.0 }),
                        ..Style::default()
                    }))
                });
            }
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Seaglass / Reactive state".into(),
            size: Size::new(620.0, 460.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        Pricing::new(smoke),
    )
}
