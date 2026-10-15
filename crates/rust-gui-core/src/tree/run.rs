//! One declaration: running the view, reconciling what it declared with the
//! nodes there were, and removing what it no longer declares.

use super::*;

impl Context {
    /// Declare once after an event or application update. An unchanged declaration
    /// reuses nodes and skips both layout and scene generation.
    pub fn run(
        &mut self,
        size: Size,
        measurer: &mut impl TextMeasurer,
        view: impl FnOnce(&mut Ui<'_>),
    ) -> FrameStats {
        // A closure view sends nothing: what it declares is read back
        // through `Response`, so no handler is kept.
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
        let declare_started = Instant::now();
        let declare_span = tracing::trace_span!("rust_gui.declare").entered();
        let mut clicks = std::mem::take(&mut self.clicks);
        let mut updates = std::mem::take(&mut self.updates);
        let mut submitted = std::mem::take(&mut self.submitted);
        let mut warnings = std::mem::take(&mut self.style_warnings);
        let mut observed = None;
        let mut theme_request = None;
        let mut clipboard = None;
        let mut redraw = false;
        let mut focus = None;
        let mut host = crate::ui::HostOutbox::default();
        let mut layers = Vec::new();
        // What reached the nodes is in the maps the declaration reads, or
        // was resolved into messages already.
        self.queued.clear();
        self.observed_new.borrow_mut().clear();
        self.view_stale = false;
        self.stats.activations = clicks.len() + updates.len() + submitted.len();
        self.mark_awaiting(
            clicks
                .iter()
                .chain(updates.keys())
                .chain(submitted.iter())
                .copied()
                .collect(),
        );
        self.skipped.set(0);
        self.declared.set(0);
        self.declared_bytes.set(0);
        self.families.borrow_mut().clear();
        self.nodes.get_mut(&self.root).unwrap().resolved = self.root_text();
        let mut ui = Ui {
            context: self,
            parent: Some(self.root),
            pending_key: None,
            clicks: &mut clicks,
            elements: self.take_spare(),
            keys: HashSet::new(),
            declared: None,
            sites: HashMap::new(),
            updates: &mut updates,
            submitted: &mut submitted,
            warnings: &mut warnings,
            observed: &mut observed,
            theme_request: &mut theme_request,
            clipboard: &mut clipboard,
            redraw: &mut redraw,
            focus: &mut focus,
            host: &mut host,
            layers: &mut layers,
            enabled: true,
            handlers,
            handler_base: 0,
        };
        view(&mut ui);
        ui.declare_toasts();
        let mut elements = ui.elements;
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
        if clipboard.is_some() {
            self.clipboard_out = clipboard;
        }
        self.redraw_requested = redraw;
        self.apply_outbox(host);
        if let Some(theme) = theme_request {
            // Declared token reads used the old theme; count this like an
            // activation so hosts declare once more before presenting.
            self.set_theme(theme);
            self.nodes.get_mut(&self.root).unwrap().resolved = self.root_text();
            self.stats.activations += 1;
        }
        let size = Size::new(size.width.max(0.0), size.height.max(0.0));
        self.resize_root(size);
        self.reconcile(self.root, elements, measurer);
        self.observed_new.borrow_mut().clear();
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
        for (node, message) in std::mem::take(&mut host.misuse) {
            self.misuse(node, message);
        }
        self.deliver_emitted(std::mem::take(&mut host.emitted));
        for id in std::mem::take(&mut host.commands) {
            self.event(crate::Event::Command(id));
        }
        self.watch_requests = host.watched;
        if host.clip.is_some() {
            self.clip_out = host.clip;
        }
        self.paste_requested |= host.paste;
        if host.files.is_some() {
            self.file_request = host.files;
        }
        self.notifications.append(&mut host.notifications);
        if let Some(menu) = host.menu
            && self.menu_shown.as_ref() != Some(&menu)
        {
            self.menu_shown = Some(menu.clone());
            self.menu_out = Some(menu);
        }
        self.window_commands.append(&mut host.window_commands);
        if let Some(tray) = host.tray
            && self.tray_shown != tray
        {
            self.tray_shown = tray.clone();
            self.tray_out = Some(tray);
        }
        self.image_requests = host.images;
        self.trim_images();
        self.apply_toasts(host.toasts);
        if host.hot_state.is_some() {
            self.hot_state = host.hot_state;
            self.hot_save = false;
        }
        if let Some(format) = host.format {
            self.set_format(format);
            self.stats.activations += 1;
        }
        if let Some(messages) = host.messages {
            // Like a theme: texts already looked up in this declaration are
            // in the old language, so the host declares once more.
            self.set_messages(messages);
            self.stats.activations += 1;
        }
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
        self.maximize_button = self
            .nodes
            .values()
            .find(|node| node.control == Control::MaximizeButton)
            .map(|node| node.bounds);
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
        let layout_span = tracing::trace_span!("rust_gui.layout").entered();
        let layout_ran = self.pending.contains(Dirty::LAYOUT);
        self.diagnose_accessibility();
        if self.pending.contains(Dirty::LAYOUT) {
            let root = self.nodes[&self.root].layout_id;
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
                            ..
                        }) = context
                        {
                            let width = known.width.or(match available.width {
                                taffy::AvailableSpace::Definite(width) => Some(width),
                                taffy::AvailableSpace::MinContent => Some(1.0),
                                taffy::AvailableSpace::MaxContent => None,
                            });
                            self.stats.text_measures += 1;
                            match spans {
                                Some(spans) => measurer.geometry_rich(spans, style, width).size,
                                None => measurer.measure_styled(text, style, width),
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
        let _paint_span = tracing::trace_span!("rust_gui.paint").entered();
        let mut scene = Scene {
            size,
            primitives: Vec::new(),
        };
        self.paint(self.root).append_to(&mut scene.primitives);
        for popover in self.popovers() {
            self.paint(popover).append_to(&mut scene.primitives);
        }
        if let Some(popup) = self.popup
            && let Some(panel) = self
                .nodes
                .get(&popup.owner)
                .and_then(|node| node.children.first())
                .copied()
        {
            self.paint(panel).append_to(&mut scene.primitives);
        }
        self.paint_ghosts(&mut scene);
        // Whatever the overlays drew last time is repainted whether they
        // draw it again or not, because nothing else knows they were
        // there.
        for rect in std::mem::take(&mut self.painted_overlays) {
            self.add_damage(rect);
        }
        self.paint_dock_drag(&mut scene, measurer);
        self.paint_tooltip(&mut scene, measurer);
        self.scene = scene;
        self.stats.scene_rebuilt = true;
        if let Some(damage) = self.pending_damage.take() {
            self.damage.push(damage);
        }
        self.stats.paint = paint_started.elapsed();
    }

    pub(super) fn reconcile(
        &mut self,
        parent: NodeId,
        elements: Vec<Element>,
        measurer: &mut impl TextMeasurer,
    ) {
        let old_children = self.nodes[&parent].children.clone();
        let inherited = self.nodes[&parent].resolved.clone();
        let mut available: HashMap<_, _> = old_children
            .iter()
            .map(|id| (self.nodes[id].key.clone(), *id))
            .collect();
        let mut children = Vec::with_capacity(elements.len());
        let (mut created_here, mut removed_here) = (0usize, 0usize);
        let mut elements = elements;
        for element in elements.drain(..) {
            let existing = available.remove(&element.key).filter(|id| {
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
                    self.create_node(parent, element, resolved, measurer)
                }
            };
            if handler.is_some() || self.nodes[&id].handler.is_some() {
                self.handler_moves.push((id, handler));
            }
            if !kept {
                self.reconcile(id, grandchildren, measurer);
            }
            children.push(id);
        }
        self.give_spare(elements);
        removed_here += available.len();
        for id in available.into_values() {
            self.remove_with_exit(id);
        }
        if old_children != children {
            self.adopt_children(parent, children);
        }
        self.diagnose_children(parent, created_here, removed_here);
    }

    /// Brings a node that was declared again up to date with its element,
    /// noting what changed. Returns the element's children.
    fn update_node(
        &mut self,
        id: NodeId,
        element: Element,
        resolved: ResolvedText,
        measurer: &mut impl TextMeasurer,
    ) -> Vec<Element> {
        let frame = self.frame;
        let node = self.nodes.get_mut(&id).unwrap();
        // Declared again: observed now is what this declaration read.
        node.observed.set(node.observing.take());
        let paint_changed = node.text != element.text
            || node.style != element.style
            || node.resolved != resolved
            || node.enabled != element.enabled
            || node.custom != element.custom
            || node.control != element.control;
        // Colors repaint; only font and wrapping changes re-measure.
        let text_changed = node.text != element.text
            || node.text_style() != resolved.text_style(element.style.wrap)
            // Bold, italic and code runs change advances without changing the text.
            || (element.kind == Kind::RichText && node.control != element.control);
        let mut changed = Dirty::empty();
        if paint_changed {
            changed |= Dirty::PAINT;
        }
        if text_changed {
            changed |= Dirty::TEXT;
        }
        if node.style != element.style {
            changed |= Dirty::STYLE;
        }
        if node.style.layout(Flow::Column) != element.style.layout(Flow::Column) {
            changed |= Dirty::LAYOUT;
        }
        if node.enabled != element.enabled
            || node.control != element.control
            || node.tooltip != element.tooltip
        {
            changed |= Dirty::ACCESSIBILITY;
        }
        if node.tooltip != element.tooltip {
            self.pending |= Dirty::ACCESSIBILITY;
        }
        if !changed.is_empty() {
            node.changed = changed;
            node.changed_frame = frame;
        }
        if text_changed && element.kind.text_leaf() {
            let measured =
                Measurement::leaf(&element, resolved.text_style(element.style.wrap), measurer);
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
        if let Some(measured) = Measurement::image(&element)
            && self.layout.get_node_context(node.layout_id) != Some(&measured)
        {
            self.layout
                .set_node_context(node.layout_id, Some(measured))
                .unwrap();
            self.pending |= Dirty::LAYOUT;
        }
        let mut layout_style = layout_style(&element.style, element.kind, resolved.direction);
        if let Some(measurement) = self.layout.get_node_context(node.layout_id) {
            anchor_minimum(&mut layout_style, &element, measurement.size);
        }
        if *self.layout.style(node.layout_id).unwrap() != layout_style {
            self.layout.set_style(node.layout_id, layout_style).unwrap();
            self.pending |= Dirty::LAYOUT;
            self.diagnostic.layout_cause.get_or_insert(id);
        }
        if node.style != element.style {
            self.pending |= Dirty::STYLE | Dirty::PAINT;
        }
        if node.enabled != element.enabled {
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        node.text = element.text;
        node.style = element.style;
        node.resolved = resolved;
        node.tooltip = element.tooltip;
        node.enabled = element.enabled;
        node.custom = element.custom;
        if let Control::Collection { offset, .. }
        | Control::TextArea {
            scroll_y: offset, ..
        } = element.control
            && node.scroll.offset != offset
        {
            node.scroll.offset = offset;
            self.geometry_dirty = true;
        }
        if let Control::TextInput { value, .. } = &element.control {
            node.editor
                .get_or_insert_with(|| Box::new(Editor::new(value)))
                .synchronize(value);
        }
        node.control = element.control;
        if paint_changed {
            self.invalidate_paint(id);
        }
        element.children
    }

    /// Creates the node for an element declared for the first time.
    /// Returns its identity and the element's children.
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
                observed: std::cell::Cell::new(
                    element.seq != 0 && self.observed_new.borrow().contains(&element.seq),
                ),
                observing: std::cell::Cell::new(false),
                key: element.key,
                kind: element.kind,
                parent: Some(parent),
                children: Vec::new(),
                children_by_key: HashMap::new(),
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
                editor: match &element.control {
                    Control::TextInput { value, .. } => Some(Box::new(Editor::new(value))),
                    _ => None,
                },
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
        self.stats.created += 1;
        self.pending |= Dirty::all();
        (id, element.children)
    }

    /// Gives `parent` its new list of children, in the tree and in Taffy.
    fn adopt_children(&mut self, parent: NodeId, children: Vec<NodeId>) {
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

    pub(super) fn remove(&mut self, id: NodeId) {
        let node = self.nodes.remove(&id).unwrap();
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
        self.submitted.remove(&id);
        self.stats.removed += 1;
        self.pending |= Dirty::TREE | Dirty::LAYOUT | Dirty::PAINT | Dirty::ACCESSIBILITY;
    }

    pub(super) fn repair_interaction(&mut self) {
        if self.popup.is_some_and(|popup| {
            !self
                .nodes
                .get(&popup.owner)
                .is_some_and(|node| node.enabled)
        }) {
            self.popup = None;
        }
        let valid = |id: NodeId| self.nodes.get(&id).is_some_and(Node::focusable);
        if self.focused.is_some_and(|id| !valid(id)) {
            self.focused = None;
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        let restored = self
            .pending_restore
            .take()
            .filter(|target| self.focused.is_none() && valid(*target));
        if restored.is_some() {
            self.focused = restored;
            self.pending |= Dirty::PAINT | Dirty::ACCESSIBILITY;
        }
        if self
            .captured
            .is_some_and(|id| !self.nodes.get(&id).is_some_and(Node::takes_pointer))
        {
            self.captured = None;
            self.pending |= Dirty::PAINT;
        }
        if self.keyboard_pressed.is_some_and(|(id, _)| !valid(id)) {
            self.keyboard_pressed = None;
            self.pending |= Dirty::PAINT;
        }
        if let Some(target) = restored {
            self.invalidate_paint(target);
        }
        self.cancel_compositions_except(self.focused);
        if self.drag.as_ref().is_some_and(|drag| {
            !self
                .nodes
                .get(&drag.source)
                .is_some_and(|node| node.enabled)
        }) {
            self.cancel_drag();
        }
    }
}
