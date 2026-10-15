//! Two windows of one application. `Effects::open_window` opens the second,
//! `App::view_window` declares it, and both windows share one `update` and
//! one message queue. `--smoke` opens the second window, waits until it has
//! been declared, sends a message from a worker thread, closes the window and
//! exits; it fails unless each of those happened.

use rust_gui::{App, AppOptions, Button, Effects, Size, Ui, WindowId, WindowRequest, run_app};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Msg {
    Bump,
    CloseSecond,
}

/// Counts what the smoke run checks, which the view and `update` update.
#[derive(Clone, Default)]
struct Checks {
    second_views: Arc<AtomicUsize>,
    bumps: Arc<AtomicUsize>,
    closes: Arc<AtomicBool>,
}

struct Windows {
    count: u32,
    second: Option<WindowId>,
    smoke: bool,
    checks: Checks,
}

impl Windows {
    fn close_second(&mut self, effects: &mut Effects<'_, Msg>) {
        if let Some(window) = self.second.take() {
            effects.close_window(window);
            self.checks.closes.store(true, Ordering::Release);
        }
    }
}

impl App for Windows {
    type Message = Msg;

    fn init(&mut self, effects: &mut Effects<'_, Msg>) {
        if !self.smoke {
            return;
        }
        self.second = Some(effects.open_window(WindowRequest {
            title: "Second window".into(),
            size: Some(Size::new(360.0, 200.0)),
            modal_owner: None,
        }));
        // The worker waits until the second window has been declared, then
        // sends the message the smoke run is about.
        let declared = self.checks.second_views.clone();
        let _ = effects.spawn(
            "smoke",
            move |sender| {
                let started = Instant::now();
                while declared.load(Ordering::Acquire) == 0
                    && started.elapsed() < Duration::from_secs(5)
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let _ = sender.send(Msg::Bump);
            },
            |message| message,
        );
    }

    fn update(&mut self, message: Msg, effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Bump => {
                self.count += 1;
                self.checks.bumps.fetch_add(1, Ordering::AcqRel);
                if self.smoke {
                    self.close_second(effects);
                }
            }
            Msg::CloseSecond => self.close_second(effects),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.label(format!("Main window: count {}", self.count));
        ui.button(Button::new("Count").on_click(Msg::Bump));
        ui.button(Button::new("Close second window").on_click(Msg::CloseSecond));
    }

    fn view_window(&self, _window: WindowId, ui: &mut Ui<'_, Msg>) {
        self.checks.second_views.fetch_add(1, Ordering::AcqRel);
        ui.label(format!("Second window: count {}", self.count));
        ui.button(Button::new("Count").on_click(Msg::Bump));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let checks = Checks::default();
    run_app(
        AppOptions {
            title: "Rust GUI / Windows".into(),
            size: Size::new(480.0, 320.0),
            frame_limit: smoke.then_some(300),
            ..AppOptions::default()
        },
        Windows {
            count: 0,
            second: None,
            smoke,
            checks: checks.clone(),
        },
    )?;
    if !smoke {
        return Ok(());
    }
    let views = checks.second_views.load(Ordering::Acquire);
    let bumps = checks.bumps.load(Ordering::Acquire);
    let closed = checks.closes.load(Ordering::Acquire);
    println!("WINDOWS_SMOKE second_views={views} bumps={bumps} closed={closed}");
    if views == 0 || bumps == 0 || !closed {
        return Err("windows smoke: the second window or its message did not happen".into());
    }
    Ok(())
}
