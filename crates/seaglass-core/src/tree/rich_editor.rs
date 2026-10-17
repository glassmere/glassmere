use super::rich_text::line_boxes;
use super::text_area_input::{content, insert_span};
use super::*;
use crate::rich_editor::{RichBlockView, RichGranularity, RichOp};
use crate::{BlockKind, EditorCommand, Mark, RichMotion, RichPosition, TextEdit, TextMove};
use seaglass_scene::{Affinity, FontFamily, TextGeometry};

/// Space between blocks; list items sit closer together.
fn gap(above: BlockKind, below: BlockKind) -> f32 {
    let list = |kind| matches!(kind, BlockKind::Bullet | BlockKind::Numbered);
    if list(above) && list(below) {
        4.0
    } else {
        10.0
    }
}

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
}

/// Retained shaping and view state; the document stays with the app.
#[derive(Default)]
pub(super) struct RichEditorState {
    layouts: Vec<BlockLayout>,
    width: f32,
    font_size: f32,
    rtl: bool,
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
        // Code reads left to right whatever the UI direction is.
        rtl: base.rtl && kind != BlockKind::Code,
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
        let content = content(node);
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
        let layout = state.layouts.get(cursor.block)?;
        if layout.preedit.is_some() || !layout.geometry.clusters.iter().any(|cluster| cluster.rtl) {
            return None;
        }
        let rtl_line = layout
            .geometry
            .line_at(cursor.offset, state.affinity)
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
        let Some((offset, affinity)) =
            layout
                .geometry
                .visual_step(cursor.offset, state.affinity, right)
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
        let layout = state.layouts.get(cursor.block)?;
        let caret = layout.geometry.caret_at(cursor.offset, state.affinity);
        let x = state.goal_x.unwrap_or(caret.x + layout.left);
        let page = content(node).height;
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
            let (offset, affinity) = layout
                .geometry
                .line_edge(cursor.offset, state.affinity, end);
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
                let caret = state.caret?;
                let delta = if key == Key::PageUp { -page } else { page };
                let origin = content(node).x;
                self.rich_position(
                    id,
                    Point {
                        x: origin + x,
                        y: caret.y + caret.height / 2.0 + delta,
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
                let (position, affinity) = self.rich_position(id, *point)?;
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

    /// Shapes every block at the content width, stacks them, keeps the caret
    /// in view and publishes the scroll range.
    // ponytail: all blocks are shaped, not just the visible ones; virtualize
    // by block if documents reach thousands of paragraphs.
    pub(super) fn prepare_rich_editors(&mut self, measurer: &mut impl TextMeasurer) {
        let ids: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| (node.kind == Kind::RichEditor).then_some(*id))
            .collect();
        for id in ids {
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
            let base = node.text_style();
            let area = content(node);
            let mut state = node.rich_editor.take().unwrap_or_default();
            let same_frame =
                state.width == area.width && state.font_size == *font_size && state.rtl == base.rtl;
            let mut old: Vec<Option<BlockLayout>> = std::mem::take(&mut state.layouts)
                .into_iter()
                .map(Some)
                .collect();
            let mut changed = !same_frame || old.len() != blocks.len();
            let mut y = 0.0;
            for (index, view) in blocks.iter().enumerate() {
                let preedit = (index == cursor.block)
                    .then_some(state.preedit.as_ref())
                    .flatten();
                let reused = old.get_mut(index).and_then(|slot| {
                    slot.take_if(|layout| {
                        same_frame
                            && layout.view == *view
                            && layout.preedit.is_none()
                            && preedit.is_none()
                    })
                });
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
                    }
                });
                layout.left = if base.rtl && view.kind != BlockKind::Code {
                    0.0
                } else {
                    indent
                };
                // Reused layouts compare against the declared view again next frame.
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
        let area = content(node);
        let base = node.text_style();
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
            let mut rule = |range: std::ops::Range<usize>, at: f32, color: Color| {
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
