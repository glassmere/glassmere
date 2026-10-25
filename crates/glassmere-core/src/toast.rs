use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What a toast is about; it picks the accent colour and how urgently
/// assistive technology announces it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    /// Announced assertively and shown longer.
    Error,
}

/// A short message in the corner of the window that leaves on its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    /// Label of one button, such as "Undo"; a click sends the message
    /// `Effects::toast_with` was given, once, and dismisses the toast.
    pub action: Option<String>,
    /// Anything the app wants back with the action, for toasts raised where
    /// their `ToastId` is not at hand, such as other threads.
    pub tag: String,
    /// How long it stays once visible; `None` stays until dismissed.
    pub duration: Option<Duration>,
}

impl Toast {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Info,
            action: None,
            tag: String::new(),
            duration: Some(Duration::from_secs(5)),
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self::new(text).kind(ToastKind::Success)
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self::new(text).kind(ToastKind::Warning)
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self::new(text).kind(ToastKind::Error)
    }

    /// Errors get eight seconds unless a duration was chosen.
    pub fn kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        if kind == ToastKind::Error && self.duration == Some(Duration::from_secs(5)) {
            self.duration = Some(Duration::from_secs(8));
        }
        self
    }

    /// Something to act on needs time to be read: at least eight seconds.
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self.duration = self.duration.map(|time| time.max(Duration::from_secs(8)));
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn sticky(mut self) -> Self {
        self.duration = None;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToastId(pub(crate) u64);

/// A toast's action button was clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastAction {
    pub id: ToastId,
    pub tag: String,
}

/// Raises toasts in one window from any thread and wakes that window. Get it
/// from `Ui::toast_sender` during a declaration and hand clones to workers.
#[derive(Clone)]
pub struct ToastSender {
    pub(crate) inbox: Arc<Mutex<Vec<Toast>>>,
    pub(crate) wake: Arc<Mutex<Box<dyn Fn() + Send>>>,
}

impl ToastSender {
    pub fn send(&self, toast: Toast) {
        // A worker that panicked while holding the lock must not silence the rest.
        self.inbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(toast);
        (self
            .wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))();
    }
}

impl std::fmt::Debug for ToastSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ToastSender")
    }
}
