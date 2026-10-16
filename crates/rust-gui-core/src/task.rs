//! Background work: cancellation, many results per job, and superseding.
//!
//! No executor is started and none is forced. A job is a thread, or a future
//! on whatever runtime the application already runs; what lives here is the
//! part a user interface needs and a plain channel does not give. A job can
//! be asked to stop, a newer job supersedes the one under the same key, and
//! a superseded job's results never reach the view, including the ones it
//! had already sent. That last promise is what makes search as you type
//! correct: the answer to the query before last cannot overtake the answer
//! to the current one.

use crate::mailbox::{Mailbox, MessageSender, message_channel};
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{SendError, TrySendError},
    },
};

/// Asks a job to stop. Cancelling is cooperative: it sets a flag, and the job
/// decides how often to look. Cloneable, and cheap to look at.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// One job's messages, tagged so that a superseded job's are recognised.
struct Message<K, T> {
    key: K,
    generation: u64,
    value: T,
}

/// What a job holds while it runs. Dropping it says the job has ended, and
/// wakes the window so a view showing that it runs can stop.
pub struct TaskSender<K, T> {
    key: K,
    generation: u64,
    cancel: Cancel,
    finished: Arc<AtomicBool>,
    sender: MessageSender<Message<K, T>>,
}

impl<K: Clone, T> TaskSender<K, T> {
    /// Sends a result without waiting. A full mailbox hands the value back;
    /// the job decides whether to drop it, keep it or wait.
    pub fn send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.sender
            .try_send(self.wrap(value))
            .map_err(|error| match error {
                TrySendError::Full(message) => TrySendError::Full(message.value),
                TrySendError::Disconnected(message) => TrySendError::Disconnected(message.value),
            })
    }

    /// Waits for room, which is what a job producing a long stream wants.
    /// Look at [`TaskSender::cancelled`] between sends: a cancelled job that
    /// blocks here waits for a view that has stopped caring.
    pub fn send_blocking(&self, value: T) -> Result<(), SendError<T>> {
        self.sender
            .send_blocking(self.wrap(value))
            .map_err(|SendError(message)| SendError(message.value))
    }

    /// Whether the view has asked this job to stop, either directly or by
    /// starting another under the same key.
    pub fn cancelled(&self) -> bool {
        self.cancel.cancelled()
    }

    fn wrap(&self, value: T) -> Message<K, T> {
        Message {
            key: self.key.clone(),
            generation: self.generation,
            value,
        }
    }
}

impl<K, T> Drop for TaskSender<K, T> {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::Release);
        self.sender.wake();
    }
}

/// What the view knows about a job it started.
struct Live {
    generation: u64,
    cancel: Cancel,
    finished: Arc<AtomicBool>,
}

impl Live {
    fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}

/// Background jobs, one at a time under each key. Drain it before declaring
/// the view, like a [`Mailbox`], which is what it is built on.
///
/// ```no_run
/// # use rust_gui_core::Tasks;
/// # fn go(tasks: &mut Tasks<String, String>, query: String) {
/// // Typing supersedes the job the last keystroke started.
/// tasks.spawn(query, move |out| {
///     for line in ["a", "b"] {
///         if out.cancelled() {
///             return;
///         }
///         let _ = out.send_blocking(line.to_owned());
///     }
/// });
/// for (query, hit) in tasks.drain(64) {
///     let _ = (query, hit);
/// }
/// # }
/// ```
pub struct Tasks<K, T> {
    sender: MessageSender<Message<K, T>>,
    mailbox: Mailbox<Message<K, T>>,
    live: HashMap<K, Live>,
    next: u64,
}

impl<K: Clone + Eq + Hash, T> Tasks<K, T> {
    /// `capacity` is how many results can wait, over all jobs together.
    /// `wake` asks for a UI turn; an application has jobs ready in
    /// `Effects::spawn`.
    pub fn new(capacity: usize, wake: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        let (sender, mailbox) = message_channel(capacity, wake);
        Self {
            sender,
            mailbox,
            live: HashMap::new(),
            next: 0,
        }
    }

    /// The sending end for a new job under `key`, cancelling and forgetting
    /// whatever ran under that key before. Hand it to a thread, or to a
    /// future on the runtime the application already has.
    pub fn start(&mut self, key: K) -> TaskSender<K, T> {
        self.cancel(&key);
        self.next += 1;
        let live = Live {
            generation: self.next,
            cancel: Cancel::new(),
            finished: Arc::new(AtomicBool::new(false)),
        };
        let sender = TaskSender {
            key: key.clone(),
            generation: live.generation,
            cancel: live.cancel.clone(),
            finished: live.finished.clone(),
            sender: self.sender.clone(),
        };
        self.live.insert(key, live);
        sender
    }

    /// Starts a job on a thread of its own, for work that blocks rather than
    /// awaits. Returns its cancellation handle, which [`Tasks::cancel`] also
    /// reaches by key.
    pub fn spawn(&mut self, key: K, job: impl FnOnce(TaskSender<K, T>) + Send + 'static) -> Cancel
    where
        K: Send + 'static,
        T: Send + 'static,
    {
        let sender = self.start(key);
        let cancel = sender.cancel.clone();
        std::thread::spawn(move || job(sender));
        cancel
    }

    /// Up to `limit` results, with the key of the job that sent them, in the
    /// order they were sent. Results from a cancelled or superseded job are
    /// dropped here rather than handed over. A partial drain asks for
    /// another UI turn, so a long stream does not stall the view.
    pub fn drain(&mut self, limit: usize) -> Vec<(K, T)> {
        let live = &self.live;
        let results: Vec<(K, T)> = self
            .mailbox
            .drain(limit)
            .into_iter()
            .filter(|message| {
                live.get(&message.key)
                    .is_some_and(|live| live.generation == message.generation)
            })
            .map(|message| (message.key, message.value))
            .collect();
        // A finished job is forgotten once nothing of its is still waiting,
        // because forgetting it earlier would drop those results as stale.
        if self.mailbox.pending() == 0 {
            self.live.retain(|_, live| !live.finished());
        }
        results
    }

    /// Asks the job under `key` to stop, and stops listening to it at once:
    /// whatever it has already sent is dropped when drained.
    pub fn cancel(&mut self, key: &K) {
        if let Some(live) = self.live.remove(key) {
            live.cancel.cancel();
        }
    }

    pub fn cancel_all(&mut self) {
        for (_, live) in self.live.drain() {
            live.cancel.cancel();
        }
    }

    /// Whether a job under `key` was started and has not ended. A job that
    /// ended is reported as such as soon as it drops its sender, without
    /// waiting for its results to be drained.
    pub fn running(&self, key: &K) -> bool {
        self.live.get(key).is_some_and(|live| !live.finished())
    }

    pub fn running_keys(&self) -> impl Iterator<Item = &K> {
        self.live
            .iter()
            .filter(|(_, live)| !live.finished())
            .map(|(key, _)| key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn counted<K: Clone + Eq + Hash, T>(capacity: usize) -> (Tasks<K, T>, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let tasks = Tasks::new(capacity, move || {
            counter.fetch_add(1, Ordering::Relaxed);
            true
        });
        (tasks, wakes)
    }

    #[test]
    fn a_superseded_job_cannot_reach_the_view_even_with_results_already_sent() {
        let (mut tasks, _) = counted::<&str, u32>(8);
        let old = tasks.start("search");
        old.send(1).unwrap();
        let new = tasks.start("search");
        // The older job is told to stop and is no longer listened to, so the
        // answer to the query before last cannot overtake this one.
        assert!(old.cancelled());
        assert!(!new.cancelled());
        old.send(2).unwrap();
        new.send(3).unwrap();
        assert_eq!(tasks.drain(10), [("search", 3)]);
    }

    #[test]
    fn cancelling_stops_the_job_and_drops_what_it_had_sent() {
        let (mut tasks, _) = counted::<&str, u32>(8);
        let job = tasks.start("scan");
        job.send(1).unwrap();
        assert!(tasks.running(&"scan"));
        tasks.cancel(&"scan");
        assert!(job.cancelled());
        assert!(!tasks.running(&"scan"));
        assert!(tasks.drain(10).is_empty());
        assert_eq!(tasks.running_keys().count(), 0);
    }

    #[test]
    fn a_job_streams_many_results_and_a_partial_drain_asks_for_another_turn() {
        let (mut tasks, wakes) = counted::<&str, u32>(8);
        let job = tasks.start("lines");
        for value in 0..5 {
            job.send(value).unwrap();
        }
        assert_eq!(tasks.drain(2), [("lines", 0), ("lines", 1)]);
        // Five sends woke the window, and so did the drain that left some.
        assert_eq!(wakes.load(Ordering::Relaxed), 6);
        assert_eq!(tasks.drain(10), [("lines", 2), ("lines", 3), ("lines", 4)]);
        assert_eq!(wakes.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn a_full_mailbox_hands_the_value_back_and_blocking_sends_wait_for_room() {
        let (mut tasks, _) = counted::<&str, u32>(1);
        let job = tasks.start("stream");
        job.send(1).unwrap();
        assert!(matches!(job.send(2), Err(TrySendError::Full(2))));
        drop(job);
        let sender = tasks.start("stream");
        let worker = std::thread::spawn(move || {
            for value in 0..4 {
                sender.send_blocking(value).unwrap();
            }
        });
        let mut seen = Vec::new();
        while seen.len() < 4 {
            seen.extend(tasks.drain(1).into_iter().map(|(_, value)| value));
        }
        worker.join().unwrap();
        assert_eq!(seen, [0, 1, 2, 3]);
    }

    #[test]
    fn a_thread_reports_that_it_has_ended_and_is_forgotten_once_drained() {
        let (mut tasks, wakes) = counted::<u32, u32>(8);
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        tasks.spawn(7, move |out| {
            out.send(70).unwrap();
            flag.store(true, Ordering::Release);
        });
        while !done.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        // Ending wakes the window on its own, so a view that shows the job as
        // running is asked to declare again even when nothing was sent.
        while wakes.load(Ordering::Relaxed) < 2 {
            std::hint::spin_loop();
        }
        assert!(!tasks.running(&7));
        assert_eq!(tasks.drain(10), [(7, 70)]);
        assert_eq!(tasks.live.len(), 0);
    }

    #[test]
    fn a_cancelled_thread_stops_at_the_next_look() {
        let (mut tasks, _) = counted::<&str, u32>(4);
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = stopped.clone();
        let cancel = tasks.spawn("loop", move |out| {
            while !out.cancelled() {
                std::hint::spin_loop();
            }
            flag.store(true, Ordering::Release);
        });
        cancel.cancel();
        while !stopped.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        assert!(stopped.load(Ordering::Acquire));
    }
}
