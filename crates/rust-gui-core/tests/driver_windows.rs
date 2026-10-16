//! Where an effect goes when an app has several windows, and what queues.
use rust_gui_core::{
    App, AppDriver, Button, Clip, Context, Effects, Event, FileRequest, MonospaceMeasurer, Pacing,
    Toast, Ui, WindowCommand, WindowId,
};
use rust_gui_scene::{Point, Size};
use std::path::PathBuf;

const ROOT: Size = Size::new(400.0, 300.0);
const MAIN: WindowId = WindowId::MAIN;
const SECOND: WindowId = WindowId(1);

#[derive(Clone)]
enum Win {
    /// The button of a window: a toast, a window command and a focus request.
    Go,
    /// A toast raised by a message that came from nowhere in particular.
    Plain,
    /// A toast aimed at a window.
    Aim(WindowId),
    /// A command aimed at a window.
    Command(WindowId),
    TwoPastes,
    TwoDialogs,
    Pasted(&'static str),
    Chosen(&'static str),
}

#[derive(Default)]
struct Wins {
    log: Vec<&'static str>,
}

impl App for Wins {
    type Message = Win;

    fn update(&mut self, message: Win, effects: &mut Effects<'_, Win>) {
        match message {
            Win::Go => {
                effects.toast(Toast::new("from input"));
                effects.window_command(WindowCommand::Minimize);
                effects.request_focus("go");
            }
            Win::Plain => effects.toast(Toast::new("plain")),
            Win::Aim(window) => {
                effects.in_window(window).toast(Toast::new("aimed"));
            }
            Win::Command(window) => {
                effects
                    .in_window(window)
                    .window_command(WindowCommand::Close);
            }
            Win::TwoPastes => {
                effects.paste(|_| Win::Pasted("first"));
                effects.paste(|_| Win::Pasted("second"));
            }
            Win::TwoDialogs => {
                let one = FileRequest {
                    title: "one".into(),
                    ..FileRequest::default()
                };
                let two = FileRequest {
                    title: "two".into(),
                    ..FileRequest::default()
                };
                effects.choose_files(one, |_| Win::Chosen("first"));
                effects.choose_files(two, |_| Win::Chosen("second"));
            }
            Win::Pasted(name) | Win::Chosen(name) => self.log.push(name),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Win>) {
        ui.keyed("go", |ui| ui.button(Button::new("go").on_click(Win::Go)));
    }
}

struct Rig {
    app: Wins,
    driver: AppDriver<Win>,
    windows: Vec<(WindowId, Context, Pacing)>,
}

impl Rig {
    fn new() -> Self {
        let mut rig = Self {
            app: Wins::default(),
            driver: AppDriver::new(|| true),
            windows: vec![
                (MAIN, Context::new(), Pacing::default()),
                (SECOND, Context::new(), Pacing::default()),
            ],
        };
        rig.frame(MAIN);
        rig.frame(SECOND);
        rig
    }

    fn context(&mut self, window: WindowId) -> &mut Context {
        &mut self.windows.iter_mut().find(|w| w.0 == window).unwrap().1
    }

    /// One window's batch and declaration, as a host runs them.
    fn frame(&mut self, window: WindowId) {
        let index = self.windows.iter().position(|w| w.0 == window).unwrap();
        let context = &mut self.windows[index].1;
        let (messages, dirty) = self.driver.batch_in(window, &mut self.app, context);
        if dirty {
            for (_, _, pacing) in &mut self.windows {
                pacing.invalidate();
            }
        }
        let (_, context, pacing) = &mut self.windows[index];
        let view = (window != MAIN).then_some(window);
        self.driver.declare(
            &self.app,
            context,
            ROOT,
            &mut MonospaceMeasurer,
            view,
            pacing,
            messages,
        );
    }

    fn click_go(&mut self, window: WindowId) {
        let context = self.context(window);
        let bounds = context.node(context.find(&["go"]).unwrap()).unwrap().bounds;
        context.event(Event::PointerMoved(Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }));
        context.event(Event::PointerDown);
        context.event(Event::PointerUp);
    }

    fn toasts(&mut self, window: WindowId) -> Vec<String> {
        self.context(window)
            .toasts()
            .map(|(_, toast)| toast.text.clone())
            .collect()
    }
}

#[test]
fn an_effect_goes_to_the_window_whose_input_sent_the_message() {
    let mut rig = Rig::new();
    rig.click_go(SECOND);
    rig.frame(SECOND);
    assert_eq!(rig.toasts(SECOND), ["from input"]);
    assert!(rig.toasts(MAIN).is_empty());
    assert_eq!(
        rig.context(SECOND).take_window_commands(),
        [WindowCommand::Minimize]
    );
    rig.frame(MAIN);
    assert!(rig.toasts(MAIN).is_empty());
    assert!(rig.context(MAIN).take_window_commands().is_empty());
}

#[test]
fn an_effect_of_a_sent_message_goes_to_the_main_window_whoever_redraws_first() {
    let mut rig = Rig::new();
    rig.driver.send(Win::Plain);
    rig.frame(SECOND);
    assert!(rig.toasts(SECOND).is_empty());
    assert_eq!(rig.driver.awaiting(), [MAIN]);
    rig.frame(MAIN);
    assert_eq!(rig.toasts(MAIN), ["plain"]);
    assert!(rig.toasts(SECOND).is_empty());
}

#[test]
fn a_focus_request_reaches_only_the_window_that_asked() {
    let mut rig = Rig::new();
    rig.click_go(SECOND);
    rig.frame(SECOND);
    let go = rig.context(SECOND).find(&["go"]);
    assert!(go.is_some());
    assert_eq!(rig.context(SECOND).focused(), go);
    rig.frame(MAIN);
    assert_eq!(rig.context(MAIN).focused(), None);
}

#[test]
fn in_window_aims_an_effect_at_one_window() {
    let mut rig = Rig::new();
    rig.driver.send(Win::Aim(SECOND));
    rig.driver.send(Win::Command(SECOND));
    rig.frame(MAIN);
    assert!(rig.toasts(MAIN).is_empty());
    assert!(rig.context(MAIN).take_window_commands().is_empty());
    assert_eq!(rig.driver.awaiting(), [SECOND]);
    rig.frame(SECOND);
    assert_eq!(rig.toasts(SECOND), ["aimed"]);
    assert_eq!(
        rig.context(SECOND).take_window_commands(),
        [WindowCommand::Close]
    );
    assert!(rig.driver.awaiting().is_empty());
}

#[test]
fn what_is_aimed_at_a_window_that_closed_is_dropped() {
    let mut rig = Rig::new();
    rig.driver.send(Win::Aim(WindowId(9)));
    rig.frame(MAIN);
    assert_eq!(rig.driver.awaiting(), [WindowId(9)]);
    rig.driver.window_closed(&mut rig.app, WindowId(9));
    assert!(rig.driver.awaiting().is_empty());
}

#[test]
fn two_pastes_in_one_batch_both_arrive_in_order() {
    let mut rig = Rig::new();
    rig.driver.send(Win::TwoPastes);
    rig.frame(MAIN);
    assert!(rig.context(MAIN).take_paste_request());
    rig.context(MAIN).set_clip(Clip::default());
    rig.frame(MAIN);
    assert_eq!(rig.app.log, ["first", "second"]);
}

#[test]
fn two_file_dialogs_in_one_batch_are_put_up_one_after_the_other() {
    let mut rig = Rig::new();
    rig.driver.send(Win::TwoDialogs);
    rig.frame(MAIN);
    let first = rig.context(MAIN).take_file_request().unwrap();
    assert_eq!(first.title, "one");
    assert!(rig.context(MAIN).take_file_request().is_none());
    rig.context(MAIN).set_chosen_files(vec![PathBuf::from("a")]);
    rig.frame(MAIN);
    assert_eq!(rig.app.log, ["first"]);
    let second = rig.context(MAIN).take_file_request().unwrap();
    assert_eq!(second.title, "two");
    rig.context(MAIN).set_chosen_files(Vec::new());
    rig.frame(MAIN);
    assert_eq!(rig.app.log, ["first", "second"]);
}
