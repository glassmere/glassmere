use super::*;
use crate::animation::{Shown, lerp_rect};
use crate::{Effect, Transform, Transition};
use glassmere_scene::Affine;

/// One interpolation between two displayed states.
#[derive(Clone, Copy, Debug)]
struct Track {
    from: Shown,
    to: Shown,
    start: Duration,
    transition: Transition,
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
        for id in std::mem::take(&mut self.animating_nodes) {
            self.invalidate_paint(id);
        }
        if !self.ghosts.is_empty() {
            self.pending |= Dirty::PAINT;
            self.damage_scene();
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
            let (progress, finished) = track.transition.progress(now.saturating_sub(track.start));
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
