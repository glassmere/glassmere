//! What assistive technology reads of an editor: its shaped lines, character
//! by character, and where the selection is.

use super::*;
use crate::{EditorCharacter, EditorLine, EditorPoint, EditorText};
use glassmere_scene::{TextCluster, TextGeometry, TextLine};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// A character's length is a single byte in the platform's tree.
const LONGEST_CHARACTER: usize = 255;

/// One paragraph of an editor, shaped, and where its top left corner is.
pub(super) struct Paragraph<'a> {
    pub text: String,
    pub geometry: &'a TextGeometry,
    pub origin: Point,
    /// Another paragraph follows, so the last line ends in a line break.
    pub breaks: bool,
}

/// Where the extent of `bytes` lies, in the geometry's coordinates. A cluster
/// that spans several characters (a ligature) is shared out by bytes.
fn extent(clusters: &[&TextCluster], bytes: &Range<usize>) -> Option<(f32, f32)> {
    let first = clusters.partition_point(|cluster| cluster.bytes.end <= bytes.start);
    let mut result: Option<(f32, f32)> = None;
    for cluster in clusters[first..]
        .iter()
        .take_while(|cluster| cluster.bytes.start < bytes.end)
    {
        let rect = cluster.rect;
        let inside = cluster.bytes.start >= bytes.start && cluster.bytes.end <= bytes.end;
        let (from, to) = if inside {
            (rect.x, rect.x + rect.width)
        } else {
            let length = (cluster.bytes.end - cluster.bytes.start) as f32;
            let start = (bytes.start.max(cluster.bytes.start) - cluster.bytes.start) as f32;
            let end = (bytes.end.min(cluster.bytes.end) - cluster.bytes.start) as f32;
            let (start, end) = if cluster.rtl {
                (length - end, length - start)
            } else {
                (start, end)
            };
            (
                rect.x + rect.width * start / length,
                rect.x + rect.width * end / length,
            )
        };
        result = Some(result.map_or((from, to), |(a, b)| (a.min(from), b.max(to))));
    }
    result
}

/// Byte offsets where a word starts. Spaces belong to the word before them,
/// except at the start of the line, where they are a word of their own.
fn word_starts(text: &str) -> Vec<usize> {
    text.split_word_bound_indices()
        .enumerate()
        .filter(|(index, (_, word))| *index == 0 || !word.chars().all(char::is_whitespace))
        .map(|(_, (start, _))| start)
        .collect()
}

/// The shaped lines of `paragraphs`, which are numbered from `first`.
/// Positions outside them move to the nearest edge.
pub(super) fn build(
    first: usize,
    paragraphs: &[Paragraph],
    anchor: EditorPoint,
    focus: EditorPoint,
) -> Option<EditorText> {
    let last = paragraphs.len().checked_sub(1)?;
    let mut lines = Vec::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let geometry = paragraph.geometry;
        let whole = [TextLine {
            y: 0.0,
            height: geometry.size.height,
            rtl: false,
            bytes: 0..paragraph.text.len(),
        }];
        let shaped = if geometry.lines.is_empty() {
            &whole[..]
        } else {
            &geometry.lines[..]
        };
        if shaped.last()?.bytes.end != paragraph.text.len() {
            return None;
        }
        let mut clusters: Vec<&TextCluster> = geometry
            .clusters
            .iter()
            .filter(|cluster| !cluster.bytes.is_empty())
            .collect();
        clusters.sort_by_key(|cluster| cluster.bytes.start);
        for (position, line) in shaped.iter().enumerate() {
            let text = paragraph.text.get(line.bytes.clone())?;
            // Length, and where it starts and ends on screen.
            let mut spans = Vec::new();
            let mut end = None;
            for (offset, grapheme) in text.grapheme_indices(true) {
                let start = line.bytes.start + offset;
                let found = extent(&clusters, &(start..start + grapheme.len()));
                let (from, to) = found.unwrap_or_else(|| {
                    let x = end.unwrap_or_else(|| geometry.caret(line.bytes.start).x);
                    (x, x)
                });
                end = Some(to);
                // A cluster longer than a length can say is split.
                let mut piece = 0;
                while piece < grapheme.len() {
                    let mut stop = (piece + LONGEST_CHARACTER).min(grapheme.len());
                    while !grapheme.is_char_boundary(stop) {
                        stop -= 1;
                    }
                    let (from, to) = if piece == 0 { (from, to) } else { (to, to) };
                    spans.push((stop - piece, from, to));
                    piece = stop;
                }
            }
            let (left, right) = if spans.is_empty() {
                let x = geometry.caret(line.bytes.start).x;
                (x, x)
            } else {
                spans
                    .iter()
                    .fold((f32::MAX, f32::MIN), |(left, right), span| {
                        (left.min(span.1), right.max(span.2))
                    })
            };
            let mut characters: Vec<EditorCharacter> = spans
                .iter()
                .map(|&(len, from, to)| EditorCharacter {
                    len,
                    position: if line.rtl { right - to } else { from - left },
                    width: to - from,
                })
                .collect();
            let mut text = text.to_string();
            if paragraph.breaks && position + 1 == shaped.len() {
                text.push('\n');
                characters.push(EditorCharacter {
                    len: 1,
                    position: right - left,
                    width: 0.0,
                });
            }
            lines.push(EditorLine {
                paragraph: first + index,
                start: line.bytes.start,
                word_starts: word_starts(&text),
                text,
                bounds: Rect {
                    x: paragraph.origin.x + left,
                    y: paragraph.origin.y + line.y,
                    width: right - left,
                    height: line.height,
                },
                rtl: line.rtl,
                characters,
            });
        }
    }
    let clamp = |point: EditorPoint| {
        if point.paragraph < first {
            EditorPoint {
                paragraph: first,
                byte: 0,
            }
        } else if point.paragraph > first + last {
            EditorPoint {
                paragraph: first + last,
                byte: paragraphs[last].text.len(),
            }
        } else {
            EditorPoint {
                byte: point
                    .byte
                    .min(paragraphs[point.paragraph - first].text.len()),
                ..point
            }
        }
    };
    Some(EditorText {
        lines,
        anchor: clamp(anchor),
        focus: clamp(focus),
    })
}

impl Context {
    /// The lines of the editor `id` and its selection, when it has been
    /// shaped as it is declared. A password has none.
    pub(super) fn editor_text(&self, id: NodeId) -> Option<EditorText> {
        let node = self.nodes.get(&id)?;
        match node.kind {
            Kind::TextInput => {
                let editor = node.editor.as_ref()?;
                let (text, geometry, selection) = editor.readable()?;
                let content = node.content_box();
                let paragraph = Paragraph {
                    text,
                    geometry,
                    origin: Point {
                        x: content.x - editor.scroll_x + editor.align_x,
                        y: content.y,
                    },
                    breaks: false,
                };
                let point = |byte| EditorPoint { paragraph: 0, byte };
                build(
                    0,
                    &[paragraph],
                    point(selection.anchor),
                    point(selection.cursor),
                )
            }
            Kind::TextArea => self.area_editor_text(id),
            Kind::RichEditor => self.rich_editor_text(id),
            _ => None,
        }
    }

    /// Moves the caret of an editor, or selects from `anchor` to `focus`.
    pub(super) fn set_selection(
        &mut self,
        id: NodeId,
        anchor: EditorPoint,
        focus: EditorPoint,
    ) -> bool {
        let Some(kind) = self.nodes.get(&id).map(|node| node.kind) else {
            return false;
        };
        match kind {
            Kind::TextInput => {
                let Some(editor) = self.nodes.get(&id).and_then(|node| node.editor.as_ref()) else {
                    return false;
                };
                if editor.masked || editor.composing() {
                    return false;
                }
                self.focus_control(id);
                self.edit(
                    id,
                    TextEdit::Select {
                        anchor: anchor.byte,
                        cursor: focus.byte,
                    },
                );
                true
            }
            Kind::TextArea => {
                self.focus_control(id);
                self.area_select(id, anchor, focus);
                true
            }
            Kind::RichEditor => {
                self.focus_control(id);
                self.rich_select(id, anchor, focus);
                true
            }
            _ => false,
        }
    }
}
