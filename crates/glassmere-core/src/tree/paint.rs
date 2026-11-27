//! Bounds, damage and the paint chunks the scene is assembled from.

use super::*;
use crate::FocusRing;
use alloc::borrow::ToOwned;
use alloc::vec;
use alloc::vec::Vec;

/// A frame names no more rectangles than this, so that a renderer does not
/// draw an unbounded number of scissored passes.
const DAMAGE_REGIONS: usize = 8;
/// Rectangles this close, in logical pixels, are one: a second pass over a
/// thin gap costs more than painting the gap.
const DAMAGE_GAP: f32 = 8.0;
/// How far a submenu's panel laps the row it opens beside.
const LAP: f32 = 2.0;
/// How many primitives the chunk of a node has room for from the start. Most
/// nodes draw fewer, the pops that close them included. A vector that starts
/// empty has room for four, and its fifth primitive moves it to a new
/// allocation.
const CHUNK_PRIMITIVES: usize = 8;

/// The row a submenu's panel opens beside, and the menu that row is in.
#[derive(Clone, Copy)]
struct Opener {
    row: Rect,
    /// The row's own padding, inside which a panel may lie.
    padding: f32,
    menu: Rect,
    rtl: bool,
}

impl Opener {
    /// Where a panel of `size` goes inside `viewport`: at the end side of the
    /// row, or at the other side when the window ends first, `level` with the
    /// row. With room at neither side it keeps inside the window over the
    /// least of the menu, and clear of the row's content, below the row or
    /// else above it, when it can.
    fn place(self, size: Size, level: f32, viewport: Rect) -> Point {
        let Self {
            row,
            padding,
            menu,
            rtl,
        } = self;
        let after = row.x + row.width - LAP;
        let before = row.x - size.width + LAP;
        let fits = |x: f32| x >= 0.0 && x + size.width <= viewport.width;
        let (first, second) = if rtl {
            (before, after)
        } else {
            (after, before)
        };
        if let Some(x) = [first, second].into_iter().find(|x| fits(*x)) {
            return Point { x, y: level };
        }
        // How far a panel at `x` reaches across `over`.
        let across = |x: f32, over: Rect| (x + size.width).min(over.x + over.width) - x.max(over.x);
        let ends = [0.0, viewport.width - size.width];
        let x = if rtl { ends } else { [ends[1], ends[0]] }
            .into_iter()
            .min_by(|a, b| across(*a, menu).total_cmp(&across(*b, menu)))
            .unwrap_or(0.0);
        let content = Rect {
            x: row.x + padding,
            width: (row.width - 2.0 * padding).max(0.0),
            ..row
        };
        let below = row.y + row.height;
        let above = row.y - size.height;
        let y = if across(x, content) <= 0.0 {
            level
        } else if below + size.height <= viewport.height {
            below
        } else if above >= 0.0 {
            above
        } else {
            level
        };
        Point { x, y }
    }
}

impl Context {
    pub(super) fn refresh_bounds(&mut self, size: Size) {
        self.has_unclipped = false;
        self.update_bounds(
            self.root,
            Point::default(),
            Rect {
                x: 0.0,
                y: 0.0,
                width: size.width,
                height: size.height,
            },
        );
        self.geometry_dirty = false;
        self.clips_valid = true;
        self.pending |= Dirty::ACCESSIBILITY;
    }

    pub(super) fn update_bounds(&mut self, id: NodeId, origin: Point, clip: Rect) {
        #[cfg(test)]
        if self.reference_walk {
            return self.update_bounds_reference(id, origin, clip);
        }
        let node = &self.nodes[&id];
        let hidden =
            node.style.hidden || node.parent.is_some_and(|parent| self.nodes[&parent].hidden);
        let unclipped = node.unclipped();
        self.has_unclipped |= unclipped;
        self.hidden_changed |= node.hidden != hidden;
        let old_bounds = node.bounds;
        let old_clip = node.clip;
        let old_scroll = node.scroll;
        // The kinds that scroll down, and those that scroll or overflow across:
        // the only nodes that go by how far their children reach.
        let scrolls_down = matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection);
        let scrolls_across = matches!(node.kind, Kind::Scroll | Kind::Collection | Kind::TabList);
        // The window a popup or popover keeps inside of. It is read when the
        // node is reached, after the walk has updated the root.
        let viewport = if matches!(node.kind, Kind::Popup | Kind::Popover) {
            self.nodes[&self.root].bounds
        } else {
            Rect::default()
        };
        // How far the children reach past the node's own box, per side.
        let (content_left, content_right, content_bottom) = if scrolls_down || scrolls_across {
            node.children
                .iter()
                .filter(|child| {
                    !matches!(self.nodes[*child].kind, Kind::Popup | Kind::Popover)
                        && !matches!(self.nodes[*child].control, Control::StripScroll { .. })
                })
                .map(|child| *self.layout.layout(self.nodes[child].layout_id).unwrap())
                .fold(
                    (0.0_f32, 0.0_f32, 0.0_f32),
                    |(left, right, bottom), layout| {
                        (
                            left.min(layout.location.x),
                            right.max(layout.location.x + layout.size.width),
                            bottom.max(layout.location.y + layout.size.height),
                        )
                    },
                )
        } else {
            (0.0, 0.0, 0.0)
        };
        // The row a submenu's panel opens beside.
        let beside = node
            .parent
            .filter(|_| node.kind == Kind::Popup)
            .map(|parent| &self.nodes[&parent])
            .filter(|parent| matches!(parent.control, Control::Submenu { .. }))
            .and_then(|row| {
                Some(Opener {
                    row: row.bounds,
                    padding: row.style.inset(),
                    menu: self.nodes[&row.parent?].bounds,
                    rtl: row.resolved.rtl(),
                })
            });
        let node = self.nodes.get_mut(&id).unwrap();
        let layout = self.layout.layout(node.layout_id).unwrap();
        node.bounds = Rect {
            x: origin.x + layout.location.x,
            y: origin.y + layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        };
        let layout_rect = Rect {
            x: layout.location.x,
            y: layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        };
        let old_layout_rect = core::mem::replace(&mut node.layout_rect, layout_rect);
        let clip = if matches!(node.kind, Kind::Popup | Kind::Popover) {
            // Context menus anchor at the request point instead of below the owner.
            let anchored = (node.kind == Kind::Popup)
                .then(|| {
                    self.popup_parents
                        .iter()
                        .copied()
                        .chain(self.popup)
                        .find(|popup| Some(popup.owner) == node.parent)
                        .and_then(|popup| popup.anchor)
                })
                .flatten();
            if let Some(anchor) = anchored {
                node.bounds.x = if node.resolved.rtl() {
                    anchor.x - node.bounds.width
                } else {
                    anchor.x
                };
                node.bounds.y = anchor.y;
            }
            node.bounds.width = node.bounds.width.min(viewport.width);
            node.bounds.height = node.bounds.height.min(viewport.height);
            if let Some(opener) = beside {
                let level = (opener.row.y - node.style.inset())
                    .min(viewport.height - node.bounds.height)
                    .max(0.0);
                let size = Size {
                    width: node.bounds.width,
                    height: node.bounds.height,
                };
                let at = opener.place(size, level, viewport);
                node.bounds.x = at.x;
                node.bounds.y = at.y;
            }
            node.bounds.x = node
                .bounds
                .x
                .clamp(0.0, (viewport.width - node.bounds.width).max(0.0));
            if beside.is_none() && node.bounds.y + node.bounds.height > viewport.height {
                let above = anchored.map_or(origin.y, |anchor| anchor.y);
                node.bounds.y = (above - node.bounds.height).max(0.0);
            }
            viewport
        } else {
            clip
        };
        // Hit tests skip what a clip rules out (`last_at`). Whatever else sets
        // a clip must keep that true of every node but a popup or popover: a
        // non-empty clip of a child starts no further left or up than its
        // parent's, and a clip is empty where its parent's is. The one empty
        // clip that does not start inside its parent's is that of a hidden
        // strip scroll button.
        node.clip = clip.intersect(node.bounds);
        node.hidden = hidden;
        if hidden {
            node.clip.width = 0.0;
            node.clip.height = 0.0;
        }
        if scrolls_down {
            node.scroll.maximum =
                (content_bottom + node.style.inset() - node.bounds.height).max(0.0);
            node.scroll.offset = node.scroll.offset.clamp(0.0, node.scroll.maximum);
        }
        let rtl = node.resolved.rtl();
        if scrolls_across {
            // Content overflows at the end side: past the right edge, or past
            // the left edge in a right-to-left container.
            let padding = node.style.inset();
            let overflow = if rtl {
                padding - content_left
            } else {
                content_right + padding - node.bounds.width
            };
            // A fraction of a pixel from rounding is not worth a scrollbar.
            node.scroll.maximum_x = if overflow >= 0.5 { overflow } else { 0.0 };
            node.scroll.offset_x = node.scroll.offset_x.clamp(0.0, node.scroll.maximum_x);
        }
        let origin = Point {
            x: node.bounds.x
                + if rtl {
                    node.scroll.offset_x
                } else {
                    -node.scroll.offset_x
                },
            y: node.bounds.y - node.scroll.offset,
        };
        // Children are cut by this box unless it is unclipped, when they
        // are cut by what cuts it.
        let clip = if unclipped { clip } else { node.clip };
        // Placing a child needs the tree mutably, so the children are copied,
        // into a list from an earlier frame when there is one. A node with no
        // children has nothing to copy and takes none.
        let mut children = if node.children.is_empty() {
            Vec::new()
        } else {
            self.id_spare.pop().unwrap_or_default()
        };
        children.extend_from_slice(&node.children);
        // A tab strip that scrolls keeps its panel menu at the end edge, and
        // the tabs stop where the menu starts.
        let bounds = node.bounds;
        let moved = old_bounds != bounds;
        let changed = moved || old_clip != node.clip || old_scroll != node.scroll;
        let (offset_x, maximum_x) = (node.scroll.offset_x, node.scroll.maximum_x);
        let strip = node.kind == Kind::TabList;
        let pinned = (node.kind == Kind::TabList && node.scroll.maximum_x > 0.0)
            .then(|| {
                node.children_by_key
                    .get(&WidgetKey::named(STRIP_MENU))
                    .copied()
            })
            .flatten()
            .map(|menu| {
                let layout = self.layout.layout(self.nodes[&menu].layout_id).unwrap();
                (menu, layout.location.x, layout.size.width)
            });
        // What a scrolling strip shows of its tabs: its box, less the menu and
        // less the button over each side with tabs hidden past it, so nothing
        // of a tab is drawn under a button.
        let tabs = (strip && maximum_x > 0.0).then(|| {
            let menu = pinned.map_or(0.0, |(_, _, width)| width);
            let (start, end) = (offset_x >= 0.5, maximum_x - offset_x >= 0.5);
            // The start side is on the right in a right-to-left strip.
            let (left_button, right_button) = if rtl { (end, start) } else { (start, end) };
            let left = bounds.x
                + if rtl { menu } else { 0.0 }
                + if left_button { STRIP_BUTTON } else { 0.0 };
            let right = bounds.x + bounds.width
                - if rtl { 0.0 } else { menu }
                - if right_button { STRIP_BUTTON } else { 0.0 };
            Rect {
                x: left,
                width: (right - left).max(0.0),
                ..bounds
            }
        });
        if moved {
            self.diagnostic.bounds_changed = true;
        }
        if changed {
            self.add_damage(old_clip);
            self.invalidate_paint(id);
        }
        self.note_layout_change(id, old_layout_rect, layout_rect);
        for &child in &children {
            // The scroll buttons sit over the edges of the part that shows
            // tabs, and only on a side with tabs hidden past it.
            if let Control::StripScroll { end } = self.nodes[&child].control {
                let hidden = if end { maximum_x - offset_x } else { offset_x };
                let menu = pinned.map_or(0.0, |(_, _, width)| width);
                let (left, right) = if rtl {
                    (bounds.x + menu, bounds.x + bounds.width)
                } else {
                    (bounds.x, bounds.x + bounds.width - menu)
                };
                // The start side is on the right in a right-to-left strip.
                let x = if end == rtl {
                    left
                } else {
                    right - STRIP_BUTTON
                };
                let layout_x = self
                    .layout
                    .layout(self.nodes[&child].layout_id)
                    .unwrap()
                    .location
                    .x;
                let origin = Point {
                    x: x - layout_x,
                    y: origin.y,
                };
                let shown = if hidden >= 0.5 {
                    clip
                } else {
                    Rect {
                        width: 0.0,
                        height: 0.0,
                        ..bounds
                    }
                };
                self.update_bounds(child, origin, shown);
                continue;
            }
            match (pinned, tabs) {
                (Some((menu, x, width)), _) if menu == child => {
                    let edge = if rtl {
                        bounds.x
                    } else {
                        bounds.x + bounds.width - width
                    };
                    let origin = Point {
                        x: edge - x,
                        y: origin.y,
                    };
                    self.update_bounds(child, origin, clip);
                }
                (_, Some(tabs)) => self.update_bounds(child, origin, clip.intersect(tabs)),
                _ => self.update_bounds(child, origin, clip),
            }
        }
        self.give_ids(children);
    }

    /// An overlay painted into the scene rather than declared as a node:
    /// its rectangle is damaged now and again on the frame it goes.
    pub(super) fn painted_overlay(&mut self, rect: Rect) {
        self.add_damage(rect);
        self.painted_overlays.push(rect);
    }

    /// Records `rect` as changed. Rectangles that overlap or nearly touch
    /// become one, and no more than `DAMAGE_REGIONS` are kept: past that the
    /// pair that costs the least extra area is joined.
    pub(super) fn add_damage(&mut self, mut rect: Rect) {
        if !(rect.width > 0.0 && rect.height > 0.0) {
            return;
        }
        let regions = &mut self.pending_damage;
        let near = |a: Rect, b: Rect| {
            a.x - DAMAGE_GAP <= b.x + b.width
                && b.x - DAMAGE_GAP <= a.x + a.width
                && a.y - DAMAGE_GAP <= b.y + b.height
                && b.y - DAMAGE_GAP <= a.y + a.height
        };
        while let Some(at) = regions.iter().position(|region| near(*region, rect)) {
            rect = rect.union(regions.swap_remove(at));
        }
        regions.push(rect);
        let area = |rect: Rect| rect.width * rect.height;
        while regions.len() > DAMAGE_REGIONS {
            let waste = |a: usize, b: usize| {
                area(regions[a].union(regions[b])) - area(regions[a]) - area(regions[b])
            };
            let (a, b) = (0..regions.len())
                .flat_map(|a| (a + 1..regions.len()).map(move |b| (a, b)))
                .min_by(|x, y| waste(x.0, x.1).total_cmp(&waste(y.0, y.1)))
                .unwrap();
            let joined = regions[a].union(regions.swap_remove(b));
            regions[a] = joined;
        }
    }

    /// Damages everything the last scene covered.
    pub(super) fn damage_scene(&mut self) {
        self.add_damage(Rect {
            x: 0.0,
            y: 0.0,
            width: self.scene.size.width,
            height: self.scene.size.height,
        });
    }

    pub(super) fn invalidate_paint(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        self.add_damage(node.clip);
        let mut next = Some(id);
        while let Some(id) = next {
            let node = self.nodes.get_mut(&id).unwrap();
            next = node.parent;
            match node.paint_cache.take() {
                Some(chunk) => {
                    self.spare_chunks.insert(id, chunk);
                }
                None => break,
            }
        }
        self.pending |= Dirty::PAINT;
    }

    /// Background, text color and radius for the node's current interaction
    /// state: theme defaults for the widget kind, then the style's overrides.
    pub(super) fn appearance(&self, id: NodeId, node: &Node) -> crate::animation::Shown {
        let theme = &self.theme;
        let pressed = self.pressed(id);
        let hovered = self.hovered == Some(id);
        let focused = self.focused == Some(id);
        let mut background = node.style.background.or(match node.kind {
            Kind::Button
            | Kind::TextInput
            | Kind::TextArea
            | Kind::RichEditor
            | Kind::ComboBox
            | Kind::MenuButton => Some(theme.control),
            Kind::Popup | Kind::Popover => Some(theme.popup),
            Kind::Progress => Some(theme.track),
            Kind::Divider | Kind::DockHandle => Some(if hovered || pressed {
                theme.accent
            } else {
                theme.track
            }),
            Kind::Dialog => {
                matches!(node.control, Control::Dialog { modal: true, .. }).then_some(theme.scrim)
            }
            _ => None,
        });
        let mut foreground = node.resolved.foreground;
        let mut radius = node.style.radius;
        let mut opacity = node.style.opacity;
        let mut transform = node.style.transform;
        if node.kind == Kind::Button {
            if !node.enabled {
                background = Some(theme.control_disabled);
            } else if pressed {
                background = Some(theme.control_pressed);
            } else if hovered {
                background = Some(theme.control_hover);
            }
        }
        if node.kind == Kind::Link && node.style.foreground.is_none() && node.enabled {
            foreground = theme.accent;
        }
        if node.kind == Kind::Disclosure && node.enabled && node.style.background.is_none() {
            if pressed {
                background = Some(theme.control_pressed);
            } else if hovered {
                background = Some(theme.control_hover);
            }
        }
        if !node.enabled {
            foreground = theme.text_disabled;
        }
        if let Some(states) = &node.style.states {
            let mut apply = |paint: &Paint| {
                if let Some(color) = paint.background {
                    background = Some(color);
                }
                if let Some(color) = paint.foreground {
                    foreground = color;
                }
                if let Some(value) = paint.radius {
                    radius = value;
                }
                if let Some(value) = paint.opacity {
                    opacity = value;
                }
                if let Some(value) = paint.transform {
                    transform = value;
                }
            };
            if focused {
                apply(&states.focused);
            }
            if hovered && node.enabled {
                apply(&states.hover);
            }
            if pressed && node.enabled {
                apply(&states.pressed);
            }
            if !node.enabled {
                apply(&states.disabled);
            }
        }
        crate::animation::Shown {
            background,
            foreground,
            radius,
            opacity,
            transform,
            knob: if matches!(node.control, Control::Switch { on: true, .. }) {
                1.0
            } else {
                0.0
            },
        }
    }

    pub(super) fn paint(&mut self, id: NodeId) -> Rc<PaintChunk> {
        if let Some(chunk) = &self.nodes[&id].paint_cache {
            self.stats.paint_cache_hits += 1;
            return chunk.clone();
        }
        self.stats.paint_nodes += 1;
        self.nodes.get_mut(&id).unwrap().painted_frame = self.frame;
        // What the chunk of this node was before its paint was invalidated.
        // The new chunk is written over it when nothing else holds it, which
        // saves the allocations of the chunk, its primitives and its text.
        let mut spare = self.spare_chunks.remove(&id);
        // A child of a tab strip is drawn only where the strip shows tabs,
        // which stops short of a pinned panel menu.
        let node = &self.nodes[&id];
        let strip_clip = node
            .parent
            .filter(|parent| self.nodes[parent].kind == Kind::TabList)
            .map(|_| node.clip)
            .filter(|clip| *clip != node.bounds);
        // Nothing of a node shows where its clip is empty, and every node
        // clips its children to its own box, so nothing of what is inside it
        // does either: a row scrolled out of a long list is not painted. What
        // can draw outside its clip is kept: a popup, which is clipped to the
        // window, a node that is animating or has a transform, which can move
        // into view, and a shadow, which reaches past its box.
        let culled = {
            let node = &self.nodes[&id];
            (node.clip.width <= 0.0 || node.clip.height <= 0.0)
                // Clipped away by an ancestor, not empty of its own accord.
                && node.bounds.width > 0.0
                && node.bounds.height > 0.0
                && !matches!(node.kind, Kind::Popup | Kind::Popover)
                && node.anim.is_none()
                && node.style.transform == crate::animation::Transform::IDENTITY
                && node.style.shadow.is_none()
                && node.style.opacity >= 1.0
                // What is inside an unclipped node can hang over its box into view.
                && !node.unclipped()
        };
        if culled
            || node.hidden
            || strip_clip.is_some_and(|clip| clip.width <= 0.0 || clip.height <= 0.0)
        {
            let chunk = PaintChunk::new(Vec::new(), 0, Vec::new()).put_in(spare);
            self.nodes.get_mut(&id).unwrap().paint_cache = Some(chunk.clone());
            return chunk;
        }
        let target = self.appearance(id, &self.nodes[&id]);
        let animation_state::Displayed { shown, layer } = self.displayed(id, target);
        self.keep_busy(id);
        let frame = self.image_frame(id);
        let crate::animation::Shown {
            background,
            foreground,
            radius,
            ..
        } = shown;
        if layer.is_some() {
            let bounds = self.nodes[&id].bounds;
            self.diagnose_layer(id, bounds);
        }
        let node = &self.nodes[&id];
        let (mut primitives, mut text) = match spare.as_mut().and_then(Rc::get_mut) {
            Some(old) => old.take_room(),
            None => {
                // Another chunk holds it too, so it is left as it is.
                spare = None;
                (Vec::new(), None)
            }
        };
        // The room the chunk had is kept, and this is the least a node has.
        primitives.reserve_exact(CHUNK_PRIMITIVES);
        let mut scene = Scene {
            primitives,
            ..Scene::default()
        };
        if let Some(clip) = strip_clip {
            scene.primitives.push(Primitive::PushClip(clip));
        }
        if let Some((opacity, transform)) = layer {
            // What an unclipped node draws past its box belongs to the layer
            // too, and how far that reaches is not known here, so the layer
            // is as big as the window; the clips above cut it as usual.
            let bounds = if node.unclipped() {
                self.nodes[&self.root].bounds
            } else {
                node.bounds
            };
            scene.primitives.push(Primitive::PushLayer {
                bounds,
                opacity,
                transform,
            });
        }
        // A shadow is behind the node, so it is drawn before the clip that
        // bounds the node's own painting.
        paint_shadow(node, radius, false, &mut scene);
        scene.primitives.push(Primitive::PushClip(node.bounds));
        // A range slider's thumb rings itself: its box is the whole slider.
        let focused =
            self.ring_shown(id) && !matches!(node.control, Control::Slider { span: Some(_), .. });
        self.paint_fill(node, focused, background, radius, &mut scene);
        self.paint_background_image(node, &mut scene, foreground, radius);
        if node.kind == Kind::RichText {
            self.paint_rich_text(id, &mut scene, foreground);
        } else if node.kind == Kind::Image {
            self.paint_image(id, &mut scene, foreground, radius, frame);
        } else if node.kind == Kind::Choice
            && !node.enabled
            && node.text == crate::platform::SEPARATOR
        {
            // A rule from edge to edge of the menu, whatever its width.
            let inner = node.content_box();
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    y: node.bounds.y + (node.bounds.height - 1.0) / 2.0,
                    height: 1.0,
                    ..inner
                },
                color: self.theme.track,
                radius: 0.0,
            });
        } else if !node.text.is_empty() {
            self.paint_selection(id, node, &mut scene);
            self.paint_text(node, foreground, text.take(), &mut scene);
        }
        self.paint_hint(node, &mut scene);
        paint_shadow(node, radius, true, &mut scene);
        if let Some(widget) = &node.custom {
            let offset = Point {
                x: node.bounds.x,
                y: node.bounds.y,
            };
            // Room for the pops that close the node, which are pushed after
            // these. Extending alone makes the vector just big enough for
            // these, and the first pop would move all of them to one twice as
            // big.
            let pops = 1 + usize::from(layer.is_some());
            let room = widget.primitives.len() + pops;
            scene.primitives.reserve_exact(room);
            scene.primitives.extend(
                widget
                    .primitives
                    .iter()
                    .map(|primitive| primitive.translated(offset)),
            );
        }
        self.paint_control(id, shown, &mut scene);
        let node = &self.nodes[&id];
        let children = node
            .children
            .iter()
            .copied()
            .filter(|child| !matches!(self.nodes[child].kind, Kind::Popup | Kind::Popover))
            .collect::<Vec<_>>();
        // Everything from here on is drawn after the children. It is only
        // ever pushed, never read, moved or removed, so the length now is
        // where it starts.
        // An unclipped node closes its clip here, so that the children are not
        // cut by it, and opens it again for what is drawn after them.
        let unclipped = node.unclipped();
        if unclipped {
            scene.primitives.push(Primitive::PopClip);
        }
        let split = scene.primitives.len();
        if unclipped {
            scene.primitives.push(Primitive::PushClip(node.bounds));
        }
        self.paint_scrollbars(id, &mut scene);
        self.paint_strip_edges(id, &mut scene);
        self.paint_calendar_today(id, &mut scene);
        self.paint_calendar_cursor(id, &mut scene);
        scene.primitives.push(Primitive::PopClip);
        if layer.is_some() {
            scene.primitives.push(Primitive::PopLayer);
        }
        if strip_clip.is_some() {
            scene.primitives.push(Primitive::PopClip);
        }
        let mut children = children;
        if self.z_count > 0 {
            // Stable: siblings that tie keep the order they were declared in.
            children.sort_by_key(|child| self.nodes[child].style.z_index);
        }
        let children = children
            .into_iter()
            .map(|child| self.paint(child))
            .collect();
        // A node that drew a great many primitives once does not keep the
        // room for them.
        let room = scene.primitives.len().max(CHUNK_PRIMITIVES);
        if scene.primitives.capacity() > room.saturating_mul(4) {
            scene.primitives.shrink_to(room);
        }
        let chunk = PaintChunk::new(scene.primitives, split, children).put_in(spare);
        self.nodes.get_mut(&id).unwrap().paint_cache = Some(chunk.clone());
        chunk
    }

    /// The node's flat colour and gradient, with the focus ring over them
    /// when `ring` is given.
    fn paint_fill(
        &self,
        node: &Node,
        ring: bool,
        background: Option<Color>,
        radius: f32,
        scene: &mut Scene,
    ) {
        if let Some(background) = background
            && background.3 > 0
        {
            scene.primitives.push(Primitive::Quad {
                rect: node.bounds,
                color: background,
                radius,
            });
        }
        // A ramp fills the node over whatever flat colour it has.
        if let Some(gradient) = &node.style.gradient
            && gradient.stops.len() >= 2
        {
            scene.primitives.push(Primitive::Gradient {
                rect: node.bounds,
                radius,
                from: Point {
                    x: gradient.from.0,
                    y: gradient.from.1,
                },
                to: Point {
                    x: gradient.to.0,
                    y: gradient.to.1,
                },
                stops: gradient.stops.clone(),
                radial: gradient.radial,
            });
        }
        if node.style.border_sides.is_some() && node.kind.holds_nodes() {
            self.paint_border_sides(node, scene);
        } else if node.style.border > 0.0 {
            self.paint_border(node, radius, scene);
        }
        if ring {
            self.paint_focus_ring(node, radius, scene);
        }
    }

    /// Whether the focus ring of `id` is drawn: it has focus, did not get
    /// it from a press on the title bar, and the last input was the
    /// keyboard or the widget takes typed text, which shows focus whatever
    /// moved it there.
    pub(super) fn ring_shown(&self, id: NodeId) -> bool {
        self.focused == Some(id)
            && self.quiet_focus != Some(id)
            && (self.keyboard_modality || self.accepts_text())
    }

    /// The node's focus ring: a stroke along its rounded edge, inside its
    /// box and inside its border, over whatever fill the node has.
    fn paint_focus_ring(&self, node: &Node, radius: f32, scene: &mut Scene) {
        let ring = node.style.focus_ring.unwrap_or(FocusRing::new(2.0));
        let color = ring.color.unwrap_or(self.theme.focus);
        let inset = ring.inset.max(0.0) + node.style.border.max(0.0);
        stroke_inside(node.bounds, radius, ring.width, color, inset, scene);
    }

    /// The node's border: a stroke along its rounded edge, inside its box.
    fn paint_border(&self, node: &Node, radius: f32, scene: &mut Scene) {
        let color = node.style.border_color.unwrap_or(self.theme.track);
        stroke_inside(node.bounds, radius, node.style.border, color, 0.0, scene);
    }

    /// The border of a node that holds nodes, each side as wide as it says,
    /// drawn straight along the inside of the box.
    fn paint_border_sides(&self, node: &Node, scene: &mut Scene) {
        let Some(sides) = node.style.border_sides else {
            return;
        };
        let color = node.style.border_color.unwrap_or(self.theme.track);
        let Rect {
            x,
            y,
            width,
            height,
        } = node.bounds;
        let (top, bottom) = (sides.top.max(0.0), sides.bottom.max(0.0));
        let (left, right) = (sides.left.max(0.0), sides.right.max(0.0));
        let middle = (height - top - bottom).max(0.0);
        for rect in [
            Rect {
                x,
                y,
                width,
                height: top.min(height),
            },
            Rect {
                x,
                y: y + height - bottom.min(height),
                width,
                height: bottom.min(height),
            },
            Rect {
                x,
                y: y + top,
                width: left.min(width),
                height: middle,
            },
            Rect {
                x: x + width - right.min(width),
                y: y + top,
                width: right.min(width),
                height: middle,
            },
        ] {
            if rect.width > 0.0 && rect.height > 0.0 {
                scene.primitives.push(Primitive::Quad {
                    rect,
                    color,
                    radius: 0.0,
                });
            }
        }
    }

    /// The node's own text, placed in its box.
    /// What is selected of a label's text, behind the text.
    fn paint_selection(&self, id: NodeId, node: &Node, scene: &mut Scene) {
        let Some(editor) = node.editor.as_ref().filter(|_| node.read_only()) else {
            return;
        };
        let text = self.text_place(node).0;
        let color = if self.focused == Some(id) {
            self.theme.highlight
        } else {
            self.theme.selection_inactive
        };
        for rect in editor.geometry.selection(editor.selection.range()) {
            scene.primitives.push(Primitive::Quad {
                rect: rect.translated(Point {
                    x: text.x,
                    y: text.y,
                }),
                color,
                radius: 0.0,
            });
        }
    }

    /// Where the text of a node is drawn, how big the layout measured it
    /// (for the leaves that were) and how wide it is.
    pub(super) fn text_place(&self, node: &Node) -> (Rect, Option<Size>, f32) {
        let mut rect = node.content_box();
        // Where the text sits in a box wider or taller than it is. A
        // wrapped block is aligned line by line by the shaper; this
        // places the block itself, which is what centres a glyph in a
        // caption button.
        //
        // Text leaves were measured for layout; other labelled controls
        // (sliders) are measured while controls are prepared.
        let measured = node
            .kind
            .text_leaf()
            .then(|| self.layout.get_node_context(node.layout_id))
            .flatten()
            .map(|measured| measured.size);
        // What the control draws before its text takes the start edge.
        let indicator = node.kind.indicator_width();
        rect.width = (rect.width - indicator).max(0.0);
        if !node.resolved.rtl() {
            rect.x += indicator;
        }
        let width = measured.map_or(node.label_width, |size| size.width - indicator);
        if width > 0.0 && node.style.wrap == TextWrap::None {
            let spare = (rect.width - width).max(0.0);
            let rtl = node.resolved.rtl();
            rect.x += match node.resolved.align {
                // `text_align` is where the line sits when it is set; it
                // follows the reading direction, as the lines of a
                // wrapped block do.
                TextAlign::Center => spare / 2.0,
                TextAlign::End if rtl => 0.0,
                TextAlign::End => spare,
                TextAlign::Justify if rtl => spare,
                TextAlign::Justify => 0.0,
                TextAlign::Start => match node.style.justify_content {
                    crate::Justify::Center => spare / 2.0,
                    crate::Justify::End => spare,
                    // Right-to-left text without an opinion goes to the end.
                    _ if rtl => spare,
                    _ => 0.0,
                },
            };
        }
        if let Some(size) = measured.filter(|size| size.height > rect.height)
            && node.style.wrap == TextWrap::None
        {
            // A line taller than the room its padding leaves takes the
            // padding, as far as the box goes, rather than lose its
            // descenders: a cell of a dense grid is such a box.
            let height = size.height.min(node.bounds.height);
            let top = rect.y - (height - rect.height) / 2.0;
            rect.y = within_box(top, node.bounds.y, node.bounds.height, height);
            rect.height = height;
        }
        if let Some(size) = measured.filter(|size| size.height > 0.0) {
            let spare = (rect.height - size.height).max(0.0);
            rect.y += match node.style.align_items {
                crate::Align::Center => spare / 2.0,
                crate::Align::End => spare,
                _ => 0.0,
            };
        }
        (rect, measured, width)
    }

    /// The text goes in `spare`, a buffer from the chunk this one replaces,
    /// when there is one.
    fn paint_text(&self, node: &Node, foreground: Color, spare: Option<String>, scene: &mut Scene) {
        let (rect, measured, width) = self.text_place(node);
        let decoration = if node.kind == Kind::Link {
            crate::TextDecoration::Underline
        } else {
            node.style.text_decoration
        };
        if decoration != crate::TextDecoration::None
            && node.style.wrap == TextWrap::None
            && let Some(size) = measured.filter(|size| size.width > 0.0)
        {
            // Under the text, as a rich text link's rule is under its run, or
            // through the middle of it.
            let y = match decoration {
                crate::TextDecoration::LineThrough => rect.y + size.height / 2.0 - 0.5,
                _ => rect.y + size.height - 3.0,
            };
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: rect.x,
                    y,
                    width: width.min(rect.width),
                    height: 1.0,
                },
                color: foreground,
                radius: 0.0,
            });
        }
        if let Some((start, end)) = self.suggestion_match(node) {
            // The part of a suggestion that was typed stands out.
            let span = |text: &str, weight| glassmere_scene::TextSpan {
                text: text.to_owned(),
                color: None,
                weight,
                italic: None,
                family: None,
            };
            scene.primitives.push(Primitive::RichText {
                rect,
                spans: vec![
                    span(&node.text[..start], None),
                    span(&node.text[start..end], Some(700)),
                    span(&node.text[end..], None),
                ],
                style: node.text_style(),
                color: foreground,
            });
        } else if node.text_style()
            == (TextStyle {
                font_size: node.resolved.font_size,
                ..TextStyle::default()
            })
        {
            let mut text = spare.unwrap_or_default();
            refill(&mut text, node.shown_text());
            scene.primitives.push(Primitive::Text {
                rect,
                text,
                font_size: node.resolved.font_size,
                color: foreground,
            });
        } else {
            let mut text = spare.unwrap_or_default();
            refill(&mut text, node.shown_text());
            scene.primitives.push(Primitive::StyledText {
                rect,
                text,
                style: node.text_style(),
                color: foreground,
            });
        }
    }

    /// A menu item's shortcut, along the edge the text ends at.
    fn paint_hint(&self, node: &Node, scene: &mut Scene) {
        if let Control::Choice { hint, .. } | Control::Submenu { hint, .. } | Control::Hint(hint) =
            &node.control
            && !hint.is_empty()
            && node.hint_width > 0.0
        {
            let padding = node.style.inset();
            let inner = (node.bounds.width - 2.0 * padding).max(0.0);
            let offset = if node.resolved.rtl() {
                0.0
            } else {
                (inner - node.hint_width).max(0.0)
            };
            scene.primitives.push(Primitive::StyledText {
                rect: Rect {
                    x: node.bounds.x + padding + offset,
                    y: node.bounds.y + padding,
                    width: node.hint_width,
                    height: (node.bounds.height - 2.0 * padding).max(0.0),
                },
                text: hint.clone(),
                style: node.text_style(),
                color: if node.enabled {
                    self.theme.text_muted
                } else {
                    self.theme.text_disabled
                },
            });
        }
    }

    /// Thumbs for the axes a node scrolls on.
    fn paint_scrollbars(&self, id: NodeId, scene: &mut Scene) {
        let width = self.bar_width(id);
        for bar in self.bars(id, width).into_iter().flatten() {
            scene.primitives.push(Primitive::Quad {
                rect: bar.thumb,
                color: self.theme.scrollbar,
                radius: width / 2.0,
            });
        }
    }

    /// A shade over each edge of a tab strip that has tabs hidden past it,
    /// beside the button that scrolls there.
    fn paint_strip_edges(&self, id: NodeId, scene: &mut Scene) {
        let node = &self.nodes[&id];
        if node.kind != Kind::TabList {
            return;
        }
        let (left, right) = self.strip_area(id);
        let rtl = node.resolved.rtl();
        let shade = self.theme.scrim;
        let scroll = node.scroll;
        // The start side is on the right in a right-to-left strip.
        for (hidden, at_left) in [
            (scroll.offset_x, !rtl),
            (scroll.maximum_x - scroll.offset_x, rtl),
        ] {
            if hidden < 0.5 {
                continue;
            }
            let (x, from, to) = if at_left {
                (left + STRIP_BUTTON, shade, Color::TRANSPARENT)
            } else {
                (
                    right - STRIP_BUTTON - STRIP_SHADE,
                    Color::TRANSPARENT,
                    shade,
                )
            };
            scene.primitives.push(Primitive::Gradient {
                rect: Rect {
                    x,
                    y: node.bounds.y,
                    width: STRIP_SHADE,
                    height: node.bounds.height,
                },
                radius: 0.0,
                from: Point { x: 0.0, y: 0.0 },
                to: Point { x: 1.0, y: 0.0 },
                stops: vec![
                    glassmere_scene::GradientStop {
                        at: 0.0,
                        color: from,
                    },
                    glassmere_scene::GradientStop { at: 1.0, color: to },
                ],
                radial: false,
            });
        }
    }

    pub(super) fn order(&self, id: NodeId, result: &mut Vec<NodeId>) {
        result.push(id);
        for child in &self.nodes[&id].children {
            self.order(*child, result);
        }
    }

    /// Whether a search for nodes whose clip holds a point may skip the
    /// subtrees it can tell hold none. Popups and popovers start their clip
    /// again from the window, so with one in the tree a clip says nothing
    /// about what lies under it. Nor does it between a change of the
    /// links and the next `refresh_bounds`, which a frame that unwinds in
    /// between leaves open.
    pub(super) fn can_prune_at(&self) -> bool {
        self.clips_valid && self.popover_count == 0 && self.popup_count == 0 && !self.has_unclipped
    }

    /// What `order` followed by a search from the back returns: the last
    /// node of the subtree at `id`, in the order `order` lists them, whose
    /// clip holds `at` and that `wanted` accepts. Children are tried last to
    /// first, then the node.
    ///
    /// With `prune`, a subtree is left out when its root's clip is empty or
    /// starts past `at` on the left or at the top: no clip under it can hold
    /// `at` either, as a child's clip is its parent's cut down to its own
    /// box. The far edges are not tested, as a baseline shift can put a
    /// child below its parent's clip.
    pub(super) fn last_at(
        &self,
        id: NodeId,
        at: Point,
        prune: bool,
        wanted: &mut impl FnMut(NodeId, &Node) -> bool,
    ) -> Option<NodeId> {
        let node = &self.nodes[&id];
        let clip = node.clip;
        if prune && (clip.width == 0.0 || clip.height == 0.0 || at.x < clip.x || at.y < clip.y) {
            return None;
        }
        if self.z_count > 0 {
            let mut stacked = node.children.clone();
            stacked.sort_by_key(|child| self.nodes[child].style.z_index);
            for child in stacked.iter().rev() {
                if let Some(found) = self.last_at(*child, at, prune, wanted) {
                    return Some(found);
                }
            }
        } else {
            for child in node.children.iter().rev() {
                if let Some(found) = self.last_at(*child, at, prune, wanted) {
                    return Some(found);
                }
            }
        }
        (clip.contains(at) && wanted(id, node)).then_some(id)
    }
}

/// The top of a line `height` tall, moved to `top` as far as a box from `y`,
/// `extent` tall, allows. A line as tall as the box is placed at its top.
///
/// This is not `f32::clamp`: that panics when its lower bound is above its
/// upper one, and `y + extent - height` can round to one step below `y` when
/// `height` equals `extent`. Here the lower bound wins.
fn within_box(top: f32, y: f32, extent: f32, height: f32) -> f32 {
    let highest = y + extent - height;
    top.min(highest).max(y)
}

/// A drop shadow behind the node, or an inset one inside it.
/// A stroke `width` thick along the rounded edge of `bounds`, `inset` in from
/// it and inside it. Square corners are drawn as four bars, which land exactly
/// on pixels; round ones as a stroked path.
fn stroke_inside(
    bounds: Rect,
    radius: f32,
    width: f32,
    color: Color,
    inset: f32,
    scene: &mut Scene,
) {
    let Rect {
        x,
        y,
        width: box_width,
        height: box_height,
    } = bounds;
    let (inner_width, inner_height) = (box_width - 2.0 * inset, box_height - 2.0 * inset);
    if width <= 0.0 || inner_width <= 0.0 || inner_height <= 0.0 {
        return;
    }
    let stroke = width.min(inner_width / 2.0).min(inner_height / 2.0);
    let (x, y) = (x + inset, y + inset);
    if radius <= inset {
        for rect in [
            Rect {
                x,
                y,
                width: inner_width,
                height: stroke,
            },
            Rect {
                x,
                y: y + inner_height - stroke,
                width: inner_width,
                height: stroke,
            },
            Rect {
                x,
                y,
                width: stroke,
                height: inner_height,
            },
            Rect {
                x: x + inner_width - stroke,
                y,
                width: stroke,
                height: inner_height,
            },
        ] {
            scene.primitives.push(Primitive::Quad {
                rect,
                color,
                radius: 0.0,
            });
        }
        return;
    }
    // The stroke is centred on the path, so the path runs half a stroke in
    // from the edge it is to fill out to.
    let half = stroke / 2.0;
    let rect = Rect {
        x: x + half,
        y: y + half,
        width: inner_width - stroke,
        height: inner_height - stroke,
    };
    scene.primitives.push(Primitive::Path {
        path: glassmere_scene::Path::rounded_rect(rect, radius - inset - half),
        fill: None,
        stroke: Some(glassmere_scene::Stroke {
            width: stroke,
            color,
        }),
    });
}

fn paint_shadow(node: &Node, radius: f32, inset: bool, scene: &mut Scene) {
    if let Some(shadow) = &node.style.shadow
        && shadow.inset == inset
        && shadow.color.3 > 0
    {
        scene.primitives.push(Primitive::Shadow {
            rect: node.bounds,
            radius,
            color: shadow.color,
            blur: shadow.blur,
            spread: shadow.spread,
            offset: Point {
                x: shadow.offset.0,
                y: shadow.offset.1,
            },
            inset,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Action, Button, Command, Direction, Event, ItemId, Key, Label, ListModel, ListState, Menu,
        MenuEntry, MenuItem, MonospaceMeasurer, PopoverOptions, Tab, Transition, Ui,
        VirtualListOptions,
    };
    use alloc::format;
    use alloc::string::String;

    impl Context {
        /// The bounds walk written the plain way, which the walk the context
        /// runs by must give the same results as: it works out everything for
        /// every node and copies the children of each.
        pub(super) fn update_bounds_reference(&mut self, id: NodeId, origin: Point, clip: Rect) {
            let viewport = self.nodes[&self.root].bounds;
            let node = &self.nodes[&id];
            let hidden =
                node.style.hidden || node.parent.is_some_and(|parent| self.nodes[&parent].hidden);
            let unclipped = node.unclipped();
            self.has_unclipped |= unclipped;
            self.hidden_changed |= node.hidden != hidden;
            let old_bounds = node.bounds;
            let old_clip = node.clip;
            let old_scroll = node.scroll;
            // How far the children reach past the node's own box, per side.
            let (content_left, content_right, content_bottom) = node
                .children
                .iter()
                .filter(|child| {
                    !matches!(self.nodes[*child].kind, Kind::Popup | Kind::Popover)
                        && !matches!(self.nodes[*child].control, Control::StripScroll { .. })
                })
                .map(|child| *self.layout.layout(self.nodes[child].layout_id).unwrap())
                .fold(
                    (0.0_f32, 0.0_f32, 0.0_f32),
                    |(left, right, bottom), layout| {
                        (
                            left.min(layout.location.x),
                            right.max(layout.location.x + layout.size.width),
                            bottom.max(layout.location.y + layout.size.height),
                        )
                    },
                );
            // The row a submenu's panel opens beside.
            let beside = node
                .parent
                .filter(|_| node.kind == Kind::Popup)
                .map(|parent| &self.nodes[&parent])
                .filter(|parent| matches!(parent.control, Control::Submenu { .. }))
                .and_then(|row| {
                    Some(Opener {
                        row: row.bounds,
                        padding: row.style.inset(),
                        menu: self.nodes[&row.parent?].bounds,
                        rtl: row.resolved.rtl(),
                    })
                });
            let node = self.nodes.get_mut(&id).unwrap();
            let layout = self.layout.layout(node.layout_id).unwrap();
            node.bounds = Rect {
                x: origin.x + layout.location.x,
                y: origin.y + layout.location.y,
                width: layout.size.width,
                height: layout.size.height,
            };
            let layout_rect = Rect {
                x: layout.location.x,
                y: layout.location.y,
                width: layout.size.width,
                height: layout.size.height,
            };
            let old_layout_rect = core::mem::replace(&mut node.layout_rect, layout_rect);
            let clip = if matches!(node.kind, Kind::Popup | Kind::Popover) {
                // Context menus anchor at the request point instead of below the owner.
                let anchored = (node.kind == Kind::Popup)
                    .then(|| {
                        self.popup_parents
                            .iter()
                            .copied()
                            .chain(self.popup)
                            .find(|popup| Some(popup.owner) == node.parent)
                            .and_then(|popup| popup.anchor)
                    })
                    .flatten();
                if let Some(anchor) = anchored {
                    node.bounds.x = if node.resolved.rtl() {
                        anchor.x - node.bounds.width
                    } else {
                        anchor.x
                    };
                    node.bounds.y = anchor.y;
                }
                node.bounds.width = node.bounds.width.min(viewport.width);
                node.bounds.height = node.bounds.height.min(viewport.height);
                if let Some(opener) = beside {
                    let level = (opener.row.y - node.style.inset())
                        .min(viewport.height - node.bounds.height)
                        .max(0.0);
                    let size = Size {
                        width: node.bounds.width,
                        height: node.bounds.height,
                    };
                    let at = opener.place(size, level, viewport);
                    node.bounds.x = at.x;
                    node.bounds.y = at.y;
                }
                node.bounds.x = node
                    .bounds
                    .x
                    .clamp(0.0, (viewport.width - node.bounds.width).max(0.0));
                if beside.is_none() && node.bounds.y + node.bounds.height > viewport.height {
                    let above = anchored.map_or(origin.y, |anchor| anchor.y);
                    node.bounds.y = (above - node.bounds.height).max(0.0);
                }
                viewport
            } else {
                clip
            };
            // Hit tests skip what a clip rules out (`last_at`). Whatever else sets
            // a clip must keep that true of every node but a popup or popover: a
            // non-empty clip of a child starts no further left or up than its
            // parent's, and a clip is empty where its parent's is. The one empty
            // clip that does not start inside its parent's is that of a hidden
            // strip scroll button.
            node.clip = clip.intersect(node.bounds);
            node.hidden = hidden;
            if hidden {
                node.clip.width = 0.0;
                node.clip.height = 0.0;
            }
            if matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection) {
                node.scroll.maximum =
                    (content_bottom + node.style.inset() - node.bounds.height).max(0.0);
                node.scroll.offset = node.scroll.offset.clamp(0.0, node.scroll.maximum);
            }
            let rtl = node.resolved.rtl();
            if matches!(node.kind, Kind::Scroll | Kind::Collection | Kind::TabList) {
                // Content overflows at the end side: past the right edge, or past
                // the left edge in a right-to-left container.
                let padding = node.style.inset();
                let overflow = if rtl {
                    padding - content_left
                } else {
                    content_right + padding - node.bounds.width
                };
                // A fraction of a pixel from rounding is not worth a scrollbar.
                node.scroll.maximum_x = if overflow >= 0.5 { overflow } else { 0.0 };
                node.scroll.offset_x = node.scroll.offset_x.clamp(0.0, node.scroll.maximum_x);
            }
            let origin = Point {
                x: node.bounds.x
                    + if rtl {
                        node.scroll.offset_x
                    } else {
                        -node.scroll.offset_x
                    },
                y: node.bounds.y - node.scroll.offset,
            };
            let clip = if unclipped { clip } else { node.clip };
            // Placing a child needs the tree mutably, so the children are copied.
            let children = node.children.clone();
            // A tab strip that scrolls keeps its panel menu at the end edge, and
            // the tabs stop where the menu starts.
            let bounds = node.bounds;
            let (offset_x, maximum_x) = (node.scroll.offset_x, node.scroll.maximum_x);
            let strip = node.kind == Kind::TabList;
            let pinned = (node.kind == Kind::TabList && node.scroll.maximum_x > 0.0)
                .then(|| {
                    node.children_by_key
                        .get(&WidgetKey::named(STRIP_MENU))
                        .copied()
                })
                .flatten()
                .map(|menu| {
                    let layout = self.layout.layout(self.nodes[&menu].layout_id).unwrap();
                    (menu, layout.location.x, layout.size.width)
                });
            // What a scrolling strip shows of its tabs: its box, less the menu and
            // less the button over each side with tabs hidden past it, so nothing
            // of a tab is drawn under a button.
            let tabs = (strip && maximum_x > 0.0).then(|| {
                let menu = pinned.map_or(0.0, |(_, _, width)| width);
                let (start, end) = (offset_x >= 0.5, maximum_x - offset_x >= 0.5);
                // The start side is on the right in a right-to-left strip.
                let (left_button, right_button) = if rtl { (end, start) } else { (start, end) };
                let left = bounds.x
                    + if rtl { menu } else { 0.0 }
                    + if left_button { STRIP_BUTTON } else { 0.0 };
                let right = bounds.x + bounds.width
                    - if rtl { 0.0 } else { menu }
                    - if right_button { STRIP_BUTTON } else { 0.0 };
                Rect {
                    x: left,
                    width: (right - left).max(0.0),
                    ..bounds
                }
            });
            let node = self.nodes.get_mut(&id).unwrap();
            if old_bounds != node.bounds {
                self.diagnostic.bounds_changed = true;
            }
            if old_bounds != node.bounds || old_clip != node.clip || old_scroll != node.scroll {
                self.add_damage(old_clip);
                self.invalidate_paint(id);
            }
            self.note_layout_change(id, old_layout_rect, layout_rect);
            for &child in &children {
                // The scroll buttons sit over the edges of the part that shows
                // tabs, and only on a side with tabs hidden past it.
                if let Control::StripScroll { end } = self.nodes[&child].control {
                    let hidden = if end { maximum_x - offset_x } else { offset_x };
                    let menu = pinned.map_or(0.0, |(_, _, width)| width);
                    let (left, right) = if rtl {
                        (bounds.x + menu, bounds.x + bounds.width)
                    } else {
                        (bounds.x, bounds.x + bounds.width - menu)
                    };
                    // The start side is on the right in a right-to-left strip.
                    let x = if end == rtl {
                        left
                    } else {
                        right - STRIP_BUTTON
                    };
                    let layout_x = self
                        .layout
                        .layout(self.nodes[&child].layout_id)
                        .unwrap()
                        .location
                        .x;
                    let origin = Point {
                        x: x - layout_x,
                        y: origin.y,
                    };
                    let shown = if hidden >= 0.5 {
                        clip
                    } else {
                        Rect {
                            width: 0.0,
                            height: 0.0,
                            ..bounds
                        }
                    };
                    self.update_bounds_reference(child, origin, shown);
                    continue;
                }
                match (pinned, tabs) {
                    (Some((menu, x, width)), _) if menu == child => {
                        let edge = if rtl {
                            bounds.x
                        } else {
                            bounds.x + bounds.width - width
                        };
                        let origin = Point {
                            x: edge - x,
                            y: origin.y,
                        };
                        self.update_bounds_reference(child, origin, clip);
                    }
                    (_, Some(tabs)) => {
                        self.update_bounds_reference(child, origin, clip.intersect(tabs))
                    }
                    _ => self.update_bounds_reference(child, origin, clip),
                }
            }
        }
    }

    /// Two contexts that are given the same frames and events, one walking
    /// bounds as `update_bounds` does and the other as
    /// `update_bounds_reference` does, and are compared after each step.
    struct Twins {
        walked: Context,
        reference: Context,
    }

    fn bits(rect: Rect) -> [u32; 4] {
        [rect.x, rect.y, rect.width, rect.height].map(f32::to_bits)
    }

    fn scroll_bits(scroll: ScrollState) -> [u32; 4] {
        [
            scroll.offset,
            scroll.maximum,
            scroll.offset_x,
            scroll.maximum_x,
        ]
        .map(f32::to_bits)
    }

    fn sorted(ids: impl Iterator<Item = NodeId>) -> Vec<u64> {
        let mut ids: Vec<u64> = ids.map(|id| id.0).collect();
        ids.sort_unstable();
        ids
    }

    impl Twins {
        fn new(direction: Direction) -> Self {
            let mut walked = Context::new();
            let mut reference = Context::new();
            reference.reference_walk = true;
            walked.set_direction(direction);
            reference.set_direction(direction);
            Self { walked, reference }
        }

        /// Runs `step` on both and compares what it returns and what it left.
        fn each<R: PartialEq + core::fmt::Debug>(&mut self, step: impl Fn(&mut Context) -> R) -> R {
            for context in [&mut self.walked, &mut self.reference] {
                context.diagnostic.bounds_changed = false;
            }
            let walked = step(&mut self.walked);
            let reference = step(&mut self.reference);
            assert_eq!(walked, reference);
            self.compare();
            walked
        }

        fn frame(&mut self, size: Size, page: &Page) {
            self.each(|context| {
                context.run(size, &mut MonospaceMeasurer, |ui| view(ui, page));
            });
        }

        fn pointer_at(&mut self, path: &[&str]) {
            self.each(|context| {
                let bounds = context.nodes[&context.find(path).unwrap()].bounds;
                context.event(Event::PointerMoved(Point {
                    x: bounds.x + bounds.width / 2.0,
                    y: bounds.y + bounds.height / 2.0,
                }))
            });
        }

        fn wheel(&mut self, delta_x: f32, delta_y: f32) {
            self.each(|context| context.event(Event::Scroll { delta_x, delta_y }));
        }

        fn press(&mut self, key: Key) {
            self.each(|context| {
                let down = context.event(Event::KeyDown {
                    key,
                    shift: false,
                    repeat: false,
                });
                (down, context.event(Event::KeyUp(key)))
            });
        }

        fn compare(&self) {
            let (walked, reference) = (&self.walked, &self.reference);
            let ids = sorted(walked.nodes.keys().copied());
            assert_eq!(ids, sorted(reference.nodes.keys().copied()));
            for id in ids {
                let id = NodeId(id);
                let (one, other) = (&walked.nodes[&id], &reference.nodes[&id]);
                let node = format!("{:?} {:?} {:?}", id, one.kind, one.key);
                assert_eq!(bits(one.bounds), bits(other.bounds), "bounds of {node}");
                assert_eq!(bits(one.clip), bits(other.clip), "clip of {node}");
                assert_eq!(
                    bits(one.layout_rect),
                    bits(other.layout_rect),
                    "layout of {node}"
                );
                assert_eq!(
                    scroll_bits(one.scroll),
                    scroll_bits(other.scroll),
                    "scroll of {node}"
                );
                assert_eq!(
                    one.paint_cache.is_some(),
                    other.paint_cache.is_some(),
                    "paint cache of {node}"
                );
                // Where a layout animation starts from, and when, are private to
                // the module that runs it: they show in what `Debug` prints.
                assert_eq!(
                    format!("{:?}", one.anim),
                    format!("{:?}", other.anim),
                    "anim of {node}"
                );
            }
            // The damage in the order it was added: the merge of more regions
            // than there is room for depends on it.
            let damage = |context: &Context| {
                context
                    .pending_damage
                    .iter()
                    .map(|rect| bits(*rect))
                    .collect::<Vec<_>>()
            };
            assert_eq!(damage(walked), damage(reference), "pending damage");
            assert_eq!(
                walked.diagnostic.bounds_changed, reference.diagnostic.bounds_changed,
                "bounds changed"
            );
            assert_eq!(
                sorted(walked.spare_chunks.keys().copied()),
                sorted(reference.spare_chunks.keys().copied()),
                "spare chunks"
            );
            assert_eq!(walked.hovered, reference.hovered);
            assert_eq!(walked.pending, reference.pending);
            assert_eq!(walked.geometry_dirty, reference.geometry_dirty);
            assert_eq!(walked.clips_valid, reference.clips_valid);
            let regions = |context: &Context| {
                context
                    .damage_regions()
                    .iter()
                    .map(|rect| bits(*rect))
                    .collect::<Vec<_>>()
            };
            assert_eq!(regions(walked), regions(reference), "damage regions");
            assert!(
                walked.scene().primitives == reference.scene().primitives,
                "scene"
            );
        }
    }

    /// What the view declares.
    struct Page {
        rows: usize,
        tabs: usize,
        selected: usize,
        popover: bool,
        /// Whether the boxes are closer together.
        tight: bool,
    }

    /// A model of rows labelled by their index.
    struct Rows(usize);

    impl ListModel for Rows {
        fn len(&self) -> usize {
            self.0
        }
        fn item_id(&self, index: usize) -> ItemId {
            ItemId(index as u64)
        }
        fn index_of(&self, id: ItemId) -> Option<usize> {
            ((id.0 as usize) < self.0).then_some(id.0 as usize)
        }
        fn label(&self, index: usize) -> String {
            format!("Row {index}")
        }
    }

    fn menu() -> Menu {
        let command = |id: &str, label: &str| MenuEntry::Command(Command::with_id(id, label, None));
        Menu::new([
            command("open", "Open..."),
            MenuEntry::submenu(
                "Recent",
                [
                    command("a", "a.txt"),
                    MenuEntry::submenu("More", [command("b", "b.txt"), command("c", "c.txt")]),
                ],
            ),
            command("save", "Save"),
        ])
    }

    /// Nested scrolling containers that overflow on both axes, a list, a
    /// tab strip that overflows, and every kind of overlay.
    fn view(ui: &mut Ui<'_>, page: &Page) {
        ui.keyed("outer", |ui| {
            ui.scroll(
                Style::default().height(Length::Px(220.0)).padding(8.0),
                |ui| {
                    for row in 0..page.rows {
                        ui.keyed(format!("row{row}"), |ui| {
                            ui.row(Style::default().gap(30.0), |ui| {
                                for column in 0..6 {
                                    ui.keyed(format!("button{column}"), |ui| {
                                        ui.button(
                                            Button::new("Open")
                                                .style(Style::button().width(Length::Px(110.0))),
                                        );
                                    });
                                }
                            });
                        });
                    }
                    ui.keyed("inner", |ui| {
                        ui.scroll(Style::default().height(Length::Px(90.0)), |ui| {
                            for row in 0..8 {
                                ui.keyed(format!("n{row}"), |ui| ui.label("nested row"));
                            }
                        });
                    });
                },
            );
        });
        // Boxes that stay apart wherever they are: more of them move than
        // damage has room for regions. They glide to where layout puts them.
        ui.keyed("boxes", |ui| {
            ui.row(
                Style::default()
                    .width(Length::Px(480.0))
                    .height(Length::Px(30.0))
                    .gap(if page.tight { 20.0 } else { 30.0 }),
                |ui| {
                    for index in 0..12 {
                        ui.keyed(format!("box{index}"), |ui| {
                            ui.label(
                                Label::new("").style(
                                    Style::default()
                                        .width(Length::Px(10.0))
                                        .height(Length::Px(10.0))
                                        .layout_transition(Transition::new(160))
                                        .background(Color::rgb(20 * index as u8, 90, 200)),
                                ),
                            );
                        });
                    }
                },
            );
        });
        ui.keyed("list", |ui| {
            ui.virtual_list(
                &Rows(60),
                &ListState::default(),
                VirtualListOptions {
                    height: 150.0,
                    row_height: 30.0,
                    ..VirtualListOptions::default()
                },
                |_| (),
            );
        });
        ui.keyed("tabs", |ui| {
            let tabs: Vec<Tab> = (0..page.tabs)
                .map(|tab| Tab::new(format!("Tab number {tab}")))
                .collect();
            ui.tabs(
                page.selected,
                &tabs,
                |_| (),
                |ui, tab| {
                    ui.label(format!("Panel {tab}"));
                },
            );
        });
        ui.keyed("area", |ui| {
            let items: Vec<MenuItem> = (0..24)
                .map(|item| MenuItem::new(format!("Item {item}")))
                .collect();
            ui.context_menu(
                &items,
                Style::default()
                    .width(Length::Px(200.0))
                    .height(Length::Px(60.0)),
                |_| (),
                |ui| {
                    ui.keyed("hint", |ui| ui.label("Right-click"));
                },
            );
        });
        ui.keyed("bar", |ui| {
            ui.row(Style::default().gap(8.0), |ui| {
                ui.keyed("share", |ui| ui.button("Share"));
                ui.keyed("pop", |ui| {
                    ui.popover(
                        page.popover,
                        PopoverOptions::default(),
                        |_| (),
                        |ui| {
                            ui.keyed("hint", |ui| ui.label("Share this with anyone"));
                            ui.keyed("copy", |ui| ui.button("Copy link"));
                        },
                    );
                });
                ui.keyed("file", |ui| {
                    ui.command_menu("File", &menu());
                });
            });
        });
    }

    const BIG: Size = Size::new(480.0, 900.0);

    /// Walks the bounds of the same tree through every kind of change, in
    /// the direction given.
    fn gallery(direction: Direction) {
        let mut twins = Twins::new(direction);
        let mut page = Page {
            rows: 30,
            tabs: 14,
            selected: 0,
            popover: false,
            tight: false,
        };
        twins.frame(BIG, &page);

        // Wheels over the containers that scroll, down and up and sideways.
        for path in [
            &["outer"][..],
            &["outer", "inner"],
            &["list"],
            &["tabs", "$strip"],
        ] {
            twins.pointer_at(path);
            for (delta_x, delta_y) in [
                (0.0, 120.0),
                (0.0, 120.0),
                (60.0, 0.0),
                (0.0, -300.0),
                (-40.0, 0.0),
            ] {
                twins.wheel(delta_x, delta_y);
            }
            twins.frame(BIG, &page);
        }
        // A tab that is not in view is brought into it.
        page.selected = 11;
        twins.frame(BIG, &page);
        page.selected = 2;
        twins.frame(BIG, &page);

        // A window narrower than the content, and then wider.
        for size in [Size::new(200.0, 200.0), Size::new(333.5, 417.25), BIG] {
            twins.frame(size, &page);
        }

        // A context menu, taller than a small window, anchored at the pointer.
        twins.pointer_at(&["area"]);
        twins.each(|context| {
            let at = context.pointer.unwrap();
            context.event(Event::ContextMenu(at))
        });
        twins.frame(BIG, &page);
        twins.pointer_at(&["area", "$popup"]);
        twins.wheel(0.0, 100.0);
        // The window shrinks while the menu is open.
        twins.frame(Size::new(300.0, 260.0), &page);
        twins.wheel(0.0, 100.0);
        twins.press(Key::Escape);
        twins.frame(BIG, &page);

        // A popover, and the window shrinking under it.
        page.popover = true;
        twins.frame(BIG, &page);
        twins.frame(Size::new(260.0, 400.0), &page);
        page.popover = false;
        twins.frame(BIG, &page);

        // A menu and its submenus, in a window with room and in one without.
        twins.each(|context| {
            let button = context.find(&["bar", "file"]).unwrap();
            context.action(button, Action::Activate)
        });
        twins.frame(BIG, &page);
        twins.press(Key::ArrowDown);
        twins.press(Key::ArrowRight);
        twins.frame(BIG, &page);
        twins.press(Key::ArrowDown);
        twins.press(Key::Enter);
        twins.frame(BIG, &page);
        twins.frame(Size::new(400.0, 300.0), &page);
        twins.press(Key::Escape);
        twins.frame(BIG, &page);

        // Content that shrinks, so that offsets are clamped, and grows again.
        twins.pointer_at(&["outer"]);
        twins.wheel(0.0, 5000.0);
        page.rows = 2;
        twins.frame(BIG, &page);
        page.rows = 30;
        twins.frame(BIG, &page);

        // Boxes that glide to where layout puts them: from their places, from
        // where they are drawn partway, and from their places again once the
        // glide is over.
        for (now, tight) in [(100, true), (140, false), (900, true), (900, false)] {
            page.tight = tight;
            twins.each(|context| context.set_now(Duration::from_millis(now)));
            twins.frame(BIG, &page);
        }

        // Nodes whose old clip or scroll differs from what the walk finds
        // while their boxes are where they were.
        twins.each(|context| {
            for node in context.nodes.values_mut() {
                node.clip.x += 3.0;
            }
            context.refresh_bounds(context.size);
        });
        twins.frame(BIG, &page);
        for path in [&["outer"][..], &["list"], &["tabs", "$strip"]] {
            twins.each(|context| {
                let id = context.find(path).unwrap();
                let scroll = &mut context.nodes.get_mut(&id).unwrap().scroll;
                scroll.maximum = 99.0;
                scroll.maximum_x = 99.0;
                context.refresh_bounds(context.size);
            });
            twins.frame(BIG, &page);
        }
        // Twelve boxes whose old clips are clear of the new ones and whose
        // places in layout have moved, so that each starts to glide: the damage
        // is added in the order the walk adds it.
        twins.each(|context| {
            for index in 0..12 {
                let id = context.find(&["boxes", &format!("box{index}")]).unwrap();
                let node = context.nodes.get_mut(&id).unwrap();
                node.clip.x += 20.0;
                node.layout_rect.x += 5.0;
            }
            context.refresh_bounds(context.size);
        });
        twins.frame(BIG, &page);
    }

    #[test]
    fn the_walk_gives_what_the_plain_one_does_through_every_kind_of_change() {
        gallery(Direction::Ltr);
    }

    #[test]
    fn the_walk_gives_what_the_plain_one_does_in_a_right_to_left_tree() {
        gallery(Direction::Rtl);
    }

    /// Values for which `y + h - h` is below `y`.
    fn rounds_below(heights: &[f32]) -> Vec<(f32, f32)> {
        let mut found = Vec::new();
        for &h in heights {
            for step in 0..4000 {
                let y = step as f32 * 0.1;
                if y + h - h < y {
                    found.push((y, h));
                    break;
                }
            }
        }
        found
    }

    #[test]
    fn a_line_as_tall_as_its_box_sits_at_its_top_where_rounding_leaves_no_room() {
        let mut cases = vec![(98.4_f32, 40.0_f32)];
        cases.extend(rounds_below(&[17.0, 24.0, 33.5, 48.0]));
        assert!(cases.len() >= 3, "{cases:?}");
        for (y, h) in cases {
            // The box has no more room than the line, and the bound
            // computed from it is below the box's own top.
            assert!(y + h - h < y, "{y} {h}");
            for top in [y - 3.0, y, y + 3.0] {
                assert_eq!(within_box(top, y, h, h), y, "{y} {h} {top}");
            }
        }
    }

    #[test]
    fn a_line_is_kept_in_its_box_as_clamp_would() {
        // (top asked for, box top, box height, line height, result)
        let table = [
            (12.0, 10.0, 40.0, 20.0, 12.0),
            (5.0, 10.0, 40.0, 20.0, 10.0),
            (10.0, 10.0, 40.0, 20.0, 10.0),
            (35.0, 10.0, 40.0, 20.0, 30.0),
            (30.0, 10.0, 40.0, 20.0, 30.0),
            (-4.0, 0.0, 16.0, 10.0, 0.0),
            (4.0, 0.0, 16.0, 10.0, 4.0),
            (80.0, 0.0, 16.0, 10.0, 6.0),
        ];
        for (top, y, extent, height, expected) in table {
            assert_eq!(within_box(top, y, extent, height), expected);
            assert_eq!(top.clamp(y, y + extent - height), expected);
        }
    }
}
