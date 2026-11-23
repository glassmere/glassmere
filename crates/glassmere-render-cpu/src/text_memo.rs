//! The text keys of the last whole frame. A frame that paints the same texts
//! again checks each against what it asked for last time instead of building
//! its key, and leaves the cache of shaped text alone when it holds exactly
//! those texts.
use glassmere_scene::{Primitive, Rect, TextSpan, TextStyle, TextWrap};
use glassmere_text::{TextKey, TextSystem};
use std::collections::HashSet;

#[cfg(test)]
thread_local! {
    /// For the tests to see what a frame did not do: how many blocks this
    /// thread has asked the cache of shaped text for, and how many times it
    /// has walked the cache for leftovers.
    static ASKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WALKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Shapes the block of a text primitive unless it is shaped already, and
/// returns its key.
pub(crate) fn ensure_text(text: &mut TextSystem, primitive: &Primitive) -> TextKey {
    #[cfg(test)]
    ASKED.with(|asked| asked.set(asked.get() + 1));
    match primitive {
        Primitive::Text {
            text: string,
            font_size,
            ..
        } => text.ensure(string, *font_size),
        Primitive::StyledText {
            rect,
            text: string,
            style,
            ..
        } => text.ensure_styled(string, style, Some(rect.width)),
        Primitive::RichText {
            rect, spans, style, ..
        } => text.ensure_rich(spans, style, Some(rect.width)),
        _ => unreachable!(),
    }
}

/// The bytes of text of a text primitive.
pub(crate) fn text_bytes(primitive: &Primitive) -> usize {
    match primitive {
        Primitive::Text { text, .. } | Primitive::StyledText { text, .. } => text.len(),
        Primitive::RichText { spans, .. } => spans.iter().map(|span| span.text.len()).sum(),
        _ => 0,
    }
}

/// The width a block is shaped at, as bits. The key leaves it out for a block
/// that does not wrap.
fn wrapped_width(style: &TextStyle, rect: &Rect) -> Option<u32> {
    (style.wrap != TextWrap::None).then(|| rect.width.to_bits())
}

/// Whether room for `held` is far more than `needed` calls for.
fn roomy(held: usize, needed: usize) -> bool {
    held > 4 * needed + 64
}

/// Makes `held` a copy of `text`, in the room it has unless that is far more
/// than the copy needs.
fn copy_text(held: &mut String, text: &str) {
    if roomy(held.capacity(), text.len()) {
        *held = text.to_owned();
    } else {
        text.clone_into(held);
    }
}

/// Makes `held` a copy of `spans`, in the room it has unless that is far more
/// than the copy needs.
fn copy_spans(held: &mut Vec<TextSpan>, spans: &[TextSpan]) {
    if roomy(held.capacity(), spans.len()) {
        *held = spans.to_vec();
    } else {
        spans.clone_into(held);
    }
}

/// What a text primitive puts into its key, as it was given. Equal asks give
/// equal keys. The other way round does not hold: a plain text and a styled
/// one in the default style share a key, and are told apart here, which only
/// costs a look-up.
///
/// A style is compared whole, so that a field added to it is compared too. Its
/// font size is the one float: a NaN never equals itself, and a block with one
/// is asked for again every frame.
enum Ask {
    Plain {
        text: String,
        size: u32,
    },
    Styled {
        text: String,
        style: TextStyle,
        width: Option<u32>,
    },
    Rich {
        spans: Vec<TextSpan>,
        style: TextStyle,
        width: Option<u32>,
    },
}

// The primitives are taken apart without `..`, so that a field added to one
// stops these from compiling until it is decided whether the key has it.
impl Ask {
    fn of(primitive: &Primitive) -> Self {
        match primitive {
            Primitive::Text {
                rect: _,
                text,
                font_size,
                color: _,
            } => Self::Plain {
                text: text.clone(),
                size: font_size.to_bits(),
            },
            Primitive::StyledText {
                rect,
                text,
                style,
                color: _,
            } => Self::Styled {
                text: text.clone(),
                style: style.clone(),
                width: wrapped_width(style, rect),
            },
            Primitive::RichText {
                rect,
                spans,
                style,
                color: _,
            } => Self::Rich {
                spans: spans.clone(),
                style: style.clone(),
                width: wrapped_width(style, rect),
            },
            _ => unreachable!(),
        }
    }

    fn matches(&self, primitive: &Primitive) -> bool {
        match (self, primitive) {
            (
                Self::Plain { text, size },
                Primitive::Text {
                    rect: _,
                    text: other,
                    font_size,
                    color: _,
                },
            ) => text == other && *size == font_size.to_bits(),
            (
                Self::Styled { text, style, width },
                Primitive::StyledText {
                    rect,
                    text: other,
                    style: other_style,
                    color: _,
                },
            ) => {
                text == other && style == other_style && *width == wrapped_width(other_style, rect)
            }
            (
                Self::Rich {
                    spans,
                    style,
                    width,
                },
                Primitive::RichText {
                    rect,
                    spans: other,
                    style: other_style,
                    color: _,
                },
            ) => {
                spans == other && style == other_style && *width == wrapped_width(other_style, rect)
            }
            _ => false,
        }
    }

    /// Becomes the ask of `primitive`, keeping the memory of the old one
    /// when it is of the same kind and not far larger than the new one needs.
    fn set(&mut self, primitive: &Primitive) {
        match (&mut *self, primitive) {
            (
                Self::Plain { text, size },
                Primitive::Text {
                    rect: _,
                    text: other,
                    font_size,
                    color: _,
                },
            ) => {
                copy_text(text, other);
                *size = font_size.to_bits();
            }
            (
                Self::Styled { text, style, width },
                Primitive::StyledText {
                    rect,
                    text: other,
                    style: other_style,
                    color: _,
                },
            ) => {
                copy_text(text, other);
                style.clone_from(other_style);
                *width = wrapped_width(other_style, rect);
            }
            (
                Self::Rich {
                    spans,
                    style,
                    width,
                },
                Primitive::RichText {
                    rect,
                    spans: other,
                    style: other_style,
                    color: _,
                },
            ) => {
                copy_spans(spans, other);
                style.clone_from(other_style);
                *width = wrapped_width(other_style, rect);
            }
            _ => *self = Self::of(primitive),
        }
    }
}

/// An ask and the key `ensure_text` gave for it.
struct Asked {
    ask: Ask,
    key: TextKey,
}

/// The asks of the last whole frame, in the order of the scene, with their
/// keys. It is not told when the cache of shaped text is edited: each text
/// checks that its key is still there, and the cache is cleaned out unless it
/// holds what the last frame left.
#[derive(Default)]
pub(crate) struct TextMemo {
    items: Vec<Asked>,
    /// How many shaped blocks the last whole frame left in the cache. `None`
    /// before the first, and when a frame did not end, for then it is not
    /// known what the cache holds.
    kept: Option<usize>,
    /// The texts met so far this frame, and whether each was asked for as
    /// last time at its place.
    at: usize,
    same: bool,
}

impl TextMemo {
    pub(crate) fn begin(&mut self) {
        self.at = 0;
        self.same = true;
    }

    /// The key of the next text of the frame, with its block shaped.
    pub(crate) fn key(&mut self, text: &mut TextSystem, primitive: &Primitive) -> &TextKey {
        let at = self.at;
        self.at += 1;
        match self.items.get_mut(at) {
            Some(item) if item.ask.matches(primitive) => {
                // Something may have let go of the block since.
                if !text.buffers.contains_key(&item.key) {
                    item.key = ensure_text(text, primitive);
                }
            }
            Some(item) => {
                self.same = false;
                // The key comes first, so that the ask and the key it answers
                // are stored together.
                let key = ensure_text(text, primitive);
                item.ask.set(primitive);
                item.key = key;
            }
            None => {
                self.same = false;
                let key = ensure_text(text, primitive);
                self.items.push(Asked {
                    ask: Ask::of(primitive),
                    key,
                });
            }
        }
        &self.items[at].key
    }

    /// Whether the cache, once every text of the frame is shaped, holds those
    /// texts and no others. Every text was asked for as last time, there are
    /// as many, and the cache has the size the last frame left it at: the
    /// blocks of the frame are all in it, and that many are all it has.
    fn settled(&self, text: &TextSystem) -> bool {
        self.same && self.items.len() == self.at && self.kept == Some(text.buffers.len())
    }

    /// Ends a whole frame: lets go of the shaped blocks that no text of it
    /// uses.
    pub(crate) fn finish(&mut self, text: &mut TextSystem) {
        let settled = self.settled(text);
        self.items.truncate(self.at);
        if !settled {
            #[cfg(test)]
            WALKED.with(|walked| walked.set(walked.get() + 1));
            // Borrowed keys, and the room for them in one go.
            let active: HashSet<&TextKey> = self.items.iter().map(|item| &item.key).collect();
            text.buffers.retain(|key, _| active.contains(key));
        }
        self.kept = Some(text.buffers.len());
        // A big scene once is not kept room for.
        if roomy(self.items.capacity(), self.items.len()) {
            self.items.shrink_to(2 * self.items.len() + 16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CpuRenderer;
    use glassmere_scene::{Color, FontFamily, Scene, Size};
    use std::cell::Cell;

    fn fira() -> TextSystem {
        TextSystem::with_fonts([include_bytes!(
            "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
        )
        .to_vec()])
    }

    fn rect(width: f32) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width,
            height: 10.0,
        }
    }

    fn plain(text: &str, font_size: f32) -> Primitive {
        Primitive::Text {
            rect: rect(50.0),
            text: text.into(),
            font_size,
            color: Color::WHITE,
        }
    }

    fn styled(text: &str, style: &TextStyle, width: f32) -> Primitive {
        Primitive::StyledText {
            rect: rect(width),
            text: text.into(),
            style: style.clone(),
            color: Color::WHITE,
        }
    }

    fn rich(spans: &[TextSpan], style: &TextStyle, width: f32) -> Primitive {
        Primitive::RichText {
            rect: rect(width),
            spans: spans.to_vec(),
            style: style.clone(),
            color: Color::WHITE,
        }
    }

    /// Every style that differs from the default one, or from the wrapping
    /// one, in a single field, and those two.
    fn styles() -> Vec<TextStyle> {
        let edits: [fn(&mut TextStyle); 12] = [
            |_| {},
            |s| s.font_size = 18.0,
            |s| s.font_size = 0.5,
            |s| s.font_size = 1.0,
            |s| s.family = FontFamily::Serif,
            |s| s.family = FontFamily::Named("x".into()),
            |s| s.weight = 700,
            |s| s.weight = 950,
            |s| s.italic = true,
            |s| s.rtl = true,
            |s| s.base = Some(true),
            |s| s.base = Some(false),
        ];
        let mut styles = Vec::new();
        for wrap in [TextWrap::None, TextWrap::Word] {
            for edit in edits {
                let mut style = TextStyle {
                    wrap,
                    ..TextStyle::default()
                };
                edit(&mut style);
                styles.push(style);
            }
        }
        styles
    }

    /// Texts that differ in every input, and some that differ in none that
    /// the key reads.
    fn primitives() -> Vec<Primitive> {
        let spans = [
            vec![TextSpan::new("ab"), TextSpan::new("c")],
            vec![TextSpan::new("a"), TextSpan::new("bc")],
            vec![TextSpan::new("abc")],
            vec![TextSpan::new("ab").weight(700), TextSpan::new("c")],
            vec![
                TextSpan::new("ab"),
                TextSpan::new("c").color(Color(1, 2, 3, 4)),
            ],
            vec![TextSpan::new("ab"), TextSpan::new("c").italic(true)],
            vec![
                TextSpan::new("ab"),
                TextSpan::new("c").family(FontFamily::Serif),
            ],
        ];
        let mut all = Vec::new();
        for text in ["", "ab"] {
            for size in [14.0, 0.5, 1.0, 0.0, -0.0] {
                all.push(plain(text, size));
            }
        }
        for style in styles() {
            for width in [50.0, 80.0, 0.25] {
                for text in ["ab", "ac"] {
                    all.push(styled(text, &style, width));
                }
                for spans in &spans {
                    all.push(rich(spans, &style, width));
                }
            }
        }
        all
    }

    #[test]
    fn an_ask_matches_only_what_would_give_its_key() {
        let all = primitives();
        let mut system = fira();
        let keys: Vec<TextKey> = all
            .iter()
            .map(|primitive| ensure_text(&mut system, primitive))
            .collect();
        let mut matched = 0;
        for (a, key) in all.iter().zip(&keys) {
            let ask = Ask::of(a);
            assert!(ask.matches(a), "{a:?} does not match itself");
            for (b, other) in all.iter().zip(&keys) {
                if ask.matches(b) {
                    matched += 1;
                    assert_eq!(key, other, "{a:?} matches {b:?}");
                }
            }
        }
        // Not only itself.
        assert!(matched > all.len());
    }

    #[test]
    fn a_width_matters_only_to_a_block_that_wraps() {
        let (none, word) = (styles()[0].clone(), styles()[12].clone());
        assert_eq!(word.wrap, TextWrap::Word);
        assert!(Ask::of(&styled("ab", &none, 50.0)).matches(&styled("ab", &none, 80.0)));
        assert!(!Ask::of(&styled("ab", &word, 50.0)).matches(&styled("ab", &word, 80.0)));
        let spans = [TextSpan::new("ab")];
        assert!(Ask::of(&rich(&spans, &none, 50.0)).matches(&rich(&spans, &none, 80.0)));
        assert!(!Ask::of(&rich(&spans, &word, 50.0)).matches(&rich(&spans, &word, 80.0)));
        // A style with a NaN is asked for again, and a plain text with one is not.
        let nan = TextStyle {
            font_size: f32::NAN,
            ..none.clone()
        };
        assert!(!Ask::of(&styled("ab", &nan, 50.0)).matches(&styled("ab", &nan, 50.0)));
        assert!(Ask::of(&plain("ab", f32::NAN)).matches(&plain("ab", f32::NAN)));
        // Neither kind of text is the other.
        assert!(!Ask::of(&plain("ab", 16.0)).matches(&styled("ab", &TextStyle::default(), 50.0)));
        assert!(!Ask::of(&styled("", &none, 50.0)).matches(&rich(&[], &none, 50.0)));
    }

    #[test]
    fn an_ask_that_is_set_is_the_ask_of_the_text_it_was_set_to() {
        let all = primitives();
        // The kinds come in runs of nine, which a step that three divides
        // would take the same few places of.
        let from: Vec<&Primitive> = all.iter().step_by(4).collect();
        assert!(from.iter().any(|a| matches!(a, Primitive::Text { .. })));
        assert!(
            from.iter()
                .any(|a| matches!(a, Primitive::StyledText { .. }))
        );
        assert!(from.iter().any(|a| matches!(a, Primitive::RichText { .. })));
        for a in from {
            for b in &all {
                let mut ask = Ask::of(a);
                ask.set(b);
                assert!(ask.matches(b), "{a:?} set to {b:?}");
            }
        }
    }

    /// What an ask has room for: bytes of text, or spans.
    fn room(ask: &Ask) -> usize {
        match ask {
            Ask::Plain { text, .. } | Ask::Styled { text, .. } => text.capacity(),
            Ask::Rich { spans, .. } => spans.capacity(),
        }
    }

    #[test]
    fn an_ask_does_not_keep_the_room_of_a_much_longer_text() {
        let style = TextStyle::default();
        let kinds: [&dyn Fn(usize) -> Primitive; 3] = [
            &|size| plain(&"x".repeat(size), 8.0),
            &|size| styled(&"x".repeat(size), &style, 50.0),
            &|size| rich(&vec![TextSpan::new("x"); size], &style, 50.0),
        ];
        for make in kinds {
            let mut ask = Ask::of(&make(4096));
            ask.set(&make(1));
            assert!(room(&ask) < 100, "{} of room", room(&ask));
            // Room that is no more than four times what the text needs, and a
            // little over, is used again, and more than that is not.
            let mut ask = Ask::of(&make(300));
            ask.set(&make(60));
            assert_eq!(room(&ask), 300);
            let mut ask = Ask::of(&make(1000));
            ask.set(&make(200));
            assert_eq!(room(&ask), 200);
        }
    }

    #[test]
    fn a_frame_like_the_last_leaves_the_cache_alone() {
        let style = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let scene = [
            plain("one", 12.0),
            styled("two", &style, 40.0),
            rich(&[TextSpan::new("th"), TextSpan::new("ree")], &style, 40.0),
            plain("one", 12.0),
        ];
        let mut system = fira();
        let mut memo = TextMemo::default();
        let mut pass = |system: &mut TextSystem, scene: &[Primitive]| {
            memo.begin();
            for primitive in scene {
                memo.key(system, primitive);
            }
            let settled = memo.settled(system);
            memo.finish(system);
            settled
        };
        // Nothing is known of the cache at first, and then it is settled.
        assert!(!pass(&mut system, &scene));
        assert_eq!(system.buffers.len(), 3);
        for _ in 0..3 {
            assert!(pass(&mut system, &scene));
            assert_eq!(system.buffers.len(), 3);
        }
        // What was added is cleaned out, once.
        system.ensure("stray", 9.0);
        assert!(!pass(&mut system, &scene));
        assert_eq!(system.buffers.len(), 3);
        assert!(pass(&mut system, &scene));
        // A text let go of is shaped again, and the cache is as the last
        // frame left it. With a stray in its place, it has a block more.
        let key = ensure_text(&mut system, &scene[0]);
        system.buffers.remove(&key);
        assert!(pass(&mut system, &scene));
        assert_eq!(system.buffers.len(), 3);
        system.buffers.remove(&key);
        system.ensure("stray", 9.0);
        assert!(!pass(&mut system, &scene));
        assert_eq!(system.buffers.len(), 3);
        // The same size, with another text in a place.
        let mut changed = scene.to_vec();
        changed[1] = styled("2", &style, 40.0);
        assert!(!pass(&mut system, &changed));
        assert_eq!(system.buffers.len(), 3);
        assert!(pass(&mut system, &changed));
        // A text that is a copy of another, which shapes nothing.
        changed[1] = plain("one", 12.0);
        assert!(!pass(&mut system, &changed));
        assert_eq!(system.buffers.len(), 2);
        // Fewer and more.
        assert!(!pass(&mut system, &changed[..3]));
        assert!(pass(&mut system, &changed[..3]));
        assert!(!pass(&mut system, &changed));
        assert!(pass(&mut system, &changed));
        assert!(!pass(&mut system, &[]));
        assert!(system.buffers.is_empty());
        assert!(pass(&mut system, &[]));
    }

    /// How many blocks `work` asked the cache for, and how often it walked
    /// the cache.
    fn counted(work: impl FnOnce()) -> (usize, usize) {
        let before = (ASKED.with(Cell::get), WALKED.with(Cell::get));
        work();
        (
            ASKED.with(Cell::get) - before.0,
            WALKED.with(Cell::get) - before.1,
        )
    }

    #[test]
    fn a_frame_like_the_last_asks_the_cache_for_nothing_and_walks_nothing() {
        let style = TextStyle {
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let scene = [
            plain("one", 12.0),
            styled("two", &style, 40.0),
            rich(&[TextSpan::new("th"), TextSpan::new("ree")], &style, 40.0),
            plain("one", 12.0),
        ];
        let mut system = fira();
        let mut memo = TextMemo::default();
        let mut pass = |system: &mut TextSystem, scene: &[Primitive]| {
            counted(|| {
                memo.begin();
                for primitive in scene {
                    memo.key(system, primitive);
                }
                memo.finish(system);
            })
        };
        // Each text is asked for, and the cache is walked, once.
        assert_eq!(pass(&mut system, &scene), (4, 1));
        for _ in 0..3 {
            assert_eq!(pass(&mut system, &scene), (0, 0));
        }
        // A block that was let go of is asked for again, by the first text
        // that wants it. The cache is as the last frame left it.
        let key = system.ensure("one", 12.0);
        system.buffers.remove(&key);
        assert_eq!(pass(&mut system, &scene), (1, 0));
        // A block no text wants gets the cache walked, once.
        system.ensure("stray", 9.0);
        assert_eq!(pass(&mut system, &scene), (0, 1));
        assert_eq!(pass(&mut system, &scene), (0, 0));
        // A text that changed is asked for, and the cache is walked.
        let mut changed = scene.to_vec();
        changed[1] = styled("2", &style, 40.0);
        assert_eq!(pass(&mut system, &changed), (1, 1));
        assert_eq!(pass(&mut system, &changed), (0, 0));
    }

    #[test]
    fn a_renderer_paints_a_whole_frame_like_the_last_without_asking_for_its_texts() {
        let style = TextStyle {
            font_size: 8.0,
            wrap: TextWrap::Word,
            ..TextStyle::default()
        };
        let at = |y: f32, mut primitive: Primitive| {
            if let Primitive::Text { rect, .. }
            | Primitive::StyledText { rect, .. }
            | Primitive::RichText { rect, .. } = &mut primitive
            {
                rect.y = y;
            }
            primitive
        };
        let scene = Scene {
            size: Size::new(64.0, 64.0),
            primitives: vec![
                at(0.0, plain("one", 8.0)),
                at(20.0, styled("two", &style, 40.0)),
                at(
                    40.0,
                    rich(&[TextSpan::new("th"), TextSpan::new("ree")], &style, 40.0),
                ),
            ],
        };
        let mut renderer = CpuRenderer::with_text_system(fira());
        let whole = |renderer: &mut CpuRenderer| {
            counted(|| {
                renderer.render([64, 64], 1.0, &scene, Color::WHITE);
            })
        };
        assert_eq!(whole(&mut renderer), (3, 1));
        assert_eq!(whole(&mut renderer), (0, 0));
        assert_eq!(whole(&mut renderer), (0, 0));
        // A frame painted in part asks for the texts it paints, and leaves
        // what the whole frames remembered as it was.
        let damage = [Rect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 10.0,
        }];
        let part = counted(|| {
            renderer.render_damaged([64, 64], 1.0, &scene, Color::WHITE, &damage);
        });
        assert_eq!(part, (1, 0));
        assert_eq!(whole(&mut renderer), (0, 0));
    }

    #[test]
    fn the_room_for_a_big_frame_is_not_kept() {
        let mut system = fira();
        let mut memo = TextMemo::default();
        let big: Vec<Primitive> = (0..300).map(|n| plain(&n.to_string(), 8.0)).collect();
        memo.begin();
        for primitive in &big {
            memo.key(&mut system, primitive);
        }
        memo.finish(&mut system);
        assert!(memo.items.capacity() >= 300);
        memo.begin();
        memo.key(&mut system, &big[0]);
        memo.finish(&mut system);
        assert_eq!(system.buffers.len(), 1);
        assert!(memo.items.capacity() < 100);
    }
}
