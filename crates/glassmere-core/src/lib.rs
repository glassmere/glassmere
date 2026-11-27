//! Stable identity, reconciliation, layout, input, and semantic metadata.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod animation;
pub mod app;
mod calendar;
mod clipboard;
mod clock;
mod collection;
mod collections;
mod command;
mod controls;
mod dock;
#[cfg(feature = "std")]
mod document;
mod drag;
mod editor;
#[cfg(not(feature = "std"))]
mod float;
mod focus;
mod image;
mod locale_format;
#[cfg(feature = "std")]
mod mailbox;
mod messages;
mod panes;
mod platform;
mod property_grid;
mod reactive;
mod rich_document;
mod rich_editor;
mod rich_text;
mod selection;
mod style;
mod stylesheet;
#[cfg(feature = "std")]
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
    CellEditor, CellValue, FILTER_DELAY, GridColumn, GridColumns, GridModel, GridMsg, GridState,
    ItemId, ItemOrder, ListModel, ListState, ModelError, RowHeights, SortDirection, TreeItem,
    TreeModel, VirtualListOptions, text_matches,
};
pub use command::{Command, Shortcut, ShortcutKey};
pub use glassmere_scene::GradientStop;
mod events;
pub use app::{App, AppDriver, Component, Effects, Pacing};
pub use controls::{DockAction, MenuItem, NumberOptions};
pub use dock::{
    DockArea, DockFloat, DockLayout, DockLayoutError, DockNode, DockPlace, DockTabs, DockTarget,
    DockWindow, DockWindowId, DockZone,
};
#[cfg(feature = "std")]
pub use document::TextDocument;
pub use drag::{DragInfo, DragOutcome, DragPayload, DropKind};
pub use editor::{CaretMovement, TextEdit, TextMove, TextSelection};
pub use events::{EventArea, UiEvent};
pub use focus::{DialogOptions, FocusScope};
pub use glassmere_scene::{FontFamily, TextSpan, TextStyle, TextWrap};
pub use glassmere_scene::{MonospaceMeasurer, TextMeasurer};
pub use image::{BackgroundImage, ImageFit, ImageOptions, ImageSource, ImageTint};
pub use locale_format::{Date, DateStyle, LocaleFormat, Time, Weekday};
#[cfg(feature = "std")]
pub use mailbox::{Mailbox, MessageSender, message_channel};
pub use messages::{BUILTIN_MESSAGES, MessageArg, Messages};
pub use panes::{
    Axis, PopoverOptions, SplitOptions, TOOLTIP_DELAY, Tab, Tooltip, TooltipPlacement,
};
#[cfg(feature = "std")]
pub use platform::{FileKind, FileRequest};
pub use platform::{Menu, MenuEntry, Notification, Tray};
pub use property_grid::{PropertyChoice, PropertyGridOptions, PropertyGridState, PropertyMsg};
pub use reactive::{GraphNode, Memo, Signal, StateKind, Store, TextSource};
pub use rich_document::{
    Block, BlockKind, InlineRun, Mark, Marks, RichDocument, RichMotion, RichPosition,
};
pub use rich_editor::{RichEditorOptions, RichEditorResponse, RichOp};
pub use rich_text::{RichRun, RichText};
pub use selection::{Indentation, Motion, Selection};
pub use style::{
    Align, AutoFlow, Direction, Edges, FocusRing, Gradient, GridArea, Justify, Length, Paint,
    ResolvedText, Shadow, StateStyles, Style, Track, TrackSize,
};
pub use stylesheet::{StyleSheet, StyleSheetError};
#[cfg(feature = "std")]
pub use task::{Cancel, TaskSender, Tasks};
#[cfg(feature = "std")]
pub use text_area::{CodeEditorOptions, FindState, Highlighter};
pub use text_area::{Diagnostic, EditorCommand, FindEdit, HighlightSpan, Severity, TextOp};
pub use theme::{TOKENS, Theme};
#[cfg(feature = "std")]
pub use toast::ToastSender;
pub use toast::{Toast, ToastAction, ToastId, ToastKind};
pub use tree::diagnostics::{
    DiagnosticKind, FrameDiagnostic, HUGE_CHILDREN, LAYER_AREA, RELAYOUT_FRAMES, REPAINT_FRAMES,
    REPAINT_MIN_NODES, UNSTABLE_ID_FRAMES,
};
pub use tree::{
    Action, CollectionInfo, Context, Dirty, EditorCharacter, EditorLine, EditorPoint,
    EditorSelection, EditorText, Event, FrameStats, GestureResponse, Key, Live, MemoryReport,
    NodeDetails, NodeId, NodeSnapshot, NumericValue, Pen, Role, ScrollState, Semantics, Swipe,
    SwipeDirection, TouchPhase,
};
pub use ui::{
    AreaBuilder, Autocomplete, Button, Calendar, Collapsible, ComboBox, CommandPalette, DatePicker,
    DockOptions, DockPanel, DragSourceBuilder, DropTargetBuilder, GestureBuilder, Label, Link,
    Progress, PropertyGrid, RangeSlider, Response, Spinner, Switch, TextInput, Ui, suggest,
};
pub use widget::{CustomWidget, WidgetBuilder, WidgetInput};
pub use window::{ResizeEdge, WindowCommand, WindowId, WindowInfo, WindowRequest};
