//! What `Effects` asks of the driver across windows, tasks and dialogs.
use rust_gui_core::{App, AppDriver, Context, Effects, MonospaceMeasurer, Ui};
use rust_gui_scene::Size;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const ROOT: Size = Size::new(400.0, 300.0);

#[derive(Clone, Debug)]
enum Msg {
    /// Starts a task under this key; it answers `Done`.
    Task(String),
    Done(u32),
    /// Reports `Effects::running` through a shared reference.
    Probe(String),
}

#[derive(Default)]
struct Tasks {
    done: u32,
    probes: Vec<bool>,
    guard: Rc<RefCell<()>>,
}

fn peek(effects: &Effects<'_, Msg>, key: &str) -> bool {
    effects.running(key)
}

impl App for Tasks {
    type Message = Msg;

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Task(key) => {
                let guard = self.guard.clone();
                effects.spawn(
                    key,
                    |out| {
                        let _ = out.send(1);
                    },
                    move |value| {
                        let _held = &guard;
                        Msg::Done(value)
                    },
                );
            }
            Msg::Done(count) => self.done += count,
            Msg::Probe(key) => self.probes.push(peek(effects, &key)),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.label(format!("Done {}", self.done));
    }
}

#[test]
fn running_needs_no_mutable_effects() {
    let mut app = Tasks::default();
    let mut context = Context::new();
    let mut driver = AppDriver::new(|| true);
    driver.send(Msg::Probe("none".into()));
    driver.frame(&mut app, &mut context, ROOT, &mut MonospaceMeasurer);
    assert_eq!(app.probes, [false]);
}

#[test]
fn a_finished_and_delivered_task_leaves_nothing_behind() {
    let mut app = Tasks::default();
    let mut context = Context::new();
    let (wake, woken) = mpsc::channel();
    let mut driver = AppDriver::new(move || wake.send(()).is_ok());
    for key in 0..20 {
        driver.send(Msg::Task(format!("job {key}")));
    }
    driver.frame(&mut app, &mut context, ROOT, &mut MonospaceMeasurer);
    let started = Instant::now();
    while app.done < 20 {
        assert!(started.elapsed() < Duration::from_secs(10), "tasks stalled");
        let _ = woken.recv_timeout(Duration::from_millis(50));
        driver.frame(&mut app, &mut context, ROOT, &mut MonospaceMeasurer);
    }
    // A last batch lets every job's end be seen and forgotten.
    while Rc::strong_count(&app.guard) > 1 {
        assert!(started.elapsed() < Duration::from_secs(10), "entries kept");
        let _ = woken.recv_timeout(Duration::from_millis(50));
        driver.frame(&mut app, &mut context, ROOT, &mut MonospaceMeasurer);
    }
}
