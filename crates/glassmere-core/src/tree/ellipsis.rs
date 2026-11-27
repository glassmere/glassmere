//! A line of text that is wider than its box, ended with an ellipsis.

use super::*;
use alloc::string::String;
use unicode_segmentation::UnicodeSegmentation;

/// What `TextOverflow::Ellipsis` made of a node's text the last time it was
/// worked out, and what it was worked out for: it is done again only when the
/// room, the text or the style is not the same.
pub(super) struct Ellipsis {
    available: f32,
    text: String,
    style: TextStyle,
    /// The text cut short and ended with `…`, or `None` when it all fits.
    shown: Option<String>,
}

const ELLIPSIS: &str = "\u{2026}";

impl Node {
    /// Whether this node ends an overlong line with an ellipsis.
    pub(super) fn ellipsizes(&self) -> bool {
        self.style.text_overflow == crate::TextOverflow::Ellipsis
            && self.style.wrap == TextWrap::None
            && self.kind.text_leaf()
    }

    /// The text the node draws: all of it, or the part that fits and `…`.
    pub(super) fn shown_text(&self) -> &str {
        self.ellipsis
            .as_ref()
            .and_then(|ellipsis| ellipsis.shown.as_deref())
            .unwrap_or(&self.text)
    }
}

/// The longest start of `text` that, with `…` after it, fits in `available`,
/// cut between grapheme clusters; `None` when all of `text` fits.
fn fit(
    text: &str,
    style: &TextStyle,
    available: f32,
    measurer: &mut impl TextMeasurer,
) -> Option<String> {
    let mut width = |text: &str| measurer.measure_styled(text, style, None).width;
    if width(text) <= available + 0.01 {
        return None;
    }
    // Where each cluster ends, the last being the whole text.
    let ends: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(at, cluster)| at + cluster.len())
        .collect();
    let candidate = |count: usize| {
        let mut out = String::with_capacity(ends[count.saturating_sub(1)] + ELLIPSIS.len());
        if count > 0 {
            out.push_str(&text[..ends[count - 1]]);
        }
        out.push_str(ELLIPSIS);
        out
    };
    // The widest start that fits, if any: the width only grows with the start.
    let (mut low, mut high) = (0, ends.len().saturating_sub(1));
    while low < high {
        let middle = (low + high).div_ceil(2);
        if width(&candidate(middle)) <= available + 0.01 {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    Some(candidate(low))
}

impl Context {
    pub(super) fn prepare_ellipsis(&mut self, ids: &[NodeId], measurer: &mut impl TextMeasurer) {
        for &id in ids {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            let available = self.text_place(node).0.width;
            let style = node.text_style();
            if node.ellipsis.as_ref().is_some_and(|done| {
                done.available == available && done.text == node.text && done.style == style
            }) {
                continue;
            }
            let shown = fit(&node.text, &style, available, measurer);
            let node = self.nodes.get_mut(&id).unwrap();
            let before = node.ellipsis.as_ref().and_then(|done| done.shown.clone());
            let changed = before != shown;
            node.ellipsis = Some(Box::new(Ellipsis {
                available,
                text: node.text.clone(),
                style,
                shown,
            }));
            if changed {
                let bounds = node.bounds;
                self.invalidate_paint(id);
                self.add_damage(bounds);
            }
        }
    }
}
