use super::*;
use crate::RichRun;
use rust_gui_scene::{FontFamily, TextGeometry, TextSpan};
use std::ops::Range;

/// Retained shaping of a rich text block and its keyboard-active link.
#[derive(Default)]
pub(super) struct RichState {
    width: f32,
    geometry: TextGeometry,
    active: usize,
}

/// Renderer spans for runs; links take `link` as their colour.
pub(super) fn spans(runs: &[RichRun], link: Color) -> Vec<TextSpan> {
    runs.iter()
        .map(|run| TextSpan {
            text: run.text.clone(),
            color: run.color.or(run.link.is_some().then_some(link)),
            weight: run.bold.then_some(700),
            italic: run.italic.then_some(true),
            family: run.code.then_some(FontFamily::Monospace),
        })
        .collect()
}

fn ranges(runs: &[RichRun]) -> impl Iterator<Item = (&RichRun, Range<usize>)> {
    let mut at = 0;
    runs.iter().map(move |run| {
        let range = at..at + run.text.len();
        at = range.end;
        (run, range)
    })
}

fn content(node: &Node) -> Rect {
    let padding = node.style.padding.max(0.0);
    Rect {
        x: node.bounds.x + padding,
        y: node.bounds.y + padding,
        width: (node.bounds.width - padding * 2.0).max(0.0),
        height: (node.bounds.height - padding * 2.0).max(0.0),
    }
}

/// The content box the text is drawn in. Wrapped blocks are aligned by the
/// shaper; a single line in a wider right-to-left box moves to the right edge.
fn aligned(node: &Node, state: &RichState) -> Rect {
    let mut content = content(node);
    if node.resolved.rtl() && node.style.wrap == TextWrap::None {
        let shift = (content.width - state.geometry.size.width).max(0.0);
        content.x += shift;
        content.width -= shift;
    }
    content
}

/// One rectangle per visual line for the clusters of a byte range.
pub(super) fn line_boxes(geometry: &TextGeometry, range: Range<usize>) -> Vec<Rect> {
    let mut lines: Vec<Rect> = Vec::new();
    for rect in geometry.selection(range) {
        match lines.iter_mut().find(|line| line.y == rect.y) {
            Some(line) => *line = line.union(rect),
            None => lines.push(rect),
        }
    }
    lines
}

impl Context {
    pub(super) fn prepare_rich_text(&mut self, measurer: &mut impl TextMeasurer) {
        let frame = self.frame;
        let accent = self.theme.accent;
        let ids: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| (node.kind == Kind::RichText).then_some(*id))
            .collect();
        for id in ids {
            let node = self.nodes.get_mut(&id).unwrap();
            let Control::RichText { runs, links } = &node.control else {
                continue;
            };
            let width = content(node).width;
            let stale = node
                .rich
                .as_ref()
                .is_none_or(|state| state.width != width || node.changed_frame == frame);
            if !stale {
                continue;
            }
            let geometry =
                measurer.geometry_rich(&spans(runs, accent), &node.text_style(), Some(width));
            let links = *links;
            let state = node.rich.get_or_insert_with(Default::default);
            state.width = width;
            state.geometry = geometry;
            state.active = state.active.min(links.saturating_sub(1));
            self.invalidate_paint(id);
        }
    }

    pub(super) fn paint_rich_text(&self, id: NodeId, scene: &mut Scene, foreground: Color) {
        let node = &self.nodes[&id];
        let (Control::RichText { runs, .. }, Some(state)) = (&node.control, node.rich.as_ref())
        else {
            return;
        };
        let theme = &self.theme;
        let content = aligned(node, state);
        let origin = Point {
            x: content.x,
            y: content.y,
        };
        for (run, range) in ranges(runs) {
            let fill = run.background.or(run.code.then_some(theme.track));
            if let Some(color) = fill {
                for rect in line_boxes(&state.geometry, range) {
                    scene.primitives.push(Primitive::Quad {
                        rect: rect.translated(origin),
                        color,
                        radius: 3.0,
                    });
                }
            }
        }
        scene.primitives.push(Primitive::RichText {
            rect: content,
            spans: spans(runs, theme.accent),
            style: node.text_style(),
            color: foreground,
        });
        let mut link = 0;
        for (run, range) in ranges(runs) {
            let is_link = run.link.is_some();
            let active = is_link && self.focused == Some(id) && link == state.active;
            link += usize::from(is_link);
            if !(run.underline || run.strikethrough || is_link) {
                continue;
            }
            let color = run
                .color
                .unwrap_or(if is_link { theme.accent } else { foreground });
            for rect in line_boxes(&state.geometry, range) {
                let mut rule = |y: f32, height: f32| {
                    scene.primitives.push(Primitive::Quad {
                        rect: Rect { y, height, ..rect }.translated(origin),
                        color,
                        radius: 0.0,
                    });
                };
                if run.underline || is_link {
                    // The keyboard-active link gets a heavier underline.
                    rule(rect.y + rect.height - 3.0, if active { 2.5 } else { 1.0 });
                }
                if run.strikethrough {
                    rule(rect.y + rect.height * 0.55, 1.0);
                }
            }
        }
    }

    /// The link under a point, as an index among the block's links.
    fn link_at(&self, id: NodeId, point: Point) -> Option<usize> {
        let node = self.nodes.get(&id)?;
        let (Control::RichText { runs, .. }, Some(state)) = (&node.control, node.rich.as_ref())
        else {
            return None;
        };
        let content = aligned(node, state);
        let local = Point {
            x: point.x - content.x,
            y: point.y - content.y,
        };
        let mut link = 0;
        for (run, range) in ranges(runs) {
            if run.link.is_none() {
                continue;
            }
            if state
                .geometry
                .selection(range)
                .iter()
                .any(|rect| rect.contains(local))
            {
                return Some(link);
            }
            link += 1;
        }
        None
    }

    fn follow_link(&mut self, id: NodeId, link: usize) {
        if let Some(state) = self.nodes.get_mut(&id).and_then(|node| node.rich.as_mut()) {
            state.active = link;
        }
        self.queue_update(id, ControlUpdate::Choice(link));
        self.invalidate_paint(id);
    }

    pub(super) fn rich_text_event(&mut self, event: &Event) -> Option<bool> {
        match event {
            Event::PointerUp => {
                let id = self.captured?;
                let point = self.pointer?;
                let link = self.link_at(id, point)?;
                self.follow_link(id, link);
                // The generic release still runs and asks for the declaration.
                None
            }
            Event::KeyDown { key, .. } => {
                let id = self.focused?;
                let node = self.nodes.get(&id)?;
                let Control::RichText { links, .. } = node.control else {
                    return None;
                };
                if links == 0 || !node.enabled {
                    return None;
                }
                let active = node.rich.as_ref().map_or(0, |state| state.active);
                match key {
                    Key::Enter | Key::Space => self.follow_link(id, active),
                    Key::ArrowRight | Key::ArrowDown | Key::ArrowLeft | Key::ArrowUp => {
                        let forward = matches!(key, Key::ArrowRight | Key::ArrowDown);
                        let next = if forward {
                            (active + 1) % links
                        } else {
                            (active + links - 1) % links
                        };
                        if let Some(state) =
                            self.nodes.get_mut(&id).and_then(|node| node.rich.as_mut())
                        {
                            state.active = next;
                        }
                        self.invalidate_paint(id);
                    }
                    _ => return None,
                }
                Some(true)
            }
            _ => None,
        }
    }
}
