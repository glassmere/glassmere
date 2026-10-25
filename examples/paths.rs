//! Paths: how strokes join and end, dashes, the two fill rules, and gradient
//! fills, drawn by a widget of the application's own.
use glassmere::{
    App, AppOptions, Brush, Cap, Color, CustomWidget, Dash, Effects, Fill, FillRule, GradientStop,
    Join, Label, Path, Point, Primitive, Semantics, Size, Stroke, Style, Ui, WidgetBuilder, run,
};

const GOLD: Color = Color(250, 204, 21, 255);

fn at(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn polygon(points: &[(f32, f32)]) -> Path {
    let mut path = Path::new();
    for (index, (x, y)) in points.iter().enumerate() {
        if index == 0 {
            path.move_to(at(*x, *y));
        } else {
            path.line_to(at(*x, *y));
        }
    }
    path
}

fn stroked(path: Path, stroke: Stroke) -> Primitive {
    Primitive::Path {
        path,
        fill: None,
        stroke: Some(stroke),
    }
}

fn filled(path: Path, fill: Fill) -> Primitive {
    Primitive::Path {
        path,
        fill: Some(fill),
        stroke: None,
    }
}

fn sampler() -> CustomWidget {
    let mut primitives = Vec::new();
    // A corner joined and capped each way.
    for (index, (join, cap)) in [
        (Join::Miter, Cap::Butt),
        (Join::Round, Cap::Round),
        (Join::Bevel, Cap::Square),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 20.0 + 70.0 * index as f32;
        primitives.push(stroked(
            polygon(&[(x, 70.0), (x + 20.0, 20.0), (x + 40.0, 70.0)]),
            Stroke {
                width: 12.0,
                color: GOLD,
                join,
                cap,
                ..Stroke::default()
            },
        ));
    }
    // A curve in dashes with round ends, and a ring of dots.
    let mut wave = Path::new();
    wave.move_to(at(20.0, 120.0))
        .cubic_to(at(70.0, 80.0), at(120.0, 160.0), at(170.0, 120.0))
        .quad_to(at(200.0, 100.0), at(230.0, 120.0));
    primitives.push(stroked(
        wave,
        Stroke {
            width: 5.0,
            color: Color::rgb(244, 114, 182),
            cap: Cap::Round,
            dash: Some(Dash::new([14.0, 8.0])),
            ..Stroke::default()
        },
    ));
    let mut ring = Path::new();
    ring.move_to(at(40.0, 190.0))
        .cubic_to(at(40.0, 150.0), at(100.0, 150.0), at(100.0, 190.0))
        .cubic_to(at(100.0, 230.0), at(40.0, 230.0), at(40.0, 190.0))
        .close();
    primitives.push(stroked(
        ring,
        Stroke {
            width: 6.0,
            color: Color::WHITE,
            cap: Cap::Round,
            // A dash of no length is a dot where the caps are round.
            dash: Some(Dash::new([0.0, 12.0])),
            ..Stroke::default()
        },
    ));
    // A pentagram filled by each rule: the even-odd one has no middle.
    for (index, rule) in [FillRule::NonZero, FillRule::EvenOdd]
        .into_iter()
        .enumerate()
    {
        let (cx, cy) = (150.0 + 90.0 * index as f32, 190.0);
        let mut star = Path::new();
        for point in 0..5 {
            let angle = std::f32::consts::PI * 4.0 * point as f32 / 5.0;
            let to = at(cx + 38.0 * angle.sin(), cy - 38.0 * angle.cos());
            if point == 0 {
                star.move_to(to);
            } else {
                star.line_to(to);
            }
        }
        star.close();
        primitives.push(filled(
            star,
            Fill {
                rule,
                ..Color::rgb(56, 189, 248).into()
            },
        ));
    }
    // Gradients run over the bounds of the path, as they do over a rectangle.
    let stops = vec![
        GradientStop {
            at: 0.0,
            color: Color::rgb(239, 68, 68),
        },
        GradientStop {
            at: 0.5,
            color: GOLD,
        },
        GradientStop {
            at: 1.0,
            color: Color::rgb(37, 99, 235),
        },
    ];
    let mut leaf = Path::new();
    leaf.move_to(at(20.0, 300.0))
        .cubic_to(at(20.0, 250.0), at(80.0, 250.0), at(130.0, 250.0))
        .cubic_to(at(130.0, 300.0), at(80.0, 310.0), at(20.0, 300.0))
        .close();
    primitives.push(filled(
        leaf,
        Fill {
            brush: Brush::Linear {
                from: at(0.0, 0.0),
                to: at(1.0, 1.0),
                stops: stops.clone(),
            },
            rule: FillRule::NonZero,
        },
    ));
    let mut disc = Path::new();
    disc.move_to(at(160.0, 275.0))
        .cubic_to(at(160.0, 250.0), at(240.0, 250.0), at(240.0, 275.0))
        .cubic_to(at(240.0, 305.0), at(160.0, 305.0), at(160.0, 275.0))
        .close();
    primitives.push(filled(
        disc,
        Fill {
            brush: Brush::Radial {
                from: at(0.5, 0.5),
                to: at(1.0, 0.5),
                stops,
            },
            rule: FillRule::NonZero,
        },
    ));
    CustomWidget {
        size: Size::new(260.0, 330.0),
        semantics: Semantics {
            label: "Path styles".into(),
            ..Semantics::default()
        },
        primitives,
        ..CustomWidget::default()
    }
}

struct Paths;

impl App for Paths {
    type Message = ();

    fn update(&mut self, _: (), _: &mut Effects<'_, ()>) {}

    fn view(&self, ui: &mut Ui<'_, ()>) {
        ui.column(
            Style {
                padding: 24.0,
                gap: 12.0,
                ..Style::default()
            },
            |ui| {
                ui.label(Label::new("Joins, caps, dashes, fill rules and gradients"));
                ui.widget(Style::default(), WidgetBuilder::new(|_| sampler()));
            },
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    run(
        AppOptions {
            title: "Glassmere / Paths".into(),
            size: Size::new(340.0, 460.0),
            frame_limit: smoke.then_some(5),
            ..AppOptions::default()
        },
        Paths,
    )?;
    Ok(())
}
