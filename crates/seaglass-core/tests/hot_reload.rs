mod support;

use seaglass_core::{App, AppDriver, Context, Effects, MonospaceMeasurer, Ui};
use seaglass_scene::Size;
use std::path::Path;

const ROOT: Size = Size::new(400.0, 300.0);

use support::shows;

#[test]
fn watched_files_are_requested_delivered_reloaded_and_survive_errors() {
    let mut context = Context::new();
    let path = Path::new("assets/motd.txt");
    let view = |ui: &mut Ui<'_>| {
        let text = ui.watched_text("assets/motd.txt");
        ui.label(text.as_deref().unwrap_or("loading"));
        // Asking twice registers once.
        let _ = ui.watched_bytes("assets/motd.txt");
    };
    let run = |context: &mut Context| context.run(ROOT, &mut MonospaceMeasurer, view);

    run(&mut context);
    assert!(shows(&context, "loading"));
    assert_eq!(context.watched_paths(), [path.to_path_buf()]);
    assert!(!context.watched_known(path));

    // The host delivers the content; the next declaration shows it.
    assert!(context.set_watched_file(path, Ok(b"hello".to_vec())));
    assert!(context.watched_known(path));
    run(&mut context);
    assert!(shows(&context, "hello"));
    // The same content again is not a change.
    assert!(!context.set_watched_file(path, Ok(b"hello".to_vec())));
    assert!(context.set_watched_file(path, Ok(b"edited".to_vec())));
    run(&mut context);
    assert!(shows(&context, "edited"));

    // A failed reload keeps the last good content and reports once.
    assert!(context.set_watched_file(path, Err("locked".into())));
    assert!(!context.set_watched_file(path, Err("locked".into())));
    assert_eq!(context.watch_error(path), Some("locked"));
    run(&mut context);
    assert!(shows(&context, "edited"));
    assert!(context.set_watched_file(path, Ok(b"back".to_vec())));
    assert_eq!(context.watch_error(path), None);

    // Bytes that are not UTF-8 are bytes only.
    assert!(context.set_watched_file(path, Ok(vec![0xff, 0xfe])));
    let mut bytes = None;
    context.run(ROOT, &mut MonospaceMeasurer, |ui| {
        assert!(ui.watched_text("assets/motd.txt").is_none());
        bytes = ui.watched_bytes("assets/motd.txt");
    });
    assert_eq!(bytes.as_deref(), Some(&[0xff, 0xfe][..]));

    // A view that stops asking stops being watched.
    context.run(ROOT, &mut MonospaceMeasurer, |ui| ui.label("nothing"));
    assert!(context.watched_paths().is_empty());
}

struct Counter {
    count: i32,
}

impl App for Counter {
    type Message = ();

    fn update(&mut self, (): (), _effects: &mut Effects<'_, ()>) {
        self.count += 1;
    }

    fn view(&self, ui: &mut Ui<'_, ()>) {
        ui.label(format!("count {}", self.count));
    }

    fn save_state(&self) -> Option<String> {
        Some(format!("count {}", self.count))
    }
}

#[test]
fn hot_state_is_collected_only_when_the_host_asks_for_it() {
    let mut context = Context::new();
    let mut app = Counter { count: 41 };
    let mut driver = AppDriver::new(|| true);
    let frame = |context: &mut Context, driver: &mut AppDriver<()>, app: &mut Counter| {
        driver.frame(app, context, ROOT, &mut MonospaceMeasurer);
    };
    frame(&mut context, &mut driver, &mut app);
    assert_eq!(context.take_hot_state(), None);
    context.request_hot_save();
    driver.send(());
    frame(&mut context, &mut driver, &mut app);
    assert_eq!(context.take_hot_state().as_deref(), Some("count 42"));
    assert_eq!(context.take_hot_state(), None);
    // The request is spent, and an idle frame asks nothing more of the app.
    frame(&mut context, &mut driver, &mut app);
    assert_eq!(context.take_hot_state(), None);
}
