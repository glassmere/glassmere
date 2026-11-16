//! The firmware of `examples/embedded-firmware` on the desk: its thermostat
//! runs through `glassmere-embedded` into a 320x240 RGB565 buffer, as it does
//! on a microcontroller, and this window shows that buffer twice as large
//! and passes the mouse on as the touch panel.
use embedded_firmware::{HEIGHT, Thermostat, WIDTH};
use glassmere::embedded::{EmbeddedUi, Framebuffer, Pixel, Rgb565};
use glassmere::{
    App, AppOptions, AreaBuilder, Effects, EventArea, Image, ImageFit, ImageOptions, Length, Point,
    Size, Style, Ui, UiEvent, run,
};
use std::time::Instant;

const SCALE: f32 = 2.0;

#[derive(Clone)]
enum Touch {
    Press(Point),
    Drag(Point),
    Release,
}

struct Simulator {
    device: EmbeddedUi<Thermostat>,
    /// The panel's memory.
    pixels: Vec<Rgb565>,
    /// What the window shows of it.
    shown: Image,
    started: Instant,
    pressed: bool,
}

impl Simulator {
    fn new() -> Self {
        let mut simulator = Self {
            device: EmbeddedUi::new(Thermostat::default(), WIDTH, HEIGHT),
            pixels: vec![Rgb565::default(); (WIDTH * HEIGHT) as usize],
            shown: Image::from_rgba8(1, 1, vec![0; 4]).expect("one pixel"),
            started: Instant::now(),
            pressed: false,
        };
        simulator.turn();
        simulator
    }

    /// One turn of the firmware's loop.
    fn turn(&mut self) {
        if !self.device.update(self.started.elapsed()) {
            return;
        }
        let mut frame = Framebuffer::new(&mut self.pixels, WIDTH, HEIGHT);
        self.device.render_damage(&mut frame);
        self.device.presented();
        let rgba = self
            .pixels
            .iter()
            .flat_map(|pixel| {
                let [r, g, b] = pixel.to_rgb();
                [r, g, b, 255]
            })
            .collect();
        self.shown = Image::from_rgba8(WIDTH, HEIGHT, rgba).expect("a full frame");
    }
}

impl App for Simulator {
    type Message = Touch;

    fn update(&mut self, touch: Touch, _effects: &mut Effects<'_, Touch>) {
        let on_panel = |point: Point| (point.x / SCALE, point.y / SCALE);
        match touch {
            Touch::Press(point) => {
                let (x, y) = on_panel(point);
                self.device.press(x, y);
                self.pressed = true;
            }
            Touch::Drag(point) if self.pressed => {
                let (x, y) = on_panel(point);
                self.device.drag(x, y);
            }
            Touch::Drag(_) => return,
            Touch::Release => {
                self.device.release();
                self.pressed = false;
            }
        }
        self.turn();
    }

    fn view(&self, ui: &mut Ui<'_, Touch>) {
        let size = Style::default()
            .width(Length::Px(WIDTH as f32 * SCALE))
            .height(Length::Px(HEIGHT as f32 * SCALE));
        let panel = AreaBuilder::new(EventArea::pointer().capturing()).on_event(|event| {
            Some(match event {
                UiEvent::PointerDown(point) => Touch::Press(point),
                UiEvent::PointerMoved(point) => Touch::Drag(point),
                UiEvent::PointerUp(_) => Touch::Release,
                _ => return None,
            })
        });
        ui.event_area(size.clone(), panel, |ui| {
            ui.image(
                &self.shown,
                ImageOptions::new("The device's display")
                    .fit(ImageFit::Fill)
                    .style(size),
            );
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Embedded".into(),
            size: Size::new(WIDTH as f32 * SCALE, HEIGHT as f32 * SCALE),
            frame_limit: smoke.then_some(2),
            ..AppOptions::default()
        },
        Simulator::new(),
    )
}
