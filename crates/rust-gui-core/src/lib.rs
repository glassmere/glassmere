//! Stable identity, reconciliation, layout, input, and semantic metadata.
mod animation;
mod clipboard;
mod collection;
mod command;
mod controls;
mod dock;
mod document;
mod drag;
mod editor;
mod focus;
mod image;
mod locale_format;
mod mailbox;
mod messages;
mod panes;
mod platform;
mod property_grid;
mod reactive;
mod rich_document;
mod rich_editor;
mod rich_text;
mod style;
mod stylesheet;
mod task;
mod text_area;
mod theme;
mod toast;
mod tree;
mod ui;
mod widget;
mod window;

pub use animation::{Animation, Easing, Effect, Keyframe, Repeat, Transform, Transition};
pub use clipboard::Clip;
pub use collection::{
    CellEditor, CellValue, FILTER_DELAY, GridColumn, GridColumns, GridModel, GridState, ItemId,
    ItemOrder, ListModel, ListResponse, ListState, ModelError, SortDirection, TreeItem, TreeModel,
    VirtualListOptions, text_matches,
};
pub use command::{Command, Shortcut, ShortcutKey};
pub use rust_gui_scene::GradientStop;
mod events;
pub use controls::{AutocompleteResponse, MenuItem, NumberOptions};
pub use dock::{
    DockArea, DockFloat, DockLayout, DockLayoutError, DockNode, DockPlace, DockTabs, DockTarget,
    DockWindow, DockWindowId, DockZone,
};
pub use document::{Indentation, Motion, Selection, TextDocument};
pub use drag::{DragInfo, DragOutcome, DragPayload, DropKind};
pub use editor::{CaretMovement, TextEdit, TextMove, TextSelection};
pub use events::{EventArea, UiEvent};
pub use focus::{DialogOptions, DialogResponse, FocusScope};
pub use image::{BackgroundImage, ImageFit, ImageOptions, ImageSource, ImageTint};
pub use locale_format::{Date, DateStyle, LocaleFormat, Time};
pub use mailbox::{Mailbox, MessageSender, message_channel};
pub use messages::{BUILTIN_MESSAGES, MessageArg, Messages};
pub use panes::{
    Axis, PopoverOptions, PopoverResponse, SplitOptions, TOOLTIP_DELAY, Tab, TabsResponse,
};
pub use platform::{FileKind, FileRequest, Menu, MenuEntry, Notification, Tray};
pub use property_grid::{PropertyGridOptions, PropertyGridResponse, PropertyGridState};
pub use reactive::{GraphNode, Memo, Signal, StateKind, Store};
pub use rich_document::{
    Block, BlockKind, InlineRun, Mark, Marks, RichDocument, RichMotion, RichPosition,
};
pub use rich_editor::{RichEditorOptions, RichEditorResponse};
pub use rich_text::{RichRun, RichText};
pub use rust_gui_scene::{FontFamily, TextSpan, TextStyle, TextWrap};
pub use rust_gui_scene::{MonospaceMeasurer, TextMeasurer};
pub use style::{
    Align, Direction, Gradient, GridArea, Justify, Length, Paint, ResolvedText, Shadow,
    StateStyles, Style, Track, TrackSize,
};
pub use stylesheet::{StyleSheet, StyleSheetError};
pub use task::{Cancel, TaskSender, Tasks};
pub use text_area::{
    CodeEditorOptions, CodeEditorResponse, Diagnostic, EditorCommand, FindState, HighlightSpan,
    Highlighter, Severity,
};
pub use theme::{TOKENS, Theme};
pub use toast::{Toast, ToastAction, ToastId, ToastKind, ToastSender};
pub use tree::diagnostics::{
    DiagnosticKind, FrameDiagnostic, HUGE_CHILDREN, LAYER_AREA, RELAYOUT_FRAMES, REPAINT_FRAMES,
    REPAINT_MIN_NODES, UNSTABLE_ID_FRAMES,
};
pub use tree::{
    Action, CollectionInfo, Context, Dirty, Event, FrameStats, GestureResponse, Key, Live,
    MemoryReport, NodeDetails, NodeId, NodeSnapshot, NumericValue, Pen, Role, ScrollState,
    Semantics, Swipe, SwipeDirection, TouchPhase,
};
pub use ui::{
    DockOptions, DockPanel, DockResponse, Properties, PropertyGrid, Response, Ui, suggest,
};
pub use widget::{Component, CustomWidget, WidgetInput};
pub use window::{ResizeEdge, WindowCommand, WindowInfo};
