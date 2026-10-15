//! Messages, an update, and one view per frame.
//!
//! An [`App`] holds the state, `update` changes it in answer to a message,
//! and `view` declares what it looks like. Widgets send messages instead of
//! reporting what happened through their `Response`, so the view reads state
//! that is already up to date and runs at most once per batch of input.

use crate::controls::ControlUpdate;
use crate::{
    Cancel, Clip, Context, FileRequest, FrameStats, LocaleFormat, MessageSender, Messages,
    Notification, Shortcut, TaskSender, Tasks, TextMeasurer, Theme, Toast, ToastAction, ToastId,
    Ui, UiEvent, WindowCommand, WindowId, WindowRequest,
};
use rust_gui_scene::Size;
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
    /// What `rust-gui-dev` keeps across a rebuild; replaces `Ui::hot_state`.
    fn save_state(&self) -> Option<String> {
        None
    }
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

/// The message of [`run_with`](../../rust_gui/fn.run_with.html): a change to
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

/// Delivered values waiting in one task table or channel.
trait Delivery {
    /// Up to `limit` values, each as `&mut Option<T>` with its task key.
    fn deliver(&mut self, limit: usize, out: &mut dyn FnMut(Option<&str>, &mut dyn Any));
    fn cancel(&mut self, _key: &str) {}
    fn running(&self, _key: &str) -> bool {
        false
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
    pub theme: Option<Theme>,
    pub messages: Option<Option<Box<dyn Messages>>>,
    pub format: Option<Option<Box<dyn LocaleFormat>>>,
    pub bindings: Vec<(String, Option<Shortcut>)>,
    pub clip: Option<Clip>,
    pub files: Option<FileRequest>,
    pub paste: bool,
    pub focus: Option<String>,
    pub notifications: Vec<Notification>,
    pub window_commands: Vec<WindowCommand>,
    pub toasts: Vec<Toast>,
    /// The id each of `toasts` got when it was raised, in the same order.
    pub toast_ids: Vec<ToastId>,
    /// Windows asked for and closed; multiple windows are not wired yet.
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
    fn paste(&mut self, message: Box<dyn Fn(Clip) -> M>);
    fn files(&mut self, message: Box<dyn Fn(Vec<PathBuf>) -> M>);
    fn task(&mut self, kind: TypeId, key: String, lift: Lift<M>);
    fn channel(&mut self, mailbox: Box<dyn Delivery>, lift: Lift<M>);
    fn toast(&mut self, toast: Toast, action: Option<OnToastAction<M>>);
}

/// Everything the driver keeps for `Effects`, typed by the app's message.
struct EffectState<M> {
    requests: Requests,
    paste: Option<Box<dyn Fn(Clip) -> M>>,
    files: Option<Box<dyn Fn(Vec<PathBuf>) -> M>>,
    task_messages: HashMap<TypeId, HashMap<String, Lift<M>>>,
    channels: Vec<(Box<dyn Delivery>, Lift<M>)>,
    /// One per entry of `requests.toasts`: what its action sends, if anything.
    toast_actions: Vec<Option<OnToastAction<M>>>,
    /// The actions of raised toasts, by id, until the toast leaves.
    toast_messages: HashMap<ToastId, OnToastAction<M>>,
}

impl<M> Sink<M> for EffectState<M> {
    fn requests(&mut self) -> &mut Requests {
        &mut self.requests
    }

    fn paste(&mut self, message: Box<dyn Fn(Clip) -> M>) {
        self.paste = Some(message);
    }

    fn files(&mut self, message: Box<dyn Fn(Vec<PathBuf>) -> M>) {
        self.files = Some(message);
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

    fn toast(&mut self, toast: Toast, action: Option<OnToastAction<M>>) {
        self.requests.toasts.push(toast);
        self.toast_actions.push(action);
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

    fn paste(&mut self, message: Box<dyn Fn(Clip) -> N>) {
        let f = self.f.clone();
        self.inner.paste(Box::new(move |clip| f(message(clip))));
    }

    fn files(&mut self, message: Box<dyn Fn(Vec<PathBuf>) -> N>) {
        let f = self.f.clone();
        self.inner.files(Box::new(move |paths| f(message(paths))));
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

    fn toast(&mut self, toast: Toast, action: Option<OnToastAction<N>>) {
        let f = self.f.clone();
        let action =
            action.map(|message| Box::new(move |found| f(message(found))) as OnToastAction<M>);
        self.inner.toast(toast, action);
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
}

impl<M: 'static> Effects<'_, M> {
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
        self.requests().theme = Some(theme);
    }

    /// Installs the source of localized text, or removes it with `None`.
    pub fn set_messages(&mut self, messages: Option<Box<dyn Messages>>) {
        self.requests().messages = Some(messages);
    }

    /// Installs locale-aware formatting, or removes it with `None`.
    pub fn set_format(&mut self, format: Option<Box<dyn LocaleFormat>>) {
        self.requests().format = Some(format);
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
        self.requests().paste = true;
        self.sink().paste(Box::new(message));
    }

    /// Puts up a file dialog; the paths chosen arrive as `message(paths)`,
    /// empty when it was cancelled.
    pub fn choose_files(
        &mut self,
        request: FileRequest,
        message: impl Fn(Vec<PathBuf>) -> M + 'static,
    ) {
        self.requests().files = Some(request);
        self.sink().files(Box::new(message));
    }

    /// Moves keyboard focus to the widget declared with this key, once the
    /// view has declared.
    pub fn request_focus(&mut self, key: impl Into<String>) {
        self.requests().focus = Some(key.into());
    }

    /// Posts a message to the desktop's notification area.
    pub fn notify(&mut self, notification: Notification) {
        self.requests().notifications.push(notification);
    }

    /// Moves, resizes, minimizes, maximizes or closes the window.
    pub fn window_command(&mut self, command: WindowCommand) {
        self.requests().window_commands.push(command);
    }

    /// Shows a toast, as `Ui::toast` does.
    pub fn toast(&mut self, toast: Toast) {
        self.sink().toast(toast, None);
    }

    /// Shows a toast whose action button sends `message(action)` when clicked.
    pub fn toast_msg(&mut self, toast: Toast, message: impl Fn(ToastAction) -> M + 'static) {
        self.sink().toast(toast, Some(Box::new(message)));
    }

    /// Opens another window, which `App::view_window` declares.
    ///
    /// Not wired to a host yet: the identity is handed out and the request
    /// kept, but no window opens.
    pub fn open_window(&mut self, request: WindowRequest) -> WindowId {
        let requests = self.requests();
        requests.next_window += 1;
        let id = WindowId(requests.next_window);
        requests.open_windows.push((id, request));
        id
    }

    /// Closes a window `open_window` opened. Not wired to a host yet.
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
    pub fn running(&mut self, key: &str) -> bool {
        self.requests()
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
        Effects {
            sink: SinkRef::Owned(Box::new(Mapped {
                inner: self.sink(),
                f: Rc::new(f),
            })),
        }
    }
}

/// Runs an [`App`] on a context: turns each batch of input into messages,
/// updates, applies what the updates asked for, and declares the view only
/// when something it reads may have changed. Hosts and the test harness
/// keep one of these beside the app, per window; applications use `run_app`.
pub struct AppDriver<M: 'static> {
    effects: EffectState<M>,
    /// Messages sent from outside, and the batch's messages; the capacity
    /// of both is kept.
    sent: Vec<M>,
    batch: Vec<M>,
    started: bool,
    /// Declare on the next frame whatever else happened.
    force: bool,
    /// What was hovered, focused and pressed when the last frame ended.
    interaction: crate::tree::Interaction,
}

impl<M: Clone + 'static> AppDriver<M> {
    /// `wake` asks the host for a frame; workers and channels call it from
    /// other threads when they deliver.
    pub fn new(wake: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            effects: EffectState {
                requests: Requests {
                    theme: None,
                    messages: None,
                    format: None,
                    bindings: Vec::new(),
                    clip: None,
                    files: None,
                    paste: false,
                    focus: None,
                    notifications: Vec::new(),
                    window_commands: Vec::new(),
                    toasts: Vec::new(),
                    toast_ids: Vec::new(),
                    open_windows: Vec::new(),
                    close_windows: Vec::new(),
                    next_window: 0,
                    tasks: HashMap::new(),
                    wake: Arc::new(wake),
                },
                paste: None,
                files: None,
                task_messages: HashMap::new(),
                channels: Vec::new(),
                toast_actions: Vec::new(),
                toast_messages: HashMap::new(),
            },
            sent: Vec::new(),
            batch: Vec::new(),
            started: false,
            force: false,
            interaction: crate::tree::Interaction::default(),
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
        self.force = true;
    }

    /// One batch: the input the context has collected since the last frame
    /// becomes messages, `update` runs on each in order, what it asked for
    /// is applied, and the view declares at most once.
    pub fn frame<A: App<Message = M>>(
        &mut self,
        app: &mut A,
        context: &mut Context,
        size: Size,
        measurer: &mut impl TextMeasurer,
    ) -> FrameStats {
        let first = !self.started;
        if first {
            self.started = true;
            app.init(&mut Effects {
                sink: SinkRef::Borrowed(&mut self.effects),
            });
        }
        let mut batch = std::mem::take(&mut self.batch);
        batch.append(&mut self.sent);
        context.resolve(&mut batch);
        self.deliver(context, &mut batch);
        let messages = batch.len();
        for message in batch.drain(..) {
            app.update(
                message,
                &mut Effects {
                    sink: SinkRef::Borrowed(&mut self.effects),
                },
            );
        }
        self.batch = batch;
        let changed = context.apply_requests(&mut self.effects.requests);
        self.adopt_toast_actions();
        context.prepare_batch();
        let declare = first
            || std::mem::take(&mut self.force)
            || messages > 0
            || changed
            || context.view_wanted(size, self.interaction);
        if declare {
            let app = &*app;
            context.run_app(size, measurer, |ui| app.view(ui));
        } else {
            context.refresh(size, measurer);
        }
        if let Some(key) = self.effects.requests.focus.take() {
            context.focus_named(&key);
        }
        self.interaction = context.interaction();
        context.note_batch(declare, messages)
    }

    /// Keeps the actions of the toasts just raised, under the ids they got.
    fn adopt_toast_actions(&mut self) {
        let effects = &mut self.effects;
        let raised = effects.requests.toast_ids.drain(..);
        for (id, action) in raised.zip(effects.toast_actions.drain(..)) {
            if let Some(action) = action {
                effects.toast_messages.insert(id, action);
            }
        }
    }

    /// What arrived for the messages `Effects` registered: the clipboard,
    /// a file dialog's answer, workers and channels.
    fn deliver(&mut self, context: &mut Context, out: &mut Vec<M>) {
        let effects = &mut self.effects;
        let messages = &mut effects.toast_messages;
        for action in context.take_toast_actions(|id| messages.contains_key(&id)) {
            if let Some(message) = messages.remove(&action.id) {
                out.push(message(action));
            }
        }
        messages.retain(|id, _| context.has_toast(*id));
        if effects.paste.is_some()
            && let Some(clip) = context.take_pasted()
        {
            out.push((effects.paste.take().unwrap())(clip));
        }
        if effects.files.is_some()
            && let Some(paths) = context.take_chosen_files()
        {
            out.push((effects.files.take().unwrap())(paths));
        }
        for (kind, tasks) in &mut effects.requests.tasks {
            let messages = effects.task_messages.get(kind);
            tasks.deliver(DELIVERED_PER_BATCH, &mut |key, value| {
                if let Some(lift) = key.and_then(|key| messages?.get(key)) {
                    out.extend(lift(value));
                }
            });
        }
        for (mailbox, lift) in &mut effects.channels {
            mailbox.deliver(DELIVERED_PER_BATCH, &mut |_, value| out.extend(lift(value)));
        }
    }
}
