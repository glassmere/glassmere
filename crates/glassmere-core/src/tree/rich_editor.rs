use super::rich_text::line_boxes;
use super::text_area_input::insert_span;
use super::*;
use crate::rich_editor::{RichBlockView, RichGranularity, RichOp};
use crate::{BlockKind, EditorCommand, Mark, RichMotion, RichPosition, TextEdit, TextMove};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::OnceCell;
use glassmere_scene::{Affinity, FontFamily, TextGeometry};

/// Space between blocks; list items sit closer together.
fn gap(above: BlockKind, below: BlockKind) -> f32 {
    let list = |kind| matches!(kind, BlockKind::Bullet | BlockKind::Numbered);
    if list(above) && list(below) {
        4.0
    } else {
        10.0
    }
}

/// What a screen reader reads of one block, as `editor_text::build` makes it
/// for the block alone: numbered 0, with its top left corner at the origin and
/// no line break after it. `None` when the geometry does not cover the text,
/// which `build` reports the same way.
type Reading = Option<Box<[crate::EditorLine]>>;

struct BlockLayout {
    view: RichBlockView,
    /// Byte offset where preedit text was spliced in, and its length.
    preedit: Option<(usize, usize)>,
    geometry: TextGeometry,
    /// Top of the block in content coordinates.
    y: f32,
    height: f32,
    /// Space reserved for the list marker, quote bar or code padding.
    indent: f32,
    /// Where the text starts: after the indent, which is on the right in
    /// right-to-left blocks.
    left: f32,
    /// The reading of this block, made by the first `rich_editor_text` after
    /// the layout was built and kept for the life of the layout. It is a
    /// function of the text of `view.spans` and of `geometry`, and nothing
    /// changes either once the layout is built: `prepare_rich_editors` only
    /// moves a layout (`y`, `left`) and puts a view with equal spans in place
    /// of `view`. Whoever makes a layout reshape in place must empty this
    /// cell. What depends on where the block is (its paragraph number, the
    /// line break after it, its place in the window, the selection) is put on
    /// at every read and never stored here.
    reading: OnceCell<Reading>,
}

/// Retained shaping and view state; the document stays with the app.
#[derive(Default)]
pub(super) struct RichEditorState {
    layouts: Vec<BlockLayout>,
    width: f32,
    font_size: f32,
    rtl: bool,
    base: Option<bool>,
    preedit: Option<String>,
    /// Absolute caret rectangle, for the IME and paging.
    caret: Option<Rect>,
    cursor: Option<RichPosition>,
    last_click: Option<(Duration, Point, u8)>,
    /// Column that consecutive vertical moves return to, in content pixels.
    goal_x: Option<f32>,
    /// Which neighbour the caret belongs to, at direction boundaries and
    /// between wrapped lines. It follows the cursor; the document only knows
    /// offsets.
    affinity: Affinity,
    pending_affinity: Option<Affinity>,
    /// Where the moves queued since the last frame lead, so that a visual
    /// move queued after them starts there and not at the last shaped cursor.
    pending: Option<RichPosition>,
    /// How many readings were made, for the tests.
    #[cfg(test)]
    fills: core::cell::Cell<usize>,
}

impl RichEditorState {
    /// The cursor a visual move starts from, with its affinity: where the
    /// queued moves lead, or else the one the last frame shaped.
    fn origin(&self, shaped: RichPosition) -> (RichPosition, Affinity) {
        match self.pending {
            Some(position) => (position, self.pending_affinity.unwrap_or_default()),
            None => (shaped, self.affinity),
        }
    }
}

/// The editor's text style, aligned the way its paragraphs read: a forced
/// base direction wins over the direction of the window.
fn editor_style(node: &Node) -> TextStyle {
    let mut style = node.text_style();
    style.rtl = style.base.unwrap_or(style.rtl);
    style
}

/// Whether a block of `kind` is laid out from the right. Code reads left to
/// right whatever the UI direction is, unless a direction was forced.
fn mirrored(base: &TextStyle, kind: BlockKind) -> bool {
    base.rtl && (kind != BlockKind::Code || base.base.is_some())
}

fn block_style(base: &TextStyle, kind: BlockKind, font_size: f32) -> TextStyle {
    TextStyle {
        base: base.base,
        font_size: font_size * RichBlockView::font_scale(kind),
        family: if kind == BlockKind::Code {
            FontFamily::Monospace
        } else {
            base.family.clone()
        },
        weight: if matches!(kind, BlockKind::Heading(_)) {
            700
        } else {
            400
        },
        italic: false,
        wrap: TextWrap::Word,
        rtl: mirrored(base, kind),
        align: base.align,
    }
}

impl Context {
    pub(super) fn focused_rich(&self) -> Option<NodeId> {
        self.focused.filter(|id| {
            self.nodes
                .get(id)
                .is_some_and(|node| node.kind == Kind::RichEditor && node.enabled)
        })
    }

    pub(super) fn rich_accepts_text(&self) -> bool {
        self.focused_rich().is_some_and(|id| {
            matches!(
                self.nodes[&id].control,
                Control::RichEditor {
                    read_only: false,
                    ..
                }
            )
        })
    }

    pub(super) fn rich_composing(&self) -> bool {
        self.focused_rich()
            .and_then(|id| self.nodes[&id].rich_editor.as_ref())
            .is_some_and(|state| state.preedit.is_some())
    }

    pub(super) fn rich_caret(&self) -> Option<Rect> {
        let node = &self.nodes[&self.focused_rich()?];
        Some(node.rich_editor.as_ref()?.caret?.intersect(node.clip))
    }

    pub(super) fn rich_op(&mut self, id: NodeId, op: RichOp) {
        let vertical = matches!(op, RichOp::SetCursor { .. });
        if let Some(state) = self
            .nodes
            .get_mut(&id)
            .and_then(|node| node.rich_editor.as_mut())
        {
            state.pending_affinity = match op {
                // Typed text keeps the caret with the character just entered.
                RichOp::Insert(_) => Some(Affinity::Upstream),
                RichOp::SetCursor { .. } => state.pending_affinity,
                _ => None,
            };
            // Any other operation leaves the cursor where only the document knows.
            state.pending = match op {
                RichOp::SetCursor {
                    position,
                    granularity: RichGranularity::Char,
                    ..
                } => Some(position),
                _ => None,
            };
        }
        match self.updates.get_mut(&id) {
            Some(ControlUpdate::RichOps(ops)) => ops.push(op),
            _ => {
                self.queue_update(id, ControlUpdate::RichOps(vec![op]));
            }
        }
        if !vertical
            && let Some(state) = self
                .nodes
                .get_mut(&id)
                .and_then(|node| node.rich_editor.as_mut())
        {
            state.goal_x = None;
        }
        self.invalidate_paint(id);
    }

    /// The document position under a window point.
    fn rich_position(&self, id: NodeId, point: Point) -> Option<(RichPosition, Affinity)> {
        let node = self.nodes.get(&id)?;
        let state = node.rich_editor.as_ref()?;
        let content = node.content_box();
        let y = point.y - content.y + node.scroll.offset;
        let block = state
            .layouts
            .iter()
            .position(|layout| y < layout.y + layout.height + 5.0)
            .unwrap_or(state.layouts.len().checked_sub(1)?);
        let layout = &state.layouts[block];
        let (mut offset, affinity) = layout.geometry.hit_affinity(Point {
            x: point.x - content.x - layout.left,
            y: (y - layout.y).clamp(0.0, (layout.height - 1.0).max(0.0)),
        });
        if let Some((at, len)) = layout.preedit
            && offset > at
        {
            offset = offset.saturating_sub(len).max(at);
        }
        Some((
            RichPosition {
                block,
                offset: offset.min(layout.view.text.len()),
            },
            affinity,
        ))
    }

    /// Extends the selection of a held pointer to a point, taken at the edge
    /// of the text when it is outside.
    pub(super) fn rich_drag(&mut self, id: NodeId, point: Point) -> Option<bool> {
        let point = self.clamp_to_viewport(id, point)?;
        let (position, affinity) = self.rich_position(id, point)?;
        self.set_rich_affinity(id, affinity);
        self.rich_op(
            id,
            RichOp::SetCursor {
                position,
                select: true,
                granularity: RichGranularity::Char,
            },
        );
        Some(true)
    }

    fn set_rich_affinity(&mut self, id: NodeId, affinity: Affinity) {
        if let Some(state) = self
            .nodes
            .get_mut(&id)
            .and_then(|node| node.rich_editor.as_mut())
        {
            state.pending_affinity = Some(affinity);
        }
    }

    /// A visual Left or Right inside a block that contains right-to-left
    /// text. `None` leaves the move to the document's logical motions.
    fn rich_horizontal(
        &mut self,
        id: NodeId,
        right: bool,
        select: bool,
        word: bool,
    ) -> Option<RichOp> {
        if self.caret_movement != crate::CaretMovement::Visual {
            return None;
        }
        let node = self.nodes.get(&id)?;
        let Control::RichEditor { cursor, .. } = node.control else {
            return None;
        };
        let state = node.rich_editor.as_ref()?;
        let (cursor, from) = state.origin(cursor);
        let layout = state.layouts.get(cursor.block)?;
        if layout.preedit.is_some() || !layout.geometry.clusters.iter().any(|cluster| cluster.rtl) {
            return None;
        }
        let rtl_line = layout
            .geometry
            .line_at(cursor.offset, from)
            .is_some_and(|line| line.rtl);
        let logical = |forward: bool, word: bool| RichOp::Move {
            motion: match (forward, word) {
                (true, false) => RichMotion::Right,
                (false, false) => RichMotion::Left,
                (true, true) => RichMotion::WordRight,
                (false, true) => RichMotion::WordLeft,
            },
            select,
        };
        let Some((offset, affinity)) = layout.geometry.visual_step(cursor.offset, from, right)
        else {
            // Leaving the visual line: its right end is the logical end only when LTR.
            return Some(logical(right != rtl_line, word));
        };
        if word {
            let forward = offset > cursor.offset || (offset == cursor.offset && right != rtl_line);
            return Some(logical(forward, true));
        }
        self.set_rich_affinity(id, affinity);
        Some(RichOp::SetCursor {
            position: RichPosition {
                block: cursor.block,
                offset,
            },
            select,
            granularity: RichGranularity::Char,
        })
    }

    /// Where a visual-line key takes the cursor, from the shaped text.
    fn rich_visual(&mut self, id: NodeId, key: Key) -> Option<RichPosition> {
        let node = self.nodes.get(&id)?;
        let Control::RichEditor { cursor, .. } = node.control else {
            return None;
        };
        let state = node.rich_editor.as_ref()?;
        let (cursor, from) = state.origin(cursor);
        let layout = state.layouts.get(cursor.block)?;
        let caret = layout.geometry.caret_at(cursor.offset, from);
        let x = state.goal_x.unwrap_or(caret.x + layout.left);
        let page = node.content_box().height;
        let in_block = |block: usize, x: f32, y: f32| {
            let layout = &state.layouts[block];
            let (offset, affinity) = layout.geometry.hit_affinity(Point {
                x: x - layout.left,
                y,
            });
            (
                RichPosition {
                    block,
                    offset: offset.min(layout.view.text.len()),
                },
                affinity,
            )
        };
        // Logical start and end of the visual line; the end keeps an upstream
        // affinity, so the caret stays on this line when the text wraps.
        let edge = |end: bool| {
            let (offset, affinity) = layout.geometry.line_edge(cursor.offset, from, end);
            (
                RichPosition {
                    block: cursor.block,
                    offset,
                },
                affinity,
            )
        };
        let (target, affinity) = match key {
            Key::Home => edge(false),
            Key::End => edge(true),
            Key::ArrowUp if caret.y > 0.5 => in_block(cursor.block, x, caret.y - 1.0),
            Key::ArrowUp if cursor.block > 0 => {
                let above = state.layouts[cursor.block - 1].height;
                in_block(cursor.block - 1, x, above - 1.0)
            }
            Key::ArrowUp => (RichPosition::default(), Affinity::Downstream),
            Key::ArrowDown if caret.y + caret.height < layout.geometry.size.height - 0.5 => {
                in_block(cursor.block, x, caret.y + caret.height + 1.0)
            }
            Key::ArrowDown if cursor.block + 1 < state.layouts.len() => {
                in_block(cursor.block + 1, x, 1.0)
            }
            Key::ArrowDown => (
                RichPosition {
                    block: cursor.block,
                    offset: layout.view.text.len(),
                },
                Affinity::Upstream,
            ),
            Key::PageUp | Key::PageDown => {
                let delta = if key == Key::PageUp { -page } else { page };
                let origin = node.content_box();
                // The caret's middle on screen, from the cursor: the last
                // frame's caret rectangle is not where queued moves lead.
                let middle =
                    origin.y + layout.y + caret.y + caret.height / 2.0 - node.scroll.offset;
                self.rich_position(
                    id,
                    Point {
                        x: origin.x + x,
                        y: middle + delta,
                    },
                )?
            }
            _ => return None,
        };
        if matches!(
            key,
            Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown
        ) && let Some(state) = self
            .nodes
            .get_mut(&id)
            .and_then(|node| node.rich_editor.as_mut())
        {
            state.goal_x = Some(x);
        }
        self.set_rich_affinity(id, affinity);
        Some(target)
    }

    pub(super) fn rich_editor_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::PointerDown | Event::PointerDownWithShift => {
                if self.rich_editor_count == 0 {
                    return None;
                }
                let point = self.pointer?;
                let id = self.hit_test(point)?;
                let node = self.nodes.get(&id)?;
                if node.kind != Kind::RichEditor || !node.enabled {
                    return None;
                }
                let (position, affinity) = self.rich_position(id, point)?;
                self.set_rich_affinity(id, affinity);
                let now = self.now;
                let state = self.nodes.get_mut(&id)?.rich_editor.as_mut()?;
                let count = match state.last_click {
                    Some((at, last, count))
                        if now.saturating_sub(at) < Duration::from_millis(400)
                            && (last.x - point.x).abs() < 4.0
                            && (last.y - point.y).abs() < 4.0 =>
                    {
                        count % 3 + 1
                    }
                    _ => 1,
                };
                state.last_click = Some((now, point, count));
                self.focus_control(id);
                self.captured = Some(id);
                self.rich_op(
                    id,
                    RichOp::SetCursor {
                        position,
                        select: matches!(event, Event::PointerDownWithShift),
                        granularity: match count {
                            2 => RichGranularity::Word,
                            3 => RichGranularity::Block,
                            _ => RichGranularity::Char,
                        },
                    },
                );
                Some(true)
            }
            Event::PointerMoved(point) => {
                let id = self.captured?;
                if self.nodes.get(&id)?.kind != Kind::RichEditor {
                    return None;
                }
                self.pointer = Some(*point);
                self.rich_drag(id, *point)
            }
            Event::Editor(command) => {
                let id = self.editor_area()?;
                if self.nodes.get(&id)?.kind != Kind::RichEditor {
                    return None;
                }
                let mark = match command {
                    EditorCommand::ToggleBold => Mark::Bold,
                    EditorCommand::ToggleItalic => Mark::Italic,
                    EditorCommand::ToggleUnderline => Mark::Underline,
                    EditorCommand::ToggleStrikethrough => Mark::Strikethrough,
                    EditorCommand::ToggleCode => Mark::Code,
                    // Search and cursor commands belong to the code editor.
                    _ => return Some(false),
                };
                self.rich_op(id, RichOp::Toggle(mark));
                Some(true)
            }
            Event::Text(edit) => {
                let id = self.focused_rich()?;
                Some(self.rich_text_edit(id, edit))
            }
            Event::KeyDown { key, shift, .. } => {
                let id = self.focused_rich()?;
                let read_only = matches!(
                    self.nodes[&id].control,
                    Control::RichEditor {
                        read_only: true,
                        ..
                    }
                );
                let motion = |motion| RichOp::Move {
                    motion,
                    select: *shift,
                };
                if matches!(key, Key::ArrowLeft | Key::ArrowRight)
                    && let Some(op) =
                        self.rich_horizontal(id, *key == Key::ArrowRight, *shift, false)
                {
                    self.rich_op(id, op);
                    return Some(true);
                }
                let op = match key {
                    Key::ArrowLeft => motion(RichMotion::Left),
                    Key::ArrowRight => motion(RichMotion::Right),
                    Key::ArrowUp
                    | Key::ArrowDown
                    | Key::Home
                    | Key::End
                    | Key::PageUp
                    | Key::PageDown => RichOp::SetCursor {
                        position: self.rich_visual(id, *key)?,
                        select: *shift,
                        granularity: RichGranularity::Char,
                    },
                    Key::Backspace => RichOp::Backspace { word: false },
                    Key::Delete => RichOp::Delete { word: false },
                    Key::Enter if !read_only => RichOp::Newline,
                    _ => return None,
                };
                self.rich_op(id, op);
                Some(true)
            }
            Event::FocusLost => {
                if let Some(state) = self
                    .focused_rich()
                    .and_then(|id| self.nodes.get_mut(&id))
                    .and_then(|node| node.rich_editor.as_mut())
                {
                    state.preedit = None;
                }
                None
            }
            _ => None,
        }
    }

    fn rich_text_edit(&mut self, id: NodeId, edit: &TextEdit) -> bool {
        let preedit = |context: &mut Self, text: Option<String>| {
            if let Some(state) = context
                .nodes
                .get_mut(&id)
                .and_then(|node| node.rich_editor.as_mut())
            {
                state.preedit = text;
            }
            context.invalidate_paint(id);
        };
        let op = match edit {
            TextEdit::Insert(text) => RichOp::Insert(text.clone()),
            TextEdit::Backspace { word } => RichOp::Backspace { word: *word },
            TextEdit::Delete { word } => RichOp::Delete { word: *word },
            TextEdit::Move {
                direction,
                select,
                word,
            } => {
                if matches!(direction, TextMove::Left | TextMove::Right)
                    && let Some(op) =
                        self.rich_horizontal(id, *direction == TextMove::Right, *select, *word)
                {
                    self.rich_op(id, op);
                    return true;
                }
                let motion = match (direction, word) {
                    (TextMove::Left, false) => RichMotion::Left,
                    (TextMove::Left, true) => RichMotion::WordLeft,
                    (TextMove::Right, false) => RichMotion::Right,
                    (TextMove::Right, true) => RichMotion::WordRight,
                    (TextMove::DocumentStart, _) => RichMotion::DocumentStart,
                    (TextMove::DocumentEnd, _) => RichMotion::DocumentEnd,
                    (TextMove::Start | TextMove::End, _) => {
                        let key = if *direction == TextMove::Start {
                            Key::Home
                        } else {
                            Key::End
                        };
                        let Some(position) = self.rich_visual(id, key) else {
                            return false;
                        };
                        self.rich_op(
                            id,
                            RichOp::SetCursor {
                                position,
                                select: *select,
                                granularity: RichGranularity::Char,
                            },
                        );
                        return true;
                    }
                };
                RichOp::Move {
                    motion,
                    select: *select,
                }
            }
            TextEdit::SelectAll => RichOp::SelectAll,
            TextEdit::Undo => RichOp::Undo,
            TextEdit::Redo => RichOp::Redo,
            TextEdit::Select { .. } => return false,
            TextEdit::Preedit { text, .. } => {
                preedit(self, (!text.is_empty()).then(|| text.clone()));
                return true;
            }
            TextEdit::Commit(text) => {
                preedit(self, None);
                RichOp::Insert(text.clone())
            }
            TextEdit::CancelComposition => {
                preedit(self, None);
                return true;
            }
        };
        self.rich_op(id, op);
        true
    }

    /// Shapes the blocks that changed at the content width, stacks all of
    /// them, keeps the caret in view and publishes the scroll range.
    // The first frame and a change of width shape every block, not just the
    // visible ones, so documents of thousands of paragraphs would need
    // per-block virtualization.
    pub(super) fn prepare_rich_editors(
        &mut self,
        ids: &[NodeId],
        measurer: &mut impl TextMeasurer,
    ) {
        for id in ids.iter().copied() {
            // Operations still queued have not reached the document.
            let queued = self.updates.contains_key(&id);
            let node = self.nodes.get_mut(&id).unwrap();
            let Control::RichEditor {
                blocks,
                cursor,
                font_size,
                ..
            } = &node.control
            else {
                continue;
            };
            let base = editor_style(node);
            let area = node.content_box();
            let mut state = node.rich_editor.take().unwrap_or_default();
            if !queued {
                state.pending = None;
            }
            let same_frame = state.width == area.width
                && state.font_size == *font_size
                && state.rtl == base.rtl
                && state.base == base.base;
            let old = core::mem::take(&mut state.layouts);
            let mut changed = !same_frame || old.len() != blocks.len();
            // What shapes a block is its kind and spans; the number and the
            // rules are only painted, and come from the declared view again.
            let composing = state.preedit.as_ref().map(|_| cursor.block);
            let fits = |layout: &BlockLayout, index: usize, view: &RichBlockView| {
                same_frame
                    && layout.preedit.is_none()
                    && composing != Some(index)
                    && layout.view.kind == view.kind
                    && layout.view.spans == view.spans
            };
            // An edit touches one run of blocks, so the layouts before it and
            // after it carry over even when blocks were inserted or removed.
            let prefix = old
                .iter()
                .zip(blocks)
                .enumerate()
                .take_while(|(index, (layout, view))| fits(layout, *index, view))
                .count();
            let suffix = (0..old.len().min(blocks.len()) - prefix)
                .take_while(|back| {
                    let (was, now) = (old.len() - 1 - back, blocks.len() - 1 - back);
                    fits(&old[was], now, &blocks[now])
                })
                .count();
            let mut old: Vec<Option<BlockLayout>> = old.into_iter().map(Some).collect();
            let mut y = 0.0;
            for (index, view) in blocks.iter().enumerate() {
                let preedit = (index == cursor.block)
                    .then_some(state.preedit.as_ref())
                    .flatten();
                let carried = if index < prefix {
                    Some(index)
                } else if index >= blocks.len() - suffix {
                    Some(old.len() - (blocks.len() - index))
                } else {
                    None
                };
                let reused = carried.and_then(|was| old[was].take());
                let indent = RichBlockView::indent(view.kind);
                let mut layout = reused.unwrap_or_else(|| {
                    changed = true;
                    let style = block_style(&base, view.kind, *font_size);
                    let mut spans = view.spans.clone();
                    let splice = preedit.map(|text| {
                        let at = cursor.offset.min(view.text.len());
                        insert_span(&mut spans, at, text);
                        (at, text.len())
                    });
                    let geometry = measurer.geometry_rich(
                        &spans,
                        &style,
                        Some((area.width - indent).max(1.0)),
                    );
                    let mut shaped = view.clone();
                    shaped.spans = spans;
                    BlockLayout {
                        height: geometry.size.height.max(style.font_size * 1.4),
                        geometry,
                        view: shaped,
                        preedit: splice,
                        y: 0.0,
                        indent,
                        left: 0.0,
                        reading: OnceCell::new(),
                    }
                });
                layout.left = if mirrored(&base, view.kind) {
                    0.0
                } else {
                    indent
                };
                // A reused layout takes the number and rules declared now.
                if layout.preedit.is_none() {
                    layout.view = view.clone();
                }
                if index > 0 {
                    y += gap(blocks[index - 1].kind, view.kind);
                }
                changed |= layout.y != y;
                layout.y = y;
                y += layout.height;
                state.layouts.push(layout);
            }
            state.width = area.width;
            state.font_size = *font_size;
            state.rtl = base.rtl;
            state.base = base.base;
            let maximum = (y - area.height).max(0.0);
            changed |= node.scroll.maximum != maximum;
            node.scroll.maximum = maximum;
            if state.cursor != Some(*cursor) {
                state.affinity = state.pending_affinity.take().unwrap_or_default();
            }
            let affinity = state.affinity;
            let caret = state.layouts.get(cursor.block).map(|layout| {
                let caret = match layout.preedit {
                    Some((_, len)) => layout.geometry.caret(cursor.offset + len),
                    None => layout.geometry.caret_at(cursor.offset, affinity),
                };
                (layout.y + caret.y, caret.height, layout.left + caret.x)
            });
            if let Some((top, height, _)) = caret
                && (state.cursor != Some(*cursor) || state.preedit.is_some())
            {
                let offset = node.scroll.offset.min(top).max(top + height - area.height);
                changed |= offset != node.scroll.offset;
                node.scroll.offset = offset;
            }
            node.scroll.offset = node.scroll.offset.clamp(0.0, maximum);
            state.caret = caret.map(|(top, height, x)| Rect {
                x: area.x + x,
                y: area.y + top - node.scroll.offset,
                width: 2.0,
                height,
            });
            state.cursor = Some(*cursor);
            node.rich_editor = Some(state);
            if changed {
                self.invalidate_paint(id);
            }
        }
    }

    pub(super) fn paint_rich_editor(&self, id: NodeId, scene: &mut Scene) {
        let node = &self.nodes[&id];
        let (
            Control::RichEditor {
                anchor,
                cursor,
                font_size,
                ..
            },
            Some(state),
        ) = (&node.control, node.rich_editor.as_ref())
        else {
            return;
        };
        let theme = &self.theme;
        let focused = self.focused == Some(id);
        let area = node.content_box();
        let base = editor_style(node);
        let (start, end) = (*anchor.min(cursor), *anchor.max(cursor));
        scene.primitives.push(Primitive::PushClip(area));
        for (index, layout) in state.layouts.iter().enumerate() {
            let top = area.y + layout.y - node.scroll.offset;
            if top + layout.height < area.y || top > area.y + area.height {
                continue;
            }
            let kind = layout.view.kind;
            let style = block_style(&base, kind, *font_size);
            let origin = Point {
                x: area.x + layout.left,
                y: top,
            };
            match kind {
                BlockKind::Code => scene.primitives.push(Primitive::Quad {
                    rect: Rect {
                        x: area.x,
                        y: top - 4.0,
                        width: area.width,
                        height: layout.height + 8.0,
                    },
                    color: theme.track,
                    radius: 6.0,
                }),
                BlockKind::Quote => scene.primitives.push(Primitive::Quad {
                    rect: Rect {
                        x: if base.rtl {
                            area.x + area.width - 3.0
                        } else {
                            area.x
                        },
                        y: top,
                        width: 3.0,
                        height: layout.height,
                    },
                    color: theme.accent,
                    radius: 1.5,
                }),
                BlockKind::Bullet | BlockKind::Numbered => {
                    scene.primitives.push(Primitive::StyledText {
                        rect: Rect {
                            x: if base.rtl {
                                area.x + area.width - layout.indent + 4.0
                            } else {
                                area.x + 4.0
                            },
                            y: top,
                            width: layout.indent,
                            height: style.font_size * 1.4,
                        },
                        text: if kind == BlockKind::Bullet {
                            "•".into()
                        } else {
                            format!("{}.", layout.view.number)
                        },
                        style: TextStyle {
                            wrap: TextWrap::None,
                            rtl: false,
                            ..style.clone()
                        },
                        color: theme.text_muted,
                    });
                }
                _ => {}
            }
            // Spliced preedit text shifts everything after it.
            let shifted = |byte: usize| match layout.preedit {
                Some((at, len)) if byte > at => byte + len,
                _ => byte,
            };
            if start != end && index >= start.block && index <= end.block {
                let from = if index == start.block {
                    start.offset
                } else {
                    0
                };
                let to = if index == end.block {
                    end.offset
                } else {
                    layout.view.text.len()
                };
                let color = if focused {
                    theme.highlight
                } else {
                    theme.selection_inactive
                };
                let mut rects = layout.geometry.selection(shifted(from)..shifted(to));
                if rects.is_empty() && index < end.block {
                    // An empty block inside the selection still shows.
                    rects.push(Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 6.0,
                        height: layout.height,
                    });
                }
                for rect in rects {
                    scene.primitives.push(Primitive::Quad {
                        rect: rect.translated(origin),
                        color,
                        radius: 0.0,
                    });
                }
            }
            for rule in layout.view.rules.iter().filter(|rule| rule.code) {
                let range = shifted(rule.range.start)..shifted(rule.range.end);
                for rect in line_boxes(&layout.geometry, range) {
                    scene.primitives.push(Primitive::Quad {
                        rect: rect.translated(origin),
                        color: theme.track,
                        radius: 3.0,
                    });
                }
            }
            if !layout.view.text.is_empty() || layout.preedit.is_some() {
                scene.primitives.push(Primitive::RichText {
                    rect: Rect {
                        x: origin.x,
                        y: top,
                        width: (area.width - layout.indent).max(1.0),
                        height: layout.height,
                    },
                    spans: layout.view.spans.clone(),
                    style: style.clone(),
                    color: node.resolved.foreground,
                });
            }
            let mut rule = |range: core::ops::Range<usize>, at: f32, color: Color| {
                for rect in line_boxes(&layout.geometry, range) {
                    scene.primitives.push(Primitive::Quad {
                        rect: Rect {
                            y: rect.y + rect.height * at,
                            height: 1.0,
                            ..rect
                        }
                        .translated(origin),
                        color,
                        radius: 0.0,
                    });
                }
            };
            for decorated in &layout.view.rules {
                let range = shifted(decorated.range.start)..shifted(decorated.range.end);
                let color = if decorated.link {
                    theme.accent
                } else {
                    node.resolved.foreground
                };
                if decorated.underline || decorated.link {
                    rule(range.clone(), 0.88, color);
                }
                if decorated.strikethrough {
                    rule(range, 0.55, color);
                }
            }
            if let Some((at, len)) = layout.preedit {
                rule(at..at + len, 0.92, theme.text);
            }
        }
        if focused && let Some(caret) = state.caret {
            scene.primitives.push(Primitive::Quad {
                rect: caret,
                color: theme.text,
                radius: 0.0,
            });
            let direction = state
                .layouts
                .get(cursor.block)
                .filter(|layout| layout.preedit.is_none())
                .and_then(|layout| {
                    layout
                        .geometry
                        .mixed_direction(cursor.offset, state.affinity)
                });
            super::control_paint::caret_flag(scene, caret, direction, theme.text);
        }
        scene.primitives.push(Primitive::PopClip);
    }
}

/// What a screen reader reads of `layout` alone.
fn block_reading(layout: &BlockLayout) -> Reading {
    let paragraph = editor_text::Paragraph {
        text: layout
            .view
            .spans
            .iter()
            .map(|span| span.text.as_str())
            .collect(),
        geometry: &layout.geometry,
        origin: Point { x: 0.0, y: 0.0 },
        breaks: false,
    };
    let read = editor_text::build(
        0,
        &[paragraph],
        crate::EditorPoint::default(),
        crate::EditorPoint::default(),
    )?;
    Some(read.lines.into_boxed_slice())
}

/// A line of a reading as it is read in its block's place: numbered, moved to
/// where the block is, and ending in a line break when another block follows.
/// The copy is made with room for the break, which a clone would have to grow.
fn placed(
    line: &crate::EditorLine,
    paragraph: usize,
    origin: Point,
    breaks: bool,
) -> crate::EditorLine {
    let mut text = String::with_capacity(line.text.len() + usize::from(breaks));
    text.push_str(&line.text);
    let mut characters = Vec::with_capacity(line.characters.len() + usize::from(breaks));
    characters.extend_from_slice(&line.characters);
    if breaks {
        text.push('\n');
        characters.push(crate::EditorCharacter {
            len: 1,
            position: line.bounds.width,
            width: 0.0,
        });
    }
    crate::EditorLine {
        paragraph,
        start: line.start,
        text,
        bounds: Rect {
            x: origin.x + line.bounds.x,
            y: origin.y + line.bounds.y,
            ..line.bounds
        },
        rtl: line.rtl,
        characters,
        word_starts: line.word_starts.clone(),
    }
}

impl Context {
    /// The blocks as assistive technology reads them, once they have been
    /// shaped as declared. What a block reads as is made once for its layout;
    /// here it is put in its place.
    pub(super) fn rich_editor_text(&self, id: NodeId) -> Option<crate::EditorText> {
        let node = self.nodes.get(&id)?;
        let Control::RichEditor {
            blocks,
            anchor,
            cursor,
            ..
        } = &node.control
        else {
            return None;
        };
        let state = node.rich_editor.as_ref()?;
        if state.layouts.len() != blocks.len()
            || state
                .layouts
                .iter()
                .zip(blocks)
                .any(|(layout, block)| layout.preedit.is_none() && layout.view != *block)
        {
            return None;
        }
        // No block is no reading, not an empty one.
        if state.layouts.is_empty() {
            return None;
        }
        let content = node.content_box();
        let mut lines = Vec::with_capacity(state.layouts.len());
        for (index, layout) in state.layouts.iter().enumerate() {
            let reading = layout
                .reading
                .get_or_init(|| {
                    #[cfg(test)]
                    state.fills.set(state.fills.get() + 1);
                    block_reading(layout)
                })
                .as_deref()?;
            let origin = Point {
                x: content.x + layout.left,
                y: content.y + layout.y - node.scroll.offset,
            };
            let breaks = index + 1 < blocks.len();
            for (position, line) in reading.iter().enumerate() {
                lines.push(placed(
                    line,
                    index,
                    origin,
                    breaks && position + 1 == reading.len(),
                ));
            }
        }
        let point = |position: RichPosition| crate::EditorPoint {
            paragraph: position.block,
            byte: position.offset,
        };
        let composing = state
            .layouts
            .get(cursor.block)
            .and_then(|layout| layout.preedit);
        let (anchor, focus) = match composing {
            // While composing, the caret is after the text being composed.
            Some((at, len)) => {
                let caret = crate::EditorPoint {
                    paragraph: cursor.block,
                    byte: at + len,
                };
                (caret, caret)
            }
            None => (point(*anchor), point(*cursor)),
        };
        // The text of a block includes what is being composed in it.
        let inside = |point: crate::EditorPoint| {
            let layout = state.layouts.get(point.paragraph)?;
            let len = layout
                .view
                .spans
                .iter()
                .map(|span| span.text.len())
                .sum::<usize>();
            Some(crate::EditorPoint {
                byte: point.byte.min(len),
                ..point
            })
        };
        Some(crate::EditorText {
            lines,
            selection: inside(anchor)
                .zip(inside(focus))
                .map(|(anchor, focus)| crate::EditorSelection { anchor, focus }),
        })
    }

    /// The blocks read over again from their geometry, to compare what is kept
    /// with what it was made of.
    #[cfg(test)]
    fn rich_editor_text_uncached(&self, id: NodeId) -> Option<crate::EditorText> {
        let node = self.nodes.get(&id)?;
        let Control::RichEditor {
            blocks,
            anchor,
            cursor,
            ..
        } = &node.control
        else {
            return None;
        };
        let state = node.rich_editor.as_ref()?;
        if state.layouts.len() != blocks.len()
            || state
                .layouts
                .iter()
                .zip(blocks)
                .any(|(layout, block)| layout.preedit.is_none() && layout.view != *block)
        {
            return None;
        }
        let content = node.content_box();
        let paragraphs: Vec<_> = state
            .layouts
            .iter()
            .enumerate()
            .map(|(index, layout)| editor_text::Paragraph {
                text: layout
                    .view
                    .spans
                    .iter()
                    .map(|span| span.text.as_str())
                    .collect(),
                geometry: &layout.geometry,
                origin: Point {
                    x: content.x + layout.left,
                    y: content.y + layout.y - node.scroll.offset,
                },
                breaks: index + 1 < blocks.len(),
            })
            .collect();
        let point = |position: RichPosition| crate::EditorPoint {
            paragraph: position.block,
            byte: position.offset,
        };
        let composing = state
            .layouts
            .get(cursor.block)
            .and_then(|layout| layout.preedit);
        let (anchor, focus) = match composing {
            // While composing, the caret is after the text being composed.
            Some((at, len)) => {
                let caret = crate::EditorPoint {
                    paragraph: cursor.block,
                    byte: at + len,
                };
                (caret, caret)
            }
            None => (point(*anchor), point(*cursor)),
        };
        editor_text::build(0, &paragraphs, anchor, focus)
    }

    /// Whether a block holds text being composed.
    pub(super) fn rich_composing_in(&self, id: NodeId) -> bool {
        self.nodes
            .get(&id)
            .and_then(|node| node.rich_editor.as_ref())
            .is_some_and(|state| state.preedit.is_some())
    }

    /// Selects from `anchor` to `focus` in blocks and bytes.
    pub(super) fn rich_select(
        &mut self,
        id: NodeId,
        anchor: crate::EditorPoint,
        focus: crate::EditorPoint,
    ) {
        let cursor = |point: crate::EditorPoint, select| RichOp::SetCursor {
            position: RichPosition {
                block: point.paragraph,
                offset: point.byte,
            },
            select,
            granularity: RichGranularity::Char,
        };
        self.rich_op(id, cursor(anchor, false));
        if focus != anchor {
            self.rich_op(id, cursor(focus, true));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Action, App, AppDriver, Block, Direction, Effects, Event, Length, MonospaceMeasurer,
        RichDocument, RichEditorOptions, Style, Ui,
    };
    use glassmere_scene::Size;

    const ROOT: Size = Size::new(640.0, 480.0);

    struct Notes {
        document: RichDocument,
        /// The width of the editor, which a test changes to reshape it.
        width: f32,
        /// The direction every block is laid out in, whatever the window's.
        direction: Option<Direction>,
    }

    impl App for Notes {
        type Message = RichOp;

        fn update(&mut self, op: RichOp, _: &mut Effects<'_, RichOp>) {
            self.document.apply(op);
        }

        fn view(&self, ui: &mut Ui<'_, RichOp>) {
            ui.keyed("editor", |ui| {
                let mut style = Style::default()
                    .width(Length::Px(self.width))
                    .height(Length::Px(300.0))
                    .padding(12.0);
                style.base_direction = self.direction;
                ui.rich_editor(
                    &self.document,
                    RichEditorOptions {
                        label: "Notes".into(),
                        style,
                        toolbar: false,
                        ..RichEditorOptions::default()
                    },
                    |op| op,
                );
            });
        }
    }

    /// An editor in a window, driven the way a host drives it.
    struct Window {
        notes: Notes,
        driver: AppDriver<RichOp>,
        context: Context,
    }

    impl Window {
        fn new(document: RichDocument) -> Self {
            Self::laid_out(document, None)
        }

        fn laid_out(document: RichDocument, direction: Option<Direction>) -> Self {
            let mut window = Self {
                notes: Notes {
                    document,
                    width: 400.0,
                    direction,
                },
                driver: AppDriver::new(|| true),
                context: Context::new(),
            };
            window.frame();
            window.frame();
            window
        }

        /// Declares once more, which a change made outside `update` needs.
        fn frame(&mut self) {
            self.driver.invalidate();
            self.driver.frame(
                &mut self.notes,
                &mut self.context,
                ROOT,
                &mut MonospaceMeasurer,
            );
        }

        fn editor(&self) -> NodeId {
            self.context
                .nodes
                .iter()
                .find(|(_, node)| node.kind == Kind::RichEditor)
                .map(|(id, _)| *id)
                .expect("a rich editor")
        }

        fn state(&self) -> &RichEditorState {
            self.context.nodes[&self.editor()]
                .rich_editor
                .as_ref()
                .unwrap()
        }

        fn state_mut(&mut self) -> &mut RichEditorState {
            let id = self.editor();
            self.context
                .nodes
                .get_mut(&id)
                .unwrap()
                .rich_editor
                .as_mut()
                .unwrap()
        }

        /// The blocks as the editor has them declared, which a test changes
        /// to differ from the ones that were shaped.
        fn declared(&mut self) -> &mut Vec<RichBlockView> {
            let id = self.editor();
            match &mut self.context.nodes.get_mut(&id).unwrap().control {
                Control::RichEditor { blocks, .. } => blocks,
                _ => panic!("a rich editor"),
            }
        }

        /// How many blocks have been read since the editor was made.
        fn fills(&self) -> usize {
            self.state().fills.get()
        }

        fn scrolled(&self) -> f32 {
            self.context.nodes[&self.editor()].scroll.offset
        }

        /// What is read of the editor, which is what reading every block over
        /// again gives.
        fn read(&self) -> Option<crate::EditorText> {
            let id = self.editor();
            let read = self.context.editor_text(id);
            assert_eq!(read, self.context.rich_editor_text_uncached(id));
            read
        }

        fn text(&self) -> crate::EditorText {
            self.read().expect("the editor has been shaped")
        }

        /// Puts the caret at `position` and applies `op` there.
        fn edit(&mut self, position: RichPosition, op: RichOp) {
            self.notes.document.set_cursor(position, false);
            self.notes.document.apply(op);
            self.frame();
        }

        fn select(&mut self, from: RichPosition, to: RichPosition) {
            self.notes.document.set_cursor(from, false);
            self.notes.document.set_cursor(to, true);
        }
    }

    fn at(block: usize, offset: usize) -> RichPosition {
        RichPosition { block, offset }
    }

    /// Blocks that are a little over a line of text long, so that they wrap
    /// to two lines in the editor and to more in a narrow one.
    fn document(blocks: usize) -> RichDocument {
        RichDocument::from_blocks(
            (0..blocks)
                .map(|n| {
                    let text = format!("paragraph {n} {}", "word ".repeat(8));
                    Block::new(BlockKind::Paragraph, &text)
                })
                .collect(),
        )
    }

    fn lines_of(text: &crate::EditorText, block: usize) -> Vec<&crate::EditorLine> {
        text.lines
            .iter()
            .filter(|line| line.paragraph == block)
            .collect()
    }

    fn starts(text: &crate::EditorText, block: usize) -> Vec<usize> {
        lines_of(text, block)
            .iter()
            .map(|line| line.start)
            .collect()
    }

    #[test]
    fn what_is_read_of_a_block_is_made_once_and_put_in_place_at_every_read() {
        let mut window = Window::new(document(200));
        assert_eq!(window.fills(), 0);
        window.read();
        assert_eq!(window.fills(), 200);
        window.read();
        assert_eq!(window.fills(), 200);

        // The pointer, the focus, the caret and a declaration of the same
        // blocks leave every reading as it is.
        window
            .context
            .event(Event::PointerMoved(Point { x: 100.0, y: 100.0 }));
        window.frame();
        window.read();
        let editor = window.editor();
        assert!(window.context.action(editor, Action::Focus));
        window.frame();
        window.read();
        window.notes.document.set_cursor(at(100, 3), false);
        window.frame();
        window.read();
        window.frame();
        window.read();
        assert_eq!(window.fills(), 200);

        // A letter makes one block's reading again.
        window.notes.document.apply(RichOp::Insert("x".into()));
        window.frame();
        let text = window.text();
        assert_eq!(window.fills(), 201);
        assert!(
            lines_of(&text, 100)
                .iter()
                .any(|line| line.text.contains('x'))
        );

        // Enter in the middle of a block makes two, and the blocks after it
        // are numbered by where they are now.
        let before = window.text();
        let filled = window.fills();
        window.notes.document.apply(RichOp::Newline);
        window.frame();
        let after = window.text();
        assert_eq!(window.notes.document.blocks().len(), 201);
        assert!(window.fills() <= filled + 2);
        for (block, view) in window.notes.document.blocks().iter().enumerate() {
            let joined: String = lines_of(&after, block)
                .iter()
                .map(|line| line.text.as_str())
                .collect();
            let end = if block == 200 { "" } else { "\n" };
            assert_eq!(joined, format!("{}{end}", view.text()), "block {block}");
        }
        for block in 0..100 {
            assert_eq!(starts(&before, block), starts(&after, block));
        }
        for block in 101..200 {
            assert_eq!(starts(&before, block), starts(&after, block + 1));
        }

        // A block added at the end ends the one before it with a line break,
        // which is put on at the read.
        let before = window.text();
        let filled = window.fills();
        let last = window.notes.document.blocks().len() - 1;
        let end = window.notes.document.blocks()[last].len();
        window.edit(at(last, end), RichOp::Newline);
        let after = window.text();
        assert_eq!(window.fills(), filled + 1);
        let (was, now) = (
            before.lines.last().unwrap(),
            &after.lines[before.lines.len() - 1],
        );
        assert_eq!(now.text, format!("{}\n", was.text));
        assert_eq!(now.characters.len(), was.characters.len() + 1);
        let added = now.characters.last().unwrap();
        assert_eq!((added.len, added.width), (1, 0.0));

        // And taking it away again takes the line break off with no reading.
        let filled = window.fills();
        window.edit(at(last + 1, 0), RichOp::Backspace { word: false });
        let again = window.text();
        assert_eq!(window.fills(), filled);
        assert_eq!(again.lines.len(), before.lines.len());
        for (was, now) in before.lines.iter().zip(&again.lines) {
            assert_eq!((&was.text, &was.characters), (&now.text, &now.characters));
        }
    }

    #[test]
    fn scrolling_moves_the_lines_that_are_kept_and_reads_no_block_again() {
        let mut window = Window::new(document(200));
        let before = window.text();
        let filled = window.fills();
        let from = window.scrolled();
        window
            .context
            .event(Event::PointerMoved(Point { x: 100.0, y: 100.0 }));
        window.context.event(Event::Scroll {
            delta_x: 0.0,
            delta_y: 140.0,
        });
        window.frame();
        let after = window.text();
        let moved = window.scrolled() - from;
        assert!(moved > 100.0, "scrolled by {moved}");
        assert_eq!(window.fills(), filled);
        assert_eq!(before.lines.len(), after.lines.len());
        for (was, now) in before.lines.iter().zip(&after.lines) {
            assert!((now.bounds.y - (was.bounds.y - moved)).abs() < 0.01);
            assert_eq!(now.bounds.x, was.bounds.x);
            assert_eq!(now.bounds.width, was.bounds.width);
            assert_eq!(now.bounds.height, was.bounds.height);
            assert_eq!(now.paragraph, was.paragraph);
            assert_eq!(now.start, was.start);
            assert_eq!(now.text, was.text);
            assert_eq!(now.characters, was.characters);
            assert_eq!(now.word_starts, was.word_starts);
        }
    }

    #[test]
    fn a_new_width_and_an_edit_to_a_mark_make_the_readings_they_change_again() {
        let mut window = Window::new(document(200));
        let wide = window.text();
        assert_eq!(window.fills(), 200);

        // Every block is shaped again at another width.
        window.notes.width = 200.0;
        window.frame();
        let narrow = window.text();
        assert_eq!(window.fills(), 400);
        assert!(narrow.lines.len() > wide.lines.len());

        // Bold changes the spans of one block.
        window.select(at(50, 0), at(50, 5));
        window.notes.document.apply(RichOp::Toggle(Mark::Bold));
        window.frame();
        window.read();
        assert_eq!(window.fills(), 401);

        // Underline over a part of a block splits its spans, and over all of
        // it leaves them as they are: only the rules that are painted change,
        // and the block keeps what was read of it.
        window.select(at(60, 0), at(60, 5));
        window.notes.document.apply(RichOp::Toggle(Mark::Underline));
        window.frame();
        window.read();
        assert_eq!(window.fills(), 402);
        let all = window.notes.document.blocks()[70].len();
        window.select(at(70, 0), at(70, all));
        window.notes.document.apply(RichOp::Toggle(Mark::Underline));
        window.frame();
        window.read();
        assert_eq!(window.fills(), 402);
    }

    #[test]
    fn renumbering_a_list_makes_no_reading_again() {
        let items = (0..50)
            .map(|n| Block::new(BlockKind::Numbered, &format!("item {n}")))
            .collect();
        let mut window = Window::new(RichDocument::from_blocks(items));
        window.read();
        assert_eq!(window.fills(), 50);
        // Enter at the end of an item adds one item, and those after it
        // change their numbers and nothing else.
        window.edit(at(20, 7), RichOp::Newline);
        assert_eq!(window.notes.document.blocks().len(), 51);
        window.read();
        assert_eq!(window.fills(), 51);
    }

    #[test]
    fn text_being_composed_is_read_with_its_block_every_frame() {
        let mut window = Window::new(document(30));
        window.read();
        let editor = window.editor();
        assert!(window.context.action(editor, Action::Focus));
        let end = window.notes.document.blocks()[10].len();
        window.notes.document.set_cursor(at(10, end), false);
        window.frame();
        window.read();
        let filled = window.fills();

        window.context.event(Event::Text(TextEdit::Preedit {
            text: "ni".into(),
            selection: None,
        }));
        window.frame();
        let text = window.text();
        assert_eq!(window.fills(), filled + 1);
        let joined: String = lines_of(&text, 10)
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert!(joined.ends_with("ni\n"), "{joined:?}");
        // The caret is after what is composed, which the block's text does
        // not yet have.
        let caret = crate::EditorPoint {
            paragraph: 10,
            byte: end + 2,
        };
        assert_eq!(
            text.selection,
            Some(crate::EditorSelection {
                anchor: caret,
                focus: caret
            })
        );

        // A block with text being composed is shaped again by every frame.
        window.frame();
        window.read();
        assert_eq!(window.fills(), filled + 2);

        window
            .context
            .event(Event::Text(TextEdit::Commit("ni".into())));
        window.frame();
        let text = window.text();
        assert_eq!(window.fills(), filled + 3);
        let joined: String = lines_of(&text, 10)
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert!(joined.ends_with("ni\n"), "{joined:?}");
    }

    #[test]
    fn geometry_that_does_not_cover_the_text_is_not_read_and_not_read_again() {
        let mut window = Window::new(document(10));
        let id = window.editor();
        // Nothing has been read yet, so the reading of the block is made from
        // what is changed here.
        let layouts = &mut window.state_mut().layouts;
        layouts[3].geometry.lines.last_mut().unwrap().bytes.end -= 1;
        assert_eq!(window.context.editor_text(id), None);
        assert_eq!(window.fills(), 4);
        assert_eq!(window.read(), None);
        assert_eq!(window.fills(), 4);

        // Editing the block shapes it again, and the editor can be read.
        window.edit(at(3, 0), RichOp::Insert("x".into()));
        assert!(window.read().is_some());
    }

    #[test]
    fn blocks_that_are_not_the_ones_declared_are_not_read() {
        let mut window = Window::new(document(10));
        // A block that is not as declared is found before any block is read.
        window.state_mut().layouts[5].view.number += 1;
        assert_eq!(window.read(), None);
        assert_eq!(window.fills(), 0);
        window.state_mut().layouts[5].view.number -= 1;

        // So is a block declared that has not been shaped, and a block shaped
        // that is no longer declared.
        let last = window.declared().pop().unwrap();
        assert_eq!(window.read(), None);
        window.declared().push(last.clone());
        window.declared().push(last);
        assert_eq!(window.read(), None);
        window.declared().pop();
        assert_eq!(window.fills(), 0);
        assert!(window.read().is_some());
        assert_eq!(window.fills(), 10);

        // No block at all is not an editor with no lines.
        let id = window.editor();
        window.state_mut().layouts.clear();
        window.declared().clear();
        assert_eq!(window.context.editor_text(id), None);
        assert_eq!(window.read(), None);
    }

    #[test]
    fn blocks_laid_out_from_the_right_are_read_where_their_text_is() {
        // Indented blocks, which a right-to-left editor indents on the right
        // and starts at its left edge.
        let kinds = [
            BlockKind::Paragraph,
            BlockKind::Heading(2),
            BlockKind::Bullet,
            BlockKind::Numbered,
            BlockKind::Quote,
            BlockKind::Code,
            BlockKind::Bullet,
            BlockKind::Paragraph,
        ];
        for direction in [Direction::Ltr, Direction::Rtl] {
            let rtl = direction == Direction::Rtl;
            let blocks = kinds
                .iter()
                .enumerate()
                .map(|(n, kind)| Block::new(*kind, &format!("\u{5d0}\u{5d1}\u{5d2} item {n}")))
                .collect();
            let mut window = Window::laid_out(RichDocument::from_blocks(blocks), Some(direction));
            let layouts = &window.state().layouts;
            assert!(layouts.iter().any(|layout| layout.indent > 0.0));
            for layout in layouts {
                assert_eq!(layout.left, if rtl { 0.0 } else { layout.indent });
            }
            let text = window.text();
            assert!(text.lines.iter().all(|line| line.rtl == rtl));
            window.read();

            // The readings that are kept sit beside the one that was made again.
            window.edit(at(2, 0), RichOp::Insert("x".into()));
            window.text();
            assert_eq!(window.fills(), kinds.len() + 1);
        }
    }
}
