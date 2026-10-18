//! Text editors and rich text links send messages. Each edit reaches `update`
//! as one `TextOp` or `RichOp`; the document is changed there, and the view
//! declares it as it is afterwards.

use glassmere_core::{
    App, Block, BlockKind, CodeEditorOptions, Effects, Event, Key, Length, RichDocument,
    RichEditorOptions, RichOp, RichText, Style, TextDocument, TextEdit, TextOp, Ui,
};
use glassmere_scene::Size;
use glassmere_testing::Harness;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Edit(TextOp),
    Rich(RichOp),
    Link(String),
    Count,
}

struct Notes {
    document: TextDocument,
    rich: RichDocument,
    text: RichText,
    links: Vec<String>,
    count: i32,
    /// Every message `update` received, in order.
    log: Vec<Msg>,
}

impl Notes {
    fn new(text: &str) -> Self {
        let mut document = TextDocument::from_text(text);
        let end = document.len_chars();
        document.set_selection(end, end);
        Self {
            document,
            rich: RichDocument::from_blocks(vec![Block::new(BlockKind::Paragraph, "Hi")]),
            text: RichText::new()
                .text("See ")
                .link("docs", "https://example.test/docs"),
            links: Vec::new(),
            count: 0,
            log: Vec::new(),
        }
    }
}

impl App for Notes {
    type Message = Msg;

    fn update(&mut self, message: Msg, _: &mut Effects<'_, Msg>) {
        self.log.push(message.clone());
        match message {
            Msg::Edit(op) => self.document.apply(op),
            Msg::Rich(op) => self.rich.apply(op),
            Msg::Link(url) => self.links.push(url),
            Msg::Count => self.count += 1,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.label(format!("Count {}", self.count));
        ui.code_editor(
            &self.document,
            CodeEditorOptions {
                label: "Source".into(),
                style: Style::default()
                    .width(Length::Px(600.0))
                    .height(Length::Px(120.0))
                    .padding(8.0),
                ..CodeEditorOptions::default()
            },
            Msg::Edit,
        );
        ui.rich_text(
            &self.text,
            Style::default().width(Length::Px(120.0)),
            Msg::Link,
        );
        ui.rich_editor(
            &self.rich,
            RichEditorOptions {
                label: "Notes".into(),
                ..RichEditorOptions::default()
            },
            Msg::Rich,
        );
    }
}

fn harness(text: &str) -> Harness<Notes> {
    Harness::app(Size::new(700.0, 900.0), Notes::new(text))
}

/// Focuses the code editor and types `text`, one batch per call.
fn type_into_source(harness: &mut Harness<Notes>, text: &str) {
    let editor = harness.by_label("Source");
    harness.focus(editor);
    harness.type_text(text);
}

#[test]
fn typing_sends_one_text_op_per_edit_and_update_applies_it() {
    let mut harness = harness("hello");
    type_into_source(&mut harness, "ab");
    // The default editor auto-closes, so typing is sent as a paired insert.
    assert_eq!(
        harness.state().log.last(),
        Some(&Msg::Edit(TextOp::InsertPaired("ab".into())))
    );
    assert_eq!(harness.state().document.text(), "helloab");
}

#[test]
fn an_applied_op_shows_after_one_declaration() {
    let mut harness = harness("hello");
    let stats = harness.send(Msg::Edit(TextOp::Insert("!".into())));
    assert_eq!((stats.messages, stats.declared), (1, true));
    assert_eq!(harness.state().document.text(), "hello!");
    assert!(harness.snapshot_scene().contains("hello!"));
}

#[test]
fn undo_after_several_typed_ops_restores_the_earlier_text() {
    let mut harness = harness("hello");
    type_into_source(&mut harness, "a");
    type_into_source(&mut harness, "b");
    type_into_source(&mut harness, "c");
    assert_eq!(harness.state().document.text(), "helloabc");

    harness.event(Event::Text(TextEdit::Undo));
    harness.run();
    let after_one = harness.state().document.text();
    assert_ne!(after_one, "helloabc");
    assert!("helloabc".starts_with(&after_one));

    // Consecutive typing is one undo step, so undo runs out before "hello".
    while harness.state().document.can_undo() {
        harness.event(Event::Text(TextEdit::Undo));
        harness.run();
    }
    assert_eq!(harness.state().document.text(), "hello");
}

#[test]
fn the_caret_stays_where_the_ops_left_it_across_declarations() {
    let mut harness = harness("hello");
    type_into_source(&mut harness, "abc");
    assert_eq!(harness.state().document.primary().cursor, 8);

    harness.press(Key::ArrowLeft);
    assert_eq!(harness.state().document.primary().cursor, 7);

    // A declaration that changes nothing in the document leaves the caret be.
    harness.send(Msg::Count);
    assert_eq!(harness.state().document.primary().cursor, 7);
    assert_eq!(harness.state().document.text(), "helloabc");
}

#[test]
fn a_rich_text_link_click_sends_its_url() {
    let mut harness = harness("hello");
    let link = harness.by_label("See docs");
    // The link sits in the right half of the block, which the click hits.
    harness.click(link);
    assert_eq!(
        harness.state().links,
        vec!["https://example.test/docs".to_owned()]
    );
}

#[test]
fn typing_in_a_rich_editor_sends_rich_ops() {
    let mut harness = harness("hello");
    let notes = harness.by_label("Notes");
    harness.focus(notes);
    harness.type_text("x");
    assert_eq!(
        harness.state().log.last(),
        Some(&Msg::Rich(RichOp::Insert("x".into())))
    );
    assert!(harness.state().rich.plain_text().contains('x'));
}
