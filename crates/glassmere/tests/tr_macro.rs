#![cfg(all(feature = "derive", feature = "fluent"))]
//! `tr!` is checked against `examples/locales/en.ftl`, named in this crate's
//! manifest, while this file compiles.
use glassmere::fluent::FluentMessages;
use glassmere::{Context, MonospaceMeasurer, Size, tr};

#[test]
fn checked_lookups_expand_to_ordinary_ones() {
    let mut context = Context::new();
    let mut messages = FluentMessages::new("cs", "en").unwrap();
    let locales = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/locales");
    assert_eq!(messages.load_dir(locales).unwrap(), 2);
    context.set_messages(Some(Box::new(messages)));
    let mut shown = Vec::new();
    let files = ["a", "b", "c", "d", "e"];
    context.run(Size::new(300.0, 200.0), &mut MonospaceMeasurer, |ui| {
        shown.push(tr!(ui, "title"));
        // Any expression that converts into a message argument.
        shown.push(tr!(ui, "files", count = files.len()));
        shown.push(tr!(ui, "saved", name = "plan.txt",));
        // The framework's own keys are known without a catalog.
        shown.push(tr!(ui, "glassmere-grid-hide", name = "Age"));
    });
    assert_eq!(
        shown,
        [
            "Lokalizace",
            "Vybráno 5 souborů",
            "Uloženo: plan.txt",
            "Skrýt sloupec Age"
        ]
    );
    assert!(context.missing_messages().is_empty());
}
