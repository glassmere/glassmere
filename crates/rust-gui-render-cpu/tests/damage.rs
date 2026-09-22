//! Whether the core's damage rectangles are complete: a window driven through
//! real input, painted once with damage and once in full, must agree pixel for
//! pixel. A rectangle the core forgets shows up here as a stale pixel.
use rust_gui_core::{Context, Event, Key, Length, MonospaceMeasurer, Style, TextEdit, Ui};
use rust_gui_render_cpu::CpuRenderer;
use rust_gui_scene::{Color, Point, Size};
use rust_gui_text::TextSystem;

const SIZE: Size = Size::new(320.0, 360.0);
const CLEAR: Color = Color::rgb(16, 23, 34);

fn fonts() -> TextSystem {
    TextSystem::with_fonts([include_bytes!(
        "../../rust-gui-render-wgpu/fonts/FiraMono-Regular.ttf"
    )
    .to_vec()])
}

#[derive(Default)]
struct App {
    typed: String,
    checked: bool,
    volume: f64,
    picked: usize,
    rows: Vec<String>,
}

fn view(ui: &mut Ui<'_>, app: &mut App) {
    ui.column("page", Style::default().padding(8.0).gap(6.0), |ui| {
        ui.label_keyed("title", "Damage");
        ui.text_input("field", "Name", &mut app.typed);
        ui.checkbox("check", "Ready", &mut app.checked);
        ui.slider("volume", "Volume", &mut app.volume, 0.0..=100.0, 1.0);
        ui.combo_box("combo", "Theme", &mut app.picked, &["Dark", "Light"]);
        if ui.button_keyed("add", "Add").clicked() {
            app.rows.push(format!("row {}", app.rows.len()));
        }
        ui.scroll(
            "list",
            Style::default()
                .width(Length::Percent(1.0))
                .height(Length::Px(80.0)),
            |ui| {
                for (index, row) in app.rows.iter().enumerate() {
                    ui.label_keyed(format!("row{index}"), row.clone());
                }
            },
        );
    });
}

#[test]
fn a_frame_painted_from_damage_is_the_frame_painted_in_full() {
    let mut context = Context::new();
    let mut app = App {
        rows: (0..8).map(|index| format!("row {index}")).collect(),
        ..App::default()
    };
    // One renderer keeps its frames and is told what changed; the other is
    // told nothing and paints everything, every time.
    let mut incremental = CpuRenderer::with_text_system(fonts());
    let mut whole = CpuRenderer::with_text_system(fonts());
    let physical = [SIZE.width as u32, SIZE.height as u32];
    let mut frame = |context: &mut Context, app: &mut App, step: &str| {
        context.run(SIZE, &mut MonospaceMeasurer, |ui| view(ui, app));
        let damage: Vec<_> = context.damage_regions().to_vec();
        let scene = context.scene();
        let partial = incremental
            .render_damaged(physical, 1.0, scene, CLEAR, &damage)
            .to_vec();
        let full = whole.render(physical, 1.0, scene, CLEAR).to_vec();
        let differing = partial
            .as_chunks::<4>()
            .0
            .iter()
            .zip(full.as_chunks::<4>().0)
            .enumerate()
            .filter(|(_, (left, right))| left != right)
            .map(|(at, (left, right))| {
                format!(
                    "({}, {}) {left:?} against {right:?}",
                    at % physical[0] as usize,
                    at / physical[0] as usize
                )
            })
            .collect::<Vec<_>>();
        assert!(
            differing.is_empty(),
            "after {step}, {} pixels differ, first: {:?}",
            differing.len(),
            &differing[..differing.len().min(4)]
        );
        // The point of it all: most steps touch a small part of the window.
        incremental.stats().redrawn
    };
    let area = u64::from(physical[0]) * u64::from(physical[1]);
    assert_eq!(frame(&mut context, &mut app, "the first frame"), area);

    let at = |context: &Context, path: &[&str]| {
        let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    };
    // Hovering repaints one control.
    let check = at(&context, &["page", "check"]);
    context.event(Event::PointerMoved(check));
    let hover = frame(&mut context, &mut app, "hovering the checkbox");
    assert!(hover < area / 4, "hovering repainted {hover} of {area}");

    // Pressing, releasing and so toggling it.
    context.event(Event::PointerDown);
    frame(&mut context, &mut app, "pressing the checkbox");
    context.event(Event::PointerUp);
    frame(&mut context, &mut app, "releasing on the checkbox");
    assert!(app.checked);

    // Focus and a caret, then typing into the field.
    let field = at(&context, &["page", "field"]);
    context.event(Event::PointerMoved(field));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    frame(&mut context, &mut app, "clicking the field");
    for letter in ["a", "b", "c"] {
        context.event(Event::Text(TextEdit::Insert(letter.into())));
        let typing = frame(&mut context, &mut app, "typing");
        assert!(typing < area / 3, "typing repainted {typing} of {area}");
    }
    assert_eq!(app.typed, "abc");

    // Dragging the slider, which moves a thumb across the window.
    let volume = at(&context, &["page", "volume"]);
    context.event(Event::PointerMoved(volume));
    context.event(Event::PointerDown);
    for step in 1..4 {
        context.event(Event::PointerMoved(Point {
            x: volume.x + 20.0 * step as f32,
            y: volume.y,
        }));
        frame(&mut context, &mut app, "dragging the slider");
    }
    context.event(Event::PointerUp);
    frame(&mut context, &mut app, "letting the slider go");
    assert!(app.volume > 50.0);

    // A popup that appears over the page and then closes.
    let combo = at(&context, &["page", "combo"]);
    context.event(Event::PointerMoved(combo));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    frame(&mut context, &mut app, "opening the combo");
    context.event(Event::KeyDown {
        key: Key::ArrowDown,
        shift: false,
        repeat: false,
    });
    frame(&mut context, &mut app, "moving in the combo");
    context.event(Event::KeyDown {
        key: Key::Enter,
        shift: false,
        repeat: false,
    });
    frame(&mut context, &mut app, "choosing in the combo");
    assert_eq!(app.picked, 1);

    // Scrolling the list, which moves its content under a clip.
    let list = at(&context, &["page", "list"]);
    context.event(Event::PointerMoved(list));
    context.event(Event::Scroll {
        delta_x: 0.0,
        delta_y: 30.0,
    });
    let scrolled = frame(&mut context, &mut app, "scrolling the list");
    assert!(scrolled < area, "scrolling repainted the window");

    // Adding a row, which moves everything below it.
    let add = at(&context, &["page", "add"]);
    context.event(Event::PointerMoved(add));
    context.event(Event::PointerDown);
    context.event(Event::PointerUp);
    frame(&mut context, &mut app, "adding a row");
    assert_eq!(app.rows.len(), 9);

    // And the window losing focus, which drops hover and the caret.
    context.event(Event::FocusLost);
    frame(&mut context, &mut app, "losing focus");
}
