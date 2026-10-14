use rust_gui_core::{Clip, Context, MonospaceMeasurer, Style, Ui};
use rust_gui_scene::{Image, Size};

const ROOT: Size = Size::new(400.0, 300.0);

fn image() -> Image {
    Image::from_rgba8(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).expect("two pixels")
}

/// What a view copies reaches the host, and what the host reads reaches the
/// view. Both go through the outbox, so neither happens inside a declaration.
#[test]
fn a_copy_reaches_the_host_and_what_it_reads_reaches_the_view() {
    let mut context = Context::new();
    let mut copy = None;
    let mut seen: Option<Clip> = None;
    let declare = |context: &mut Context, copy: &mut Option<Clip>, seen: &mut Option<Clip>| {
        context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
            if let Some(clip) = copy.take() {
                ui.copy(clip);
            }
            *seen = ui.pasted().cloned();
            ui.keyed("state", |ui| ui.label("ready"));
        });
    };

    declare(&mut context, &mut copy, &mut seen);
    assert_eq!(context.take_clip(), None);
    assert_eq!(seen, None);

    // A shape: private bytes for this application, a line of text for
    // everything else, and an image of it beside them.
    copy = Some(
        Clip::custom("app/shape", vec![1, 2, 3])
            .with_text("Shape")
            .with_image(image()),
    );
    declare(&mut context, &mut copy, &mut seen);
    let taken = context.take_clip().expect("the host is handed the copy");
    assert_eq!(taken.text, "Shape");
    assert_eq!(taken.bytes("app/shape"), Some(&[1, 2, 3][..]));
    assert_eq!(taken.bytes("app/other"), None);
    assert_eq!(taken.image.as_ref().map(Image::size), Some(image().size()));
    assert_eq!(context.take_clip(), None, "handed over once");

    // Asking is separate from reading: the host answers on its own time.
    let mut asked = false;
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.request_paste();
        asked = true;
        ui.keyed("state", |ui| ui.label("ready"));
    });
    assert!(asked && context.take_paste_request());
    assert!(!context.take_paste_request(), "asked once");
    context.set_clip(Clip::text("from somewhere else"));
    declare(&mut context, &mut copy, &mut seen);
    assert_eq!(
        seen.as_ref().map(|clip| clip.text.as_str()),
        Some("from somewhere else")
    );
    // It stays readable, so a view can come back to it.
    declare(&mut context, &mut copy, &mut seen);
    assert!(seen.is_some());
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
    let mut context = Context::new();
    let stats = context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        ui.keyed("page", |ui| {
            ui.column(Style::default(), |ui| {
                ui.copy(Clip::text("from inside a container"));
                ui.keyed("label", |ui| ui.label("Copied"));
            })
        });
    });
    assert_eq!(stats.activations, 0);
    assert_eq!(
        context.take_clip().map(|clip| clip.text),
        Some("from inside a container".to_owned())
    );
}
