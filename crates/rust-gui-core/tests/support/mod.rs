//! Drives a state through the message model the way a host does, for the
//! kernel's own tests: widgets send [`Action`]s on the state, and each
//! [`Driven::frame`] applies what arrived, then declares at most once.
//!
//! A test that used to read `clicked()` or a `&mut` binding inside the view
//! puts what it observes in the state instead, and asserts on
//! [`Driven::state`] after the frame.
//!
//! An action that must also ask the framework for something (the clipboard,
//! a theme, a window) is made with [`act_fx`], or sent with
//! [`Driven::effects`]. The windows the app opens are kept like a host keeps
//! them, each with a context of its own: see [`Driven::window`].
#![allow(dead_code)]

use rust_gui_core::app::Action;
use rust_gui_core::{
    App, AppDriver, Context, Effects, FrameStats, MonospaceMeasurer, Pacing, Ui, WindowId,
    WindowRequest,
};
use rust_gui_scene::Size;
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// What a test's view is: a function of the state that declares widgets
/// sending actions on it.
pub type View<S> = Box<dyn Fn(&S, &mut Ui<'_, Action<S>>)>;

/// What a test's view of a second window is.
pub type WindowView<S> = Box<dyn Fn(&S, WindowId, &mut Ui<'_, Action<S>>)>;

/// What a test does when the user closes a second window.
pub type OnClosed<S> = Box<dyn Fn(&mut S, WindowId, &mut Effects<'_, Action<S>>)>;

/// A change to the state that may also ask the framework for something.
type WithEffects<S> = Box<dyn FnOnce(&mut S, &mut Effects<'_, Action<S>>)>;

thread_local! {
    /// What actions made by [`act_fx`] asked for while they ran, erased
    /// because the state type is the caller's. Taken by the same `update`.
    static EFFECTS: RefCell<Vec<Box<dyn Any>>> = const { RefCell::new(Vec::new()) };
}

struct Closures<S: 'static> {
    state: S,
    view: View<S>,
    window_view: Option<WindowView<S>>,
    on_closed: Option<OnClosed<S>>,
}

impl<S: 'static> App for Closures<S> {
    type Message = Action<S>;

    fn update(&mut self, action: Action<S>, effects: &mut Effects<'_, Action<S>>) {
        action(&mut self.state);
        let queued = EFFECTS.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
        for f in queued {
            if let Ok(f) = f.downcast::<WithEffects<S>>() {
                f(&mut self.state, effects);
            }
        }
    }

    fn view(&self, ui: &mut Ui<'_, Action<S>>) {
        (self.view)(&self.state, ui);
    }

    fn view_window(&self, window: WindowId, ui: &mut Ui<'_, Action<S>>) {
        match &self.window_view {
            Some(view) => view(&self.state, window, ui),
            None => self.view(ui),
        }
    }

    fn window_closed(&mut self, window: WindowId, effects: &mut Effects<'_, Action<S>>) {
        if let Some(on_closed) = &self.on_closed {
            on_closed(&mut self.state, window, effects);
        }
    }
}

/// A window the app opened with `Effects::open_window`, which the driver
/// keeps the way a host does: its own context, size and pacing.
pub struct Opened {
    pub request: WindowRequest,
    pub context: Context,
    pub size: Size,
    pacing: Pacing,
}

pub struct Driven<S: 'static> {
    app: Closures<S>,
    driver: AppDriver<Action<S>>,
    pub context: Context,
    pub size: Size,
    main: Pacing,
    /// The windows `Effects::open_window` opened, until they are closed.
    windows: HashMap<WindowId, Opened>,
}

impl<S: 'static> Driven<S> {
    /// Declares the first frame.
    pub fn new(size: Size, state: S, view: impl Fn(&S, &mut Ui<'_, Action<S>>) + 'static) -> Self {
        let mut driven = Self {
            app: Closures {
                state,
                view: Box::new(view),
                window_view: None,
                on_closed: None,
            },
            driver: AppDriver::new(|| true),
            context: Context::new(),
            size,
            main: Pacing::default(),
            windows: HashMap::new(),
        };
        driven.frame();
        driven
    }

    /// One batch of every window: the events queued on each context since the
    /// last frame become actions, they are applied in order, and each view
    /// declares if it is due. Returns the main window's stats.
    pub fn frame(&mut self) -> FrameStats {
        let (main_messages, mut dirty) = self.driver.batch(&mut self.app, &mut self.context);
        let mut counts = Vec::new();
        for (id, window) in &mut self.windows {
            let (messages, changed) = self.driver.batch(&mut self.app, &mut window.context);
            counts.push((*id, messages));
            dirty |= changed;
        }
        self.carry_out_windows();
        if dirty {
            self.invalidate_all();
        }
        let stats = self.driver.declare(
            &self.app,
            &mut self.context,
            self.size,
            &mut MonospaceMeasurer,
            None,
            &mut self.main,
            main_messages,
        );
        for (id, messages) in counts {
            if let Some(window) = self.windows.get_mut(&id) {
                self.driver.declare(
                    &self.app,
                    &mut window.context,
                    window.size,
                    &mut MonospaceMeasurer,
                    Some(id),
                    &mut window.pacing,
                    messages,
                );
            }
        }
        stats
    }

    /// Closes the windows the last batches asked to close and opens the ones
    /// they asked for, each declaring its first frame as a host's does when
    /// the OS shows it.
    fn carry_out_windows(&mut self) {
        let (opened, closed) = self.driver.take_windows();
        for id in closed {
            self.windows.remove(&id);
        }
        for (id, request) in opened {
            let size = request.size.unwrap_or(Size::new(400.0, 300.0));
            let mut window = Opened {
                request,
                context: Context::new(),
                size,
                pacing: Pacing::default(),
            };
            self.driver.declare(
                &self.app,
                &mut window.context,
                size,
                &mut MonospaceMeasurer,
                Some(id),
                &mut window.pacing,
                0,
            );
            self.windows.insert(id, window);
        }
    }

    fn invalidate_all(&mut self) {
        self.main.invalidate();
        for window in self.windows.values_mut() {
            window.pacing.invalidate();
        }
    }

    /// Replaces the view; the next frame declares it.
    pub fn set_view(&mut self, view: impl Fn(&S, &mut Ui<'_, Action<S>>) + 'static) {
        self.app.view = Box::new(view);
        self.invalidate_all();
    }

    /// The view of the windows `Effects::open_window` opens; without one they
    /// declare the main view.
    pub fn set_window_view(
        &mut self,
        view: impl Fn(&S, WindowId, &mut Ui<'_, Action<S>>) + 'static,
    ) {
        self.app.window_view = Some(Box::new(view));
        self.invalidate_all();
    }

    /// What `App::window_closed` does, in terms of the state.
    pub fn on_window_closed(
        &mut self,
        f: impl Fn(&mut S, WindowId, &mut Effects<'_, Action<S>>) + 'static,
    ) {
        self.app.on_closed = Some(Box::new(f));
    }

    /// Sends an action as if a widget had, applied by the next frame.
    pub fn send(&mut self, action: impl Fn(&mut S) + 'static) {
        self.driver.send(Rc::new(action));
    }

    /// Sends a change that also asks the framework for something, applied by
    /// the next frame: what an `update` does with its `Effects`.
    pub fn effects(&mut self, f: impl Fn(&mut S, &mut Effects<'_, Action<S>>) + 'static) {
        self.driver.send(act_fx(f));
    }

    pub fn state(&self) -> &S {
        &self.app.state
    }

    /// The state, to change from outside; the next frame declares.
    pub fn state_mut(&mut self) -> &mut S {
        self.invalidate_all();
        &mut self.app.state
    }

    /// The windows `Effects::open_window` opened and nobody closed.
    pub fn window_ids(&self) -> Vec<WindowId> {
        let mut ids: Vec<_> = self.windows.keys().copied().collect();
        ids.sort_by_key(|id| id.0);
        ids
    }

    /// One of those windows, to drive its context.
    pub fn window(&mut self, id: WindowId) -> &mut Opened {
        self.windows.get_mut(&id).expect("the window is open")
    }

    /// The user (or the OS) closes a window: the app is told, and answers.
    pub fn close_window_from_outside(&mut self, id: WindowId) {
        assert!(self.windows.remove(&id).is_some(), "the window is open");
        self.driver.window_closed(&mut self.app, id);
        self.carry_out_windows();
        self.invalidate_all();
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

/// An action that also asks the framework for something, for `on_click`:
/// the clipboard, a theme, a window, the rest of [`Effects`].
pub fn act_fx<S: 'static>(f: impl Fn(&mut S, &mut Effects<'_, Action<S>>) + 'static) -> Action<S> {
    let f = Rc::new(f);
    Rc::new(move |_| {
        let f = f.clone();
        let queued: WithEffects<S> = Box::new(move |state, effects| f(state, effects));
        EFFECTS.with(|queue| queue.borrow_mut().push(Box::new(queued)));
    })
}
