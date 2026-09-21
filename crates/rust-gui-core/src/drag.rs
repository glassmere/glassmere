use crate::{ItemId, NodeId};
use rust_gui_scene::Point;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropKind {
    Text,
    Item,
    Files,
}

/// In-process payloads. Dropping reports data; the app decides whether to copy,
/// move, load, or reject it. File payloads never trigger filesystem operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DragPayload {
    Text(String),
    Item { domain: String, id: ItemId },
    Files(Vec<std::path::PathBuf>),
}
impl DragPayload {
    pub fn kind(&self) -> DropKind {
        match self {
            Self::Text(_) => DropKind::Text,
            Self::Item { .. } => DropKind::Item,
            Self::Files(_) => DropKind::Files,
        }
    }
}

/// How a drag that the host handed to the operating system ended. The
/// framework never touches the dragged files; an app that moves rather than
/// copies acts on `Dropped` itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragOutcome {
    /// Another application, or another window, accepted the payload.
    Dropped,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
pub struct DragInfo {
    pub source: bool,
    pub can_drop: bool,
    pub dragging: bool,
}

pub(crate) struct DragState {
    pub source: NodeId,
    pub payload: DragPayload,
    pub start: Point,
    pub active: bool,
    pub over: Option<NodeId>,
}
