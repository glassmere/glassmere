use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc::{self, Receiver, SendError, SyncSender, TrySendError},
};

struct Shared {
    pending: AtomicUsize,
    wake: Box<dyn Fn() -> bool + Send + Sync>,
}

/// Bounded nonblocking messages from workers or application-owned async runtimes.
pub struct MessageSender<T> {
    sender: SyncSender<T>,
    shared: Arc<Shared>,
}
impl<T> Clone for MessageSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            shared: self.shared.clone(),
        }
    }
}
impl<T> MessageSender<T> {
    /// Sends without waiting. A full mailbox hands the message back, so the
    /// sender decides whether to drop it, keep it or wait.
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.shared.pending.fetch_add(1, Ordering::AcqRel);
        match self.sender.try_send(message) {
            Ok(()) => {
                (self.shared.wake)();
                Ok(())
            }
            Err(error) => {
                self.shared.pending.fetch_sub(1, Ordering::AcqRel);
                Err(error)
            }
        }
    }

    /// Waits for room, which is what a worker producing a long stream wants:
    /// the alternative is a retry loop of its own. Never call it on the UI
    /// thread, which is the thread that makes room.
    pub fn send_blocking(&self, message: T) -> Result<(), SendError<T>> {
        self.shared.pending.fetch_add(1, Ordering::AcqRel);
        match self.sender.send(message) {
            Ok(()) => {
                (self.shared.wake)();
                Ok(())
            }
            Err(error) => {
                self.shared.pending.fetch_sub(1, Ordering::AcqRel);
                Err(error)
            }
        }
    }

    /// Asks for a UI turn without sending anything, for a worker whose news
    /// is that it has stopped.
    pub fn wake(&self) -> bool {
        (self.shared.wake)()
    }
}
/// Drain before widget declaration. Partial drains schedule another UI turn.
pub struct Mailbox<T> {
    receiver: Receiver<T>,
    shared: Arc<Shared>,
}
impl<T> Mailbox<T> {
    pub fn drain(&self, limit: usize) -> Vec<T> {
        debug_assert!(limit > 0, "mailbox drain limit must be positive");
        let limit = limit.max(1);
        let messages: Vec<_> = self.receiver.try_iter().take(limit).collect();
        self.shared
            .pending
            .fetch_sub(messages.len(), Ordering::AcqRel);
        if self.shared.pending.load(Ordering::Acquire) > 0 {
            (self.shared.wake)();
        }
        messages
    }

    /// Messages sent and not yet drained.
    pub fn pending(&self) -> usize {
        self.shared.pending.load(Ordering::Acquire)
    }
}
/// A bounded mailbox and its sending end. `wake` asks for a UI turn and
/// answers whether the window is still there; an application has one ready
/// in [`crate::Effects::channel`].
pub fn message_channel<T>(
    capacity: usize,
    wake: impl Fn() -> bool + Send + Sync + 'static,
) -> (MessageSender<T>, Mailbox<T>) {
    debug_assert!(capacity > 0, "mailbox capacity must be positive");
    let capacity = capacity.max(1);
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let shared = Arc::new(Shared {
        pending: AtomicUsize::new(0),
        wake: Box::new(wake),
    });
    (
        MessageSender {
            sender,
            shared: shared.clone(),
        },
        Mailbox { receiver, shared },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_messages_preserve_order_report_backpressure_and_rewake_partial_drains() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = wakes.clone();
        let (sender, receiver) = message_channel(2, move || {
            counted.fetch_add(1, Ordering::Relaxed);
            true
        });
        sender.try_send(1).unwrap();
        sender.clone().try_send(2).unwrap();
        assert!(matches!(sender.try_send(3), Err(TrySendError::Full(3))));
        assert_eq!(receiver.drain(1), [1]);
        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        assert_eq!(receiver.drain(10), [2]);
        assert!(receiver.drain(1).is_empty());
        drop(receiver);
        assert!(matches!(
            sender.try_send(4),
            Err(TrySendError::Disconnected(4))
        ));
    }
}
