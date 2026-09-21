use super::rich_text::line_boxes;
use super::text_area_input::{content, insert_span};
use super::*;
use crate::rich_editor::{RichBlockView, RichGranularity, RichOp};
use crate::{BlockKind, EditorCommand, Mark, RichMotion, RichPosition, TextEdit, TextMove};
use rust_gui_scene::{FontFamily, TextGeometry};

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
    indent: f32,
}

/// Retained shaping and view state; the document stays with the app.
#[derive(Default)]
pub(super) struct RichEditorState {
    layouts: Vec<BlockLayout>,
    width: f32,
    font_size: f32,
    preedit: Option<String>,
    /// Absolute caret rectangle, for the IME and paging.
    caret: Option<Rect>,
    cursor: Option<RichPosition>,
    last_click: Option<(Duration, Point, u8)>,
    /// Column that consecutive vertical moves return to, in content pixels.
    goal_x: Option<f32>,
}

fn block_style(base: &TextStyle, kind: BlockKind, font_size: f32) -> TextStyle {
    TextStyle {
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
        match self.updates.get_mut(&id) {
            Some(ControlUpdate::RichOps(ops)) => ops.push(op),
            _ => {
                self.updates.insert(id, ControlUpdate::RichOps(vec![op]));
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
    fn rich_position(&self, id: NodeId, point: Point) -> Option<RichPosition> {
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
        let mut offset = layout.geometry.hit(Point {
            x: point.x - content.x - layout.indent,
            y: (y - layout.y).clamp(0.0, (layout.height - 1.0).max(0.0)),
        });
        if let Some((at, len)) = layout.preedit
            && offset > at
        {
            offset = offset.saturating_sub(len).max(at);
        }
        Some(RichPosition {
            block,
            offset: offset.min(layout.view.text.len()),
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
        let caret = layout.geometry.caret(cursor.offset);
        let x = state.goal_x.unwrap_or(caret.x + layout.indent);
        let page = content(node).height;
        let in_block = |block: usize, x: f32, y: f32| {
            let layout = &state.layouts[block];
            RichPosition {
                block,
                offset: layout
                    .geometry
                    .hit(Point {
                        x: x - layout.indent,
                        y,
                    })
                    .min(layout.view.text.len()),
            }
        };
        let middle = caret.y + caret.height / 2.0;
        let target = match key {
            Key::Home => in_block(cursor.block, -1.0e6, middle),
            Key::End => {
                let end = in_block(cursor.block, 1.0e6, middle);
                // The offset after a wrapped line's last character is drawn at
                // the start of the next line; stay on this line instead.
                let wrapped = layout.geometry.caret(end.offset).y > caret.y + 0.5;
                let before = layout
                    .geometry
                    .clusters
                    .iter()
                    .find(|cluster| cluster.bytes.end == end.offset)
                    .map(|cluster| cluster.bytes.start);
                match before {
                    Some(offset) if wrapped => RichPosition { offset, ..end },
                    _ => end,
                }
            }
            Key::ArrowUp if caret.y > 0.5 => in_block(cursor.block, x, caret.y - 1.0),
            Key::ArrowUp if cursor.block > 0 => {
                let above = state.layouts[cursor.block - 1].height;
                in_block(cursor.block - 1, x, above - 1.0)
            }
            Key::ArrowUp => RichPosition::default(),
            Key::ArrowDown if caret.y + caret.height < layout.geometry.size.height - 0.5 => {
                in_block(cursor.block, x, caret.y + caret.height + 1.0)
            }
            Key::ArrowDown if cursor.block + 1 < state.layouts.len() => {
                in_block(cursor.block + 1, x, 1.0)
            }
            Key::ArrowDown => RichPosition {
                block: cursor.block,
                offset: layout.view.text.len(),
            },
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
                let position = self.rich_position(id, point)?;
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
                let position = self.rich_position(id, *point)?;
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
            let same_frame = state.width == area.width && state.font_size == *font_size;
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
                    }
                });
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
            let maximum = (y - area.height).max(0.0);
            changed |= node.scroll.maximum != maximum;
            node.scroll.maximum = maximum;
            let caret = state.layouts.get(cursor.block).map(|layout| {
                let shift = layout.preedit.map_or(0, |(_, len)| len);
                let caret = layout.geometry.caret(cursor.offset + shift);
                (layout.y + caret.y, caret.height, layout.indent + caret.x)
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
                x: area.x + layout.indent,
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
                        x: area.x,
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
                            x: area.x + 4.0,
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
        }
        scene.primitives.push(Primitive::PopClip);
    }
}
