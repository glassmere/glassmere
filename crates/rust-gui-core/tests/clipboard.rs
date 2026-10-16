mod support;

use rust_gui_core::Clip;
use rust_gui_scene::{Image, Size};
use support::{Driven, act};

const ROOT: Size = Size::new(400.0, 300.0);

fn image() -> Image {
    Image::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).expect("two pixels")
}

/// What a view copies reaches the host, and what the host reads reaches the
/// view. Both go through the outbox, so neither happens inside a declaration.
#[derive(Default)]
struct Board {
    seen: Option<Clip>,
}

fn board() -> Driven<Board> {
    Driven::new(ROOT, Board::default(), |_, ui| {
        ui.keyed("state", |ui| ui.label("ready"));
    })
}

#[test]
fn a_copy_reaches_the_host_and_what_it_reads_reaches_the_view() {
    let mut app = board();
    app.frame();
    assert_eq!(app.context.take_clip(), None);
    assert_eq!(app.state().seen, None);

    // A shape: private bytes for this application, a line of text for
    // everything else, and an image of it beside them.
    app.effects(|_, effects| {
        effects.copy(
            Clip::custom("app/shape", vec![1, 2, 3])
                .with_text("Shape")
                .with_image(image()),
        );
    });
    app.frame();
    let taken = app
        .context
        .take_clip()
        .expect("the host is handed the copy");
    assert_eq!(taken.text, "Shape");
    assert_eq!(taken.bytes("app/shape"), Some(&[1, 2, 3][..]));
    assert_eq!(taken.bytes("app/other"), None);
    assert_eq!(taken.image.as_ref().map(Image::size), Some(image().size()));
    assert_eq!(app.context.take_clip(), None, "handed over once");

    // Asking is separate from reading: the host answers on its own time.
    app.effects(|_, effects| {
        effects.paste(|clip| act(move |s: &mut Board| s.seen = Some(clip.clone())));
    });
    app.frame();
    assert!(app.context.take_paste_request());
    assert!(!app.context.take_paste_request(), "asked once");
    assert_eq!(app.state().seen, None);
    app.context.set_clip(Clip::text("from somewhere else"));
    app.frame();
    assert_eq!(
        app.state().seen.as_ref().map(|clip| clip.text.as_str()),
        Some("from somewhere else")
    );
}

#[test]
fn a_clip_carries_every_form_at_once() {
    let clip = Clip::image(image()).with_text("A picture");
    assert!(!clip.is_empty() && clip.custom.is_none());
    assert_eq!(clip.text, "A picture");
    assert!(Clip::default().is_empty());
    assert_eq!(Clip::text("x").bytes("anything"), None);
    assert_eq!(
        Clip::custom("app/thing", vec![9]).bytes("app/thing"),
        Some(&[9][..])
    );
    // Nothing else sees a clip with no text, which is worth saying out loud.
    assert!(Clip::custom("app/thing", vec![9]).text.is_empty());
}

/// The style argument is the node's own, so a view can copy from anywhere.
#[test]
fn copying_does_not_disturb_the_declaration() {
    let mut app = board();
    app.effects(|_, effects| effects.copy(Clip::text("from inside a container")));
    let stats = app.frame();
    assert_eq!(stats.activations, 0);
    assert_eq!(
        app.context.take_clip().map(|clip| clip.text),
        Some("from inside a container".to_owned())
    );
}
