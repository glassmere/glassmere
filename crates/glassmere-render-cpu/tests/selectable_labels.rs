//! Selecting in a label that the real text shaper lays out, which reports
//! lines the test measurer never does: one with no characters in it, and line
//! endings other than a line feed.
use glassmere_core::{Context, Event, Key, Label, Length, Style, TextWrap, Ui};
use glassmere_scene::{Point, Rect, Size, TextGeometry, TextMeasurer, TextSpan, TextStyle};
use glassmere_text::TextSystem;
use std::time::Duration;

/// The shaper, keeping the geometry it gave last so that a test can aim at
/// the characters it drew.
struct Recording {
    text: TextSystem,
    last: Option<TextGeometry>,
}

impl TextMeasurer for Recording {
    fn measure_styled(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> Size {
        self.text.measure_styled(text, style, width)
    }

    fn geometry(&mut self, text: &str, style: &TextStyle, width: Option<f32>) -> TextGeometry {
        let geometry = self.text.geometry(text, style, width);
        self.last = Some(geometry.clone());
        geometry
    }

    fn geometry_rich(
        &mut self,
        spans: &[TextSpan],
        style: &TextStyle,
        width: Option<f32>,
    ) -> TextGeometry {
        self.text.geometry_rich(spans, style, width)
    }

    fn ascent(&mut self, style: &TextStyle) -> f32 {
        self.text.ascent(style)
    }
}

/// A selectable label, 100 px wide and wrapped, in a window.
struct Shown {
    context: Context,
    geometry: TextGeometry,
    bounds: Rect,
    clock: u64,
}

impl Shown {
    fn new(text: &'static str) -> Self {
        let mut measurer = Recording {
            text: TextSystem::with_fonts([include_bytes!(
                "../../glassmere-render-wgpu/fonts/FiraMono-Regular.ttf"
            )
            .to_vec()]),
            last: None,
        };
        let mut context = Context::new();
        context.run(
            Size::new(400.0, 600.0),
            &mut measurer,
            move |ui: &mut Ui<'_>| {
                ui.keyed("label", |ui| {
                    ui.label(
                        Label::new(text).selectable().style(
                            Style::default()
                                .width(Length::Px(100.0))
                                .wrap(TextWrap::Word),
                        ),
                    )
                });
            },
        );
        let bounds = context
            .node(context.find(&["label"]).unwrap())
            .unwrap()
            .bounds;
        Self {
            context,
            geometry: measurer.last.unwrap(),
            bounds,
            clock: 1000,
        }
    }

    /// The first cluster that starts at `byte`.
    fn cluster_at(&self, byte: usize) -> usize {
        self.geometry
            .clusters
            .iter()
            .position(|cluster| cluster.bytes.start == byte)
            .unwrap()
    }

    /// The first cluster drawn on the visual line `line` that is not at its
    /// start.
    fn cluster_in(&self, line: usize) -> usize {
        let line = &self.geometry.lines[line];
        self.geometry
            .clusters
            .iter()
            .position(|cluster| {
                (cluster.rect.y - line.y).abs() < 0.5 && cluster.bytes.start > line.bytes.start
            })
            .unwrap()
    }

    /// Presses `times` times in a row on the left half of a cluster, 100 ms
    /// apart, a second after the last row.
    fn click(&mut self, cluster: usize, times: u64) {
        let rect = self.geometry.clusters[cluster].rect;
        let at = Point {
            x: self.bounds.x + rect.x + 1.0,
            y: self.bounds.y + rect.y + rect.height / 2.0,
        };
        for press in 0..times {
            self.context
                .set_now(Duration::from_millis(self.clock + press * 100));
            self.context.event(Event::PointerMoved(at));
            self.context.event(Event::PointerDown);
            self.context.event(Event::PointerUp);
        }
        self.clock += 1000;
    }

    /// Whether the key was left for the view around the label.
    fn left_to_scroll(&mut self, key: Key, shift: bool) -> bool {
        !self.context.event(Event::KeyDown {
            key,
            shift,
            repeat: false,
        })
    }

    fn selected(&self) -> Option<&str> {
        self.context.selected_text()
    }
}

/// Hebrew, then a Latin word wider than the label: the shaper reports a line
/// of no characters and no bytes between the lines that hold the word.
const OVERLONG_WORD: &str = "\u{5d1}\u{5d9}\u{5ea} https://example.com/very/long/path/name \
                             \u{5e2}\u{5d5}\u{5dc}\u{5dd} ok a order \u{5e9}\u{5dc}\u{5d5}\u{5dd} shipped";

#[test]
fn a_line_without_characters_is_not_moved_to_by_shift_and_up_or_down() {
    let mut shown = Shown::new(OVERLONG_WORD);
    let lines = &shown.geometry.lines;
    let empty = (0..lines.len())
        .find(|line| {
            let y = lines[*line].y;
            !shown
                .geometry
                .clusters
                .iter()
                .any(|cluster| (cluster.rect.y - y).abs() < 0.5)
        })
        .expect("the shaper reports a line of no characters for this text");
    assert!(empty > 0 && empty + 1 < lines.len());

    let above = shown.cluster_in(empty - 1);
    shown.click(above, 1);
    assert_eq!(shown.selected(), Some(""));
    assert!(shown.left_to_scroll(Key::ArrowDown, true));
    assert_eq!(shown.selected(), Some(""));

    let below = shown.cluster_in(empty + 1);
    shown.click(below, 1);
    assert_eq!(shown.selected(), Some(""));
    assert!(shown.left_to_scroll(Key::ArrowUp, true));
    assert_eq!(shown.selected(), Some(""));
}

#[test]
fn shift_and_up_or_down_do_not_start_from_a_paragraph_end_the_shaper_gives_no_place() {
    // The shaper leaves out the two spaces that end the first line, which do
    // not fit its width, so the end of a triple click has no place on screen.
    let mut shown = Shown::new("4711 a see  \nsecond line here\nthird line");
    assert_eq!(shown.geometry.lines[0].bytes, 0..10);
    assert_eq!(shown.geometry.lines[1].bytes, 13..19);
    let first = shown.cluster_at(0);
    shown.click(first, 3);
    assert_eq!(shown.selected(), Some("4711 a see  "));
    for key in [Key::ArrowUp, Key::ArrowDown] {
        assert!(shown.left_to_scroll(key, true), "{key:?}");
        assert_eq!(shown.selected(), Some("4711 a see  "), "{key:?}");
    }
}

#[test]
fn shift_and_down_visit_a_blank_line_the_shaper_draws() {
    let mut shown = Shown::new("one\n\nthree");
    let first = shown.cluster_at(2);
    shown.click(first, 1);
    assert!(!shown.left_to_scroll(Key::ArrowDown, true));
    assert_eq!(shown.selected(), Some("e\n"));
    assert!(!shown.left_to_scroll(Key::ArrowDown, true));
    assert_eq!(shown.selected(), Some("e\n\nth"));
    assert!(!shown.left_to_scroll(Key::ArrowUp, true));
    assert_eq!(shown.selected(), Some("e\n"));
}

#[test]
fn shift_and_down_do_not_end_between_the_carriage_return_and_the_line_feed() {
    // The shaper starts the blank line of "a\n\r\nb" between the two.
    let mut shown = Shown::new("a\n\r\nb");
    let first = shown.cluster_at(0);
    shown.click(first, 1);
    assert!(shown.left_to_scroll(Key::ArrowDown, true));
    assert_eq!(shown.selected(), Some(""));
}

#[test]
fn a_triple_click_takes_the_line_the_shaper_shows_for_each_line_ending() {
    for (text, at, line) in [
        ("one\rtwo", 4, "two"),
        ("one\rtwo", 0, "one"),
        ("a\n\rb", 3, "b"),
        ("one\r\ntwo", 5, "two"),
        ("one\ntwo", 4, "two"),
    ] {
        let mut shown = Shown::new(text);
        let cluster = shown.cluster_at(at);
        shown.click(cluster, 3);
        assert_eq!(shown.selected(), Some(line), "{text:?} at {at}");
    }
}

#[test]
fn a_double_click_with_a_pixel_of_movement_keeps_the_word() {
    let mut shown = Shown::new("order 4711 ships");
    let cluster = shown.cluster_at(8);
    let rect = shown.geometry.clusters[cluster].rect;
    let at = Point {
        x: shown.bounds.x + rect.x + 1.0,
        y: shown.bounds.y + rect.y + rect.height / 2.0,
    };
    shown.context.set_now(Duration::from_millis(1000));
    shown.context.event(Event::PointerMoved(at));
    shown.context.event(Event::PointerDown);
    shown.context.event(Event::PointerUp);
    shown.context.set_now(Duration::from_millis(1100));
    shown.context.event(Event::PointerDown);
    shown.context.event(Event::PointerMoved(Point {
        x: at.x + 1.0,
        ..at
    }));
    assert_eq!(shown.selected(), Some("4711"));
    shown.context.event(Event::PointerUp);
    assert_eq!(shown.selected(), Some("4711"));
}
