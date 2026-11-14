//! Bounds, damage and the paint chunks the scene is assembled from.

use super::*;

impl Context {
    pub(super) fn refresh_bounds(&mut self, size: Size) {
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
        self.pending |= Dirty::ACCESSIBILITY;
    }

    pub(super) fn update_bounds(&mut self, id: NodeId, origin: Point, clip: Rect) {
        let viewport = self.nodes[&self.root].bounds;
        let node = &self.nodes[&id];
        let old_bounds = node.bounds;
        let old_clip = node.clip;
        let old_scroll = node.scroll;
        // How far the children reach past the node's own box, per side.
        let (content_left, content_right, content_bottom) = node
            .children
            .iter()
            .filter(|child| {
                !matches!(self.nodes[child].kind, Kind::Popup | Kind::Popover)
                    && !matches!(self.nodes[child].control, Control::StripScroll { .. })
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
            .map(|parent| (parent.bounds, parent.resolved.rtl()));
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
        let old_layout_rect = std::mem::replace(&mut node.layout_rect, layout_rect);
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
            if let Some((row, rtl)) = beside {
                // At the end side of its row, lapping it slightly, or at the
                // other side when the window ends first; level with the row.
                const LAP: f32 = 2.0;
                let after = row.x + row.width - LAP;
                let before = row.x - node.bounds.width + LAP;
                let fits_after = after + node.bounds.width <= viewport.width;
                let fits_before = before >= 0.0;
                node.bounds.x = match (rtl, fits_after, fits_before) {
                    (false, true, _) | (true, true, false) => after,
                    (true, _, true) | (false, false, true) => before,
                    _ => after,
                };
                node.bounds.y = (row.y - node.style.padding.max(0.0))
                    .min(viewport.height - node.bounds.height)
                    .max(0.0);
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
        node.clip = clip.intersect(node.bounds);
        if matches!(node.kind, Kind::Scroll | Kind::Popup | Kind::Collection) {
            node.scroll.maximum =
                (content_bottom + node.style.padding.max(0.0) - node.bounds.height).max(0.0);
            node.scroll.offset = node.scroll.offset.clamp(0.0, node.scroll.maximum);
        }
        let rtl = node.resolved.rtl();
        if matches!(node.kind, Kind::Scroll | Kind::Collection | Kind::TabList) {
            // Content overflows at the end side: past the right edge, or past
            // the left edge in a right-to-left container.
            let padding = node.style.padding.max(0.0);
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
        let clip = node.clip;
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
        for child in children {
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
    }

    /// An overlay painted into the scene rather than declared as a node:
    /// its rectangle is damaged now and again on the frame it goes.
    pub(super) fn painted_overlay(&mut self, rect: Rect) {
        self.add_damage(rect);
        self.painted_overlays.push(rect);
    }

    pub(super) fn add_damage(&mut self, rect: Rect) {
        if rect.width > 0.0 && rect.height > 0.0 {
            self.pending_damage = Some(self.pending_damage.map_or(rect, |old| old.union(rect)));
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
            if node.paint_cache.take().is_none() {
                break;
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
        }
    }

    pub(super) fn paint(&mut self, id: NodeId) -> Arc<PaintChunk> {
        if let Some(chunk) = &self.nodes[&id].paint_cache {
            self.stats.paint_cache_hits += 1;
            return chunk.clone();
        }
        self.stats.paint_nodes += 1;
        self.nodes.get_mut(&id).unwrap().painted_frame = self.frame;
        // A child of a tab strip is drawn only where the strip shows tabs,
        // which stops short of a pinned panel menu.
        let node = &self.nodes[&id];
        let strip_clip = node
            .parent
            .filter(|parent| self.nodes[parent].kind == Kind::TabList)
            .map(|_| node.clip)
            .filter(|clip| *clip != node.bounds);
        if strip_clip.is_some_and(|clip| clip.width <= 0.0 || clip.height <= 0.0) {
            let chunk = Arc::new(PaintChunk {
                before: Vec::new(),
                children: Vec::new(),
                after: Vec::new(),
            });
            self.nodes.get_mut(&id).unwrap().paint_cache = Some(chunk.clone());
            return chunk;
        }
        let target = self.appearance(id, &self.nodes[&id]);
        let animation_state::Displayed { shown, layer } = self.displayed(id, target);
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
        let mut scene = Scene::default();
        if let Some(clip) = strip_clip {
            scene.primitives.push(Primitive::PushClip(clip));
        }
        if let Some((opacity, transform)) = layer {
            scene.primitives.push(Primitive::PushLayer {
                bounds: node.bounds,
                opacity,
                transform,
            });
        }
        // A shadow is behind the node, so it is drawn before the clip that
        // bounds the node's own painting.
        paint_shadow(node, radius, false, &mut scene);
        scene.primitives.push(Primitive::PushClip(node.bounds));
        let focused = self.focused == Some(id) && self.quiet_focus != Some(id);
        self.paint_fill(node, focused, background, radius, &mut scene);
        self.paint_background_image(node, &mut scene, foreground, radius);
        if node.kind == Kind::RichText {
            self.paint_rich_text(id, &mut scene, foreground);
        } else if node.kind == Kind::Image {
            self.paint_image(id, &mut scene, foreground, radius);
        } else if !node.text.is_empty() {
            self.paint_text(node, foreground, &mut scene);
        }
        self.paint_hint(node, &mut scene);
        paint_shadow(node, radius, true, &mut scene);
        if let Some(widget) = &node.custom {
            let offset = Point {
                x: node.bounds.x,
                y: node.bounds.y,
            };
            scene.primitives.extend(
                widget
                    .primitives
                    .iter()
                    .map(|primitive| primitive.translated(offset)),
            );
        }
        self.paint_control(id, &mut scene);
        let node = &self.nodes[&id];
        let children = node
            .children
            .iter()
            .copied()
            .filter(|child| !matches!(self.nodes[child].kind, Kind::Popup | Kind::Popover))
            .collect::<Vec<_>>();
        let before = std::mem::take(&mut scene.primitives);
        self.paint_scrollbars(id, &mut scene);
        self.paint_strip_edges(id, &mut scene);
        scene.primitives.push(Primitive::PopClip);
        if layer.is_some() {
            scene.primitives.push(Primitive::PopLayer);
        }
        if strip_clip.is_some() {
            scene.primitives.push(Primitive::PopClip);
        }
        let after = scene.primitives;
        let children = children
            .into_iter()
            .map(|child| self.paint(child))
            .collect();
        let chunk = Arc::new(PaintChunk {
            before,
            children,
            after,
        });
        self.nodes.get_mut(&id).unwrap().paint_cache = Some(chunk.clone());
        chunk
    }

    /// The node's flat colour and gradient, with the focus ring when it has
    /// focus.
    fn paint_fill(
        &self,
        node: &Node,
        focused: bool,
        background: Option<Color>,
        radius: f32,
        scene: &mut Scene,
    ) {
        // There is no stroke primitive. Over an opaque background the ring is
        // the focus colour with the background drawn back inside it, which
        // keeps rounded corners. That cannot work for a node that is
        // transparent or translucent (a scroll page, a bare container): the
        // focus colour would flood it. Those get four edges instead.
        let opaque = background.filter(|background| background.3 == 255);
        if let (true, Some(background)) = (focused, opaque) {
            scene.primitives.push(Primitive::Quad {
                rect: node.bounds,
                color: self.theme.focus,
                radius,
            });
            let rect = Rect {
                x: node.bounds.x + 2.0,
                y: node.bounds.y + 2.0,
                width: (node.bounds.width - 4.0).max(0.0),
                height: (node.bounds.height - 4.0).max(0.0),
            };
            scene.primitives.push(Primitive::Quad {
                rect,
                color: background,
                radius: (radius - 2.0).max(0.0),
            });
        } else {
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
            if focused {
                let Rect {
                    x,
                    y,
                    width,
                    height,
                } = node.bounds;
                let ring = 2.0_f32.min(width / 2.0).min(height / 2.0);
                for rect in [
                    Rect {
                        x,
                        y,
                        width,
                        height: ring,
                    },
                    Rect {
                        x,
                        y: y + height - ring,
                        width,
                        height: ring,
                    },
                    Rect {
                        x,
                        y,
                        width: ring,
                        height,
                    },
                    Rect {
                        x: x + width - ring,
                        y,
                        width: ring,
                        height,
                    },
                ] {
                    scene.primitives.push(Primitive::Quad {
                        rect,
                        color: self.theme.focus,
                        radius: 0.0,
                    });
                }
            }
        }
    }

    /// The node's own text, placed in its box.
    fn paint_text(&self, node: &Node, foreground: Color, scene: &mut Scene) {
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
        let width = measured.map_or(node.label_width, |size| size.width);
        if width > 0.0 && node.style.wrap == TextWrap::None {
            let spare = (rect.width - width).max(0.0);
            rect.x += match node.style.justify_content {
                crate::Justify::Center => spare / 2.0,
                crate::Justify::End => spare,
                // Right-to-left text without an opinion goes to the end.
                _ if node.resolved.rtl() => spare,
                _ => 0.0,
            };
        }
        if let Some(size) = measured.filter(|size| size.height > 0.0) {
            let spare = (rect.height - size.height).max(0.0);
            rect.y += match node.style.align_items {
                crate::Align::Center => spare / 2.0,
                crate::Align::End => spare,
                _ => 0.0,
            };
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
            scene.primitives.push(Primitive::Text {
                rect,
                text: node.text.clone(),
                font_size: node.resolved.font_size,
                color: foreground,
            });
        } else {
            scene.primitives.push(Primitive::StyledText {
                rect,
                text: node.text.clone(),
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
            let padding = node.style.padding.max(0.0);
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
}

/// A drop shadow behind the node, or an inset one inside it.
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
