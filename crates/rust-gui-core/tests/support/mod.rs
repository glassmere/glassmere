//! Drives a state through the message model the way a host does, for the
//! kernel's own tests: widgets send [`Action`]s on the state, and each
//! [`Driven::frame`] applies what arrived, then declares at most once.
//!
//! A test that used to read `clicked()` or a `&mut` binding inside the view
//! puts what it observes in the state instead, and asserts on
//! [`Driven::state`] after the frame.
#![allow(dead_code)]

use rust_gui_core::app::Action;
use rust_gui_core::{App, AppDriver, Context, Effects, FrameStats, MonospaceMeasurer, Ui};
use rust_gui_scene::Size;
use std::rc::Rc;

/// What a test's view is: a function of the state that declares widgets
/// sending actions on it.
pub type View<S> = Box<dyn Fn(&S, &mut Ui<'_, Action<S>>)>;

struct Closures<S: 'static> {
    state: S,
    view: View<S>,
}

impl<S: 'static> App for Closures<S> {
    type Message = Action<S>;

    fn update(&mut self, action: Action<S>, _effects: &mut Effects<'_, Action<S>>) {
        action(&mut self.state);
    }

    fn view(&self, ui: &mut Ui<'_, Action<S>>) {
        (self.view)(&self.state, ui);
    }
}

pub struct Driven<S: 'static> {
    app: Closures<S>,
    driver: AppDriver<Action<S>>,
    pub context: Context,
    pub size: Size,
}

impl<S: 'static> Driven<S> {
    /// Declares the first frame.
    pub fn new(size: Size, state: S, view: impl Fn(&S, &mut Ui<'_, Action<S>>) + 'static) -> Self {
        let mut driven = Self {
            app: Closures {
                state,
                view: Box::new(view),
            },
            driver: AppDriver::new(|| true),
            context: Context::new(),
            size,
        };
        driven.frame();
        driven
    }

    /// One batch: the events queued on `context` since the last frame become
    /// actions, they are applied in order, and the view declares if it is due.
    pub fn frame(&mut self) -> FrameStats {
        self.driver.frame(
            &mut self.app,
            &mut self.context,
            self.size,
            &mut MonospaceMeasurer,
        )
    }

    /// Replaces the view; the next frame declares it.
    pub fn set_view(&mut self, view: impl Fn(&S, &mut Ui<'_, Action<S>>) + 'static) {
        self.app.view = Box::new(view);
        self.driver.invalidate();
    }

    /// Sends an action as if a widget had, applied by the next frame.
    pub fn send(&mut self, action: impl Fn(&mut S) + 'static) {
        self.driver.send(Rc::new(action));
    }

    pub fn state(&self) -> &S {
        &self.app.state
    }

    /// The state, to change from outside; the next frame declares.
    pub fn state_mut(&mut self) -> &mut S {
        self.driver.invalidate();
        &mut self.app.state
    }
}

/// An action that sets one field: `on(|s: &mut State, v| s.name = v)` for a
/// widget's `impl Fn(T) -> Action<S>` parameter.
pub fn on<S: 'static, T: Clone + 'static>(
    set: impl Fn(&mut S, T) + Clone + 'static,
) -> impl Fn(T) -> Action<S> + 'static {
    move |value: T| {
        let set = set.clone();
        Rc::new(move |state: &mut S| set(state, value.clone())) as Action<S>
    }
}

/// An action that does one thing, for `on_click`.
pub fn act<S: 'static>(f: impl Fn(&mut S) + 'static) -> Action<S> {
    Rc::new(f)
}
