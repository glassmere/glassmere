use super::*;
use crate::animation::{Shown, lerp_rect};
use crate::{Easing, Effect, Transform, Transition};
use alloc::boxed::Box;
use alloc::vec::Vec;
use glassmere_scene::Affine;

/// One interpolation between two displayed states.
#[derive(Clone, Copy, Debug)]
struct Track {
    from: Shown,
    to: Shown,
    start: Duration,
    transition: Transition,
}

/// The last transition progress worked out, kept because nodes that share a
/// transition and a start ask for the same value in the same frame.
#[derive(Clone, Copy)]
pub(super) struct ProgressMemo {
    transition: Transition,
    elapsed: Duration,
    result: (f32, bool),
}

impl ProgressMemo {
    /// `transition.progress(elapsed)`, reusing the previous call when every
    /// input of it is the same.
    fn progress(
        slot: &mut Option<Self>,
        transition: &Transition,
        elapsed: Duration,
    ) -> (f32, bool) {
        if let Some(memo) = slot
            && memo.elapsed == elapsed
            && same_inputs(&memo.transition, transition)
        {
            return memo.result;
        }
        let result = transition.progress(elapsed);
        *slot = Some(Self {
            transition: *transition,
            elapsed,
            result,
        });
        result
    }
}

/// Whether two transitions give the same progress at the same elapsed time.
/// Curve parameters are compared by bits, and NaN never matches.
fn same_inputs(a: &Transition, b: &Transition) -> bool {
    a.duration == b.duration
        && a.delay == b.delay
        && match (a.easing, b.easing) {
            (Easing::CubicBezier(a1, a2, a3, a4), Easing::CubicBezier(b1, b2, b3, b4)) => {
                [(a1, b1), (a2, b2), (a3, b3), (a4, b4)]
                    .iter()
                    .all(|(x, y)| x.to_bits() == y.to_bits() && !x.is_nan())
            }
            (Easing::Linear, Easing::Linear)
            | (Easing::EaseIn, Easing::EaseIn)
            | (Easing::EaseOut, Easing::EaseOut)
            | (Easing::EaseInOut, Easing::EaseInOut) => true,
            _ => false,
        }
}

/// A layout animation from a previous parent-relative rectangle.
#[derive(Clone, Copy, Debug)]
struct Flip {
    from: Rect,
    start: Duration,
    transition: Transition,
}

/// Retained animation state, allocated only for nodes that animate.
#[derive(Debug, Default)]
pub(super) struct NodeAnim {
    target: Option<Shown>,
    shown: Option<Shown>,
    track: Option<Track>,
    enter: Option<Effect>,
    flip: Option<Flip>,
    keyframes_start: Option<Duration>,
    keyframes: Option<crate::Animation>,
}

impl NodeAnim {
    pub(super) fn entering(effect: Option<Effect>) -> Option<Box<Self>> {
        effect.map(|effect| {
            Box::new(Self {
                enter: Some(effect),
                ..Self::default()
            })
        })
    }
}

/// Paint of a removed subtree that fades out without being part of the tree.
pub(super) struct Ghost {
    primitives: Vec<Primitive>,
    bounds: Rect,
    clip: Rect,
    effect: Effect,
    start: Duration,
}

/// What `paint` needs for one node this frame.
pub(super) struct Displayed {
    pub shown: Shown,
    /// Layer to wrap the node's chunk in, when opacity or transform apply.
    pub layer: Option<(f32, Affine)>,
}

impl Context {
    /// Animations are skipped (transitions jump, keyframes hold their last
    /// frame, enter/exit effects do not run) while set.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        if self.reduced_motion != reduced {
            self.reduced_motion = reduced;
            self.ghosts.clear();
            for node in self.nodes.values_mut() {
                node.paint_cache = None;
            }
            // An animated image changes frame on screen.
            for id in self.image_plays.keys().copied().collect::<Vec<_>>() {
                self.invalidate_paint(id);
            }
            self.pending |= Dirty::PAINT;
        }
    }

    /// Whether another frame is needed without input. Hosts redraw while true.
    pub fn animating(&self) -> bool {
        !self.animating_nodes.is_empty()
            || !self.ghosts.is_empty()
            || self.redraw_requested
            || self.flinging()
    }

    /// Marks last frame's animated nodes dirty so this frame re-evaluates them.
    pub(super) fn begin_animation_frame(&mut self) {
        for id in core::mem::take(&mut self.animating_nodes) {
            self.invalidate_paint(id);
        }
        if !self.ghosts.is_empty() {
            self.pending |= Dirty::PAINT;
            // A ghost is clipped before its layer is transformed, so whatever
            // it draws, moved or faded, lands inside its clip. That includes
            // the frame it ends on: it was drawn there last time.
            let clips: Vec<_> = self.ghosts.iter().map(|ghost| ghost.clip).collect();
            for clip in clips {
                self.add_damage(clip);
            }
        }
    }

    /// Advances one node's animation state toward `target` and returns what to paint.
    pub(super) fn displayed(&mut self, id: NodeId, target: Shown) -> Displayed {
        let now = self.now;
        let reduced = self.reduced_motion;
        let node = self.nodes.get_mut(&id).unwrap();
        let bounds = node.bounds;
        let animated = node.style.transition.is_some() || node.style.animation.is_some();
        if node.anim.is_none() && !animated {
            return Displayed {
                shown: target,
                layer: layer_for(target.opacity, target.transform.to_affine(bounds)),
            };
        }
        let transition = node.style.transition;
        let animation = node.style.animation.clone();
        let layout_rect = node.layout_rect;
        let anim = node.anim.get_or_insert_with(Default::default);
        let mut running = false;

        // State track: start on first paint (enter) or whenever the target moves.
        if anim.target != Some(target) {
            let first = anim.shown.is_none();
            let start = if first {
                anim.enter.take().filter(|_| !reduced).map(|effect| {
                    let from = Shown {
                        opacity: target.opacity * effect.opacity,
                        transform: target.transform.combined(effect.transform),
                        ..target
                    };
                    (from, effect.transition)
                })
            } else {
                transition
                    .filter(|_| !reduced)
                    .map(|transition| (anim.shown.unwrap_or(target), transition))
            };
            anim.track = start.map(|(from, transition)| Track {
                from,
                to: target,
                start: now,
                transition,
            });
            anim.target = Some(target);
            if anim.track.is_none() {
                anim.shown = Some(target);
            }
        }
        if let Some(track) = anim.track {
            let elapsed = now.saturating_sub(track.start);
            let (progress, finished) =
                ProgressMemo::progress(&mut self.progress_memo, &track.transition, elapsed);
            anim.shown = Some(track.from.lerp(track.to, progress));
            if finished {
                anim.shown = Some(track.to);
                anim.track = None;
            } else {
                running = true;
            }
        }
        let mut shown = anim.shown.unwrap_or(target);

        // Keyframes overlay the state without feeding back into it.
        if anim.keyframes != animation {
            anim.keyframes = animation.clone();
            anim.keyframes_start = animation.as_ref().map(|_| now);
        }
        if let (Some(animation), Some(start)) = (&animation, anim.keyframes_start) {
            let elapsed = if reduced {
                Duration::MAX
            } else {
                now.saturating_sub(start)
            };
            let (frame, active) = animation.sample(elapsed);
            running |= active && !reduced;
            if let Some(opacity) = frame.opacity {
                shown.opacity *= opacity;
            }
            if let Some(transform) = frame.transform {
                shown.transform = shown.transform.combined(transform);
            }
            if let Some(color) = frame.background {
                shown.background = Some(color);
            }
            if let Some(color) = frame.foreground {
                shown.foreground = color;
            }
        }

        // Layout animation: draw at the interpolated old position and size.
        let mut affine = shown.transform.to_affine(bounds);
        if let Some(flip) = anim.flip {
            let (progress, finished) = flip.transition.progress(now.saturating_sub(flip.start));
            if finished || reduced {
                anim.flip = None;
            } else {
                running = true;
                let current = lerp_rect(flip.from, layout_rect, progress);
                let scale = |from: f32, to: f32| if to > 0.0 { from / to } else { 1.0 };
                let origin = Point {
                    x: bounds.x,
                    y: bounds.y,
                };
                affine = affine.then(
                    Affine::scale(
                        scale(current.width, layout_rect.width),
                        scale(current.height, layout_rect.height),
                    )
                    .about(origin)
                    .then(Affine::translate(
                        current.x - layout_rect.x,
                        current.y - layout_rect.y,
                    )),
                );
            }
        }
        if running {
            self.animating_nodes.push(id);
        }
        if !affine.is_identity() {
            // Transformed pixels land outside the node's clip, so damage the
            // whole scene rather than computing the transformed box.
            self.damage_scene();
        }
        Displayed {
            shown,
            layer: layer_for(shown.opacity, affine),
        }
    }

    /// Starts a layout animation when a node's parent-relative rectangle changes.
    pub(super) fn note_layout_change(&mut self, id: NodeId, old: Rect, new: Rect) {
        if old == new
            || (old.width == 0.0 && old.height == 0.0)
            || self.reduced_motion
            // Following a window resize live is the point; easing behind it lags.
            || self.resizing
        {
            return;
        }
        let now = self.now;
        let node = self.nodes.get_mut(&id).unwrap();
        let Some(transition) = node.style.layout_transition else {
            return;
        };
        let anim = node.anim.get_or_insert_with(Default::default);
        // Interrupting starts from where the node is currently drawn.
        let from = anim.flip.map_or(old, |flip| {
            let (progress, _) = flip.transition.progress(now.saturating_sub(flip.start));
            lerp_rect(flip.from, old, progress)
        });
        anim.flip = Some(Flip {
            from,
            start: now,
            transition,
        });
        self.invalidate_paint(id);
    }

    /// Keeps a removed subtree's paint as a ghost when it declares an exit effect.
    pub(super) fn remove_with_exit(&mut self, id: NodeId) {
        if !self.reduced_motion {
            self.capture_ghosts(id);
        }
        self.remove(id);
    }

    /// Ghosts the topmost nodes of the subtree that declare an exit effect, so
    /// a dialog panel fades even though its layer node is what gets removed.
    fn capture_ghosts(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        let Some(effect) = node.style.exit else {
            for child in node.children.clone() {
                self.capture_ghosts(child);
            }
            return;
        };
        let mut primitives = Vec::new();
        self.paint(id).append_to(&mut primitives);
        let node = &self.nodes[&id];
        self.ghosts.push(Ghost {
            primitives,
            bounds: node.bounds,
            clip: node.clip,
            effect,
            start: self.now,
        });
        self.pending |= Dirty::PAINT;
    }

    /// Paints ghosts after normal content and drops the finished ones.
    pub(super) fn paint_ghosts(&mut self, scene: &mut Scene) {
        let now = self.now;
        self.ghosts.retain(|ghost| {
            let (progress, finished) = ghost
                .effect
                .transition
                .progress(now.saturating_sub(ghost.start));
            if finished {
                return false;
            }
            let opacity = crate::animation::lerp(1.0, ghost.effect.opacity, progress);
            let transform = Transform::IDENTITY.lerp(ghost.effect.transform, progress);
            scene.primitives.push(Primitive::PushClip(ghost.clip));
            scene.primitives.push(Primitive::PushLayer {
                bounds: ghost.bounds,
                opacity,
                transform: transform.to_affine(ghost.bounds),
            });
            scene.primitives.extend(ghost.primitives.iter().cloned());
            scene.primitives.push(Primitive::PopLayer);
            scene.primitives.push(Primitive::PopClip);
            true
        });
    }
}

fn layer_for(opacity: f32, affine: Affine) -> Option<(f32, Affine)> {
    let opacity = opacity.clamp(0.0, 1.0);
    (opacity < 1.0 || !affine.is_identity()).then_some((opacity, affine))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same_result(a: (f32, bool), b: (f32, bool)) -> bool {
        (a.0.to_bits() == b.0.to_bits() || (a.0.is_nan() && b.0.is_nan())) && a.1 == b.1
    }

    fn transitions() -> Vec<Transition> {
        let mut all = Vec::new();
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::CubicBezier(0.2, 0.0, 0.0, 1.0),
            Easing::CubicBezier(0.0, 0.5, 1.0, 0.5),
            Easing::CubicBezier(-0.0, 0.5, 1.0, 0.5),
            Easing::CubicBezier(f32::NAN, 0.5, 1.0, 0.5),
        ] {
            for (duration, delay) in [(300, 0), (301, 0), (300, 16), (0, 0), (0, 16)] {
                all.push(Transition::new(duration).easing(easing).delay(delay));
            }
        }
        all
    }

    fn elapsed_times() -> [Duration; 11] {
        [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_micros(15_999),
            Duration::from_millis(15),
            Duration::from_millis(16),
            Duration::from_micros(150_000),
            Duration::from_micros(150_001),
            Duration::from_millis(150),
            Duration::from_millis(300),
            Duration::from_millis(316),
            Duration::MAX,
        ]
    }

    #[test]
    fn the_memo_answers_what_a_direct_call_answers_in_any_order() {
        let all = transitions();
        let times = elapsed_times();
        let mut slot = None;
        // Each pair is asked three times in a row and then again after
        // every other pair, so hits, misses and evictions all occur.
        for _ in 0..2 {
            for transition in &all {
                for &elapsed in &times {
                    for _ in 0..3 {
                        let memoised = ProgressMemo::progress(&mut slot, transition, elapsed);
                        assert!(
                            same_result(memoised, transition.progress(elapsed)),
                            "{transition:?} at {elapsed:?}"
                        );
                    }
                }
            }
        }
        for &elapsed in &times {
            for transition in all.iter().rev() {
                let memoised = ProgressMemo::progress(&mut slot, transition, elapsed);
                assert!(
                    same_result(memoised, transition.progress(elapsed)),
                    "{transition:?} at {elapsed:?}"
                );
            }
        }
    }

    #[test]
    fn transitions_that_differ_in_any_input_do_not_share_an_entry() {
        let base = Transition::new(300).easing(Easing::CubicBezier(0.2, 0.0, 0.0, 1.0));
        assert!(same_inputs(&base, &base));
        let others = [
            Transition::new(301).easing(base.easing),
            base.delay(16),
            base.easing(Easing::CubicBezier(0.25, 0.0, 0.0, 1.0)),
            base.easing(Easing::CubicBezier(0.2, 0.1, 0.0, 1.0)),
            base.easing(Easing::CubicBezier(0.2, 0.0, 0.1, 1.0)),
            base.easing(Easing::CubicBezier(0.2, 0.0, 0.0, 0.9)),
            base.easing(Easing::EaseOut),
        ];
        for other in &others {
            assert!(!same_inputs(&base, other), "{other:?}");
            assert!(!same_inputs(other, &base), "{other:?}");
        }
        let easings = [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::CubicBezier(0.2, 0.0, 0.0, 1.0),
        ];
        for (i, &first) in easings.iter().enumerate() {
            for (j, &second) in easings.iter().enumerate() {
                let (a, b) = (
                    Transition::new(300).easing(first),
                    Transition::new(300).easing(second),
                );
                assert_eq!(same_inputs(&a, &b), i == j, "{first:?} and {second:?}");
            }
        }
    }

    #[test]
    fn signed_zeros_and_nan_never_match() {
        let curve = |x1: f32| Transition::new(300).easing(Easing::CubicBezier(x1, 0.5, 1.0, 0.5));
        assert!(same_inputs(&curve(0.0), &curve(0.0)));
        assert!(!same_inputs(&curve(0.0), &curve(-0.0)));
        assert!(!same_inputs(&curve(-0.0), &curve(0.0)));
        assert!(!same_inputs(&curve(f32::NAN), &curve(f32::NAN)));
    }
}
