//! Touch and pen contacts. Every contact is a pointer of its own: it has its
//! own position, hover, capture and drags, which are swapped into the
//! context's pointer fields while its events run. Widgets therefore need no
//! touch code, and two sliders follow two fingers.
//!
//! What differs from a mouse: a press that moves pans the scroll view under
//! it and keeps going after release; a press that rests opens the context
//! menu, or picks up a drag source; there is no hover once the finger lifts.
use super::*;
use std::collections::VecDeque;

/// Movement that turns a press into a pan, in logical pixels.
const SLOP: f32 = 10.0;
const LONG_PRESS: Duration = Duration::from_millis(500);
/// Pixels per second below which a release does not fling and a fling ends.
const FLING_MINIMUM: f32 = 50.0;
/// Seconds in which a fling loses all but 1/e of its speed.
const FLING_DECAY: f32 = 0.325;
/// Positions older than this do not count toward the speed at release.
const VELOCITY_WINDOW: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    /// The system took the contact away, such as for a gesture of its own.
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Down and still within the slop: a tap, a long press or a pan to be.
    Pressing,
    /// A control follows the finger: a slider, a divider, a dock tab, a drag.
    Dragging,
    Panning,
    /// Spent: the long press fired, or another pointer holds the control.
    Held,
}

pub(super) struct Touch {
    // The pointer state widgets read, swapped in while this contact's events run.
    pointer: Option<Point>,
    hovered: Option<NodeId>,
    captured: Option<NodeId>,
    drag: Option<Box<crate::drag::DragState>>,
    dock_drag: Option<Box<dock_input::DockDrag>>,
    dock_grab: Option<dock_input::DockGrab>,

    mode: Mode,
    start: Point,
    last: Point,
    since: Duration,
    /// Recent positions, for the speed at release.
    samples: VecDeque<(Duration, Point)>,
    force: Option<f32>,
}

pub(super) struct Fling {
    /// Where the finger lifted; the scroll view under it keeps moving.
    at: Point,
    velocity: Point,
    last: Duration,
}

impl Context {
    /// Exchanges the context's pointer state with a contact's. Called in
    /// pairs around whatever should see the contact as the pointer.
    fn swap_touch(&mut self, touch: &mut Touch) {
        std::mem::swap(&mut self.pointer, &mut touch.pointer);
        std::mem::swap(&mut self.hovered, &mut touch.hovered);
        std::mem::swap(&mut self.captured, &mut touch.captured);
        std::mem::swap(&mut self.drag, &mut touch.drag);
        std::mem::swap(&mut self.dock_drag, &mut touch.dock_drag);
        std::mem::swap(&mut self.dock_grab, &mut touch.dock_grab);
    }

    /// Runs `events` with the contact as the pointer.
    fn as_touch(&mut self, touch: &mut Touch, events: impl FnOnce(&mut Self) -> bool) -> bool {
        // A declaration may have removed what the contact held since its last event.
        let nodes = &self.nodes;
        let alive = |id: &NodeId| nodes.get(id).is_some_and(Node::takes_pointer);
        touch.hovered = touch.hovered.filter(alive);
        touch.captured = touch.captured.filter(alive);
        if touch
            .drag
            .as_ref()
            .is_some_and(|drag| !nodes.contains_key(&drag.source))
        {
            touch.drag = None;
        }
        self.swap_touch(touch);
        let handled = events(self);
        self.swap_touch(touch);
        handled
    }

    /// Lets go without activating anything. Runs as the contact.
    fn release_press(&mut self) -> bool {
        let held = [self.captured.take(), self.hovered.take()];
        for id in held.into_iter().flatten() {
            self.invalidate_paint(id);
        }
        self.dock_grab = None;
        self.dock_drag = None;
        self.cancel_drag() | held.iter().any(Option::is_some)
    }

    /// Whether some contact presses `id`, for its pressed appearance.
    pub(super) fn touch_pressed(&self, id: NodeId) -> bool {
        self.touches
            .values()
            .any(|touch| touch.captured == Some(id) && touch.hovered == Some(id))
    }

    pub(super) fn touch_wake(&self) -> Option<Duration> {
        self.touches
            .values()
            .filter(|touch| touch.mode == Mode::Pressing)
            .map(|touch| touch.since + LONG_PRESS)
            .min()
    }

    pub(super) fn flinging(&self) -> bool {
        self.fling.is_some()
    }

    pub(super) fn stop_touches(&mut self) {
        self.fling = None;
        for (_, mut touch) in std::mem::take(&mut self.touches) {
            self.as_touch(&mut touch, Self::release_press);
        }
    }

    pub(super) fn touch_event(
        &mut self,
        id: u64,
        phase: TouchPhase,
        position: Point,
        force: Option<f32>,
    ) -> bool {
        let handled = match phase {
            TouchPhase::Started => self.touch_started(id, position, force),
            TouchPhase::Moved => self.touch_moved(id, position, force),
            TouchPhase::Ended => self.touch_ended(id, position),
            TouchPhase::Cancelled => self.touches.remove(&id).is_some_and(|mut touch| {
                self.as_touch(&mut touch, Self::release_press);
                true
            }),
        };
        if handled {
            self.pending |= Dirty::PAINT;
        }
        handled
    }

    fn touch_started(&mut self, id: u64, position: Point, force: Option<f32>) -> bool {
        self.fling = None;
        // A contact the host never ended must not leave its press behind.
        if let Some(mut stale) = self.touches.remove(&id) {
            self.as_touch(&mut stale, Self::release_press);
        }
        let mut touch = Touch {
            pointer: None,
            hovered: None,
            captured: None,
            drag: None,
            dock_drag: None,
            dock_grab: None,
            mode: Mode::Pressing,
            start: position,
            last: position,
            since: self.now,
            samples: VecDeque::from([(self.now, position)]),
            force,
        };
        // One pointer per control: a second finger on a slider that is being
        // dragged would fight the first. Decided before the press, which
        // already moves a slider.
        let target = self.hit_test(position);
        let taken = target.is_some()
            && (self.captured == target
                || self.touches.values().any(|other| other.captured == target));
        if taken {
            touch.mode = Mode::Held;
            self.touches.insert(id, touch);
            return false;
        }
        let mut follows = false;
        self.as_touch(&mut touch, |context| {
            context.handle_event(Event::PointerMoved(position));
            context.handle_event(Event::PointerDown);
            follows = context.dock_grab.is_some()
                || context.captured.is_some_and(|id| {
                    matches!(
                        context.nodes[&id].kind,
                        Kind::Slider | Kind::Divider | Kind::DockHandle
                    )
                });
            true
        });
        if follows {
            touch.mode = Mode::Dragging;
        }
        self.touches.insert(id, touch);
        true
    }

    fn touch_moved(&mut self, id: u64, position: Point, force: Option<f32>) -> bool {
        let Some(mut touch) = self.touches.remove(&id) else {
            return false;
        };
        touch.force = force;
        touch.samples.push_back((self.now, position));
        while touch
            .samples
            .front()
            .is_some_and(|(at, _)| self.now.saturating_sub(*at) > VELOCITY_WINDOW)
            && touch.samples.len() > 2
        {
            touch.samples.pop_front();
        }
        let (dx, dy) = (position.x - touch.start.x, position.y - touch.start.y);
        if touch.mode == Mode::Pressing && dx * dx + dy * dy > SLOP * SLOP {
            // The finger travels, so this was never a tap: whatever it
            // pressed is let go, and from here on it moves the content.
            self.as_touch(&mut touch, Self::release_press);
            touch.mode = Mode::Panning;
            touch.last = touch.start;
        }
        let handled = match touch.mode {
            Mode::Pressing | Mode::Held => false,
            Mode::Dragging => self.as_touch(&mut touch, |context| {
                context.handle_event(Event::PointerMoved(position))
            }),
            Mode::Panning => {
                let (x, y) = (touch.last.x - position.x, touch.last.y - position.y);
                touch.last = position;
                self.as_touch(&mut touch, |context| {
                    context.pointer = Some(position);
                    let vertical = y != 0.0 && context.scroll_at_pointer(y);
                    let horizontal = x != 0.0 && context.scroll_x_at_pointer(x);
                    vertical || horizontal
                })
            }
        };
        self.touches.insert(id, touch);
        handled
    }

    fn touch_ended(&mut self, id: u64, position: Point) -> bool {
        let Some(mut touch) = self.touches.remove(&id) else {
            return false;
        };
        match touch.mode {
            Mode::Pressing | Mode::Dragging => {
                let dragging = touch.mode == Mode::Dragging;
                self.as_touch(&mut touch, |context| {
                    if dragging {
                        context.handle_event(Event::PointerMoved(position));
                    }
                    context.handle_event(Event::PointerUp);
                    true
                });
            }
            Mode::Panning if !self.reduced_motion => {
                let (first, latest) = (touch.samples.front(), touch.samples.back());
                if let (Some((from, start)), Some((to, end))) = (first, latest) {
                    let seconds = to.saturating_sub(*from).as_secs_f32();
                    // The content moves against the finger.
                    let velocity = Point {
                        x: (start.x - end.x) / seconds.max(0.001),
                        y: (start.y - end.y) / seconds.max(0.001),
                    };
                    if seconds > 0.0 && velocity.x.hypot(velocity.y) >= FLING_MINIMUM {
                        self.fling = Some(Fling {
                            at: position,
                            velocity,
                            last: self.now,
                        });
                    }
                }
            }
            Mode::Panning | Mode::Held => {}
        }
        // No hover is left behind where the finger was.
        self.as_touch(&mut touch, Self::release_press);
        true
    }

    /// The part of touch that happens by the clock: long presses and flings.
    pub(super) fn update_touches(&mut self) {
        let due: Vec<u64> = self
            .touches
            .iter()
            .filter(|(_, touch)| {
                touch.mode == Mode::Pressing && self.now >= touch.since + LONG_PRESS
            })
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            let mut touch = self.touches.remove(&id).unwrap();
            let at = touch.start;
            touch.mode = Mode::Held;
            self.as_touch(&mut touch, |context| {
                // Resting on something draggable picks it up; anywhere else
                // it asks for the context menu, as a right click would.
                if let Some(drag) = &mut context.drag {
                    drag.active = true;
                    let source = drag.source;
                    context.captured = None;
                    context.invalidate_paint(source);
                    context.pending |= Dirty::ACCESSIBILITY;
                    return true;
                }
                context.release_press();
                context.handle_event(Event::ContextMenu(at))
            });
            if touch.drag.is_some() {
                touch.mode = Mode::Dragging;
            }
            self.touches.insert(id, touch);
            self.pending |= Dirty::PAINT;
        }

        let Some(mut fling) = self.fling.take() else {
            return;
        };
        let seconds = self.now.saturating_sub(fling.last).as_secs_f32();
        if seconds <= 0.0 {
            self.fling = Some(fling);
            return;
        }
        fling.last = self.now;
        // Speed decays exponentially; this is the distance covered meanwhile.
        let decay = (-seconds / FLING_DECAY).exp();
        let travel = FLING_DECAY * (1.0 - decay);
        let (x, y) = (fling.velocity.x * travel, fling.velocity.y * travel);
        fling.velocity.x *= decay;
        fling.velocity.y *= decay;
        // Scrolling goes by the pointer, so the mouse's place is borrowed.
        let mouse = self.pointer.replace(fling.at);
        let vertical = y != 0.0 && self.scroll_at_pointer(y);
        let horizontal = x != 0.0 && self.scroll_x_at_pointer(x);
        self.pointer = mouse;
        self.hovered = mouse.and_then(|position| self.hit_test(position));
        // Against the end of the content there is nothing left to do.
        if (vertical || horizontal) && fling.velocity.x.hypot(fling.velocity.y) >= FLING_MINIMUM {
            self.fling = Some(fling);
        }
    }

    /// Contacts that are down: their id, position and, where the device
    /// reports it, pressure from 0 to 1.
    pub fn touches(&self) -> impl Iterator<Item = (u64, Point, Option<f32>)> + '_ {
        self.touches
            .iter()
            .map(|(id, touch)| (*id, touch.last_seen(), touch.force))
    }
}

impl Touch {
    fn last_seen(&self) -> Point {
        self.samples.back().map_or(self.start, |(_, point)| *point)
    }
}
