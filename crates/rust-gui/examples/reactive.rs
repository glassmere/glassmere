//! Optional reactive state. Ordinary Rust state is still the way to write an
//! application here; a store earns its place when a value is derived from
//! several others and read in several places. Press F12 and open the State
//! tab to see what each derived value read and which one moved last.
use rust_gui::{AppOptions, Color, Label, Length, Size, Store, Style, TextWrap, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
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

    let mut frames = 0;
    run(
        AppOptions {
            title: "Rust GUI / Reactive state".into(),
            size: Size::new(620.0, 460.0),
            frame_limit: smoke.then_some(20),
            ..AppOptions::default()
        },
        move |ui| {
            ui.observe(&store);
            if smoke {
                ui.request_redraw();
                frames += 1;
                // Move one value and say what came out, so an unattended run
                // leaves evidence that the chain recomputed.
                if frames == 3 {
                    quantity.set(10.0);
                }
                if frames == 5 {
                    println!("TOTAL {:.2}", total.get());
                }
            }
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
                ui.label(Label::new("A subtotal, its tax and the total are derived from the three values below. \
                     Press F12 and open the State tab to see the graph.").style(Style {
                        width: Length::Percent(1.0),
                        wrap: TextWrap::Word,
                        ..Style::default()
                    }));
                let mut count = quantity.get();
                if ui.slider("Quantity", &mut count, 1.0..=20.0, 1.0)
                    .changed
                {
                    quantity.set(count);
                }
                let mut each = price.get();
                if ui.slider("Price each", &mut each, 1.0..=50.0, 0.5)
                    .changed
                {
                    price.set(each);
                }
                let mut cut = discount.get();
                if ui.checkbox("Ten percent off", &mut cut).changed {
                    discount.set(cut);
                }
                for (key, label, value) in [
                    ("subtotal", "Subtotal", subtotal.get()),
                    ("tax", "Tax", tax.get()),
                    ("total", "Total", total.get()),
                ] {
                    ui.keyed(key, |ui| ui.label(Label::new(format!("{label}: {value:.2}")).style(Style {
                            foreground: Some(Color::rgb(103, 232, 206)),
                            font_size: Some(if key == "total" { 20.0 } else { 15.0 }),
                            ..Style::default()
                        })));
                }
            });
        },
    )
}
