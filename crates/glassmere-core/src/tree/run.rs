//! One declaration: running the view, reconciling what it declared with the
//! nodes there were, and removing what it no longer declares.

use super::*;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

impl Context {
    /// Declare once after an event or application update. An unchanged declaration
    /// reuses nodes and skips both layout and scene generation.
    pub fn run(
        &mut self,
        size: Size,
        measurer: &mut impl TextMeasurer,
        view: impl FnOnce(&mut Ui<'_>),
    ) -> FrameStats {
        // A view declared without an `App` has no `update` to send to, so
        // no handler is kept.
        let mut handlers = Vec::new();
        self.declare(size, measurer, &mut handlers, view);
        self.handler_moves.clear();
        self.freed_handlers.clear();
        self.note_batch(true, 0)
    }

    /// Runs the view once and brings the tree, layout and scene up to date
    /// with it. The handlers it declares are left in `handlers`.
    pub(super) fn declare<M: 'static>(
        &mut self,
        size: Size,
        measurer: &mut impl TextMeasurer,
        handlers: &mut Vec<crate::app::Handler<M>>,
        view: impl FnOnce(&mut Ui<'_, M>),
    ) {
        let measurer = &mut Timed {
            inner: measurer,
            spent: Duration::ZERO,
        };
        self.begin_frame(size);
        self.preparing.valid = false;
        let declare_started = Instant::now();
        let declare_span = tracing::trace_span!("glassmere.declare").entered();
        let mut clicks = core::mem::take(&mut self.clicks);
        let mut updates = core::mem::take(&mut self.updates);
        // A submit is only ever a handler's: nothing declared reads one.
        self.submitted.clear();
        let mut warnings = core::mem::take(&mut self.style_warnings);
        let mut observed = None;
        let mut redraw = false;
        let mut focus = None;
        let mut host = crate::ui::HostOutbox::default();
        let mut layers = Vec::new();
        // What reached the nodes is in the maps the declaration reads, or
        // was resolved into messages already.
        self.queued.clear();
        self.observed_new.borrow_mut().clear();
        self.bound_new.borrow_mut().clear();
        self.view_stale = false;
        self.view_stale_handled = false;
        self.mark_awaiting(clicks.iter().chain(updates.keys()).copied().collect());
        self.skipped.set(0);
        self.declared.set(0);
        self.declared_bytes.set(0);
        self.families.borrow_mut().clear();
        self.nodes.get_mut(&self.root).unwrap().resolved = self.root_text();
        let panics = self.misuse_panics();
        let mut ui = Ui {
            context: self,
            parent: Some(self.root),
            pending_key: None,
            clicks: &mut clicks,
            elements: self.take_spare(),
            keys: Default::default(),
            declared: None,
            sites: Default::default(),
            updates: &mut updates,
            warnings: &mut warnings,
            observed: &mut observed,
            redraw: &mut redraw,
            focus: &mut focus,
            host: &mut host,
            layers: &mut layers,
            enabled: true,
            handlers,
            handler_base: 0,
        };
        let ((), refused) = crate::reactive::declaring(panics, || {
            view(&mut ui);
            ui.declare_toasts();
        });
        let mut elements = ui.elements;
        for message in refused {
            self.misuse(None, message);
        }
        let mut dialog_keys = HashSet::new();
        for layer in layers {
            if dialog_keys.insert(layer.key.clone()) {
                elements.push(layer);
            } else {
                host.misuse
                    .push((None, format!("duplicate dialog key: {:?}", layer.key)));
            }
        }
        self.style_warnings = warnings;
        // After the view, so the graph holds what this frame read.
        self.state_graph = observed
            .as_ref()
            .map(crate::Store::graph)
            .unwrap_or_default();
        self.redraw_requested = redraw;
        self.apply_outbox(host);
        let size = Size::new(size.width.max(0.0), size.height.max(0.0));
        self.resize_root(size);
        self.reconcile(self.root, elements, measurer);
        self.observed_new.borrow_mut().clear();
        self.bound_new.borrow_mut().clear();
        self.refresh_bound(measurer);
        self.stats.skipped = self.skipped.get();
        self.after_reconcile(focus);
        drop(declare_span);
        self.stats.declare = declare_started.elapsed();
        self.layout_pass(size, measurer);
        self.prepare_controls(measurer);
        self.update_hover();
        self.stats.elements = self.declared.get();
        self.stats.element_bytes = self.declared_bytes.get();
        self.paint_and_finish(size, measurer);
    }

    /// Paints what is dirty and closes the frame's statistics.
    pub(super) fn paint_and_finish(
        &mut self,
        size: Size,
        measurer: &mut Timed<'_, impl TextMeasurer>,
    ) {
        if self.pending.contains(Dirty::PAINT) {
            self.paint_pass(size, measurer);
        }
        self.diagnose_paint();
        self.stats.dirty = self.pending;
        self.stats.shaping = measurer.spent;
        self.pending = Dirty::empty();
    }

    /// Resets the per-frame record and advances the clocks that run before
    /// anything is declared.
    pub(super) fn begin_frame(&mut self, size: Size) {
        self.stats = FrameStats::default();
        self.size = size;
        self.frame += 1;
        self.stats.frame = self.frame;
        self.damage.clear();
        self.resizing = size != self.scene.size;
        self.begin_animation_frame();
        self.update_images();
        self.update_toasts();
        self.update_touches();
    }

    /// Ancestors of everything with input waiting, so that a memoized
    /// container knows in one lookup whether it must declare again.
    fn mark_awaiting(&mut self, waiting: Vec<NodeId>) {
        self.awaiting.clear();
        for id in waiting {
            let mut ancestor = Some(id);
            while let Some(node) = ancestor.filter(|id| self.awaiting.insert(*id)) {
                ancestor = self.nodes.get(&node).and_then(|node| node.parent);
            }
        }
    }

    /// Takes in what the view asked the host for during the declaration.
    fn apply_outbox(&mut self, mut host: crate::ui::HostOutbox) {
        for (node, message) in core::mem::take(&mut host.misuse) {
            self.misuse(node, message);
        }
        for id in core::mem::take(&mut host.commands) {
            // The command's button reports this on the next declaration, and
            // no input follows a menu pick to bring one.
            if self.event(crate::Event::Command(id)) {
                self.redraw_requested = true;
            }
        }
        #[cfg(feature = "std")]
        {
            self.watch_requests = host.watched;
        }
        if let Some(menu) = host.menu
            && self.menu_shown.as_ref() != Some(&menu)
        {
            self.menu_shown = Some(menu.clone());
            self.menu_out = Some(menu);
        }
        if let Some(tray) = host.tray
            && self.tray_shown != tray
        {
            self.tray_shown = tray.clone();
            self.tray_out = Some(tray);
        }
        #[cfg(feature = "std")]
        {
            self.image_requests = host.images;
            self.trim_images();
        }
        self.apply_toasts(host.toasts);
    }

    /// Gives the root the window's size, when that changed.
    pub(super) fn resize_root(&mut self, size: Size) {
        if self.scene.size == size {
            return;
        }
        let root = self.nodes.get_mut(&self.root).unwrap();
        root.style.width = Length::Px(size.width);
        root.style.height = Length::Px(size.height);
        let direction = self.direction;
        self.layout
            .set_style(
                root.layout_id,
                layout_style(&root.style, Kind::Column, direction),
            )
            .unwrap();
        self.pending |= Dirty::LAYOUT | Dirty::PAINT;
    }

    /// What the reconciled tree implies: command shortcuts, event areas,
    /// interaction state that pointed at removed nodes, modality and focus.
    fn after_reconcile(&mut self, focus: Option<(Option<NodeId>, WidgetKey)>) {
        let commands: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.enabled)
            .filter_map(|(id, node)| match &node.control {
                Control::Command {
                    shortcut: Some(shortcut),
                    ..
                } => Some((*id, shortcut.clone())),
                _ => None,
            })
            .filter(|(id, _)| self.attached(*id))
            .collect();
        let mut shortcuts = HashSet::new();
        for (id, shortcut) in commands {
            // Two scopes may bind the same keys; one scope may not.
            let scope = self.scope_of(id);
            if !shortcuts.insert((scope, shortcut.clone())) {
                self.misuse(
                    Some(id),
                    format!("duplicate enabled command shortcut in one scope: {shortcut:?}"),
                );
            }
        }
        let caption = |control: Control| {
            self.nodes
                .iter()
                .find(|(_, node)| node.control == control)
                .map(|(id, _)| *id)
        };
        self.caption_buttons = [
            caption(Control::MinimizeButton),
            caption(Control::MaximizeButton),
            caption(Control::CloseButton),
        ];
        self.areas = self
            .nodes
            .iter()
            .filter_map(|(id, node)| match node.control {
                Control::EventArea(area) => Some((*id, area)),
                // A widget of the application's own hears what it asked to.
                _ => node
                    .custom
                    .as_ref()
                    .map(|widget| widget.input)
                    .filter(|input| *input != crate::EventArea::default())
                    .map(|input| (*id, input)),
            })
            .filter(|(id, _)| self.attached(*id))
            .collect();
        self.pending_area_events
            .borrow_mut()
            .retain(|id, _| self.areas.contains_key(id));
        self.repair_interaction();
        self.enforce_modal();
        self.focus_new_popovers();
        if let Some((parent, key)) = focus
            && let Some(target) = self
                .nodes
                .get(&parent.unwrap_or(self.root))
                .and_then(|node| node.children_by_key.get(&key))
                .copied()
            && self.nodes[&target].focusable()
        {
            self.focus_control(target);
        }
        self.update_tooltip();
        self.note_focus();
    }

    /// Computes layout when something asked for it, then the bounds that
    /// follow from it.
    pub(super) fn layout_pass(&mut self, size: Size, measurer: &mut Timed<'_, impl TextMeasurer>) {
        let layout_started = Instant::now();
        let layout_span = tracing::trace_span!("glassmere.layout").entered();
        let layout_ran = self.pending.contains(Dirty::LAYOUT);
        self.diagnose_accessibility();
        if self.pending.contains(Dirty::LAYOUT) {
            let root = self.nodes[&self.root].layout_id;
            self.layout_run += 1;
            let pass = self.layout_run;
            self.layout
                .compute_layout_with_measure(
                    root,
                    taffy::Size {
                        width: taffy::AvailableSpace::Definite(size.width),
                        height: taffy::AvailableSpace::Definite(size.height),
                    },
                    |known, available, _, context, _| {
                        let measured = if let Some(Measurement {
                            wrapped: Some((text, style)),
                            spans,
                            remembered,
                            ..
                        }) = context
                        {
                            let width = known.width.or(match available.width {
                                taffy::AvailableSpace::Definite(width) => Some(width),
                                taffy::AvailableSpace::MinContent => Some(1.0),
                                taffy::AvailableSpace::MaxContent => None,
                            });
                            self.stats.text_measures += 1;
                            let bits = width.map(f32::to_bits);
                            // The measurer gives one answer to identical
                            // arguments within a pass, so a repeat is
                            // answered from the leaf.
                            match remembered.0 {
                                Some((at, same, size)) if at == pass && same == bits => size,
                                _ => {
                                    let size = match spans {
                                        Some(spans) => {
                                            measurer.geometry_rich(spans, style, width).size
                                        }
                                        None => measurer.measure_styled(text, style, width),
                                    };
                                    remembered.0 = Some((pass, bits, size));
                                    size
                                }
                            }
                        } else {
                            context.map(|context| context.size).unwrap_or_default()
                        };
                        taffy::Size {
                            width: known.width.unwrap_or(measured.width),
                            height: known.height.unwrap_or(measured.height),
                        }
                    },
                )
                .unwrap();
            self.stats.layout_passes = 1;
            self.geometry_dirty = true;
            self.pending |= Dirty::PAINT;
        }
        if self.geometry_dirty {
            self.refresh_bounds(size);
        }
        // A tab that became the selected one is brought into its strip.
        for tab in core::mem::take(&mut self.reveal_tabs) {
            if let Some(strip) = self.nodes.get(&tab).and_then(|node| node.parent) {
                self.reveal_in_strip(strip, tab);
            }
        }
        drop(layout_span);
        self.stats.layout = layout_started.elapsed();
        self.diagnose_layout(layout_ran);
    }

    /// Follows the pointer to whatever is under it after layout moved things.
    pub(super) fn update_hover(&mut self) {
        let hover = self.pointer.and_then(|p| self.hit_test(p));
        if hover != self.hovered {
            for id in [self.hovered, hover].into_iter().flatten() {
                self.invalidate_paint(id);
            }
            self.hovered = hover;
            self.pending |= Dirty::PAINT;
        }
    }

    /// Assembles the scene: the tree, then everything drawn over it.
    fn paint_pass(&mut self, size: Size, measurer: &mut Timed<'_, impl TextMeasurer>) {
        let paint_started = Instant::now();
        let _paint_span = tracing::trace_span!("glassmere.paint").entered();
        // The application's measurer is called while the overlays are drawn,
        // and it can unwind. So all the painting is done before the last scene
        // is written over, and a pass that unwinds leaves it as it was. What
        // is drawn over the tree waits in a scene of its own until the tree is
        // in.
        let root = self.paint(self.root);
        let mut above = Vec::new();
        for popover in self.popovers() {
            above.push(self.paint(popover));
        }
        for panel in self.popup_panels() {
            above.push(self.paint(panel));
        }
        // The chunks that nothing was written over, a removed node's among
        // them, are let go.
        self.spare_chunks.clear();
        let mut over = Scene {
            size,
            primitives: Vec::new(),
        };
        self.paint_ghosts(&mut over);
        // Whatever the overlays drew last time is repainted whether they
        // draw it again or not, because nothing else knows they were
        // there.
        for rect in core::mem::take(&mut self.painted_overlays) {
            self.add_damage(rect);
        }
        self.paint_dock_drag(&mut over, measurer);
        self.paint_tooltip(&mut over, measurer);
        // Nothing from here on runs the application's code. The chunks are
        // written over the last scene's primitives, and what they do not
        // reach is cut off.
        self.assembly += 1;
        let assembly = self.assembly;
        let mut primitives = core::mem::take(&mut self.scene.primitives);
        let mut at = 0;
        for chunk in core::iter::once(root).chain(above) {
            #[cfg(debug_assertions)]
            let from = at;
            chunk.place(&mut primitives, &mut at, assembly);
            #[cfg(debug_assertions)]
            assert_eq!(chunk.assert_placed(&primitives, from), at);
        }
        primitives.truncate(at);
        primitives.append(&mut over.primitives);
        let mut scene = Scene { size, primitives };
        // A scene that has shrunk well below the room it holds gives back
        // some of it.
        let len = scene.primitives.len();
        if scene.primitives.capacity() > len.max(256).saturating_mul(4) {
            scene.primitives.shrink_to(len.saturating_mul(2));
        }
        self.scene = scene;
        self.stats.scene_rebuilt = true;
        self.damage.append(&mut self.pending_damage);
        self.stats.paint = paint_started.elapsed();
    }

    pub(super) fn reconcile(
        &mut self,
        parent: NodeId,
        mut elements: Vec<Element>,
        measurer: &mut impl TextMeasurer,
    ) {
        // A dock's panels are found by id while its subtree is reconciled.
        let outer = (self.nodes[&parent].kind == Kind::Dock).then(|| self.enter_dock(Some(parent)));
        let old_children = self.nodes[&parent].children.clone();
        let inherited = self.nodes[&parent].resolved.clone();
        let total = elements.len();
        // A child is nearly always where it was last time, so it is looked
        // for there first. Only when one is not is a map of the children
        // not yet matched built, with a copy of each of their keys. Until it
        // is built, the elements before this one all matched in place.
        let mut matched_in_place = 0;
        let mut available: Option<crate::collections::KeyMap<NodeId>> = None;
        // The new children, made only once they differ from the old ones.
        let mut children: Option<Vec<NodeId>> = None;
        let (mut created_here, mut removed_here) = (0usize, 0usize);
        for (place, element) in elements.iter_mut().enumerate() {
            let in_place = available.is_none()
                && old_children
                    .get(place)
                    .is_some_and(|id| self.nodes[id].key == element.key);
            let found = if in_place {
                matched_in_place = place + 1;
                Some(old_children[place])
            } else {
                available
                    .get_or_insert_with(|| {
                        let mut map = crate::collections::KeyMap::default();
                        for id in &old_children[matched_in_place.min(old_children.len())..] {
                            map.insert(self.nodes[id].key.clone(), *id);
                        }
                        map
                    })
                    .remove(&element.key)
            };
            let existing = found
                .or_else(|| self.claim_panel(parent, element))
                .filter(|id| {
                    if self.nodes[id].kind == element.kind {
                        true
                    } else {
                        self.remove(*id);
                        removed_here += 1;
                        false
                    }
                });
            let resolved = element.style.resolve_text(&inherited);
            // A memoized container that was not declared again keeps the
            // children it has, so its subtree is never visited.
            let kept = existing.is_some_and(|id| {
                element.children.is_empty()
                    && matches!(element.control, Control::Memo(hash)
                        if self.memo_hash(id) == Some(hash))
            });
            let handler = element.handler;
            let (id, grandchildren) = match existing {
                Some(id) => {
                    self.stats.reused += 1;
                    (id, self.update_node(id, element, resolved, measurer))
                }
                None => {
                    created_here += 1;
                    let element = core::mem::replace(
                        element,
                        Element::new(WidgetKey::Named(String::new()), Kind::Column),
                    );
                    self.create_node(parent, element, resolved, measurer)
                }
            };
            if handler.is_some() || self.nodes[&id].handler.is_some() {
                self.handler_moves.push((id, handler));
            }
            if !kept {
                self.reconcile(id, grandchildren, measurer);
            }
            match &mut children {
                Some(children) => children.push(id),
                None if old_children.get(place) == Some(&id) => {}
                None => {
                    let mut list = Vec::with_capacity(total);
                    list.extend_from_slice(&old_children[..place.min(old_children.len())]);
                    list.push(id);
                    children = Some(list);
                }
            }
        }
        self.give_spare(elements);
        // Without the map every element matched in place, and the children
        // left over are those that follow them. With it they are the ones it
        // still holds, in the order they had among the old children: the
        // order of a table is not the same from one run to the next.
        let unmatched: Vec<NodeId> = match available {
            Some(available) => old_children[matched_in_place.min(old_children.len())..]
                .iter()
                .filter(|id| {
                    self.nodes
                        .get(*id)
                        .is_some_and(|node| available.get(&node.key) == Some(*id))
                })
                .copied()
                .collect(),
            None => old_children[total.min(old_children.len())..].to_vec(),
        };
        for id in unmatched {
            // A dock panel another parent claimed is no longer a child here.
            if self
                .nodes
                .get(&id)
                .is_some_and(|node| node.parent == Some(parent))
            {
                removed_here += 1;
                self.remove_with_exit(id);
            }
        }
        // With no new list made, the children are those there were, less
        // any that followed.
        if children.is_some() || total != old_children.len() {
            let children = children.unwrap_or_else(|| old_children[..total].to_vec());
            self.adopt_children(parent, children);
        }
        if let Some(outer) = outer {
            self.release_panels(parent);
            self.leave_dock(outer);
        }
        self.diagnose_children(parent, created_here, removed_here);
    }

    /// The content of the panel `element` declares, when the dock being
    /// reconciled has it in another place: it moves under `parent` with its
    /// whole subtree, which is what keeps focus, scroll offsets and editing
    /// history across a change in the dock's structure.
    fn claim_panel(&mut self, parent: NodeId, element: &Element) -> Option<NodeId> {
        let WidgetKey::Panel(panel) = &element.key else {
            return None;
        };
        let id = *self.panels.get(&self.dock.get()?)?.get(panel)?;
        if let Some(old) = self.nodes[&id].parent {
            let siblings = self.nodes[&old]
                .children
                .iter()
                .copied()
                .filter(|child| *child != id)
                .collect();
            self.adopt_children(old, siblings);
        }
        self.clips_valid = false;
        self.nodes.get_mut(&id)?.parent = Some(parent);
        Some(id)
    }

    /// Whether `id` is panel content that the dock being reconciled keeps
    /// instead of removing it with the place it was in, because the dock may
    /// put it somewhere else. [`Context::release_panels`] removes what no
    /// one took and no group shows.
    fn park_panel(&mut self, id: NodeId) -> bool {
        let Some(dock) = self.dock.get() else {
            return false;
        };
        let WidgetKey::Panel(panel) = &self.nodes[&id].key else {
            return false;
        };
        let kept = self
            .panels
            .get(&dock)
            .is_some_and(|panels| panels.get(panel) == Some(&id));
        if kept {
            let node = self.nodes.get_mut(&id).unwrap();
            node.parent = None;
            let clip = node.clip;
            self.add_damage(clip);
        }
        kept
    }

    /// Removes the panels of `dock` that no group declared this time and
    /// that no group shows either. A shown panel whose tab is not the
    /// selected one stays parked, detached from the tree with its whole
    /// subtree, so that it comes back as it was when its tab is selected.
    /// The tree never reaches it, and [`Context::attached`] keeps what scans
    /// the nodes from finding it.
    fn release_panels(&mut self, dock: NodeId) {
        let mut shown = Vec::new();
        self.shown_panels(dock, &mut shown);
        let Some(panels) = self.panels.get_mut(&dock) else {
            return;
        };
        let mut gone = Vec::new();
        panels.retain(|panel, id| {
            let released = self.nodes[id].parent.is_none() && !shown.contains(panel);
            if released {
                gone.push(*id);
            }
            !released
        });
        // Whatever was shown has gone already, with its exit effects, when
        // it left its group.
        for id in gone {
            self.remove(id);
        }
    }

    /// The ids of the panels the groups under `id` show, without looking
    /// into the panels.
    fn shown_panels(&self, id: NodeId, shown: &mut Vec<String>) {
        let node = &self.nodes[&id];
        match &node.control {
            Control::DockGroup { panels, .. } => shown.extend(panels.iter().cloned()),
            _ => {
                for child in &node.children {
                    self.shown_panels(*child, shown);
                }
            }
        }
    }

    /// Brings a node that was declared again up to date with its element,
    /// noting what changed. Returns the element's children. The kind, style
    /// and editor are changed only here and in `create_node`, and only by a
    /// declaration: see `Preparing::valid`.
    fn update_node(
        &mut self,
        id: NodeId,
        element: &mut Element,
        resolved: ResolvedText,
        measurer: &mut impl TextMeasurer,
    ) -> Vec<Element> {
        let frame = self.frame;
        self.attach_bound(id, element.seq);
        let node = self.nodes.get_mut(&id).unwrap();
        // Declared again: observed now is what this declaration read.
        node.observed.set(node.observing.take());
        // Most nodes are declared as they were. Each of these comparisons
        // is made once, and what follows from two things being equal is not
        // computed to find that out again: a style that is the same lays
        // out the same, and text resolved the same has the same text style.
        let style_same = node.style == element.style;
        let control_same = node.control == element.control;
        let resolved_same = node.resolved == resolved;
        let text_same = node.text == element.text;
        let paint_changed = !text_same
            || !style_same
            || !resolved_same
            || node.enabled != element.enabled
            || node.custom != element.custom
            || !control_same;
        // Colors repaint; only font and wrapping changes re-measure.
        let text_style_same = (resolved_same && node.style.wrap == element.style.wrap)
            || node.text_style() == resolved.text_style(element.style.wrap);
        let text_changed = !text_same
            || !text_style_same
            // Bold, italic and code runs change advances without changing the text.
            || (element.kind == Kind::RichText && !control_same);
        let mut changed = Dirty::empty();
        if paint_changed {
            changed |= Dirty::PAINT;
        }
        if text_changed {
            changed |= Dirty::TEXT;
        }
        if !style_same {
            changed |= Dirty::STYLE;
        }
        if !style_same && node.style.layout(Flow::Column) != element.style.layout(Flow::Column) {
            changed |= Dirty::LAYOUT;
        }
        if node.enabled != element.enabled || !control_same || node.tooltip != element.tooltip {
            changed |= Dirty::ACCESSIBILITY;
        }
        if node.tooltip != element.tooltip {
            self.pending |= Dirty::ACCESSIBILITY;
            // The bubble on show is drawn from it.
            if self.tooltip.is_some_and(|(owner, _)| owner == id) {
                self.pending |= Dirty::PAINT;
            }
        }
        if !changed.is_empty() {
            node.changed = changed;
            node.changed_frame = frame;
        }
        if text_changed && element.kind.text_leaf() {
            let measured =
                Measurement::leaf(element, resolved.text_style(element.style.wrap), measurer);
            self.stats.text_measures += 1;
            if self.layout.get_node_context(node.layout_id) != Some(&measured) {
                self.layout
                    .set_node_context(node.layout_id, Some(measured))
                    .unwrap();
                self.pending |= Dirty::LAYOUT;
                self.diagnostic.layout_cause.get_or_insert(id);
            }
            self.pending |= Dirty::TEXT | Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        if node.custom != element.custom {
            if let Some(widget) = &element.custom
                && self.layout.get_node_context(node.layout_id)
                    != Some(&Measurement::fixed(widget.size))
            {
                self.layout
                    .set_node_context(node.layout_id, Some(Measurement::fixed(widget.size)))
                    .unwrap();
                self.pending |= Dirty::LAYOUT;
            }
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        if let Some(measured) = Measurement::image(element)
            && self.layout.get_node_context(node.layout_id) != Some(&measured)
        {
            self.layout
                .set_node_context(node.layout_id, Some(measured))
                .unwrap();
            self.pending |= Dirty::LAYOUT;
        }
        // The layout style comes from the style, the kind, the direction
        // and, for a text leaf that anchors an overlay, its measured size.
        // With none of those changed it is the one the layout already has.
        let anchors =
            element.kind.text_leaf() && (!element.children.is_empty() || !node.children.is_empty());
        if !style_same || anchors || node.resolved.direction != resolved.direction {
            let mut layout_style = layout_style(&element.style, element.kind, resolved.direction);
            if let Some(measurement) = self.layout.get_node_context(node.layout_id) {
                anchor_minimum(&mut layout_style, element, measurement.size);
            }
            if *self.layout.style(node.layout_id).unwrap() != layout_style {
                self.layout.set_style(node.layout_id, layout_style).unwrap();
                self.pending |= Dirty::LAYOUT;
                self.diagnostic.layout_cause.get_or_insert(id);
            }
        }
        if !style_same {
            self.pending |= Dirty::STYLE | Dirty::PAINT;
        }
        if node.enabled != element.enabled {
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        if matches!(element.control, Control::Tab { selected: true, .. })
            && !matches!(node.control, Control::Tab { selected: true, .. })
        {
            self.reveal_tabs.push(id);
        }
        // Taking leaves an empty value in the element, so each old value is
        // dropped here, in this order, and not when the declared elements
        // are cleared. The style, the tooltip and the control can hold
        // images of the application's, whose drop it sees; the text and the
        // custom widget, whose images the painted chunk holds until the
        // next paint anyway, are taken the same way to leave nothing
        // behind. The control and the children are taken last because they
        // are read until then (the children by the anchor's layout style);
        // nothing reads the others again. A None over a None is not written:
        // the move would copy a whole widget or control to leave everything
        // as it was.
        node.text = core::mem::take(&mut element.text);
        node.style = core::mem::take(&mut element.style);
        node.resolved = resolved;
        node.tooltip = element.tooltip.take();
        node.enabled = element.enabled;
        if node.custom.is_some() || element.custom.is_some() {
            node.custom = element.custom.take();
        }
        if let Control::Collection { offset, .. }
        | Control::TextArea {
            scroll_y: offset, ..
        } = element.control
            && node.scroll.offset != offset
        {
            node.scroll.offset = offset;
            self.geometry_dirty = true;
        }
        if let Control::TextInput {
            value, password, ..
        } = &element.control
        {
            let editor = node
                .editor
                .get_or_insert_with(|| Box::new(Editor::new(value)));
            editor.synchronize(value);
            editor.masked = *password && !editor.revealed;
        }
        if matches!(element.control, Control::SelectableText) {
            node.editor
                .get_or_insert_with(|| Box::new(Editor::block(&node.text)))
                .synchronize(&node.text);
        } else if node.kind == Kind::Label {
            node.editor = None;
        }
        if !(matches!(node.control, Control::None) && matches!(element.control, Control::None)) {
            node.control = core::mem::take(&mut element.control);
        }
        if paint_changed {
            self.invalidate_paint(id);
        }
        core::mem::take(&mut element.children)
    }

    /// Creates the node for an element declared for the first time.
    /// Returns its identity and the element's children. Only a declaration
    /// does this: see `Preparing::valid`.
    fn create_node(
        &mut self,
        parent: NodeId,
        element: Element,
        resolved: ResolvedText,
        measurer: &mut impl TextMeasurer,
    ) -> (NodeId, Vec<Element>) {
        let measured = if let Some(widget) = &element.custom {
            Measurement::fixed(widget.size)
        } else if element.kind.text_leaf() {
            self.stats.text_measures += 1;
            Measurement::leaf(&element, resolved.text_style(element.style.wrap), measurer)
        } else {
            Measurement::image(&element).unwrap_or(Measurement::fixed(Size::default()))
        };
        let mut leaf_style = layout_style(&element.style, element.kind, resolved.direction);
        anchor_minimum(&mut leaf_style, &element, measured.size);
        let layout_id = self
            .layout
            .new_leaf_with_context(leaf_style, measured)
            .unwrap();
        let id = NodeId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("node identity exhausted");
        if let (WidgetKey::Panel(panel), Some(dock)) = (&element.key, self.dock.get()) {
            self.panels
                .entry(dock)
                .or_default()
                .insert(panel.clone(), id);
        }
        if matches!(element.control, Control::Tab { selected: true, .. }) {
            self.reveal_tabs.push(id);
        }
        let editor = match &element.control {
            Control::TextInput {
                value, password, ..
            } => {
                let mut editor = Editor::new(value);
                editor.masked = *password;
                Some(Box::new(editor))
            }
            Control::SelectableText => Some(Box::new(Editor::block(&element.text))),
            _ => None,
        };
        if element.kind == Kind::Popover {
            self.popover_count += 1;
        }
        if element.kind == Kind::Popup {
            self.popup_count += 1;
        }
        if element.kind == Kind::TextArea {
            self.text_area_count += 1;
        }
        if element.kind == Kind::RichEditor {
            self.rich_editor_count += 1;
        }
        self.nodes.insert(
            id,
            Node {
                anim: animation_state::NodeAnim::entering(element.style.enter),
                area: None,
                rich: None,
                rich_editor: None,
                label_width: 0.0,
                hint_width: 0.0,
                churn: 0,
                layout_rect: Rect::default(),
                handler: None,
                observed: core::cell::Cell::new(
                    element.seq != 0 && self.observed_new.borrow().contains(&element.seq),
                ),
                observing: core::cell::Cell::new(false),
                bound: None,
                key: element.key,
                kind: element.kind,
                parent: Some(parent),
                children: Vec::new(),
                children_by_key: Default::default(),
                text: element.text,
                style: element.style,
                enabled: element.enabled,
                layout_id,
                bounds: Rect::default(),
                clip: Rect::default(),
                scroll: ScrollState {
                    offset: if let Control::Collection { offset, .. } = element.control {
                        offset
                    } else {
                        0.0
                    },
                    maximum: 0.0,
                    ..ScrollState::default()
                },
                custom: element.custom,
                paint_cache: None,
                editor,
                control: element.control,
                changed: Dirty::all(),
                changed_frame: self.frame,
                painted_frame: 0,
                resolved,
                scope: matches!(
                    element.kind,
                    Kind::FocusScope | Kind::Dialog | Kind::Popover
                )
                .then(|| Box::new(ScopeState::default())),
                tooltip: element.tooltip,
            },
        );
        // Popovers and autofocus scopes take focus when they appear.
        if element.kind == Kind::Popover
            || matches!(
                self.nodes[&id].control,
                Control::FocusScope {
                    scope: crate::FocusScope {
                        autofocus: true,
                        ..
                    }
                }
            )
        {
            self.new_popovers.push(id);
        }
        self.attach_bound(id, element.seq);
        self.stats.created += 1;
        self.pending |= Dirty::all();
        (id, element.children)
    }

    /// Gives `parent` its new list of children, in the tree and in Taffy.
    /// Only a declaration does this: see `Preparing::valid`.
    fn adopt_children(&mut self, parent: NodeId, children: Vec<NodeId>) {
        self.clips_valid = false;
        let layout_children: Vec<_> = children.iter().map(|id| self.nodes[id].layout_id).collect();
        self.layout
            .set_children(self.nodes[&parent].layout_id, &layout_children)
            .unwrap();
        let children_by_key = children
            .iter()
            .map(|id| (self.nodes[id].key.clone(), *id))
            .collect();
        let node = self.nodes.get_mut(&parent).unwrap();
        node.children = children;
        node.children_by_key = children_by_key;
        self.invalidate_paint(parent);
        self.pending |= Dirty::TREE | Dirty::LAYOUT | Dirty::PAINT | Dirty::ACCESSIBILITY;
    }

    /// Only a declaration removes nodes: see `Preparing::valid`.
    pub(super) fn remove(&mut self, id: NodeId) {
        self.clips_valid = false;
        if self.park_panel(id) {
            return;
        }
        let node = self.nodes.remove(&id).unwrap();
        if self
            .taken_range
            .as_ref()
            .is_some_and(|(taken, _)| *taken == id)
        {
            self.taken_range = None;
        }
        if node.kind == Kind::Popover {
            self.popover_count -= 1;
        }
        if node.kind == Kind::Popup {
            self.popup_count -= 1;
        }
        if node.kind == Kind::TextArea {
            self.text_area_count -= 1;
        }
        if node.kind == Kind::RichEditor {
            self.rich_editor_count -= 1;
        }
        if node.kind == Kind::Dock {
            // The panels parked while their tabs were not selected go with it;
            // the others are children, removed below.
            for panel in self
                .panels
                .remove(&id)
                .into_iter()
                .flatten()
                .map(|(_, id)| id)
            {
                if self.nodes[&panel].parent.is_none() {
                    self.remove(panel);
                }
            }
        }
        // Popovers fall back to their anchor when nothing was focused before them.
        if let Some(state) = &node.scope
            && let Some(target) = state.restore_to.or((node.kind == Kind::Popover)
                .then_some(node.parent)
                .flatten())
            && matches!(
                node.control,
                Control::Dialog { .. }
                    | Control::Popover
                    | Control::FocusScope {
                        scope: crate::FocusScope { restore: true, .. }
                    }
            )
            && self.pending_restore.is_none()
        {
            self.pending_restore = Some(target);
        }
        self.add_damage(node.clip);
        for child in node.children {
            self.remove(child);
        }
        self.layout.remove(node.layout_id).unwrap();
        if let Some(slot) = node.handler {
            self.freed_handlers.push(slot);
        }
        self.clicks.remove(&id);
        self.updates.remove(&id);
        self.calendar_views.remove(&id);
        self.submitted.remove(&id);
        self.stats.removed += 1;
        self.pending |= Dirty::TREE | Dirty::LAYOUT | Dirty::PAINT | Dirty::ACCESSIBILITY;
    }

    pub(super) fn repair_interaction(&mut self) {
        // A menu whose owner went away or was disabled closes, and every
        // submenu opened from it with it.
        let gone = self.popup_levels().position(|popup| {
            !self.attached(popup.owner)
                || !self
                    .nodes
                    .get(&popup.owner)
                    .is_some_and(|node| node.enabled)
        });
        if let Some(depth) = gone {
            self.popup = (depth > 0).then(|| self.popup_parents[depth - 1]);
            self.popup_parents.truncate(depth.saturating_sub(1));
        }
        let valid = |this: &Self, id: NodeId| {
            this.attached(id) && this.nodes.get(&id).is_some_and(Node::focusable)
        };
        if self.focused.is_some_and(|id| !valid(self, id)) {
            self.focused = None;
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        let restored = self
            .pending_restore
            .take()
            .filter(|target| self.focused.is_none() && valid(self, *target));
        if restored.is_some() {
            self.focused = restored;
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        if self.captured.is_some_and(|id| {
            !self.attached(id) || !self.nodes.get(&id).is_some_and(Node::takes_pointer)
        }) {
            self.captured = None;
            self.taken_range = None;
            self.pending |= Dirty::PAINT;
        }
        if self
            .keyboard_pressed
            .is_some_and(|(id, _)| !valid(self, id))
        {
            self.keyboard_pressed = None;
            self.pending |= Dirty::PAINT;
        }
        if let Some(target) = restored {
            self.invalidate_paint(target);
        }
        self.cancel_compositions_except(self.focused);
        if self.drag.as_ref().is_some_and(|drag| {
            !self.attached(drag.source)
                || !self
                    .nodes
                    .get(&drag.source)
                    .is_some_and(|node| node.enabled)
        }) {
            self.cancel_drag();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::FastSet;
    use crate::{CustomWidget, Effect, Label, MonospaceMeasurer};
    use alloc::vec;
    use glassmere_scene::Primitive;

    const SIZE: Size = Size::new(400.0, 4000.0);

    fn next_random(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    /// A random number below `below`.
    fn below(state: &mut u64, below: usize) -> usize {
        (next_random(state) >> 16) as usize % below
    }

    /// What a view declares: the key of each child, and whether it is a
    /// button and not a label.
    type Declared = Vec<(String, bool)>;

    fn declare(context: &mut Context, declared: &Declared, style: &Style) -> FrameStats {
        context.run(SIZE, &mut MonospaceMeasurer, |ui| {
            for (key, button) in declared {
                ui.keyed(key.clone(), |ui| {
                    if *button {
                        ui.button(key.as_str());
                    } else {
                        ui.label(Label::new(key.clone()).style(style.clone()));
                    }
                });
            }
        })
    }

    fn root_children(context: &Context) -> Vec<NodeId> {
        context.nodes[&context.root].children.clone()
    }

    /// `declared` after a few random edits: children added, dropped, moved
    /// and turned from labels into buttons or back.
    fn edited(declared: &Declared, state: &mut u64) -> Declared {
        let mut next = declared.clone();
        if below(state, 4) == 0 {
            return next;
        }
        for _ in 0..1 + below(state, 4) {
            match below(state, 7) {
                0 | 1 if next.len() < 40 => {
                    let key = format!("k{}", below(state, 60));
                    if next.iter().all(|(other, _)| *other != key) {
                        let at = below(state, next.len() + 1);
                        next.insert(at, (key, below(state, 2) == 0));
                    }
                }
                2 if !next.is_empty() => {
                    let at = below(state, next.len());
                    next.remove(at);
                }
                3 if next.len() > 1 => {
                    let (one, other) = (below(state, next.len()), below(state, next.len()));
                    next.swap(one, other);
                }
                4 if !next.is_empty() => {
                    let from = below(state, next.len());
                    let child = next.remove(from);
                    let to = below(state, next.len() + 1);
                    next.insert(to, child);
                }
                5 if !next.is_empty() => {
                    let at = below(state, next.len());
                    next[at].1 = !next[at].1;
                }
                6 if !next.is_empty() => next.truncate(below(state, next.len())),
                _ => {}
            }
        }
        next
    }

    // A child is the node it was when its key and kind are still declared,
    // and a new one otherwise; whatever is not declared again is gone. This
    // is worked out here from the lists, a child at a time.
    #[test]
    fn children_are_kept_removed_and_created_as_a_list_of_keys_says() {
        let mut state = 0x9e37_79b9_7f4a_7c15;
        let mut context = Context::new();
        let style = Style::default();
        let mut declared = Declared::new();
        let mut ids: Vec<NodeId> = Vec::new();
        let mut seen: FastSet<NodeId> = FastSet::default();
        let mut moved = 0;
        for round in 0..600 {
            let next = edited(&declared, &mut state);
            let stats = declare(&mut context, &next, &style);
            let now = root_children(&context);
            assert_eq!(now.len(), next.len(), "round {round}");
            let (mut created, mut reused) = (0, 0);
            for (at, (key, button)) in next.iter().enumerate() {
                let node = &context.nodes[&now[at]];
                assert_eq!(node.key, WidgetKey::Named(key.clone()), "round {round}");
                assert_eq!(
                    node.kind,
                    if *button { Kind::Button } else { Kind::Label },
                    "round {round}"
                );
                assert_eq!(
                    context.nodes[&context.root].children_by_key[&node.key],
                    now[at]
                );
                match declared.iter().position(|old| old == &next[at]) {
                    Some(old) => {
                        assert_eq!(now[at], ids[old], "{key} was made again in round {round}");
                        reused += 1;
                    }
                    None => {
                        assert!(seen.insert(now[at]), "{key} was not made in round {round}");
                        created += 1;
                    }
                }
            }
            let gone: Vec<NodeId> = ids.iter().filter(|id| !now.contains(id)).copied().collect();
            assert!(gone.iter().all(|id| !context.nodes.contains_key(id)));
            assert_eq!(
                (stats.created, stats.reused, stats.removed),
                (created, reused, gone.len()),
                "round {round}"
            );
            // The list of children is written again exactly when it differs.
            // A new context starts with everything to be done.
            if round > 0 {
                assert_eq!(
                    stats.dirty.contains(Dirty::TREE),
                    now != ids,
                    "round {round}: {declared:?} then {next:?}"
                );
                moved += usize::from(now != ids);
            }
            seen.extend(now.iter().copied());
            (declared, ids) = (next, now);
        }
        assert!(moved > 200 && moved < 599, "{moved}");
    }

    /// Which of the children with these `bounds` the ghosts the last frame
    /// painted are of, in the order they are painted.
    fn ghosts_painted(context: &Context, bounds: &[Rect]) -> Vec<usize> {
        context
            .scene()
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                Primitive::PushLayer { bounds: layer, .. } => {
                    bounds.iter().position(|rect| rect == layer)
                }
                _ => None,
            })
            .collect()
    }

    // The children a frame no longer declares are removed in the order they
    // had among the old children, and each leaves a ghost, so that order is
    // in the scene.
    #[test]
    fn removed_children_leave_ghosts_in_the_order_they_had() {
        let mut state = 0x2545_f491_4f6c_dd1d;
        let style = Style::default().exit(Effect::fade(100));
        for case in 0..600 {
            let count = 4 + below(&mut state, 37);
            let names: Vec<String> = (0..count)
                .map(|at| format!("{}-{at}", next_random(&mut state) % 10_000))
                .collect();
            let declared: Declared = names.iter().map(|name| (name.clone(), false)).collect();
            let mut context = Context::new();
            declare(&mut context, &declared, &style);
            let bounds: Vec<Rect> = root_children(&context)
                .iter()
                .map(|id| context.nodes[id].bounds)
                .collect();
            assert!(bounds.windows(2).all(|pair| pair[0].y < pair[1].y));
            // A prefix stays, and from there some are dropped from among
            // those that stay after them.
            let from = below(&mut state, count - 3);
            let dropped = 3 + below(&mut state, count - from - 2);
            let after = from + dropped;
            let mut remaining = declared.clone();
            remaining.drain(from..after);
            declare(&mut context, &remaining, &style);
            let expected: Vec<usize> = (from..after).collect();
            assert_eq!(
                ghosts_painted(&context, &bounds),
                expected,
                "case {case}: {count} children, from {from}, {dropped} dropped"
            );
        }
    }

    // No builder gives a column a custom widget or a control, so these
    // elements are made here. Declared again with any other pair, or with
    // the same, the node holds the pair it was last declared with.
    #[test]
    fn a_node_declared_again_holds_the_custom_widget_and_the_control_it_was_given() {
        let widget = |width: f32| CustomWidget {
            size: Size::new(width, 10.0),
            ..CustomWidget::default()
        };
        let checkbox = |checked: bool| Control::Checkbox {
            label: "box".into(),
            checked,
        };
        let pairs = [
            (None, Control::None),
            (Some(widget(10.0)), Control::None),
            (None, checkbox(true)),
            (Some(widget(10.0)), checkbox(true)),
            (Some(widget(20.0)), checkbox(false)),
            (None, Control::SelectableText),
        ];
        let declare = |context: &mut Context,
                       (custom, control): &(Option<CustomWidget>, Control)| {
            let mut element = Element::new(WidgetKey::named("node"), Kind::Column);
            element.custom = custom.clone();
            element.control = control.clone();
            context.reconcile(context.root, vec![element], &mut MonospaceMeasurer);
            let id = context.nodes[&context.root].children[0];
            let node = &context.nodes[&id];
            assert_eq!(node.custom, *custom);
            assert_eq!(node.control, *control);
        };
        for first in &pairs {
            for second in &pairs {
                let mut context = Context::new();
                declare(&mut context, first);
                declare(&mut context, second);
                declare(&mut context, second);
                declare(&mut context, first);
            }
        }
    }
}
