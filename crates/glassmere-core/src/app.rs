//! Messages, an update, and one view per frame.
//!
//! An [`App`] holds the state, `update` changes it in answer to a message,
//! and `view` declares what it looks like. Widgets send messages instead of
//! reporting what happened through their `Response`, so the view reads state
//! that is already up to date and runs at most once per batch of input.

use crate::controls::ControlUpdate;
use crate::{
    Cancel, Clip, Context, FileRequest, FrameStats, LocaleFormat, MessageSender, Messages,
    Notification, NumberOptions, Shortcut, TaskSender, Tasks, TextMeasurer, Theme, Toast,
    ToastAction, ToastId, Ui, UiEvent, WindowCommand, WindowId, WindowRequest,
};
use glassmere_scene::Size;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// An application: its state, how a message changes it, and what it looks
/// like. `view` reads `&self` and declares; only `update` changes the state.
pub trait App: 'static {
    type Message: Clone + 'static;
    /// Once, before the first view: open channels, spawn startup work.
    fn init(&mut self, _effects: &mut Effects<'_, Self::Message>) {}
    fn update(&mut self, message: Self::Message, effects: &mut Effects<'_, Self::Message>);
    fn view(&self, ui: &mut Ui<'_, Self::Message>);
    /// What `glassmere-dev` keeps across a rebuild.
    fn save_state(&self) -> Option<String> {
        None
    }
    /// A window opened with `Effects::open_window` was closed by the user or
    /// the OS (or went with its owner). Not called for a window `update`
    /// closed with `Effects::close_window`, which the app already knows of.
    fn window_closed(&mut self, _window: WindowId, _effects: &mut Effects<'_, Self::Message>) {}
    /// A window opened with `Effects::open_window`; the main window uses `view`.
    fn view_window(&self, _window: WindowId, ui: &mut Ui<'_, Self::Message>) {
        self.view(ui)
    }
}

/// A part of an application with a message of its own. The parent declares
/// it with [`Ui::component`], which turns its messages into the parent's,
/// and forwards them back in its own `update`, giving it
/// [`Effects::map`] for what it asks of the framework.
pub trait Component {
    type Message: Clone + 'static;
    fn update(&mut self, message: Self::Message, effects: &mut Effects<'_, Self::Message>);
    fn view(&self, ui: &mut Ui<'_, Self::Message>);
}

/// The message of [`run_with`](../../glassmere/fn.run_with.html): a change to
/// apply to the state. It costs one `Rc` per handler per declaration, so
/// applications of any size use an enum.
pub type Action<S> = Rc<dyn Fn(&mut S)>;

/// One message per operation a widget reports in one update, into `out`.
type Batch<M> = Box<dyn Fn(ControlUpdate, &mut Vec<M>)>;

/// What a widget sends when the user does something to it. Kept in a slot of
/// the node that declared it, never in the element, which is compared every
/// frame.
pub(crate) enum Handler<M> {
    /// Sent as it is: a click.
    Message(M),
    /// Built from the value the widget reports: an edit, a drag.
    Map(Box<dyn Fn(ControlUpdate) -> Option<M>>),
    /// Built from an event that reached the node.
    Event(Box<dyn Fn(UiEvent) -> Option<M>>),
    /// One message per operation the widget reports in one update: a batch
    /// of text edits, each sent in order.
    Each(Batch<M>),
    /// A text field, which reports both edits and Enter.
    Field {
        change: Option<Box<dyn Fn(ControlUpdate) -> Option<M>>>,
        submit: Option<M>,
    },
    /// An autocomplete field: edits and picks send messages, Enter without a
    /// highlighted suggestion sends `submit`.
    Autocomplete { change: Batch<M>, submit: Option<M> },
    /// A number field: a commit parses its draft in the context's locale and
    /// sends `on` of the number when that differs from the declared one.
    Number {
        on: Box<dyn Fn(f64) -> M>,
        options: Box<NumberOptions>,
    },
    /// A stepper button: steps the number field of its row by this many steps.
    Step(f64),
    /// Sent as it is, and the focus goes back to the text of the editor the
    /// button belongs to: a toolbar button.
    Refocus(M),
}

impl<M: 'static> Handler<M> {
    /// The same handler sending `f` of what it sent. A message is converted
    /// now; a function is wrapped, which allocates.
    pub(crate) fn map<N: 'static>(self, f: &Rc<dyn Fn(M) -> N>) -> Handler<N> {
        let wrap = |inner: Box<dyn Fn(ControlUpdate) -> Option<M>>| {
            let f = f.clone();
            Box::new(move |update| inner(update).map(|message| f(message)))
                as Box<dyn Fn(ControlUpdate) -> Option<N>>
        };
        match self {
            Self::Message(message) => Handler::Message(f(message)),
            Self::Map(inner) => Handler::Map(wrap(inner)),
            Self::Event(inner) => {
                let f = f.clone();
                Handler::Event(Box::new(move |event| {
                    inner(event).map(|message| f(message))
                }))
            }
            Self::Field { change, submit } => Handler::Field {
                change: change.map(wrap),
                submit: submit.map(|message| f(message)),
            },
            Self::Each(inner) => {
                let f = f.clone();
                Handler::Each(Box::new(move |update, out| {
                    let mut sent = Vec::new();
                    inner(update, &mut sent);
                    out.extend(sent.into_iter().map(|message| f(message)));
                }))
            }
            Self::Autocomplete { change, submit } => {
                let g = f.clone();
                Handler::Autocomplete {
                    change: Box::new(move |update, out| {
                        let mut sent = Vec::new();
                        change(update, &mut sent);
                        out.extend(sent.into_iter().map(|message| g(message)));
                    }),
                    submit: submit.map(|message| f(message)),
                }
            }
            Self::Number { on, options } => {
                let f = f.clone();
                Handler::Number {
                    on: Box::new(move |number| f(on(number))),
                    options,
                }
            }
            Self::Step(steps) => Handler::Step(steps),
            Self::Refocus(message) => Handler::Refocus(f(message)),
        }
    }
}

/// The handlers of one `App`, in slots that nodes name by index. The context
/// keeps it as one `Box<dyn Any>`, so that the kernel stays free of the
/// message type.
pub(crate) struct HandlerSlab<M> {
    pub slots: Vec<Option<Handler<M>>>,
    pub free: Vec<u32>,
    /// What the declaration in progress declared, in order; moved into the
    /// slots once reconciled. Its capacity is kept from frame to frame.
    pub frame: Vec<Handler<M>>,
    /// The node each of `frame` went to.
    pub targets: Vec<Option<crate::NodeId>>,
}

impl<M> Default for HandlerSlab<M> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            frame: Vec::new(),
            targets: Vec::new(),
        }
    }
}

/// Turns what a worker or a channel delivered, behind `&mut Option<T>`, into
/// a message. Erasing `T` this way lets `Effects::map` compose it without
/// knowing it.
type Lift<M> = Box<dyn Fn(&mut dyn Any) -> Option<M>>;

/// What a toast's action button sends, built from the action taken.
type OnToastAction<M> = Box<dyn Fn(ToastAction) -> M>;

/// What a paste or a file dialog sends once answered, and for which window.
type OnPaste<M> = (WindowId, Box<dyn Fn(Clip) -> M>);
type OnFiles<M> = (WindowId, Box<dyn Fn(Vec<PathBuf>) -> M>);

/// Delivered values waiting in one task table or channel.
trait Delivery {
    /// Up to `limit` values, each as `&mut Option<T>` with its task key.
    fn deliver(&mut self, limit: usize, out: &mut dyn FnMut(Option<&str>, &mut dyn Any));
    fn cancel(&mut self, _key: &str) {}
    fn running(&self, _key: &str) -> bool {
        false
    }
    /// Whether the table still holds the job under this key: running, or
    /// finished with results waiting.
    fn tracks(&self, _key: &str) -> bool {
        true
    }
    fn as_any(&mut self) -> &mut dyn Any;
}

impl<T: Send + 'static> Delivery for Tasks<String, T> {
    fn deliver(&mut self, limit: usize, out: &mut dyn FnMut(Option<&str>, &mut dyn Any)) {
        for (key, value) in self.drain(limit) {
            out(Some(&key), &mut Some(value));
        }
    }

    fn cancel(&mut self, key: &str) {
        Tasks::cancel(self, &key.to_owned());
    }

    fn running(&self, key: &str) -> bool {
        Tasks::running(self, &key.to_owned())
    }

    fn tracks(&self, key: &str) -> bool {
        Tasks::tracks(self, &key.to_owned())
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl<T: Send + 'static> Delivery for crate::Mailbox<T> {
    fn deliver(&mut self, limit: usize, out: &mut dyn FnMut(Option<&str>, &mut dyn Any)) {
        for value in self.drain(limit) {
            out(None, &mut Some(value));
        }
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// How many values a task table or a channel hands over per batch, and how
/// many may wait in one.
const DELIVERED_PER_BATCH: usize = 256;
const TASK_CAPACITY: usize = 1024;

/// What `update` asked for that does not depend on the message type.
pub(crate) struct Requests {
    /// The latest theme, messages and format `update` asked for, each with
    /// the number of times it was asked: every window's context applies it
    /// once, whichever window's batch ran the `update`.
    pub theme: (u64, Option<Theme>),
    pub messages: (u64, Option<Rc<dyn Messages>>),
    pub format: (u64, Option<Rc<dyn LocaleFormat>>),
    pub bindings: Vec<(String, Option<Shortcut>)>,
    pub clip: Option<Clip>,
    /// What is aimed at one window waits here until that window's batch
    /// applies it: the window whose input sent the message `update` is
    /// running on, the main window for a message from anywhere else, or the
    /// one `Effects::in_window` names.
    pub files: Vec<(WindowId, FileRequest)>,
    /// Windows with a file dialog asked for and not yet answered; the next
    /// request for one waits for the answer.
    pub file_busy: Vec<WindowId>,
    pub paste: Vec<WindowId>,
    pub focus: Vec<(WindowId, String)>,
    pub notifications: Vec<Notification>,
    pub window_commands: Vec<(WindowId, WindowCommand)>,
    /// Each toast with a number that ties it to its action.
    pub toasts: Vec<(WindowId, u64, Toast)>,
    /// The id each toast got in its window when it was raised.
    pub toast_ids: Vec<(u64, WindowId, ToastId)>,
    next_toast: u64,
    /// The window whose input sent the message `update` is running on.
    origin: WindowId,
    /// Windows `update` asked to open and to close, taken by the host.
    pub open_windows: Vec<(WindowId, WindowRequest)>,
    pub close_windows: Vec<WindowId>,
    next_window: u64,
    tasks: HashMap<TypeId, Box<dyn Delivery>>,
    wake: Arc<dyn Fn() -> bool + Send + Sync>,
}

/// Where `Effects` puts what it is asked, typed by the message it is asked
/// in: the driver's own, or a component's mapped onto it.
trait Sink<M> {
    fn requests(&mut self) -> &mut Requests;
    fn requests_ref(&self) -> &Requests;
    fn paste(&mut self, window: WindowId, message: Box<dyn Fn(Clip) -> M>);
    fn files(&mut self, window: WindowId, message: Box<dyn Fn(Vec<PathBuf>) -> M>);
    fn task(&mut self, kind: TypeId, key: String, lift: Lift<M>);
    fn channel(&mut self, mailbox: Box<dyn Delivery>, lift: Lift<M>);
    fn toast(&mut self, window: WindowId, toast: Toast, action: Option<OnToastAction<M>>);
}

/// Everything the driver keeps for `Effects`, typed by the app's message.
struct EffectState<M> {
    requests: Requests,
    /// What each window's pastes and file dialogs send, oldest first.
    paste: Vec<OnPaste<M>>,
    files: Vec<OnFiles<M>>,
    task_messages: HashMap<TypeId, HashMap<String, Lift<M>>>,
    channels: Vec<(Box<dyn Delivery>, Lift<M>)>,
    /// What the action of a toast of `requests.toasts` sends, by its number.
    toast_actions: HashMap<u64, OnToastAction<M>>,
    /// The actions of raised toasts, by window and id, until the toast leaves.
    toast_messages: HashMap<(WindowId, ToastId), OnToastAction<M>>,
}

impl<M> Sink<M> for EffectState<M> {
    fn requests(&mut self) -> &mut Requests {
        &mut self.requests
    }

    fn requests_ref(&self) -> &Requests {
        &self.requests
    }

    fn paste(&mut self, window: WindowId, message: Box<dyn Fn(Clip) -> M>) {
        self.paste.push((window, message));
    }

    fn files(&mut self, window: WindowId, message: Box<dyn Fn(Vec<PathBuf>) -> M>) {
        self.files.push((window, message));
    }

    fn task(&mut self, kind: TypeId, key: String, lift: Lift<M>) {
        self.task_messages
            .entry(kind)
            .or_default()
            .insert(key, lift);
    }

    fn channel(&mut self, mailbox: Box<dyn Delivery>, lift: Lift<M>) {
        self.channels.push((mailbox, lift));
    }

    fn toast(&mut self, window: WindowId, toast: Toast, action: Option<OnToastAction<M>>) {
        let number = self.requests.next_toast;
        self.requests.next_toast += 1;
        self.requests.toasts.push((window, number, toast));
        if let Some(action) = action {
            self.toast_actions.insert(number, action);
        }
    }
}

/// A component's sink: what it asks for in its own message, sent on in its
/// parent's.
struct Mapped<'a, N, M> {
    inner: &'a mut dyn Sink<M>,
    f: Rc<dyn Fn(N) -> M>,
}

impl<N: 'static, M: 'static> Sink<N> for Mapped<'_, N, M> {
    fn requests(&mut self) -> &mut Requests {
        self.inner.requests()
    }

    fn requests_ref(&self) -> &Requests {
        self.inner.requests_ref()
    }

    fn paste(&mut self, window: WindowId, message: Box<dyn Fn(Clip) -> N>) {
        let f = self.f.clone();
        self.inner
            .paste(window, Box::new(move |clip| f(message(clip))));
    }

    fn files(&mut self, window: WindowId, message: Box<dyn Fn(Vec<PathBuf>) -> N>) {
        let f = self.f.clone();
        self.inner
            .files(window, Box::new(move |paths| f(message(paths))));
    }

    fn task(&mut self, kind: TypeId, key: String, lift: Lift<N>) {
        let f = self.f.clone();
        self.inner
            .task(kind, key, Box::new(move |value| lift(value).map(|n| f(n))));
    }

    fn channel(&mut self, mailbox: Box<dyn Delivery>, lift: Lift<N>) {
        let f = self.f.clone();
        self.inner
            .channel(mailbox, Box::new(move |value| lift(value).map(|n| f(n))));
    }

    fn toast(&mut self, window: WindowId, toast: Toast, action: Option<OnToastAction<N>>) {
        let f = self.f.clone();
        let action =
            action.map(|message| Box::new(move |found| f(message(found))) as OnToastAction<M>);
        self.inner.toast(window, toast, action);
    }
}

enum SinkRef<'a, M> {
    Borrowed(&'a mut dyn Sink<M>),
    Owned(Box<dyn Sink<M> + 'a>),
}

/// What `init` and `update` ask of the framework. Applied after the batch's
/// updates and before the view.
pub struct Effects<'a, M> {
    sink: SinkRef<'a, M>,
    /// The window [`Effects::in_window`] named, for what is asked after it.
    window: Option<WindowId>,
}

impl<'a, M> Effects<'a, M> {
    fn new(sink: &'a mut dyn Sink<M>) -> Self {
        Self {
            sink: SinkRef::Borrowed(sink),
            window: None,
        }
    }
}

impl<M: 'static> Effects<'_, M> {
    fn sink_ref(&self) -> &dyn Sink<M> {
        match &self.sink {
            SinkRef::Borrowed(sink) => &**sink,
            SinkRef::Owned(sink) => &**sink,
        }
    }

    /// The window what is asked now is aimed at.
    fn target(&self) -> WindowId {
        self.window
            .unwrap_or_else(|| self.sink_ref().requests_ref().origin)
    }

    /// Aims what is asked next, on this `Effects`, at `window`: [`toast`],
    /// [`toast_with`], [`window_command`], [`request_focus`], [`paste`] and
    /// [`choose_files`] (whose dialog is parented to it). Without it each
    /// goes to the window whose input sent the message `update` is handling,
    /// or to the main window, `WindowId::MAIN`, for a message that came from a
    /// task, a channel or `AppDriver::send`. Theme, messages and format
    /// reach every window and are not aimed.
    ///
    /// [`toast`]: Effects::toast
    /// [`toast_with`]: Effects::toast_with
    /// [`window_command`]: Effects::window_command
    /// [`request_focus`]: Effects::request_focus
    /// [`paste`]: Effects::paste
    /// [`choose_files`]: Effects::choose_files
    pub fn in_window(&mut self, window: WindowId) -> &mut Self {
        self.window = Some(window);
        self
    }

    fn sink(&mut self) -> &mut dyn Sink<M> {
        match &mut self.sink {
            SinkRef::Borrowed(sink) => &mut **sink,
            SinkRef::Owned(sink) => &mut **sink,
        }
    }

    fn requests(&mut self) -> &mut Requests {
        self.sink().requests()
    }

    /// Switches design tokens; the view declares once, with the new theme.
    pub fn set_theme(&mut self, theme: Theme) {
        let requests = self.requests();
        requests.theme = (requests.theme.0 + 1, Some(theme));
    }

    /// Installs the source of localized text, or removes it with `None`.
    pub fn set_messages(&mut self, messages: Option<Box<dyn Messages>>) {
        let requests = self.requests();
        requests.messages = (requests.messages.0 + 1, messages.map(Rc::from));
    }

    /// Installs locale-aware formatting, or removes it with `None`.
    pub fn set_format(&mut self, format: Option<Box<dyn LocaleFormat>>) {
        let requests = self.requests();
        requests.format = (requests.format.0 + 1, format.map(Rc::from));
    }

    /// Binds a command to other keys; `None` unbinds it.
    pub fn set_binding(&mut self, id: impl Into<String>, shortcut: Option<Shortcut>) {
        self.requests().bindings.push((id.into(), shortcut));
    }

    /// Puts something on the clipboard.
    pub fn copy(&mut self, clip: Clip) {
        self.requests().clip = Some(clip);
    }

    /// Reads the clipboard; what is on it arrives as `message(clip)`.
    pub fn paste(&mut self, message: impl Fn(Clip) -> M + 'static) {
        let window = self.target();
        self.requests().paste.push(window);
        self.sink().paste(window, Box::new(message));
    }

    /// Puts up a file dialog; the paths chosen arrive as `message(paths)`,
    /// empty when it was cancelled.
    pub fn choose_files(
        &mut self,
        request: FileRequest,
        message: impl Fn(Vec<PathBuf>) -> M + 'static,
    ) {
        let window = self.target();
        self.requests().files.push((window, request));
        self.sink().files(window, Box::new(message));
    }

    /// Moves keyboard focus to the widget declared with this key, once the
    /// view has declared.
    pub fn request_focus(&mut self, key: impl Into<String>) {
        let window = self.target();
        self.requests().focus.push((window, key.into()));
    }

    /// Posts a message to the desktop's notification area.
    pub fn notify(&mut self, notification: Notification) {
        self.requests().notifications.push(notification);
    }

    /// Moves, resizes, minimizes, maximizes or closes the window.
    pub fn window_command(&mut self, command: WindowCommand) {
        let window = self.target();
        self.requests().window_commands.push((window, command));
    }

    /// Shows a short message in the bottom corner at the end of the reading
    /// direction. It leaves after its duration, which pauses while the
    /// pointer or the keyboard is on any toast. Four show at once and later
    /// ones wait. A toast equal to one already there restarts it instead of
    /// stacking. Its action button, if it has one, only closes it.
    pub fn toast(&mut self, toast: Toast) {
        let window = self.target();
        self.sink().toast(window, toast, None);
    }

    /// [`Effects::toast`] whose action button sends `message(action)` when
    /// clicked.
    pub fn toast_with(&mut self, toast: Toast, message: impl Fn(ToastAction) -> M + 'static) {
        let window = self.target();
        self.sink().toast(window, toast, Some(Box::new(message)));
    }

    /// Opens another window, which `App::view_window` declares. The identity
    /// is handed out now; the host opens the window after the batch.
    pub fn open_window(&mut self, request: WindowRequest) -> WindowId {
        let requests = self.requests();
        requests.next_window += 1;
        let id = WindowId(requests.next_window);
        requests.open_windows.push((id, request));
        id
    }

    /// Closes a window `open_window` opened. The host closes it after the batch.
    pub fn close_window(&mut self, window: WindowId) {
        self.requests().close_windows.push(window);
    }

    /// Runs `job` on a thread of its own. What it sends arrives as
    /// `message(value)`, in batches of its own; a newer job under the same
    /// key supersedes this one, and nothing a superseded or cancelled job
    /// sends arrives.
    pub fn spawn<T: Send + 'static>(
        &mut self,
        key: impl Into<String>,
        job: impl FnOnce(TaskSender<String, T>) + Send + 'static,
        message: impl Fn(T) -> M + 'static,
    ) -> Cancel {
        let key = key.into();
        let kind = TypeId::of::<T>();
        let requests = self.requests();
        let wake = requests.wake.clone();
        let tasks = requests
            .tasks
            .entry(kind)
            .or_insert_with(|| Box::new(Tasks::<String, T>::new(TASK_CAPACITY, move || wake())))
            .as_any()
            .downcast_mut::<Tasks<String, T>>()
            .expect("one task table per value type");
        let cancel = tasks.spawn(key.clone(), job);
        self.sink().task(
            kind,
            key,
            Box::new(move |value| value.downcast_mut::<Option<T>>()?.take().map(&message)),
        );
        cancel
    }

    /// Stops the job under this key, whatever it sends.
    pub fn cancel(&mut self, key: &str) {
        for tasks in self.requests().tasks.values_mut() {
            tasks.cancel(key);
        }
    }

    /// Whether a job under this key is still running.
    pub fn running(&self, key: &str) -> bool {
        self.sink_ref()
            .requests_ref()
            .tasks
            .values()
            .any(|tasks| tasks.running(key))
    }

    /// A channel for values from anywhere: each arrives as `message(value)`,
    /// in a batch of its own. Typically opened in `init`.
    pub fn channel<T: Send + 'static>(
        &mut self,
        capacity: usize,
        message: impl Fn(T) -> M + 'static,
    ) -> MessageSender<T> {
        let wake = self.requests().wake.clone();
        let (sender, mailbox) = crate::message_channel::<T>(capacity, move || wake());
        self.sink().channel(
            Box::new(mailbox),
            Box::new(move |value| value.downcast_mut::<Option<T>>()?.take().map(&message)),
        );
        sender
    }

    /// The effects of a component, whose messages `f` turns into this one's.
    pub fn map<N: 'static>(&mut self, f: impl Fn(N) -> M + 'static) -> Effects<'_, N> {
        let window = self.window;
        Effects {
            sink: SinkRef::Owned(Box::new(Mapped {
                inner: self.sink(),
                f: Rc::new(f),
            })),
            window,
        }
    }
}

/// What a host keeps for one window of an app: whether it has declared, and
/// whether its next declaration is due whatever else happened, plus what the
/// pointer, focus and keyboard were on when it last declared.
#[derive(Default)]
pub struct Pacing {
    declared: bool,
    force: bool,
    interaction: crate::tree::Interaction,
}

impl Pacing {
    /// The window declares on its next frame whatever else happened.
    pub fn invalidate(&mut self) {
        self.force = true;
    }

    /// Whether the window's next frame declares for that reason alone.
    pub fn invalidated(&self) -> bool {
        self.force
    }
}

/// Runs an [`App`] on a context: turns each batch of input into messages,
/// updates, applies what the updates asked for, and declares the view only
/// when something it reads may have changed. Hosts and the test harness
/// keep one of these beside the app; applications use `run`.
pub struct AppDriver<M: 'static> {
    effects: EffectState<M>,
    /// Messages sent from outside, and the batch's messages; the capacity
    /// of both is kept.
    sent: Vec<M>,
    batch: Vec<M>,
    started: bool,
    /// The pacing of the window `frame` drives.
    own: Pacing,
}

impl<M: Clone + 'static> AppDriver<M> {
    /// `wake` asks the host for a frame; workers and channels call it from
    /// other threads when they deliver.
    pub fn new(wake: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            effects: EffectState {
                requests: Requests {
                    theme: (0, None),
                    messages: (0, None),
                    format: (0, None),
                    bindings: Vec::new(),
                    clip: None,
                    files: Vec::new(),
                    file_busy: Vec::new(),
                    paste: Vec::new(),
                    focus: Vec::new(),
                    notifications: Vec::new(),
                    window_commands: Vec::new(),
                    toasts: Vec::new(),
                    toast_ids: Vec::new(),
                    next_toast: 0,
                    origin: WindowId::MAIN,
                    open_windows: Vec::new(),
                    close_windows: Vec::new(),
                    next_window: 0,
                    tasks: HashMap::new(),
                    wake: Arc::new(wake),
                },
                paste: Vec::new(),
                files: Vec::new(),
                task_messages: HashMap::new(),
                channels: Vec::new(),
                toast_actions: HashMap::new(),
                toast_messages: HashMap::new(),
            },
            sent: Vec::new(),
            batch: Vec::new(),
            started: false,
            own: Pacing::default(),
        }
    }

    /// A message for the next frame, before the ones its input produces.
    pub fn send(&mut self, message: M) {
        self.sent.push(message);
    }

    /// Whether a message is waiting for the next frame.
    pub fn has_messages(&self) -> bool {
        !self.sent.is_empty()
    }

    /// Makes the next frame declare: after a scale factor change, a hot
    /// reload, or a change to the app made outside `update`.
    pub fn invalidate(&mut self) {
        self.own.invalidate();
    }

    /// The windows `update` asked to open and to close since the last call,
    /// for the host to carry out.
    pub fn take_windows(&mut self) -> (Vec<(WindowId, WindowRequest)>, Vec<WindowId>) {
        let requests = &mut self.effects.requests;
        let opened = std::mem::take(&mut requests.open_windows);
        let closed = std::mem::take(&mut requests.close_windows);
        for window in &closed {
            self.forget_window(*window);
        }
        (opened, closed)
    }

    /// Tells the app a window of its own was closed from outside. Every view
    /// is due, and the windows `update` asked for are in
    /// [`take_windows`](Self::take_windows).
    pub fn window_closed<A: App<Message = M>>(&mut self, app: &mut A, window: WindowId) {
        self.forget_window(window);
        self.effects.requests.origin = WindowId::MAIN;
        app.window_closed(window, &mut Effects::new(&mut self.effects));
    }

    /// Drops what was aimed at a window that is gone, and the messages that
    /// were to answer it.
    fn forget_window(&mut self, window: WindowId) {
        let effects = &mut self.effects;
        let requests = &mut effects.requests;
        requests.files.retain(|(target, _)| *target != window);
        requests.file_busy.retain(|target| *target != window);
        requests.paste.retain(|target| *target != window);
        requests.focus.retain(|(target, _)| *target != window);
        requests
            .window_commands
            .retain(|(target, _)| *target != window);
        let actions = &mut effects.toast_actions;
        requests.toasts.retain(|(target, number, _)| {
            let keep = *target != window;
            if !keep {
                actions.remove(number);
            }
            keep
        });
        effects.paste.retain(|(target, _)| *target != window);
        effects.files.retain(|(target, _)| *target != window);
        effects
            .toast_messages
            .retain(|(target, _), _| *target != window);
    }

    /// The windows with something aimed at them that their next batch
    /// applies. A host gives each one a frame, which is how an effect aimed
    /// at a window other than the one that ran `update` gets there.
    pub fn awaiting(&self) -> Vec<WindowId> {
        let requests = &self.effects.requests;
        let mut windows = Vec::new();
        let mut note = |window: WindowId| {
            if !windows.contains(&window) {
                windows.push(window);
            }
        };
        requests
            .toasts
            .iter()
            .for_each(|(window, ..)| note(*window));
        requests
            .window_commands
            .iter()
            .for_each(|(window, _)| note(*window));
        requests.focus.iter().for_each(|(window, _)| note(*window));
        requests.paste.iter().for_each(|window| note(*window));
        for (window, _) in &requests.files {
            if !requests.file_busy.contains(window) {
                note(*window);
            }
        }
        windows
    }

    /// The first half of a batch, on the context of one window: the input
    /// the context has collected since its last frame becomes messages, and
    /// `update` runs on each in order. Returns how many messages there were,
    /// and whether they or a request `update` made may change what a view
    /// reads. When that is so, every window's view is due, so a host with
    /// several windows invalidates each one's [`Pacing`].
    pub fn batch<A: App<Message = M>>(
        &mut self,
        app: &mut A,
        context: &mut Context,
    ) -> (usize, bool) {
        self.batch_in(WindowId::MAIN, app, context)
    }

    /// [`batch`](Self::batch) on the context of `window`, `WindowId::MAIN`
    /// for the first one. What the updates ask of one window goes to the
    /// window whose input sent the message, and to the main window for a
    /// message from [`send`](Self::send), a task or a channel; `window`'s
    /// context gets what was aimed at it, from this batch or an earlier one.
    pub fn batch_in<A: App<Message = M>>(
        &mut self,
        window: WindowId,
        app: &mut A,
        context: &mut Context,
    ) -> (usize, bool) {
        if !self.started {
            self.started = true;
            self.effects.requests.origin = WindowId::MAIN;
            app.init(&mut Effects::new(&mut self.effects));
        }
        let mut batch = std::mem::take(&mut self.batch);
        batch.append(&mut self.sent);
        let sent = batch.len();
        context.resolve(&mut batch);
        let own = self.deliver(window, context, &mut batch);
        let messages = batch.len();
        for (index, message) in batch.drain(..).enumerate() {
            self.effects.requests.origin = if index < sent || index >= own {
                WindowId::MAIN
            } else {
                window
            };
            app.update(message, &mut Effects::new(&mut self.effects));
        }
        self.effects.requests.origin = WindowId::MAIN;
        self.batch = batch;
        let changed = context.apply_requests(window, &mut self.effects.requests);
        self.adopt_toast_actions();
        context.prepare_batch();
        (messages, messages > 0 || changed)
    }

    /// The second half of a batch, on the same context: declares the view
    /// if it is due, and lays out and paints either way. `window` is `None`
    /// for the main window, which declares with `App::view`; any other
    /// declares with `App::view_window`. `messages` is what `batch` returned.
    #[allow(clippy::too_many_arguments)]
    pub fn declare<A: App<Message = M>>(
        &mut self,
        app: &A,
        context: &mut Context,
        size: Size,
        measurer: &mut impl TextMeasurer,
        window: Option<WindowId>,
        pacing: &mut Pacing,
        messages: usize,
    ) -> FrameStats {
        context.answer_hot_save(|| app.save_state());
        let forced = std::mem::take(&mut pacing.force);
        let declare = !pacing.declared || forced || context.view_wanted(size, pacing.interaction);
        if declare {
            pacing.declared = true;
            match window {
                None => context.run_app(size, measurer, |ui| app.view(ui)),
                Some(id) => context.run_app(size, measurer, |ui| app.view_window(id, ui)),
            };
        } else {
            context.refresh(size, measurer);
        }
        let target = window.unwrap_or(WindowId::MAIN);
        let mut focus = None;
        self.effects.requests.focus.retain(|(window, key)| {
            if *window == target {
                focus = Some(key.clone());
            }
            *window != target
        });
        if let Some(key) = focus {
            context.focus_named(&key);
        }
        pacing.interaction = context.interaction();
        context.note_batch(declare, messages)
    }

    /// Keeps the actions of the toasts just raised, under the ids they got.
    fn adopt_toast_actions(&mut self) {
        let effects = &mut self.effects;
        for (number, window, id) in effects.requests.toast_ids.drain(..) {
            if let Some(action) = effects.toast_actions.remove(&number) {
                effects.toast_messages.insert((window, id), action);
            }
        }
    }

    /// One batch of a single-window host: [`batch`](Self::batch), then
    /// [`declare`](Self::declare) with this driver's own pacing.
    pub fn frame<A: App<Message = M>>(
        &mut self,
        app: &mut A,
        context: &mut Context,
        size: Size,
        measurer: &mut impl TextMeasurer,
    ) -> FrameStats {
        let (messages, dirty) = self.batch(app, context);
        if dirty {
            self.own.invalidate();
        }
        let mut pacing = std::mem::take(&mut self.own);
        let stats = self.declare(&*app, context, size, measurer, None, &mut pacing, messages);
        self.own = pacing;
        stats
    }

    /// What arrived for the messages `Effects` registered: the toast
    /// actions, the clipboard and a file dialog's answer for this window,
    /// then workers and channels. Returns where the messages from workers and
    /// channels begin in `out`: the main window's.
    fn deliver(&mut self, window: WindowId, context: &mut Context, out: &mut Vec<M>) -> usize {
        let effects = &mut self.effects;
        let messages = &mut effects.toast_messages;
        for action in context.take_toast_actions(|id| messages.contains_key(&(window, id))) {
            if let Some(message) = messages.remove(&(window, action.id)) {
                out.push(message(action));
            }
        }
        messages.retain(|(target, id), _| *target != window || context.has_toast(*id));
        if effects.paste.iter().any(|(target, _)| *target == window)
            && let Some(clip) = context.take_pasted()
        {
            // One answer serves every paste asked before it came.
            for (_, message) in effects
                .paste
                .extract_if(.., |(target, _)| *target == window)
            {
                out.push(message(clip.clone()));
            }
        }
        if let Some(first) = effects
            .files
            .iter()
            .position(|(target, _)| *target == window)
            && let Some(paths) = context.take_chosen_files()
        {
            effects
                .requests
                .file_busy
                .retain(|target| *target != window);
            let (_, message) = effects.files.remove(first);
            out.push(message(paths));
        }
        let own_end = out.len();
        for (kind, tasks) in &mut effects.requests.tasks {
            let messages = effects.task_messages.get(kind);
            tasks.deliver(DELIVERED_PER_BATCH, &mut |key, value| {
                if let Some(lift) = key.and_then(|key| messages?.get(key)) {
                    out.extend(lift(value));
                }
            });
            // A job the table has forgotten ended and had everything
            // delivered, or was cancelled; its way back to a message goes too.
            if let Some(messages) = effects.task_messages.get_mut(kind) {
                messages.retain(|key, _| tasks.tracks(key));
            }
        }
        for (mailbox, lift) in &mut effects.channels {
            mailbox.deliver(DELIVERED_PER_BATCH, &mut |_, value| out.extend(lift(value)));
        }
        own_end
    }
}

/// One messages source or format shared by every window's context.
pub(crate) struct Shared<T: ?Sized>(pub Rc<T>);

impl Messages for Shared<dyn Messages> {
    fn locale(&self) -> &str {
        self.0.locale()
    }

    fn text(&self, key: &str, args: &[(&str, crate::MessageArg)]) -> Option<String> {
        self.0.text(key, args)
    }
}

impl LocaleFormat for Shared<dyn LocaleFormat> {
    fn number(&self, value: f64, decimals: usize) -> String {
        self.0.number(value, decimals)
    }

    fn parse_number(&self, text: &str) -> Option<f64> {
        self.0.parse_number(text)
    }

    fn date(&self, date: crate::Date, style: crate::DateStyle) -> String {
        self.0.date(date, style)
    }

    fn time(&self, time: crate::Time, seconds: bool) -> String {
        self.0.time(time, seconds)
    }
}
