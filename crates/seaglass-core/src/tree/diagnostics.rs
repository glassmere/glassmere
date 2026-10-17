use super::*;

/// What a framework diagnostic is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticKind {
    /// A container's children are destroyed and recreated frame after frame,
    /// which loses focus, scroll and editing state: their keys are not stable.
    UnstableIds,
    /// Layout runs every frame without moving anything.
    Relayout,
    /// Most of the tree repaints every frame.
    Repaint,
    /// An interactive control has no accessible name.
    Inaccessible,
    /// A render layer is large enough to cost real GPU memory and fill rate.
    OversizedLayer,
    /// A container declares a very large number of children directly.
    HugeCollection,
    /// A file the view asked for could not be read or decoded.
    Resource,
    /// A style asks for a font family that is not installed, so the text is
    /// drawn in another one.
    Font,
    /// The view used the API in a way it does not support, such as two
    /// siblings with one key. Debug builds panic instead; release builds
    /// leave the offending widget out and always report it.
    Misuse,
}

/// One framework warning. Repeats of the same kind on the same node count up
/// instead of adding entries.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameDiagnostic {
    pub kind: DiagnosticKind,
    pub node: Option<NodeId>,
    /// Keys from the root, such as `page/list/main.rs:42:9#3`.
    pub path: String,
    pub message: String,
    pub count: u32,
    /// The last frame that reported it.
    pub frame: u64,
}

/// Consecutive frames of wholesale child replacement before it is reported.
pub const UNSTABLE_ID_FRAMES: u8 = 8;
/// Consecutive frames of layout that moved nothing.
pub const RELAYOUT_FRAMES: u32 = 30;
/// Consecutive frames that repainted more than half of a tree of at least
/// `REPAINT_MIN_NODES` nodes.
pub const REPAINT_FRAMES: u32 = 60;
pub const REPAINT_MIN_NODES: usize = 20;
/// Layer area in logical pixels, about a 2560 by 1600 surface.
pub const LAYER_AREA: f32 = 4_096_000.0;
/// Direct children of one container.
pub const HUGE_CHILDREN: usize = 500;
const CAPACITY: usize = 256;

/// Streaks the per-frame checks keep between frames.
#[derive(Default)]
pub(super) struct DiagnosticState {
    pub entries: Vec<FrameDiagnostic>,
    pub enabled: bool,
    relayout: u32,
    repaint: u32,
    /// The first node that made this frame's layout dirty.
    pub layout_cause: Option<NodeId>,
    pub bounds_changed: bool,
    /// Whether a misuse panics rather than being skipped and reported.
    pub misuse_panics: bool,
}

impl DiagnosticState {
    pub fn new() -> Self {
        Self {
            enabled: cfg!(debug_assertions),
            misuse_panics: cfg!(debug_assertions),
            ..Self::default()
        }
    }
}

impl Context {
    /// Framework warnings collected so far: unstable ids, needless relayout,
    /// excessive repainting, controls without an accessible name, oversized
    /// layers and huge unvirtualized containers. Collected in debug builds by
    /// default; see `set_diagnostics`.
    pub fn diagnostics(&self) -> &[FrameDiagnostic] {
        &self.diagnostic.entries
    }

    /// Turns the checks on or off. They are on in debug builds and off in
    /// release builds unless switched here.
    pub fn set_diagnostics(&mut self, enabled: bool) {
        self.diagnostic.enabled = enabled;
    }

    pub fn diagnostics_enabled(&self) -> bool {
        self.diagnostic.enabled
    }

    /// Reports a font family the host could not find. Hosts call this after
    /// a declaration, for the families `requested_families` names.
    pub fn report_font(&mut self, message: impl Into<String>) {
        self.diagnose(DiagnosticKind::Font, None, message.into());
    }

    /// Families a style named during the last declaration, for a host that
    /// can tell whether they are installed.
    pub fn requested_families(&self) -> Vec<String> {
        self.families.borrow().iter().cloned().collect()
    }

    /// Whether a misuse of the API panics, as it does in debug builds, or
    /// leaves the widget out and is reported as a
    /// [`DiagnosticKind::Misuse`] diagnostic, as in release builds.
    pub(crate) fn misuse_panics(&self) -> bool {
        self.diagnostic.misuse_panics
    }

    #[cfg(test)]
    pub(crate) fn set_misuse_panics(&mut self, panics: bool) {
        self.diagnostic.misuse_panics = panics;
    }

    /// Panics in a debug build; otherwise reports `message`, even with the
    /// other checks off, because what it describes was left out.
    pub(super) fn misuse(&mut self, node: Option<NodeId>, message: String) {
        assert!(!self.diagnostic.misuse_panics, "{message}");
        self.record(DiagnosticKind::Misuse, node, message);
    }

    pub fn clear_diagnostics(&mut self) {
        self.diagnostic.entries.clear();
    }

    fn node_path(&self, id: NodeId) -> String {
        let mut keys = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            let Some(node) = self.nodes.get(&id) else {
                break;
            };
            if node.parent.is_some() {
                keys.push(node.key.describe());
            }
            next = node.parent;
        }
        keys.reverse();
        keys.join("/")
    }

    /// A file that could not be used. Named rather than pointed at, because
    /// the node that asked for it is often long gone by the time the host
    /// answers.
    pub(super) fn diagnose_resource(&mut self, path: &std::path::Path, error: &str) {
        self.diagnose(
            DiagnosticKind::Resource,
            None,
            format!("{}: {error}", path.display()),
        );
    }

    pub(super) fn diagnose(&mut self, kind: DiagnosticKind, node: Option<NodeId>, message: String) {
        if self.diagnostic.enabled {
            self.record(kind, node, message);
        }
    }

    fn record(&mut self, kind: DiagnosticKind, node: Option<NodeId>, message: String) {
        let frame = self.frame;
        if let Some(entry) = self
            .diagnostic
            .entries
            .iter_mut()
            .find(|entry| entry.kind == kind && entry.node == node)
        {
            entry.count += 1;
            entry.frame = frame;
            entry.message = message;
            return;
        }
        if self.diagnostic.entries.len() >= CAPACITY {
            return;
        }
        let path = node.map_or_else(String::new, |id| self.node_path(id));
        tracing::warn!(target: "seaglass::diagnostics", ?kind, path, "{message}");
        self.diagnostic.entries.push(FrameDiagnostic {
            kind,
            node,
            path,
            message,
            count: 1,
            frame,
        });
    }

    /// After a parent's children were reconciled: wholesale replacement and size.
    pub(super) fn diagnose_children(&mut self, parent: NodeId, created: usize, removed: usize) {
        if !self.diagnostic.enabled {
            return;
        }
        let Some(node) = self.nodes.get_mut(&parent) else {
            return;
        };
        let count = node.children.len();
        // The same number of children leaving and arriving, again and again,
        // is what a key built from changing data looks like.
        node.churn = if created > 0 && created == removed {
            node.churn.saturating_add(1)
        } else {
            0
        };
        let churn = node.churn;
        let virtualized = node.kind == Kind::Collection;
        if churn == UNSTABLE_ID_FRAMES {
            self.diagnose(
                DiagnosticKind::UnstableIds,
                Some(parent),
                format!(
                    "{created} children were recreated in each of the last {UNSTABLE_ID_FRAMES} \
                     frames; their keys change every frame, so they lose focus, scroll and \
                     editing state. Key them by a stable identity."
                ),
            );
        }
        if count > HUGE_CHILDREN && !virtualized && created > 0 {
            self.diagnose(
                DiagnosticKind::HugeCollection,
                Some(parent),
                format!(
                    "{count} children are declared directly; every one is reconciled, laid out \
                     and kept in memory. Use a virtual list, tree or grid."
                ),
            );
        }
    }

    /// Controls touched by this declaration that assistive technology cannot name.
    pub(super) fn diagnose_accessibility(&mut self) {
        if !self.diagnostic.enabled {
            return;
        }
        let frame = self.frame;
        let unnamed: Vec<(NodeId, Role, bool)> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.changed_frame == frame && node.enabled)
            .filter_map(|(id, node)| {
                let semantics = node.semantics();
                let interactive = matches!(
                    semantics.role,
                    Role::Button
                        | Role::TextInput
                        | Role::Checkbox
                        | Role::Radio
                        | Role::Slider
                        | Role::ComboBox
                        | Role::MenuButton
                        | Role::Tab
                );
                let described = node
                    .tooltip
                    .as_ref()
                    .is_some_and(|text| !text.trim().is_empty());
                let empty = semantics.label.trim().is_empty();
                let symbols = !empty && !semantics.label.chars().any(char::is_alphanumeric);
                (interactive && !described && (empty || symbols)).then_some((
                    *id,
                    semantics.role,
                    empty,
                ))
            })
            .collect();
        for (id, role, empty) in unnamed {
            self.diagnose(
                DiagnosticKind::Inaccessible,
                Some(id),
                if empty {
                    format!("{role:?} has no accessible name; give it a label or a tooltip.")
                } else {
                    format!(
                        "{role:?} is named only by symbols, which screen readers skip or \
                         misread; add a tooltip that says what it does."
                    )
                },
            );
        }
    }

    /// After layout and bounds: did the pass change anything?
    pub(super) fn diagnose_layout(&mut self, ran: bool) {
        let state = &mut self.diagnostic;
        let cause = state.layout_cause.take();
        let moved = std::mem::take(&mut state.bounds_changed);
        if !state.enabled {
            return;
        }
        state.relayout = if ran && !moved { state.relayout + 1 } else { 0 };
        if state.relayout == RELAYOUT_FRAMES {
            self.diagnose(
                DiagnosticKind::Relayout,
                cause,
                format!(
                    "Layout ran in each of the last {RELAYOUT_FRAMES} frames without moving \
                     anything. A text or layout style here changes every frame but always \
                     measures the same; give it a fixed size or stop redeclaring it."
                ),
            );
        }
    }

    /// After paint: how much of the tree was repainted?
    pub(super) fn diagnose_paint(&mut self) {
        if !self.diagnostic.enabled {
            return;
        }
        let (painted, total) = (self.stats.paint_nodes, self.nodes.len());
        // A fling scrolls a page for a second or so, which repaints it every
        // frame; that is the framework's doing, and it ends by itself.
        let heavy = total >= REPAINT_MIN_NODES && painted * 2 > total && !self.flinging();
        self.diagnostic.repaint = if heavy {
            self.diagnostic.repaint + 1
        } else {
            0
        };
        if self.diagnostic.repaint == REPAINT_FRAMES {
            self.diagnose(
                DiagnosticKind::Repaint,
                None,
                format!(
                    "{painted} of {total} nodes repainted in each of the last {REPAINT_FRAMES} \
                     frames. Something near the root changes every frame (a style, a theme \
                     token, scroll or bounds) and invalidates the paint caches below it."
                ),
            );
        }
    }

    pub(super) fn diagnose_layer(&mut self, id: NodeId, bounds: Rect) {
        let area = bounds.width * bounds.height;
        if area > LAYER_AREA {
            self.diagnose(
                DiagnosticKind::OversizedLayer,
                Some(id),
                format!(
                    "A {:.0} by {:.0} layer is composited offscreen for opacity or a transform; \
                     that is {:.1} megapixels of texture. Animate a smaller subtree.",
                    bounds.width,
                    bounds.height,
                    area / 1.0e6
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Context, DiagnosticKind, MonospaceMeasurer, PopoverOptions, Ui};
    use seaglass_scene::Size;

    /// A context that treats misuse as a release build does, with the other
    /// checks off to show misuse is reported regardless.
    fn release() -> Context {
        let mut context = Context::new();
        context.set_misuse_panics(false);
        context.set_diagnostics(false);
        context
    }

    fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) {
        context.run(Size::new(800.0, 600.0), &mut MonospaceMeasurer, view);
    }

    fn labels(context: &Context) -> Vec<String> {
        context
            .snapshots()
            .into_iter()
            .map(|node| node.semantics.label)
            .filter(|label| !label.is_empty())
            .collect()
    }

    fn misuses(context: &Context) -> Vec<String> {
        context
            .diagnostics()
            .iter()
            .filter(|entry| entry.kind == DiagnosticKind::Misuse)
            .map(|entry| entry.message.clone())
            .collect()
    }

    #[test]
    fn a_signal_written_while_a_view_declares_is_a_misuse_and_is_not_written() {
        let mut context = release();
        let store = crate::Store::new();
        let signal = store.signal("count", 0);
        frame(&mut context, |ui| {
            signal.set(1);
            ui.label(format!("count {}", signal.get()));
        });
        assert_eq!(signal.get(), 0, "a refused write changed the value");
        let misuses = misuses(&context);
        assert_eq!(misuses.len(), 1, "{misuses:?}");
        assert!(misuses[0].contains("`count`"), "{misuses:?}");
        // Outside a declaration the same write goes ahead.
        signal.set(2);
        assert_eq!(signal.get(), 2);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "was written while a view was declaring")]
    fn a_signal_written_while_a_view_declares_panics_in_a_debug_build() {
        let mut context = Context::new();
        let store = crate::Store::new();
        let signal = store.signal("count", 0);
        frame(&mut context, |_| signal.set(1));
    }

    #[test]
    fn a_duplicate_key_leaves_the_second_widget_out_and_reports_it() {
        let mut context = release();
        frame(&mut context, |ui| {
            ui.keyed("same", |ui| ui.label("first"));
            ui.keyed("same", |ui| ui.label("second"));
            // Would have landed on "first" had the skip not been tracked.
            ui.tooltip("help");
            ui.label("after");
        });
        assert_eq!(labels(&context), ["first", "after"]);
        let first = context.nodes.values().find(|node| node.text == "first");
        assert_eq!(first.unwrap().tooltip, None);
        let reported = misuses(&context);
        assert_eq!(reported.len(), 1);
        assert!(reported[0].contains("duplicate widget key"), "{reported:?}");
    }

    #[test]
    fn a_popover_without_an_anchor_is_left_out() {
        let mut context = release();
        frame(&mut context, |ui| {
            ui.popover(
                true,
                PopoverOptions::default(),
                |_| (),
                |ui| ui.label("inside"),
            );
            ui.label("after");
        });
        assert_eq!(labels(&context), ["after"]);
        assert!(misuses(&context)[0].contains("popover needs a preceding anchor"));
    }

    #[test]
    fn a_slider_over_an_empty_range_is_left_out() {
        let mut context = release();
        frame(&mut context, |ui| {
            ui.slider("volume", 0.0, 1.0..=1.0, 0.1, |_| ());
            ui.label("after");
        });
        assert_eq!(labels(&context), ["after"]);
        assert!(misuses(&context)[0].contains("slider requires"));
    }
}
