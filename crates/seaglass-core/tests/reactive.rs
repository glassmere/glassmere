use seaglass_core::{Context, Length, MonospaceMeasurer, StateKind, Store, Style, Ui};
use seaglass_scene::Size;

const ROOT: Size = Size::new(400.0, 300.0);

/// A store a view is declared from: a temperature, the same in another unit,
/// and a sentence about it. Nothing here is the framework's business until
/// the view says so.
#[test]
fn a_view_declared_from_a_store_shows_its_graph_to_the_inspector() {
    let store = Store::new();
    let celsius = store.signal("celsius", 20i32);
    let read = celsius.clone();
    let fahrenheit = store.memo("fahrenheit", move || read.get() * 9 / 5 + 32);
    let both = fahrenheit.clone();
    let caption = store.memo("caption", move || format!("{} F", both.get()));
    let mut context = Context::new();
    let declare = |context: &mut Context| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            ui.observe(&store);
            ui.keyed("caption", |ui| ui.label(caption.get()));
        });
    };

    declare(&mut context);
    let graph = context.state_graph().to_vec();
    let names: Vec<&str> = graph.iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["celsius", "fahrenheit", "caption"]);
    assert_eq!(graph[0].kind, StateKind::Signal);
    assert_eq!(graph[1].inputs, [0]);
    assert_eq!(graph[2].inputs, [1]);
    assert!(graph.iter().all(|node| !node.dirty));

    // The change reaches the far end of the chain, and the view says so.
    celsius.set(100);
    declare(&mut context);
    let graph = context.state_graph();
    assert_eq!(graph[0].changed_at, store.version());
    assert!(graph.iter().all(|node| !node.dirty), "the view read them");
    assert!(context.snapshots().iter().any(|node| {
        context
            .details(node.id)
            .is_some_and(|details| details.text == "212 F")
    }));
}

/// The store's version is hashable state, so a subtree can be declared again
/// only when something it was declared from has moved.
#[test]
fn a_memoized_subtree_follows_the_store_version() {
    let store = Store::new();
    let items = store.signal("items", vec!["one".to_owned()]);
    let unrelated = store.signal("unrelated", 0u32);
    let mut declared = 0;
    let mut context = Context::new();
    let frame = |context: &mut Context, declared: &mut usize| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            ui.observe(&store);
            ui.keyed("list", |ui| {
                ui.memo(
                    &store.version(),
                    Style::default().width(Length::Percent(1.0)),
                    |ui| {
                        *declared += 1;
                        items.with(|items| {
                            for (index, item) in items.iter().enumerate() {
                                ui.keyed(format!("item{index}"), |ui| ui.label(item.clone()));
                            }
                        });
                    },
                )
            });
        });
    };

    frame(&mut context, &mut declared);
    frame(&mut context, &mut declared);
    assert_eq!(declared, 1, "nothing moved, so nothing was declared again");
    items.update(|items| items.push("two".to_owned()));
    frame(&mut context, &mut declared);
    assert_eq!(declared, 2);
    // The store's version is the whole store: a change anywhere in it
    // declares the subtree again, which is the price of one number.
    unrelated.set(1);
    frame(&mut context, &mut declared);
    assert_eq!(declared, 3);
    // Reading one value's version instead is the narrower bargain.
    let version = items.version();
    unrelated.set(2);
    assert_eq!(items.version(), version);
}
