//! Whether the core's damage rectangles are complete: a window driven through
//! real input, painted once with damage and once in full, must agree pixel for
//! pixel. A rectangle the core forgets shows up here as a stale pixel.
use rust_gui_core::{
    App, AppDriver, Button, Context, Effects, Event, Key, Length, MonospaceMeasurer, Style,
    TextEdit, Ui,
};
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
struct Form {
    typed: String,
    checked: bool,
    volume: f64,
    picked: usize,
    rows: Vec<String>,
}

#[derive(Clone)]
enum Msg {
    Typed(String),
    Checked(bool),
    Volume(f64),
    Picked(usize),
    Add,
}

impl App for Form {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::Typed(text) => self.typed = text,
            Msg::Checked(checked) => self.checked = checked,
            Msg::Volume(volume) => self.volume = volume,
            Msg::Picked(index) => self.picked = index,
            Msg::Add => self.rows.push(format!("row {}", self.rows.len())),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        ui.keyed("page", |ui| {
            ui.column(Style::default().padding(8.0).gap(6.0), |ui| {
                ui.keyed("title", |ui| ui.label("Damage"));
                ui.keyed("field", |ui| ui.text_input("Name", &self.typed, Msg::Typed));
                ui.keyed("check", |ui| {
                    ui.checkbox("Ready", self.checked, Msg::Checked)
                });
                ui.keyed("volume", |ui| {
                    ui.slider("Volume", self.volume, 0.0..=100.0, 1.0, Msg::Volume)
                });
                ui.keyed("combo", |ui| {
                    ui.combo_box("Theme", self.picked, &["Dark", "Light"], Msg::Picked)
                });
                ui.keyed("add", |ui| ui.button(Button::new("Add").on_click(Msg::Add)));
                ui.keyed("list", |ui| {
                    ui.scroll(
                        Style::default()
                            .width(Length::Percent(1.0))
                            .height(Length::Px(80.0)),
                        |ui| {
                            for (index, row) in self.rows.iter().enumerate() {
                                ui.keyed(format!("row{index}"), |ui| ui.label(row.clone()));
                            }
                        },
                    )
                });
            })
        });
    }
}

#[test]
fn a_frame_painted_from_damage_is_the_frame_painted_in_full() {
    let mut context = Context::new();
    let mut app = Form {
        rows: (0..8).map(|index| format!("row {index}")).collect(),
        ..Form::default()
    };
    // One renderer keeps its frames and is told what changed; the other is
    // told nothing and paints everything, every time.
    let mut incremental = CpuRenderer::with_text_system(fonts());
    let mut whole = CpuRenderer::with_text_system(fonts());
    let physical = [SIZE.width as u32, SIZE.height as u32];
    let mut driver = AppDriver::new(|| true);
    let mut frame = |context: &mut Context, app: &mut Form, step: &str| {
        driver.frame(app, context, SIZE, &mut MonospaceMeasurer);
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

/// A menu in a title bar: opened by a click, closed by another on the same
/// button. What the popup covered has to be painted again when it goes.
#[test]
fn closing_a_menu_by_its_own_button_leaves_nothing_of_it_behind() {
    use rust_gui_core::{Command, Menu, MenuEntry};

    let mut context = Context::new();
    let menu = Menu::new([MenuEntry::submenu(
        "File",
        [
            MenuEntry::Command(Command::with_id("open", "Open...", None)),
            MenuEntry::Separator,
            MenuEntry::Command(Command::with_id("save", "Save as...", None)),
        ],
    )]);
    let mut incremental = CpuRenderer::with_text_system(fonts());
    let mut whole = CpuRenderer::with_text_system(fonts());
    let physical = [SIZE.width as u32, SIZE.height as u32];
    let mut frame = |context: &mut Context, step: &str| {
        // One declaration per frame: what a click changed is in this one.
        context.run(SIZE, &mut MonospaceMeasurer, |ui| {
            ui.keyed("bar", |ui| {
                ui.title_bar(Style::default().padding(6.0).gap(8.0), |ui| {
                    ui.keyed("name", |ui| ui.label("Title bar"));
                    ui.keyed("menu", |ui| ui.menu_bar(&menu));
                })
            });
            ui.keyed("page", |ui| {
                ui.column(Style::default().padding(8.0).gap(6.0), |ui| {
                    for row in 0..6 {
                        ui.keyed(format!("row{row}"), |ui| {
                            ui.label(format!("A line of text {row}"))
                        });
                    }
                })
            });
        });
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
            .map(|(at, _)| (at % physical[0] as usize, at / physical[0] as usize))
            .collect::<Vec<_>>();
        assert!(
            differing.is_empty(),
            "after {step}, {} pixels differ, first: {:?}",
            differing.len(),
            &differing[..differing.len().min(4)]
        );
    };
    frame(&mut context, "the first frame");

    let button = context
        .node(context.find(&["bar", "menu", "menu0"]).unwrap())
        .unwrap()
        .bounds;
    let at = Point {
        x: button.x + button.width / 2.0,
        y: button.y + button.height / 2.0,
    };
    let click = |context: &mut Context| {
        context.event(Event::PointerMoved(at));
        context.event(Event::PointerDown);
        context.event(Event::PointerUp);
    };
    click(&mut context);
    frame(&mut context, "opening the menu");
    assert!(
        context.find(&["bar", "menu", "menu0", "$popup"]).is_some(),
        "the menu is open"
    );
    click(&mut context);
    frame(&mut context, "closing it with the same button");
    assert!(
        context.find(&["bar", "menu", "menu0", "$popup"]).is_none(),
        "the menu is closed"
    );
}

/// Everything the examples draw over their content: a tooltip on a dwell, a
/// context menu, a popover, a modal dialog with its scrim, and a toast that
/// expires on its own. Each covers part of the window and then goes, which
/// is where stale pixels come from.
#[test]
fn every_overlay_leaves_the_window_as_it_found_it() {
    use rust_gui_core::{DialogOptions, MenuItem, PopoverOptions, TOOLTIP_DELAY, Toast};
    use std::time::Duration;

    /// The overlays the test opens and closes, and the toast it raises once.
    #[derive(Default)]
    struct Over {
        dialog: bool,
        popover: bool,
    }

    #[derive(Clone)]
    enum OverMsg {
        Dialog(bool),
        Popover(bool),
        Pick,
    }

    impl App for Over {
        type Message = OverMsg;

        fn init(&mut self, effects: &mut Effects<'_, OverMsg>) {
            effects.toast(Toast::new("Saved").duration(Duration::from_millis(600)));
        }

        fn update(&mut self, message: OverMsg, _effects: &mut Effects<'_, OverMsg>) {
            match message {
                OverMsg::Dialog(open) => self.dialog = open,
                OverMsg::Popover(open) => self.popover = open,
                OverMsg::Pick => {}
            }
        }

        fn view(&self, ui: &mut Ui<'_, OverMsg>) {
            ui.keyed("page", |ui| {
                ui.column(Style::default().padding(8.0).gap(6.0), |ui| {
                    ui.keyed("title", |ui| ui.label("Overlays"));
                    ui.keyed("hover", |ui| ui.button("Hover me"));
                    ui.tooltip("What this button does");
                    ui.keyed("area", |ui| {
                        ui.context_menu(
                            &[MenuItem::new("Cut"), MenuItem::new("Copy")],
                            Style::default()
                                .width(Length::Percent(1.0))
                                .height(Length::Px(60.0)),
                            |_| OverMsg::Pick,
                            |ui| ui.keyed("inside", |ui| ui.label("Right click here")),
                        )
                    });
                    ui.keyed("popover", |ui| {
                        ui.popover(
                            self.popover,
                            PopoverOptions::default(),
                            OverMsg::Popover,
                            |ui| {
                                ui.keyed("note", |ui| ui.label("A popover over the page"));
                            },
                        )
                    });
                    for row in 0..4 {
                        ui.keyed(format!("row{row}"), |ui| {
                            ui.label(format!("A line of text {row}"))
                        });
                    }
                    ui.dialog(
                        "dialog",
                        self.dialog,
                        DialogOptions::new("A dialog"),
                        OverMsg::Dialog,
                        |ui| ui.keyed("body", |ui| ui.label("Over everything")),
                    );
                })
            });
        }
    }

    let mut context = Context::new();
    let mut app = Over::default();
    let mut incremental = CpuRenderer::with_text_system(fonts());
    let mut whole = CpuRenderer::with_text_system(fonts());
    let physical = [SIZE.width as u32, SIZE.height as u32];
    let mut clock = Duration::ZERO;
    let mut driver = AppDriver::new(|| true);
    let mut frame = |context: &mut Context,
                     driver: &mut AppDriver<OverMsg>,
                     app: &mut Over,
                     tick: Duration,
                     step: &str| {
        clock += tick;
        context.set_now(clock);
        driver.frame(app, context, SIZE, &mut MonospaceMeasurer);
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
            .map(|(at, _)| (at % physical[0] as usize, at / physical[0] as usize))
            .collect::<Vec<_>>();
        assert!(
            differing.is_empty(),
            "after {step}, {} pixels differ, first: {:?}",
            differing.len(),
            &differing[..differing.len().min(4)]
        );
    };
    let tick = Duration::from_millis(16);
    frame(&mut context, &mut driver, &mut app, tick, "the first frame");
    // The toast is raised on the first frame and expires on its own.
    for step in 0..60 {
        frame(
            &mut context,
            &mut driver,
            &mut app,
            tick,
            &format!("toast frame {step}"),
        );
    }

    let at = |context: &Context, path: &[&str]| {
        let bounds = context.node(context.find(path).unwrap()).unwrap().bounds;
        Point {
            x: bounds.x + bounds.width / 2.0,
            y: bounds.y + bounds.height / 2.0,
        }
    };
    // A tooltip appears after the dwell and goes when the pointer leaves.
    let hover = at(&context, &["page", "hover"]);
    context.event(Event::PointerMoved(hover));
    frame(&mut context, &mut driver, &mut app, tick, "hovering");
    frame(
        &mut context,
        &mut driver,
        &mut app,
        TOOLTIP_DELAY,
        "the tooltip appearing",
    );
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "the tooltip settling",
    );
    context.event(Event::PointerMoved(Point { x: 4.0, y: 4.0 }));
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "the tooltip going",
    );

    // A context menu over the page, then dismissed with Escape.
    let area = at(&context, &["page", "area"]);
    context.event(Event::ContextMenu(area));
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "the context menu",
    );
    context.event(Event::KeyDown {
        key: Key::Escape,
        shift: false,
        repeat: false,
    });
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "dismissing the context menu",
    );

    // A popover, opened and closed by the application.
    driver.send(OverMsg::Popover(true));
    frame(&mut context, &mut driver, &mut app, tick, "the popover");
    driver.send(OverMsg::Popover(false));
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "closing the popover",
    );

    // A modal dialog, with the scrim over everything.
    driver.send(OverMsg::Dialog(true));
    frame(&mut context, &mut driver, &mut app, tick, "the dialog");
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "the dialog settling",
    );
    driver.send(OverMsg::Dialog(false));
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "closing the dialog",
    );
    frame(
        &mut context,
        &mut driver,
        &mut app,
        tick,
        "after the dialog",
    );
}
