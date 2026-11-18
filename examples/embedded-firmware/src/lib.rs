//! A thermostat for a 320x240 panel: the application, and the loop that
//! runs it on a board. Nothing here needs an operating system; `main.rs` is
//! the firmware around it, and `cargo run --example embedded` in the
//! workspace shows the same application in a desktop window.
#![no_std]

extern crate alloc;

use alloc::format;
use core::time::Duration;
use glassmere_core::{Align, App, Button, Effects, Label, Length, Progress, Style, Ui};
use glassmere_embedded::{EmbeddedUi, Framebuffer, Rgb565};

pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 240;
/// Rows painted at a time: 320 x 24 pixels are 15 KiB of RAM, not the 150 KiB
/// of a whole frame.
const BAND: u32 = 24;

pub struct Thermostat {
    /// Tenths of a degree, as the sensor reports them.
    pub measured: i32,
    pub target: i32,
    pub heating: bool,
}

impl Default for Thermostat {
    fn default() -> Self {
        Self {
            measured: 204,
            target: 215,
            heating: true,
        }
    }
}

#[derive(Clone)]
pub enum Message {
    Warmer,
    Cooler,
    Heating(bool),
    /// From the sensor, outside the UI.
    Measured(i32),
}

impl App for Thermostat {
    type Message = Message;

    fn update(&mut self, message: Message, _effects: &mut Effects<'_, Message>) {
        match message {
            Message::Warmer => self.target = (self.target + 5).min(300),
            Message::Cooler => self.target = (self.target - 5).max(100),
            Message::Heating(on) => self.heating = on,
            Message::Measured(tenths) => self.measured = tenths,
        }
    }

    fn view(&self, ui: &mut Ui<'_, Message>) {
        let degrees = |tenths: i32| format!("{}.{} °C", tenths / 10, tenths % 10);
        let fill = Style::default().width(Length::Percent(1.0));
        ui.column(fill.clone().padding(12.0).gap(10.0), |ui| {
            ui.label(Label::new("Living room").style(Style::default().font_size(13.0)));
            ui.label(Label::new(degrees(self.measured)).style(Style::default().font_weight(700)));
            ui.row(fill.clone().gap(8.0).align_items(Align::Center), |ui| {
                ui.button(Button::new("-").on_click(Message::Cooler));
                ui.label(
                    Label::new(format!("Target {}", degrees(self.target)))
                        .style(Style::default().grow(1.0)),
                );
                ui.button(Button::new("+").on_click(Message::Warmer));
            });
            ui.checkbox("Heating", self.heating, Message::Heating);
            let reached = f64::from(self.measured - 100) / f64::from((self.target - 100).max(1));
            ui.progress(Progress::new("Reached"), self.heating.then_some(reached));
        });
    }
}

/// What the loop needs from the hardware.
pub trait Board {
    /// Time since the board started.
    fn now(&mut self) -> Duration;
    /// Where the panel is pressed, in pixels.
    fn touch(&mut self) -> Option<(u16, u16)>;
    /// A new reading of the temperature sensor, in tenths of a degree.
    fn temperature(&mut self) -> Option<i32>;
    /// Sends `pixels`, rows of `width`, to the panel with their first one at
    /// `x`, `y`.
    fn flush(&mut self, x: u16, y: u16, width: u16, pixels: &[Rgb565]);
    /// Sleeps until an interrupt or until `until`, whichever is first.
    fn sleep(&mut self, until: Option<Duration>);
}

pub fn run(mut board: impl Board) -> ! {
    let mut ui = EmbeddedUi::new(Thermostat::default(), WIDTH, HEIGHT);
    let mut band = [Rgb565::default(); (WIDTH * BAND) as usize];
    let mut pressed = false;
    loop {
        match (board.touch(), pressed) {
            (Some((x, y)), false) => ui.press(f32::from(x), f32::from(y)),
            (Some((x, y)), true) => ui.drag(f32::from(x), f32::from(y)),
            (None, true) => ui.release(),
            (None, false) => {}
        }
        pressed = board.touch().is_some();
        if let Some(tenths) = board.temperature() {
            ui.send(Message::Measured(tenths));
        }

        if ui.update(board.now()) {
            for at in 0..ui.damage().len() {
                let damage = ui.damage()[at];
                // Whole pixels around the rectangle, inside the screen.
                let left = (damage.x.max(0.0) as u32).min(WIDTH);
                let top = (damage.y.max(0.0) as u32).min(HEIGHT);
                let right = ((damage.x + damage.width + 1.0).max(0.0) as u32).min(WIDTH);
                let bottom = ((damage.y + damage.height + 1.0).max(0.0) as u32).min(HEIGHT);
                let width = right - left;
                let mut y = top;
                while y < bottom && width > 0 {
                    let rows = BAND.min(bottom - y);
                    let pixels = &mut band[..(width * rows) as usize];
                    let mut frame = Framebuffer::new(pixels, width, rows).at(left as i32, y as i32);
                    ui.render(&mut frame);
                    board.flush(left as u16, y as u16, width as u16, pixels);
                    y += rows;
                }
            }
            ui.presented();
        }
        if !ui.animating() {
            board.sleep(ui.next_wake());
        }
    }
}
