//! Children a frame no longer declares are removed in the order they had,
//! whatever order the tables that find them hold them in, so what the
//! removals leave, the ghosts of their exit effects and the damage, is the
//! same in every context.
use glassmere_core::{Context, Effect, Label, MonospaceMeasurer, Style, Ui};
use glassmere_scene::{Primitive, Rect, Size};
use std::time::Duration;

const ROOT: Size = Size::new(400.0, 800.0);
const COUNT: usize = 30;
const GONE: [usize; 13] = [3, 4, 7, 8, 9, 12, 15, 16, 20, 21, 22, 25, 29];

fn frame(context: &mut Context, now: u64, names: &[String]) {
    context.set_now(Duration::from_millis(now));
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_>| {
        for name in names {
            ui.keyed(name.as_str(), |ui| {
                ui.label(Label::new(name.as_str()).style(Style::default().exit(Effect::fade(100))))
            });
        }
    });
}

/// The bounds of the children that go, in the order they had, the ghosts
/// the removal frame painted in the order they are painted, and the damage
/// of the frame after it, which repaints where the ghosts are.
struct Removal {
    gone: Vec<Rect>,
    ghosts: Vec<Rect>,
    damage: Vec<Rect>,
}

fn ghosts_in(context: &Context, gone: &[Rect]) -> Vec<Rect> {
    context
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::PushLayer { bounds, .. } if gone.contains(bounds) => Some(*bounds),
            _ => None,
        })
        .collect()
}

fn removal() -> Removal {
    let names: Vec<String> = (0..COUNT).map(|at| format!("item-{}", at * 7919)).collect();
    let mut context = Context::new();
    frame(&mut context, 0, &names);
    let bounds = |context: &Context, name: &String| {
        context
            .node(context.find(&[name.as_str()]).unwrap())
            .unwrap()
            .bounds
    };
    let gone: Vec<Rect> = GONE
        .iter()
        .map(|at| bounds(&context, &names[*at]))
        .collect();
    let kept: Vec<String> = names
        .iter()
        .enumerate()
        .filter(|(at, _)| !GONE.contains(at))
        .map(|(_, name)| name.clone())
        .collect();
    frame(&mut context, 1000, &kept);
    let ghosts = ghosts_in(&context, &gone);
    frame(&mut context, 1050, &kept);
    Removal {
        gone,
        ghosts,
        damage: context.damage_regions().to_vec(),
    }
}

#[test]
fn ghosts_and_damage_follow_the_old_order_of_the_children_in_every_context() {
    let first = removal();
    let second = removal();
    assert!(first.gone.windows(2).all(|pair| pair[0].y < pair[1].y));
    assert_eq!(first.ghosts, first.gone);
    assert_eq!(second.ghosts, second.gone);
    assert_eq!(first.gone, second.gone);
    assert_eq!(first.damage, second.damage);
    assert!(first.damage.len() > 1);
}
