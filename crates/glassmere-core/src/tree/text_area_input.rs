use super::*;
use crate::text_area::{AreaLine, Granularity, MarkKind, TextOp};
use crate::{EditorCommand, Motion, TextEdit, TextMove};
use glassmere_scene::{Affinity, FontFamily, TextGeometry, TextSpan};

struct LineLayout {
    line: AreaLine,
    /// Byte offset where preedit text was spliced in, and its length.
    preedit: Option<(usize, usize)>,
    geometry: TextGeometry,
}

/// Retained view state of a text area; the document itself stays with the app.
#[derive(Default)]
pub(super) struct AreaState {
    layouts: Vec<LineLayout>,
    first_line: usize,
    /// Width of the line numbers and marks left of the text.
    pub(super) gutter: f32,
    /// Advance of one digit, for right-aligned line numbers.
    digit: f32,
    line_height: f32,
    /// The primary cursor the last preparation saw, as `(line, byte)`.
    primary: Option<(usize, usize)>,
    preedit: Option<String>,
    caret: Option<Rect>,
    last_click: Option<(Duration, Point, u8)>,
    /// Escape was pressed: the next Tab moves focus instead of indenting.
    tab_escape: bool,
    /// Which neighbour the primary caret belongs to. The document only knows
    /// offsets, so this lives with the view and follows the primary cursor.
    affinity: Affinity,
    /// Affinity for the position the queued operations will lead to.
    pending_affinity: Option<Affinity>,
}

/// What a declaration needs to know about the previous frame.
#[derive(Clone, Copy)]
pub(crate) struct AreaView {
    pub scroll_y: f32,
    pub viewport: f32,
    pub primary: Option<(usize, usize)>,
}

fn text_style(font_size: f32) -> TextStyle {
    TextStyle {
        font_size,
        family: FontFamily::Monospace,
        wrap: TextWrap::None,
        ..TextStyle::default()
    }
}

impl Context {
    pub(crate) fn area_view(&self, id: NodeId) -> Option<AreaView> {
        let node = self.nodes.get(&id)?;
        Some(AreaView {
            scroll_y: node.scroll.offset,
            viewport: node.content_box().height,
            primary: node.area.as_ref().and_then(|area| area.primary),
        })
    }

    /// Queues a copy or cut on the focused multi-line editor, whose text lives
    /// in an app-owned document: the editor sends it as an operation, which
    /// the application applies and copies. False when another kind of node
    /// has focus.
    pub fn request_clipboard(&mut self, cut: bool) -> bool {
        if let Some(id) = self.focused_rich() {
            self.rich_op(id, crate::rich_editor::RichOp::Clipboard { cut });
            return true;
        }
        let Some(id) = self.focused_area() else {
            return false;
        };
        self.area_op(id, TextOp::Clipboard { cut });
        true
    }

    pub(super) fn focused_area(&self) -> Option<NodeId> {
        self.focused.filter(|id| {
            self.nodes
                .get(id)
                .is_some_and(|node| node.kind == Kind::TextArea && node.enabled)
        })
    }

    /// The text area of the code editor that holds the focus, whether the
    /// focus is in the text or in its find bar.
    pub(super) fn editor_area(&self) -> Option<NodeId> {
        if let Some(id) = self.focused_area().or_else(|| self.focused_rich()) {
            return Some(id);
        }
        let mut next = self.focused;
        while let Some(id) = next {
            let node = self.nodes.get(&id)?;
            if node.kind == Kind::CodeEditor {
                return node
                    .children_by_key
                    .get(&WidgetKey::Named("$area".into()))
                    .copied();
            }
            next = node.parent;
        }
        None
    }

    /// Whether `Event::Editor` commands currently have a target.
    pub fn editor_focused(&self) -> bool {
        self.editor_area().is_some()
    }

    /// The message of the diagnostic under the pointer.
    pub(super) fn area_diagnostic(&self, id: NodeId) -> Option<String> {
        let node = self.nodes.get(&id)?;
        let Control::TextArea { marks, .. } = &node.control else {
            return None;
        };
        let (line, byte) = self.area_position(id, self.pointer?)?;
        marks.iter().find_map(|mark| match &mark.kind {
            MarkKind::Diagnostic(_, message)
                if mark.line == line && mark.from <= byte && byte < mark.to.max(mark.from + 1) =>
            {
                Some(message.clone())
            }
            _ => None,
        })
    }

    pub(super) fn area_accepts_text(&self) -> bool {
        self.focused_area().is_some_and(|id| {
            matches!(
                self.nodes[&id].control,
                Control::TextArea {
                    read_only: false,
                    ..
                }
            )
        })
    }

    pub(super) fn area_composing(&self) -> bool {
        self.focused_area()
            .and_then(|id| self.nodes[&id].area.as_ref())
            .is_some_and(|area| area.preedit.is_some())
    }

    pub(super) fn area_caret(&self) -> Option<Rect> {
        let node = &self.nodes[&self.focused_area()?];
        Some(node.area.as_ref()?.caret?.intersect(node.clip))
    }

    pub(super) fn area_op(&mut self, id: NodeId, op: TextOp) {
        if let Some(area) = self.nodes.get_mut(&id).and_then(|node| node.area.as_mut()) {
            area.pending_affinity = match op {
                // Typed text keeps the caret with the character just entered.
                TextOp::Insert(_) => Some(Affinity::Upstream),
                // Set by whoever resolved the position from the shaped line.
                TextOp::Pointer { .. } => area.pending_affinity,
                _ => None,
            };
        }
        match self.updates.get_mut(&id) {
            Some(ControlUpdate::TextOps(ops)) => ops.push(op),
            _ => {
                self.queue_update(id, ControlUpdate::TextOps(vec![op]));
            }
        }
        self.invalidate_paint(id);
    }

    /// The `(line, byte)` under a point, clamped into the declared lines.
    fn area_position(&self, id: NodeId, point: Point) -> Option<(usize, usize)> {
        self.area_hit(id, point).map(|(line, byte, _)| (line, byte))
    }

    fn area_hit(&self, id: NodeId, point: Point) -> Option<(usize, usize, Affinity)> {
        let node = self.nodes.get(&id)?;
        let area = node.area.as_ref()?;
        let Control::TextArea { total_lines, .. } = node.control else {
            return None;
        };
        let content = node.content_box();
        let row = ((point.y - content.y + node.scroll.offset) / area.line_height.max(1.0)).floor();
        let line = (row.max(0.0) as usize).min(total_lines.saturating_sub(1));
        let Some(layout) = line
            .checked_sub(area.first_line)
            .and_then(|index| area.layouts.get(index))
        else {
            return Some((line, 0, Affinity::Downstream));
        };
        let x = point.x - content.x - area.gutter + node.scroll.offset_x;
        let (mut byte, affinity) = layout.geometry.hit_affinity(Point {
            x,
            y: area.line_height / 2.0,
        });
        // Positions inside spliced preedit text map back to the splice point.
        if let Some((at, len)) = layout.preedit {
            byte = if byte <= at {
                byte
            } else {
                byte.saturating_sub(len).max(at)
            };
        }
        Some((line, byte.min(layout.line.text.len()), affinity))
    }

    /// Extends the selection of a held pointer to a point. The point is taken
    /// at the edge of the text when it is outside, so that how far an edge
    /// scroll has gone, not how far the pointer is, decides what is selected.
    pub(super) fn area_drag(&mut self, id: NodeId, point: Point) -> Option<bool> {
        let point = self.clamp_to_viewport(id, point)?;
        let (line, byte, affinity) = self.area_hit(id, point)?;
        self.set_pending_affinity(id, affinity);
        self.area_op(
            id,
            TextOp::Pointer {
                line,
                byte,
                select: true,
                granularity: Granularity::Char,
                add: false,
            },
        );
        Some(true)
    }

    fn set_pending_affinity(&mut self, id: NodeId, affinity: Affinity) {
        if let Some(area) = self.nodes.get_mut(&id).and_then(|node| node.area.as_mut()) {
            area.pending_affinity = Some(affinity);
        }
    }

    /// A visual Left or Right for a single cursor on a line that contains
    /// right-to-left text. Other lines, several cursors and logical movement
    /// use the document's own motions.
    fn area_visual(&mut self, id: NodeId, right: bool, select: bool, word: bool) -> Option<TextOp> {
        if self.caret_movement != crate::CaretMovement::Visual {
            return None;
        }
        let node = self.nodes.get(&id)?;
        let Control::TextArea {
            primary,
            first_line,
            selection_count: 1,
            ..
        } = node.control
        else {
            return None;
        };
        let area = node.area.as_ref()?;
        let layout = area.layouts.get(primary.0.checked_sub(first_line)?)?;
        if layout.preedit.is_some() || !layout.geometry.clusters.iter().any(|cluster| cluster.rtl) {
            return None;
        }
        let step = layout.geometry.visual_step(primary.1, area.affinity, right);
        let rtl_line = layout.geometry.lines.first().is_some_and(|line| line.rtl);
        let Some((byte, affinity)) = step else {
            // Leaving the line: its right end is the logical end only when LTR.
            return Some(TextOp::Move {
                motion: if right != rtl_line {
                    Motion::Right
                } else {
                    Motion::Left
                },
                select,
            });
        };
        if word {
            // Inside a right-to-left run the arrow points against the text.
            let forward = byte > primary.1 || (byte == primary.1 && right != rtl_line);
            return Some(TextOp::Move {
                motion: if forward {
                    Motion::WordRight
                } else {
                    Motion::WordLeft
                },
                select,
            });
        }
        self.set_pending_affinity(id, affinity);
        Some(TextOp::Pointer {
            line: primary.0,
            byte,
            select,
            granularity: Granularity::Char,
            add: false,
        })
    }

    pub(super) fn text_area_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::PointerDown | Event::PointerDownWithShift => {
                let point = self.pointer?;
                let id = self.hit_test(point)?;
                let node = self.nodes.get(&id)?;
                if node.kind != Kind::TextArea || !node.enabled {
                    return None;
                }
                let (line, byte, affinity) = self.area_hit(id, point)?;
                self.set_pending_affinity(id, affinity);
                let now = self.now;
                let area = self.nodes.get_mut(&id)?.area.as_mut()?;
                let count = match area.last_click {
                    Some((at, last, count))
                        if now.saturating_sub(at) < Duration::from_millis(400)
                            && (last.x - point.x).abs() < 4.0
                            && (last.y - point.y).abs() < 4.0 =>
                    {
                        count % 3 + 1
                    }
                    _ => 1,
                };
                area.last_click = Some((now, point, count));
                area.tab_escape = false;
                self.focus_control(id);
                self.captured = Some(id);
                self.area_op(
                    id,
                    TextOp::Pointer {
                        line,
                        byte,
                        select: matches!(event, Event::PointerDownWithShift),
                        granularity: match count {
                            2 => Granularity::Word,
                            3 => Granularity::Line,
                            _ => Granularity::Char,
                        },
                        add: false,
                    },
                );
                Some(true)
            }
            Event::PointerMoved(point) => {
                let id = self.captured?;
                if self.nodes.get(&id)?.kind != Kind::TextArea {
                    return None;
                }
                self.pointer = Some(*point);
                self.area_drag(id, *point)
            }
            Event::Text(edit) => {
                let id = self.focused_area()?;
                Some(self.area_text(id, edit))
            }
            Event::Editor(EditorCommand::AddCursorAt(point)) => {
                let id = self.hit_test(*point)?;
                if self.nodes.get(&id)?.kind != Kind::TextArea {
                    return None;
                }
                let (line, byte) = self.area_position(id, *point)?;
                self.focus_control(id);
                self.area_op(
                    id,
                    TextOp::Pointer {
                        line,
                        byte,
                        select: false,
                        granularity: Granularity::Char,
                        add: true,
                    },
                );
                Some(true)
            }
            Event::Editor(command) => {
                let id = self.editor_area()?;
                if self.nodes.get(&id)?.kind != Kind::TextArea {
                    return None;
                }
                self.area_op(id, TextOp::Command(*command));
                Some(true)
            }
            // Escape in the find bar closes it; focus returns to the text.
            Event::KeyDown {
                key: Key::Escape, ..
            } if self.focused_area().is_none() && self.popup.is_none() => {
                let id = self
                    .editor_area()
                    .filter(|id| self.nodes[id].kind == Kind::TextArea)?;
                self.area_op(id, TextOp::Command(EditorCommand::CloseFind));
                Some(true)
            }
            Event::KeyDown { key, shift, .. } => {
                let id = self.focused_area()?;
                self.area_key(id, *key, *shift)
            }
            Event::FocusLost => {
                if let Some(area) = self
                    .focused_area()
                    .and_then(|id| self.nodes.get_mut(&id))
                    .and_then(|node| node.area.as_mut())
                {
                    area.preedit = None;
                }
                None
            }
            _ => None,
        }
    }

    fn area_text(&mut self, id: NodeId, edit: &TextEdit) -> bool {
        if let TextEdit::Move {
            direction: direction @ (TextMove::Left | TextMove::Right),
            select,
            word,
        } = edit
            && let Some(op) = self.area_visual(id, *direction == TextMove::Right, *select, *word)
        {
            self.area_op(id, op);
            return true;
        }
        let op = match edit {
            TextEdit::Insert(text) => TextOp::Insert(text.clone()),
            TextEdit::Backspace { word } => TextOp::Backspace { word: *word },
            TextEdit::Delete { word } => TextOp::Delete { word: *word },
            TextEdit::Move {
                direction,
                select,
                word,
            } => TextOp::Move {
                motion: match (direction, word) {
                    (TextMove::Left, false) => Motion::Left,
                    (TextMove::Left, true) => Motion::WordLeft,
                    (TextMove::Right, false) => Motion::Right,
                    (TextMove::Right, true) => Motion::WordRight,
                    (TextMove::Start, _) => Motion::LineStart,
                    (TextMove::End, _) => Motion::LineEnd,
                    (TextMove::DocumentStart, _) => Motion::DocumentStart,
                    (TextMove::DocumentEnd, _) => Motion::DocumentEnd,
                },
                select: *select,
            },
            TextEdit::SelectAll => TextOp::SelectAll,
            TextEdit::Undo => TextOp::Undo,
            TextEdit::Redo => TextOp::Redo,
            // Byte selections address single-line inputs only.
            TextEdit::Select { .. } => return false,
            TextEdit::Preedit { text, .. } => {
                let area = self.nodes.get_mut(&id).and_then(|node| node.area.as_mut());
                if let Some(area) = area {
                    area.preedit = (!text.is_empty()).then(|| text.clone());
                }
                self.invalidate_paint(id);
                return true;
            }
            TextEdit::Commit(text) => {
                if let Some(area) = self.nodes.get_mut(&id).and_then(|node| node.area.as_mut()) {
                    area.preedit = None;
                }
                TextOp::Insert(text.clone())
            }
            TextEdit::CancelComposition => {
                if let Some(area) = self.nodes.get_mut(&id).and_then(|node| node.area.as_mut()) {
                    area.preedit = None;
                }
                self.invalidate_paint(id);
                return true;
            }
        };
        self.area_op(id, op);
        true
    }

    fn area_key(&mut self, id: NodeId, key: Key, shift: bool) -> Option<bool> {
        let node = &self.nodes[&id];
        let Control::TextArea {
            read_only,
            selection_count,
            ..
        } = node.control
        else {
            return None;
        };
        let area = node.area.as_ref()?;
        let page = ((node.content_box().height / area.line_height.max(1.0)) as isize - 1).max(1);
        let escaped = area.tab_escape;
        let motion = |motion| TextOp::Move {
            motion,
            select: shift,
        };
        if matches!(key, Key::ArrowLeft | Key::ArrowRight)
            && let Some(op) = self.area_visual(id, key == Key::ArrowRight, shift, false)
        {
            self.area_op(id, op);
            return Some(true);
        }
        let op = match key {
            Key::ArrowLeft => motion(Motion::Left),
            Key::ArrowRight => motion(Motion::Right),
            Key::ArrowUp => motion(Motion::Up),
            Key::ArrowDown => motion(Motion::Down),
            Key::Home => motion(Motion::LineStart),
            Key::End => motion(Motion::LineEnd),
            Key::PageUp => motion(Motion::Lines(-page)),
            Key::PageDown => motion(Motion::Lines(page)),
            Key::Backspace => TextOp::Backspace { word: false },
            Key::Delete => TextOp::Delete { word: false },
            Key::Enter if !read_only => TextOp::Newline,
            // Keyboard users leave the editor with Escape, then Tab.
            Key::Tab if !read_only && !escaped => {
                if shift {
                    TextOp::Outdent
                } else {
                    TextOp::Indent
                }
            }
            Key::Escape => {
                if let Some(area) = self.nodes.get_mut(&id).and_then(|node| node.area.as_mut()) {
                    area.tab_escape = true;
                }
                if selection_count > 1 {
                    self.area_op(id, TextOp::CollapseSelections);
                    return Some(true);
                }
                // Dialogs and popups still see Escape.
                return None;
            }
            _ => return None,
        };
        if let Some(area) = self.nodes.get_mut(&id).and_then(|node| node.area.as_mut()) {
            area.tab_escape = false;
        }
        self.area_op(id, op);
        Some(true)
    }

    /// Shapes the declared lines, sizes the gutter, keeps the primary caret in
    /// view horizontally and publishes the scroll range.
    pub(super) fn prepare_text_areas(&mut self, measurer: &mut impl TextMeasurer) {
        let ids: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| (node.kind == Kind::TextArea).then_some(*id))
            .collect();
        for id in ids {
            let node = self.nodes.get_mut(&id).unwrap();
            let Control::TextArea {
                lines,
                first_line,
                total_lines,
                primary,
                font_size,
                line_numbers,
                ..
            } = &node.control
            else {
                continue;
            };
            let mut area = node.area.take().unwrap_or_default();
            let style = text_style(*font_size);
            let line_height = font_size * 1.4;
            let digit = measurer.measure_styled("0", &style, None).width;
            let gutter = if *line_numbers {
                // Room for a diagnostic dot on the left and a gap on the right.
                digit * (total_lines.to_string().len().max(2) as f32) + 26.0
            } else {
                0.0
            };
            let mut old: Vec<Option<LineLayout>> = std::mem::take(&mut area.layouts)
                .into_iter()
                .map(Some)
                .collect();
            let old_first = area.first_line;
            let mut changed = gutter != area.gutter || *first_line != old_first;
            for (index, line) in lines.iter().enumerate() {
                let number = first_line + index;
                let preedit = (number == primary.0)
                    .then_some(area.preedit.as_ref())
                    .flatten();
                let reused = number
                    .checked_sub(old_first)
                    .and_then(|slot| old.get_mut(slot))
                    .and_then(|slot| {
                        slot.take_if(|layout| {
                            layout.line == *line && layout.preedit.is_none() && preedit.is_none()
                        })
                    });
                area.layouts.push(reused.unwrap_or_else(|| {
                    changed = true;
                    let mut spans = line.spans.clone();
                    let splice = preedit.map(|text| {
                        let at = primary.1.min(line.text.len());
                        insert_span(&mut spans, at, text);
                        (at, text.len())
                    });
                    LineLayout {
                        geometry: measurer.geometry_rich(&spans, &style, None),
                        line: AreaLine {
                            text: line.text.clone(),
                            spans,
                        },
                        preedit: splice,
                    }
                }));
            }
            area.first_line = *first_line;
            area.gutter = gutter;
            area.digit = digit;
            area.line_height = line_height;

            if area.primary != Some(*primary) {
                area.affinity = area.pending_affinity.take().unwrap_or_default();
            }
            let affinity = area.affinity;
            let content = node.content_box();
            let text_width = (content.width - gutter).max(1.0);
            let mut offset_x = node.scroll.offset_x;
            let caret = primary
                .0
                .checked_sub(*first_line)
                .and_then(|index| area.layouts.get(index))
                .map(|layout| match layout.preedit {
                    Some((_, len)) => layout.geometry.caret(primary.1 + len),
                    None => layout.geometry.caret_at(primary.1, affinity),
                });
            if let Some(caret) = caret
                && (area.primary != Some(*primary) || area.preedit.is_some())
            {
                offset_x = offset_x
                    .min(caret.x)
                    .max(caret.x - text_width + digit * 2.0)
                    .max(0.0);
            }
            // Lines are not wrapped; the widest one declared sets the range,
            // with room for the caret after its last character.
            let widest = area
                .layouts
                .iter()
                .map(|layout| layout.geometry.size.width)
                .fold(0.0, f32::max);
            let maximum_x = if widest > text_width {
                widest + digit * 2.0 - text_width
            } else {
                0.0
            };
            let offset_x = offset_x.clamp(0.0, maximum_x);
            changed |= node.scroll.offset_x != offset_x || node.scroll.maximum_x != maximum_x;
            node.scroll.offset_x = offset_x;
            node.scroll.maximum_x = maximum_x;
            area.caret = caret.map(|caret| Rect {
                x: content.x + gutter + caret.x - offset_x,
                y: content.y + primary.0 as f32 * line_height - node.scroll.offset,
                width: 2.0,
                height: line_height,
            });
            area.primary = Some(*primary);
            let maximum = (*total_lines as f32 * line_height - content.height).max(0.0);
            changed |= node.scroll.maximum != maximum;
            node.scroll.maximum = maximum;
            node.scroll.offset = node.scroll.offset.clamp(0.0, maximum);
            node.area = Some(area);
            if changed {
                self.invalidate_paint(id);
            }
        }
    }

    pub(super) fn paint_text_area(&self, id: NodeId, scene: &mut Scene) {
        let node = &self.nodes[&id];
        let (
            Control::TextArea {
                first_line,
                selections,
                primary,
                font_size,
                line_numbers,
                marks,
                gutter_marks,
                ..
            },
            Some(area),
        ) = (&node.control, node.area.as_ref())
        else {
            return;
        };
        let theme = &self.theme;
        let focused = self.focused == Some(id);
        let content = node.content_box();
        let line_height = area.line_height;
        let text_x = content.x + area.gutter;
        let origin_y = content.y - node.scroll.offset;
        let row = |line: usize| origin_y + line as f32 * line_height;
        let visible = |y: f32| y + line_height > content.y && y < content.y + content.height;
        let style = text_style(*font_size);
        scene.primitives.push(Primitive::PushClip(content));
        let caret_only = selections
            .iter()
            .all(|selection| selection.start == selection.end);
        if focused && caret_only && visible(row(primary.0)) {
            let text = theme.text;
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: content.x,
                    y: row(primary.0),
                    width: content.width,
                    height: line_height,
                },
                color: Color(text.0, text.1, text.2, 16),
                radius: 0.0,
            });
        }
        if *line_numbers {
            for index in 0..area.layouts.len() {
                let line = first_line + index;
                if !visible(row(line)) {
                    continue;
                }
                let label = (line + 1).to_string();
                let width = area.digit * label.len() as f32;
                scene.primitives.push(Primitive::StyledText {
                    rect: Rect {
                        x: content.x + (area.gutter - 12.0 - width).max(0.0),
                        y: row(line),
                        width: width + 4.0,
                        height: line_height,
                    },
                    text: label,
                    style: style.clone(),
                    color: if line == primary.0 && focused {
                        theme.text
                    } else {
                        theme.text_muted
                    },
                });
            }
        }
        for (line, severity) in gutter_marks {
            if visible(row(*line)) {
                scene.primitives.push(Primitive::Quad {
                    rect: Rect {
                        x: content.x + 2.0,
                        y: row(*line) + line_height / 2.0 - 3.0,
                        width: 6.0,
                        height: 6.0,
                    },
                    color: severity.color(),
                    radius: 3.0,
                });
            }
        }
        scene.primitives.push(Primitive::PushClip(Rect {
            x: text_x,
            width: (content.width - area.gutter).max(0.0),
            ..content
        }));
        let origin_x = text_x - node.scroll.offset_x;
        let layout_of = |line: usize| {
            line.checked_sub(*first_line)
                .and_then(|index| area.layouts.get(index))
        };
        // Search matches and bracket marks sit under the selection and the text.
        let accent = theme.accent;
        for mark in marks {
            let color = match mark.kind {
                MarkKind::Match => theme.selection_inactive,
                MarkKind::CurrentMatch => Color(accent.0, accent.1, accent.2, 120),
                MarkKind::Bracket => Color(theme.text.0, theme.text.1, theme.text.2, 56),
                MarkKind::Diagnostic(..) => continue,
            };
            let Some(layout) = layout_of(mark.line).filter(|_| visible(row(mark.line))) else {
                continue;
            };
            for rect in layout.geometry.selection(mark.from..mark.to) {
                scene.primitives.push(Primitive::Quad {
                    rect: Rect {
                        x: origin_x + rect.x,
                        y: row(mark.line),
                        width: rect.width,
                        height: line_height,
                    },
                    color,
                    radius: 2.0,
                });
            }
        }
        for selection in selections {
            if selection.start == selection.end {
                continue;
            }
            for line in selection.start.0..=selection.end.0 {
                let Some(layout) = layout_of(line).filter(|_| visible(row(line))) else {
                    continue;
                };
                let from = if line == selection.start.0 {
                    selection.start.1
                } else {
                    0
                };
                let to = if line == selection.end.0 {
                    selection.end.1
                } else {
                    layout.line.text.len()
                };
                let color = if focused {
                    theme.highlight
                } else {
                    theme.selection_inactive
                };
                for rect in layout.geometry.selection(from..to) {
                    scene.primitives.push(Primitive::Quad {
                        rect: Rect {
                            x: origin_x + rect.x,
                            y: row(line),
                            width: rect.width,
                            height: line_height,
                        },
                        color,
                        radius: 0.0,
                    });
                }
                // A selected line break shows as a short tail after the text.
                if line < selection.end.0 {
                    scene.primitives.push(Primitive::Quad {
                        rect: Rect {
                            x: origin_x + layout.geometry.size.width,
                            y: row(line),
                            width: font_size * 0.5,
                            height: line_height,
                        },
                        color,
                        radius: 0.0,
                    });
                }
            }
        }
        for (index, layout) in area.layouts.iter().enumerate() {
            let line = first_line + index;
            if !visible(row(line)) || layout.line.text.is_empty() && layout.preedit.is_none() {
                continue;
            }
            scene.primitives.push(Primitive::RichText {
                rect: Rect {
                    x: origin_x,
                    y: row(line),
                    width: layout.geometry.size.width + 2.0,
                    height: line_height,
                },
                spans: layout.line.spans.clone(),
                style: style.clone(),
                color: node.resolved.foreground,
            });
            if let Some((at, len)) = layout.preedit {
                let (from, to) = (layout.geometry.caret(at), layout.geometry.caret(at + len));
                scene.primitives.push(Primitive::Quad {
                    rect: Rect {
                        x: origin_x + from.x,
                        y: row(line) + line_height - 2.0,
                        width: (to.x - from.x).max(1.0),
                        height: 1.0,
                    },
                    color: theme.text,
                    radius: 0.0,
                });
            }
        }
        for mark in marks {
            let MarkKind::Diagnostic(severity, _) = &mark.kind else {
                continue;
            };
            let Some(layout) = layout_of(mark.line).filter(|_| visible(row(mark.line))) else {
                continue;
            };
            let (from, to) = (
                layout.geometry.caret(mark.from),
                layout.geometry.caret(mark.to),
            );
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: origin_x + from.x.min(to.x),
                    y: row(mark.line) + line_height - 3.0,
                    width: (to.x - from.x).abs().max(font_size * 0.5),
                    height: 2.0,
                },
                color: severity.color(),
                radius: 1.0,
            });
        }
        if focused {
            for selection in selections {
                let Some(layout) = layout_of(selection.cursor.0) else {
                    continue;
                };
                let shift = layout
                    .preedit
                    .filter(|_| selection.primary)
                    .map_or(0, |(_, len)| len);
                let affinity = if selection.primary && shift == 0 {
                    area.affinity
                } else {
                    Affinity::Downstream
                };
                let caret = layout
                    .geometry
                    .caret_at(selection.cursor.1 + shift, affinity);
                let rect = Rect {
                    x: origin_x + caret.x,
                    y: row(selection.cursor.0),
                    width: 2.0,
                    height: line_height,
                };
                scene.primitives.push(Primitive::Quad {
                    rect,
                    color: theme.text,
                    radius: 0.0,
                });
                super::control_paint::caret_flag(
                    scene,
                    rect,
                    layout
                        .geometry
                        .mixed_direction(selection.cursor.1 + shift, affinity),
                    theme.text,
                );
            }
        }
        scene.primitives.push(Primitive::PopClip);
        scene.primitives.push(Primitive::PopClip);
    }
}

/// Inserts `text` as its own span at byte `at` of the concatenated spans.
pub(super) fn insert_span(spans: &mut Vec<TextSpan>, mut at: usize, text: &str) {
    let mut index = 0;
    while index < spans.len() && at > spans[index].text.len() {
        at -= spans[index].text.len();
        index += 1;
    }
    let inserted = TextSpan::new(text);
    if index == spans.len() {
        spans.push(inserted);
        return;
    }
    let tail = spans[index].text.split_off(at);
    let rest = TextSpan {
        text: tail,
        ..spans[index].clone()
    };
    spans.insert(index + 1, inserted);
    spans.insert(index + 2, rest);
    spans.retain(|span| !span.text.is_empty());
}

impl Context {
    /// The declared lines as assistive technology reads them, once they have
    /// been shaped as declared. The primary selection is the one reported.
    pub(super) fn area_editor_text(&self, id: NodeId) -> Option<crate::EditorText> {
        let node = self.nodes.get(&id)?;
        let Control::TextArea {
            lines,
            first_line,
            total_lines,
            selections,
            primary,
            ..
        } = &node.control
        else {
            return None;
        };
        let area = node.area.as_ref()?;
        if area.first_line != *first_line
            || area.layouts.len() != lines.len()
            || area
                .layouts
                .iter()
                .zip(lines)
                .any(|(layout, line)| layout.line.text != line.text)
        {
            return None;
        }
        let content = node.content_box();
        let paragraphs: Vec<_> = area
            .layouts
            .iter()
            .enumerate()
            .map(|(index, layout)| editor_text::Paragraph {
                text: layout
                    .line
                    .spans
                    .iter()
                    .map(|span| span.text.as_str())
                    .collect(),
                geometry: &layout.geometry,
                origin: Point {
                    x: content.x + area.gutter - node.scroll.offset_x,
                    y: content.y + (first_line + index) as f32 * area.line_height
                        - node.scroll.offset,
                },
                breaks: first_line + index + 1 < *total_lines,
            })
            .collect();
        let point = |(paragraph, byte)| crate::EditorPoint { paragraph, byte };
        let (anchor, focus) = match (
            area.layouts.iter().find_map(|layout| layout.preedit),
            selections.iter().find(|selection| selection.primary),
        ) {
            // While composing, the caret is after the text being composed.
            (Some((at, len)), _) => ((primary.0, at + len), (primary.0, at + len)),
            (None, Some(selection)) if selection.cursor == selection.start => {
                (selection.end, selection.start)
            }
            (None, Some(selection)) => (selection.start, selection.end),
            (None, None) => (*primary, *primary),
        };
        editor_text::build(*first_line, &paragraphs, point(anchor), point(focus))
    }

    /// Whether the area holds text being composed.
    pub(super) fn area_composing_in(&self, id: NodeId) -> bool {
        self.nodes
            .get(&id)
            .and_then(|node| node.area.as_ref())
            .is_some_and(|area| area.preedit.is_some())
    }

    /// Selects from `anchor` to `focus` in document lines and bytes.
    pub(super) fn area_select(
        &mut self,
        id: NodeId,
        anchor: crate::EditorPoint,
        focus: crate::EditorPoint,
    ) {
        let pointer = |point: crate::EditorPoint, select| TextOp::Pointer {
            line: point.paragraph,
            byte: point.byte,
            select,
            granularity: Granularity::Char,
            add: false,
        };
        self.area_op(id, pointer(anchor, false));
        if focus != anchor {
            self.area_op(id, pointer(focus, true));
        }
    }
}
