use rust_gui_core::{Context, Event, Length, MonospaceMeasurer, Style, Ui};
use rust_gui_scene::{Point, Size};

const ROOT: Size = Size::new(400.0, 400.0);

#[derive(Default)]
struct App {
    /// What the memoized subtree is declared from.
    items: Vec<String>,
    /// Bumped by a button inside the subtree.
    clicks: usize,
    /// How often the content actually ran.
    declared: usize,
    /// What the last declaration reported.
    ran: bool,
    heading: String,
}

fn frame(context: &mut Context, app: &mut App) -> rust_gui_core::FrameStats {
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.label_keyed("heading", app.heading.clone());
        app.ran = ui.memo(
            "list",
            &(&app.items, app.clicks),
            Style::default().width(Length::Percent(1.0)),
            |ui| {
                app.declared += 1;
                for (index, item) in app.items.iter().enumerate() {
                    ui.label_keyed(format!("item{index}"), item.clone());
                }
                if ui
                    .button_styled("bump", "Bump", true, Style::button())
                    .clicked()
                {
                    app.clicks += 1;
                }
            },
        );
    })
}

fn labels(context: &Context) -> Vec<String> {
    context
        .snapshots()
        .into_iter()
        .map(|node| node.semantics.label)
        .filter(|label| !label.is_empty())
        .collect()
}

fn click(context: &mut Context, path: &[&str]) {
    let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
    let centre = Point {
        x: bounds.x + bounds.width / 2.0,
        y: bounds.y + bounds.height / 2.0,
    };
    context.event(Event::PointerMoved(centre));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
}

#[test]
fn an_unchanged_state_keeps_the_subtree_without_declaring_it() {
    let mut context = Context::new();
    let mut app = App {
        items: vec!["one".into(), "two".into()],
        ..App::default()
    };
    let stats = frame(&mut context, &mut app);
    assert!(app.ran && app.declared == 1 && stats.skipped == 0);
    let shown = labels(&context);
    assert!(shown.contains(&"one".to_owned()) && shown.contains(&"two".to_owned()));
    let scene = context.scene().clone();

    // The same state again: the content does not run and nothing changes.
    let stats = frame(&mut context, &mut app);
    assert!(!app.ran, "the content ran again");
    assert_eq!((app.declared, stats.skipped), (1, 1));
    assert_eq!(labels(&context), shown, "the subtree is still there");
    assert_eq!(context.scene(), &scene, "and paints the same");
    assert_eq!(stats.removed, 0, "nothing was taken for undeclared");

    // A different state declares it again.
    app.items.push("three".into());
    let stats = frame(&mut context, &mut app);
    assert!(app.ran && app.declared == 2 && stats.skipped == 0);
    assert!(labels(&context).contains(&"three".to_owned()));
}

#[test]
fn a_skipped_frame_still_repaints_and_lays_out_the_rest() {
    let mut context = Context::new();
    let mut app = App {
        items: vec!["one".into()],
        heading: "first".into(),
        ..App::default()
    };
    frame(&mut context, &mut app);
    let inside = context.find(&["list", "item0"]).unwrap();
    let before = context.node(inside).unwrap().bounds;

    // Only the heading changes, and it is longer, so the subtree moves down.
    app.heading = "first\nsecond\nthird".into();
    let stats = frame(&mut context, &mut app);
    assert_eq!((stats.skipped, app.declared), (1, 1));
    let after = context.node(inside).unwrap().bounds;
    assert!(after.y > before.y, "{before:?} then {after:?}");
    assert_eq!(
        context.find(&["list", "item0"]),
        Some(inside),
        "kept its id"
    );
}

#[test]
fn input_inside_a_skipped_subtree_still_reaches_its_widget() {
    let mut context = Context::new();
    let mut app = App {
        items: vec!["one".into()],
        ..App::default()
    };
    frame(&mut context, &mut app);
    // A frame with nothing to do is skipped.
    frame(&mut context, &mut app);
    assert_eq!(app.declared, 1);

    // The click belongs to a button that was not declared this frame: the
    // frame carrying it declares the subtree again so the button sees it.
    click(&mut context, &["list", "bump"]);
    let stats = frame(&mut context, &mut app);
    assert_eq!((app.clicks, app.declared, stats.skipped), (1, 2, 0));
    // The click changed the state the subtree is declared from, which is only
    // seen on the frame after it, and the one after that is quiet again.
    let stats = frame(&mut context, &mut app);
    assert_eq!((app.clicks, app.declared, stats.skipped), (1, 3, 0));
    let stats = frame(&mut context, &mut app);
    assert_eq!((app.clicks, app.declared, stats.skipped), (1, 3, 1));
}

#[test]
fn memos_nest_and_hold_the_state_of_the_widgets_inside() {
    let (mut context, mut outer, mut inner) = (Context::new(), 0usize, 0usize);
    let mut typed = String::from("start");
    let mut runs = (0usize, 0usize);
    let declare = |context: &mut Context,
                   outer: usize,
                   inner: usize,
                   typed: &mut String,
                   runs: &mut (usize, usize)| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            ui.memo("outer", &outer, Style::default(), |ui| {
                runs.0 += 1;
                ui.text_input("field", "Field", typed);
                ui.memo("inner", &inner, Style::default(), |ui| {
                    runs.1 += 1;
                    ui.label_keyed("deep", "deep");
                });
            });
        })
    };
    declare(&mut context, outer, inner, &mut typed, &mut runs);
    let field = context.find(&["outer", "field"]).unwrap();
    let deep = context.find(&["outer", "inner", "deep"]).unwrap();
    assert_eq!(runs, (1, 1));

    // Typing reaches the field although its state did not change: the frame
    // that carries the edit declares the containers above it, but not the
    // inner one, which holds nothing that is waiting.
    context.action(field, rust_gui_core::Action::Focus);
    context.action(field, rust_gui_core::Action::SetText("edited".into()));
    let stats = declare(&mut context, outer, inner, &mut typed, &mut runs);
    assert_eq!(typed, "edited");
    assert_eq!((runs, stats.skipped), ((2, 1), 1));

    // Nothing waiting and nothing changed: the outer one is skipped, so the
    // inner one is not even reached.
    let stats = declare(&mut context, outer, inner, &mut typed, &mut runs);
    assert_eq!((runs, stats.skipped), ((2, 1), 1));
    assert_eq!(context.find(&["outer", "inner", "deep"]), Some(deep));

    // Changing one state declares that container alone.
    outer += 1;
    declare(&mut context, outer, inner, &mut typed, &mut runs);
    assert_eq!(runs, (3, 1), "the inner subtree was kept");
    inner += 1;
    outer += 1;
    let stats = declare(&mut context, outer, inner, &mut typed, &mut runs);
    assert_eq!((runs, stats.skipped), ((4, 2), 0));
    assert_eq!(
        context.find(&["outer", "field"]),
        Some(field),
        "kept its id"
    );
    assert_eq!(typed, "edited", "and what was typed into it");
}
