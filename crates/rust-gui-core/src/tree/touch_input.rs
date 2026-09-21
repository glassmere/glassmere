//! Touch and pen contacts. Every contact is a pointer of its own: it has its
//! own position, hover, capture and drags, which are swapped into the
//! context's pointer fields while its events run. Widgets therefore need no
//! touch code, and two sliders follow two fingers.
//!
//! A contact that lands on a `Ui::gesture_area` presses nothing: it moves,
//! scales and turns whatever the app shows there.
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
    /// One of a gesture area's fingers.
    Gesture(NodeId),
}

/// What happened in a `Ui::gesture_area` since the last declaration. One
/// finger pans; two also scale and turn; a pinch on a trackpad and Ctrl with
/// the wheel scale about the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureResponse {
    /// Fingers are down in the area.
    pub active: bool,
    pub pan_x: f32,
    pub pan_y: f32,
    /// A factor: 1 is no change, 2 is twice as large.
    pub scale: f32,
    /// Clockwise radians.
    pub rotation: f32,
    /// Where the change is centred, from the area's top left corner: between
    /// the fingers, or under the pointer.
    pub center: Option<Point>,
    /// A tap that was neither a pan nor a long press.
    pub tapped: Option<Point>,
    /// The firmest contact in the area, 0 to 1, where the device reports it.
    pub pressure: Option<f32>,
}

impl Default for GestureResponse {
    fn default() -> Self {
        Self {
            active: false,
            pan_x: 0.0,
            pan_y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            center: None,
            tapped: None,
            pressure: None,
        }
    }
}

impl GestureResponse {
    pub fn changed(&self) -> bool {
        self.pan_x != 0.0 || self.pan_y != 0.0 || self.scale != 1.0 || self.rotation != 0.0
    }
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
    /// Left the slop, so neither a tap nor a long press any more.
    travelled: bool,
    /// The long press had its chance.
    rested: bool,
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
            .filter(|touch| touch.awaits_long_press())
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
            travelled: false,
            rested: false,
        };
        let target = self.hit_test(position);
        if let Some(area) = self.gesture_area_at(position, target) {
            touch.mode = Mode::Gesture(area);
            self.touches.insert(id, touch);
            return true;
        }
        // One pointer per control: a second finger on a slider that is being
        // dragged would fight the first. Decided before the press, which
        // already moves a slider.
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
        touch.travelled |= dx * dx + dy * dy > SLOP * SLOP;
        if touch.mode == Mode::Pressing && touch.travelled {
            // The finger travels, so this was never a tap: whatever it
            // pressed is let go, and from here on it moves the content.
            self.as_touch(&mut touch, Self::release_press);
            touch.mode = Mode::Panning;
            touch.last = touch.start;
        }
        let handled = match touch.mode {
            Mode::Pressing | Mode::Held => false,
            Mode::Gesture(area) => {
                // A lone finger waits out the slop, then catches up; with a
                // second finger down nothing is a tap any more.
                let partner = self.gesture_partner(area);
                if touch.travelled || partner.is_some() {
                    let from = std::mem::replace(&mut touch.last, position);
                    self.gesture_moved(area, from, position, partner)
                } else {
                    false
                }
            }
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
            Mode::Gesture(area) => {
                if !touch.travelled && self.gesture_partner(area).is_none() {
                    self.gesture(area, position, |gesture, local| {
                        gesture.tapped = Some(local)
                    });
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
            .filter(|(_, touch)| touch.awaits_long_press() && self.now >= touch.since + LONG_PRESS)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            let mut touch = self.touches.remove(&id).unwrap();
            let at = touch.start;
            touch.rested = true;
            if let Mode::Gesture(area) = touch.mode {
                // A canvas keeps its finger unless a menu really opened:
                // resting before a pan is how people aim.
                let alone = self.gesture_partner(area).is_none();
                if alone
                    && self.as_touch(&mut touch, |context| {
                        context.handle_event(Event::ContextMenu(at))
                    })
                {
                    touch.mode = Mode::Held;
                    self.pending |= Dirty::PAINT;
                }
                self.touches.insert(id, touch);
                continue;
            }
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

impl Context {
    /// The gesture area a new contact at `position` belongs to. A control
    /// inside the area is pressed as anywhere else, and whatever lies over
    /// the area (a popup, a dialog) keeps the contact from it.
    fn gesture_area_at(&self, position: Point, target: Option<NodeId>) -> Option<NodeId> {
        let mut order = Vec::new();
        self.order(self.root, &mut order);
        let area = order.into_iter().rev().find(|id| {
            let node = &self.nodes[id];
            node.kind == Kind::GestureArea && node.enabled && node.clip.contains(position)
        })?;
        let Some(target) = target else {
            return Some(area);
        };
        // Only something the area sits in may be under the finger as well.
        let mut ancestor = self.nodes[&area].parent;
        while let Some(id) = ancestor {
            if id == target {
                return Some(area);
            }
            ancestor = self.nodes[&id].parent;
        }
        None
    }

    /// Another finger of the area: the one with the lowest id.
    // ponytail: a third finger moves the picture against the first only.
    // Upgrade path: a least-squares fit over all contacts.
    fn gesture_partner(&self, area: NodeId) -> Option<Point> {
        self.touches
            .iter()
            .filter(|(_, touch)| touch.mode == Mode::Gesture(area))
            .min_by_key(|(id, _)| **id)
            .map(|(_, touch)| touch.last)
    }

    /// Adds to what the area reports at its next declaration.
    fn gesture(
        &mut self,
        area: NodeId,
        center: Point,
        change: impl FnOnce(&mut GestureResponse, Point),
    ) -> bool {
        let Some(node) = self.nodes.get(&area) else {
            return false;
        };
        let local = Point {
            x: center.x - node.bounds.x,
            y: center.y - node.bounds.y,
        };
        let update = self
            .updates
            .entry(area)
            .or_insert_with(|| ControlUpdate::Gesture(GestureResponse::default()));
        if let ControlUpdate::Gesture(gesture) = update {
            gesture.center = Some(local);
            change(gesture, local);
        }
        true
    }

    fn gesture_moved(
        &mut self,
        area: NodeId,
        from: Point,
        to: Point,
        partner: Option<Point>,
    ) -> bool {
        let Some(partner) = partner else {
            return self.gesture(area, to, |gesture, _| {
                gesture.pan_x += to.x - from.x;
                gesture.pan_y += to.y - from.y;
            });
        };
        let middle = Point {
            x: (to.x + partner.x) / 2.0,
            y: (to.y + partner.y) / 2.0,
        };
        let (before, after) = (
            (from.x - partner.x, from.y - partner.y),
            (to.x - partner.x, to.y - partner.y),
        );
        let (length_before, length_after) = (before.0.hypot(before.1), after.0.hypot(after.1));
        self.gesture(area, middle, |gesture, _| {
            // The point between the fingers moves half as far as one of them.
            gesture.pan_x += (to.x - from.x) / 2.0;
            gesture.pan_y += (to.y - from.y) / 2.0;
            // Fingers on top of each other have no direction to speak of.
            if length_before > 1.0 && length_after > 1.0 {
                gesture.scale *= length_after / length_before;
                let turn = after.1.atan2(after.0) - before.1.atan2(before.0);
                gesture.rotation += (turn + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
            }
        })
    }

    /// A pinch or a turn from a trackpad, or Ctrl with the wheel: applied to
    /// the gesture area under the mouse pointer, if there is one.
    pub(super) fn pointer_gesture(&mut self, factor: f32, radians: f32) -> bool {
        let Some(position) = self.pointer else {
            return false;
        };
        let Some(area) = self.gesture_area_at(position, self.hit_test(position)) else {
            return false;
        };
        if !factor.is_finite() || factor <= 0.0 || !radians.is_finite() {
            return false;
        }
        self.pending |= Dirty::PAINT;
        self.gesture(area, position, |gesture, _| {
            gesture.scale *= factor;
            gesture.rotation += radians;
        })
    }

    /// Whether fingers are down in the area, and the firmest of them.
    pub(crate) fn gesture_contacts(&self, area: NodeId) -> (bool, Option<f32>) {
        let mut contacts = self
            .touches
            .values()
            .filter(|touch| touch.mode == Mode::Gesture(area))
            .peekable();
        (
            contacts.peek().is_some(),
            contacts.filter_map(|touch| touch.force).reduce(f32::max),
        )
    }
}

impl Touch {
    fn awaits_long_press(&self) -> bool {
        !self.rested && !self.travelled && matches!(self.mode, Mode::Pressing | Mode::Gesture(_))
    }

    fn last_seen(&self) -> Point {
        self.samples.back().map_or(self.start, |(_, point)| *point)
    }
}
