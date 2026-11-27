use super::*;
use alloc::vec::Vec;

/// A small flag at the top of a caret in mixed-direction text, pointing the
/// way the text at the caret runs.
pub(super) fn caret_flag(scene: &mut Scene, caret: Rect, rtl: Option<bool>, color: Color) {
    let Some(rtl) = rtl else {
        return;
    };
    scene.primitives.push(Primitive::Quad {
        rect: Rect {
            x: if rtl {
                caret.x - 4.0
            } else {
                caret.x + caret.width
            },
            y: caret.y,
            width: 4.0,
            height: 2.0,
        },
        color,
        radius: 0.0,
    });
}

impl Context {
    /// How far a node's first baseline sits below its top edge, when it holds
    /// text at all. A container answers with its first child that does.
    fn baseline_of(&self, id: NodeId, measurer: &mut impl TextMeasurer) -> Option<f32> {
        let node = self.nodes.get(&id)?;
        if node.kind.text_leaf() && !node.text.is_empty() {
            let padding = node.style.padding.max(0.0);
            return Some(padding + measurer.ascent(&node.text_style()));
        }
        node.children.iter().find_map(|child| {
            let inside = self.nodes.get(child)?.bounds.y - node.bounds.y;
            Some(inside + self.baseline_of(*child, measurer)?)
        })
    }

    /// `by` is never negative: hit tests rely on a clip not moving up.
    fn shift(&mut self, id: NodeId, by: f32) {
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        node.bounds.y += by;
        node.clip.y += by;
        node.paint_cache = None;
        for child in self.nodes[&id].children.clone() {
            self.shift(child, by);
        }
    }

    /// Lines the children of a row up by the baseline of their text rather
    /// than by their boxes, which is what makes labels of different sizes sit
    /// on one line.
    fn align_baselines(&mut self, rows: &[NodeId], measurer: &mut impl TextMeasurer) {
        for &row in rows {
            let children = self.nodes[&row].children.clone();
            let baselines: Vec<Option<f32>> = children
                .iter()
                .map(|child| {
                    let inside = self.nodes[child].bounds.y - self.nodes[&row].bounds.y;
                    self.baseline_of(*child, measurer)
                        .map(|baseline| inside + baseline)
                })
                .collect();
            let Some(lowest) = baselines.iter().flatten().copied().reduce(f32::max) else {
                continue;
            };
            for (child, baseline) in children.iter().zip(&baselines) {
                let Some(baseline) = baseline else {
                    continue;
                };
                let by = lowest - baseline;
                if by.abs() > 0.01 {
                    self.shift(*child, by);
                    self.pending |= Dirty::PAINT;
                }
            }
        }
    }

    /// In debug builds, holds the lists kept from an earlier frame to what a
    /// walk over the nodes finds now. It allocates nothing.
    #[cfg(debug_assertions)]
    fn check_found(&self, found: &Preparing) {
        let frame = self.frame;
        let (mut rows, mut areas, mut texts, mut rich, mut editors, mut ellipsized) =
            (0, 0, 0, 0, 0, 0);
        for (id, node) in &self.nodes {
            assert!(
                node.changed_frame != frame,
                "a node changed without a declaration"
            );
            if node.style.align_items == crate::Align::Baseline && node.children.len() > 1 {
                assert_eq!(found.baseline_rows.get(rows), Some(id));
                rows += 1;
            }
            match node.kind {
                Kind::TextArea => {
                    assert_eq!(found.text_areas.get(areas), Some(id));
                    areas += 1;
                }
                Kind::RichText => {
                    assert_eq!(found.rich_texts.get(texts), Some(id));
                    texts += 1;
                }
                Kind::RichEditor => {
                    assert_eq!(found.rich_editors.get(rich), Some(id));
                    rich += 1;
                }
                _ => {}
            }
            if node.editor.is_some() {
                assert_eq!(found.editors.get(editors), Some(id));
                editors += 1;
            }
            if node.ellipsizes() {
                assert_eq!(found.ellipsized.get(ellipsized), Some(id));
                ellipsized += 1;
            }
        }
        assert_eq!(
            [rows, areas, texts, rich, editors, ellipsized],
            [
                found.baseline_rows.len(),
                found.text_areas.len(),
                found.rich_texts.len(),
                found.rich_editors.len(),
                found.editors.len(),
                found.ellipsized.len(),
            ],
            "the kept lists are out of date"
        );
    }

    #[cfg(not(debug_assertions))]
    fn check_found(&self, _found: &Preparing) {}

    pub(super) fn prepare_controls(&mut self, measurer: &mut impl TextMeasurer) {
        // One walk over the nodes finds everything that needs preparing.
        // Each kind used to walk them all itself, which was seven walks a
        // frame whether anything had changed or not.
        let frame = self.frame;
        let mut found = core::mem::take(&mut self.preparing);
        if found.valid {
            // Nodes come, go and change kind, style, children or editor only
            // while a declaration reconciles, and a declaration starts by
            // dropping the lists. What the last walk found is therefore still
            // what a walk finds, in the same order. Nothing is marked changed
            // in a frame that did not declare, so no label or hint is due.
            found.labelled.clear();
            found.hinted.clear();
            self.check_found(&found);
        } else {
            found.clear();
            for (id, node) in &self.nodes {
                if node.style.align_items == crate::Align::Baseline && node.children.len() > 1 {
                    found.baseline_rows.push(*id);
                }
                match node.kind {
                    Kind::TextArea => found.text_areas.push(*id),
                    Kind::RichText => found.rich_texts.push(*id),
                    Kind::RichEditor => found.rich_editors.push(*id),
                    Kind::Slider | Kind::RangeSlider
                        if node.changed_frame == frame && node.resolved.rtl() =>
                    {
                        found.labelled.push(*id);
                    }
                    _ => {}
                }
                if node.changed_frame == frame
                    && matches!(&node.control,
                        Control::Choice { hint, .. }
                        | Control::Submenu { hint, .. }
                        | Control::Hint(hint) if !hint.is_empty())
                {
                    found.hinted.push(*id);
                }
                if node.editor.is_some() {
                    found.editors.push(*id);
                }
                if node.ellipsizes() {
                    found.ellipsized.push(*id);
                }
            }
        }
        self.align_baselines(&found.baseline_rows, measurer);
        self.prepare_text_areas(&found.text_areas, measurer);
        for id in &found.labelled {
            let node = self.nodes.get_mut(id).unwrap();
            node.label_width = measurer
                .measure_styled(&node.text, &node.text_style(), None)
                .width;
        }
        for id in &found.hinted {
            let node = &self.nodes[id];
            let (Control::Choice { hint, .. }
            | Control::Submenu { hint, .. }
            | Control::Hint(hint)) = &node.control
            else {
                continue;
            };
            let width = measurer
                .measure_styled(hint, &node.text_style(), None)
                .width;
            self.nodes.get_mut(id).unwrap().hint_width = width;
        }
        self.prepare_ellipsis(&found.ellipsized, measurer);
        self.prepare_rich_text(&found.rich_texts, measurer);
        self.prepare_rich_editors(&found.rich_editors, measurer);
        for id in &found.editors {
            let Some(node) = self.nodes.get_mut(id) else {
                continue;
            };
            let width = (node.bounds.width - node.style.padding.max(0.0) * 2.0).max(1.0);
            let text_style = node.text_style();
            // A forced paragraph direction aligns the text as it reads.
            let rtl = text_style.base.unwrap_or(node.resolved.rtl());
            let reflowed = node
                .editor
                .as_mut()
                .is_some_and(|editor| editor.prepare(measurer, text_style, width, rtl));
            if reflowed {
                self.invalidate_paint(*id);
            }
        }
        // The lists keep their room for the next frame, and stand for the
        // nodes until the next declaration.
        found.valid = true;
        self.preparing = found;
        if let Some(popup) = self.popup
            && popup.reveal_highlight
            && let Some(panel) = self
                .nodes
                .get(&popup.owner)
                .and_then(|node| node.children.first())
            && let Some(choice) = self.nodes[panel].children.get(popup.highlighted).copied()
        {
            self.scroll_into_view(choice);
            self.popup.as_mut().unwrap().reveal_highlight = false;
        }
    }

    /// A busy indicator draws what the clock says, so it asks for the next
    /// frame itself; only its own box is damaged by it. It stands still
    /// while motion is off, and while nobody can see it.
    pub(super) fn keep_busy(&mut self, id: NodeId) {
        let node = &self.nodes[&id];
        if node.busy() && !self.reduced_motion && node.clip.width > 0.0 && node.clip.height > 0.0 {
            self.animating_nodes.push(id);
        }
    }

    /// Where a busy indicator is in its cycle, from 0 up to but not
    /// including 1; fixed at the start while motion is off.
    fn busy_phase(&self, period: Duration) -> f32 {
        if self.reduced_motion {
            return 0.0;
        }
        let elapsed = self.now.as_nanos() % period.as_nanos();
        (elapsed as f64 / period.as_nanos() as f64) as f32
    }

    pub(super) fn paint_control(
        &self,
        id: NodeId,
        shown: crate::animation::Shown,
        scene: &mut Scene,
    ) {
        let node = &self.nodes[&id];
        if node.kind == Kind::TextArea {
            self.paint_text_area(id, scene);
        }
        if node.kind == Kind::RichEditor {
            self.paint_rich_editor(id, scene);
        }
        if self.drag.as_ref().is_some_and(|drag| drag.active && (drag.over == Some(id) || (self.focused == Some(id) && matches!(node.control, Control::DropTarget { accepts, .. } if accepts == drag.payload.kind())))) {
            for rect in [Rect { height: 2.0, ..node.bounds }, Rect { y: node.bounds.y + node.bounds.height - 2.0, height: 2.0, ..node.bounds }] {
                scene.primitives.push(Primitive::Quad { rect, color: self.theme.accent, radius: 0.0 });
            }
        }
        if let Some(editor) = node.editor.as_ref().filter(|_| !node.read_only()) {
            let content = node.content_box();
            let origin = Point {
                x: content.x - editor.scroll_x + editor.align_x,
                y: content.y,
            };
            scene.primitives.push(Primitive::PushClip(content));
            if !editor.composing() {
                for rect in editor
                    .geometry
                    .selection(editor.shown_range(editor.selection.range()))
                {
                    scene.primitives.push(Primitive::Quad {
                        rect: rect.translated(origin),
                        color: if self.focused == Some(id) {
                            self.theme.highlight
                        } else {
                            self.theme.selection_inactive
                        },
                        radius: 0.0,
                    });
                }
            }
            let display = editor.shown();
            let placeholder = if let Control::TextInput { placeholder, .. } = &node.control {
                placeholder.as_str()
            } else {
                ""
            };
            scene.primitives.push(Primitive::StyledText {
                rect: Rect {
                    x: origin.x,
                    y: origin.y,
                    width: editor.geometry.size.width.max(content.width) + 2.0,
                    height: content.height,
                },
                text: if display.is_empty() {
                    placeholder.into()
                } else {
                    display.clone()
                },
                style: TextStyle {
                    wrap: TextWrap::None,
                    ..node.text_style()
                },
                color: if display.is_empty() || !node.enabled {
                    self.theme.text_muted
                } else {
                    node.resolved.foreground
                },
            });
            if let Some(range) = editor.composition_range() {
                for rect in editor.geometry.selection(editor.shown_range(range)) {
                    scene.primitives.push(Primitive::Quad {
                        rect: Rect {
                            y: rect.y + rect.height - 2.0,
                            height: 1.0,
                            ..rect
                        }
                        .translated(origin),
                        color: self.theme.accent,
                        radius: 0.0,
                    });
                }
            }
            if self.focused == Some(id)
                && node.enabled
                && let Some(caret) = editor.caret()
            {
                scene.primitives.push(Primitive::Quad {
                    rect: caret.translated(origin),
                    color: self.theme.text,
                    radius: 0.0,
                });
                caret_flag(
                    scene,
                    caret.translated(origin),
                    editor.caret_direction(),
                    self.theme.text,
                );
            }
            scene.primitives.push(Primitive::PopClip);
        }
        match node.control {
            Control::Progress { value, .. } => {
                self.paint_progress(node, value, shown.radius, scene)
            }
            Control::Spinner { .. } => self.paint_spinner(node, scene),
            Control::Switch { .. } => self.paint_switch(node, shown.knob, scene),
            Control::Checkbox { checked, .. } => self.paint_mark(node, false, checked, scene),
            Control::Radio { selected, .. } => self.paint_mark(node, true, selected, scene),
            Control::RangeSlider { .. } => self.paint_range_track(node, scene),
            Control::Disclosure { open, .. } => {
                self.paint_chevron(node, open, shown.foreground, scene)
            }
            _ => {}
        }
        if let Control::Slider {
            value,
            min,
            max,
            span,
            ..
        } = node.control
        {
            let (low, high) = span.unwrap_or((min, max));
            let track = slider_track(node.bounds);
            let fraction = ((value - low) / (high - low)) as f32;
            let rtl = node.resolved.rtl();
            let thumb = if rtl { 1.0 - fraction } else { fraction };
            // The thumbs of a range slider sit on the track that the control
            // holding them draws.
            if span.is_none() {
                scene.primitives.push(Primitive::Quad {
                    rect: track,
                    color: self.theme.track,
                    radius: 2.0,
                });
                scene.primitives.push(Primitive::Quad {
                    rect: Rect {
                        x: if rtl {
                            track.x + track.width * thumb
                        } else {
                            track.x
                        },
                        width: track.width * fraction,
                        ..track
                    },
                    color: self.theme.accent,
                    radius: 2.0,
                });
            } else if self.focused == Some(id) && self.quiet_focus != Some(id) {
                scene.primitives.push(Primitive::Quad {
                    rect: Rect {
                        x: track.x + track.width * thumb - 9.0,
                        y: track.y - 7.0,
                        width: 18.0,
                        height: 18.0,
                    },
                    color: self.theme.focus,
                    radius: 9.0,
                });
            }
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: track.x + track.width * thumb - 7.0,
                    y: track.y - 5.0,
                    width: 14.0,
                    height: 14.0,
                },
                color: if node.enabled {
                    self.theme.thumb
                } else {
                    self.theme.text_muted
                },
                radius: 7.0,
            });
        }
    }

    /// A range slider's track, with the part between its thumbs filled. The
    /// thumbs are its children, so they are drawn over it.
    fn paint_range_track(&self, node: &Node, scene: &mut Scene) {
        let thumbs: Vec<(f64, (f64, f64))> = node
            .children
            .iter()
            .filter_map(|child| match self.nodes[child].control {
                Control::Slider {
                    value,
                    span: Some(span),
                    ..
                } => Some((value, span)),
                _ => None,
            })
            .collect();
        let [(low, span), (high, _)] = thumbs[..] else {
            return;
        };
        let track = slider_track(node.bounds);
        let at = |value: f64| ((value - span.0) / (span.1 - span.0)) as f32;
        let (from, to) = if node.resolved.rtl() {
            (1.0 - at(high), 1.0 - at(low))
        } else {
            (at(low), at(high))
        };
        scene.primitives.push(Primitive::Quad {
            rect: track,
            color: self.theme.track,
            radius: 2.0,
        });
        scene.primitives.push(Primitive::Quad {
            rect: Rect {
                x: track.x + track.width * from,
                width: track.width * (to - from),
                ..track
            },
            color: self.theme.accent,
            radius: 2.0,
        });
    }

    /// The part of a progress bar that is done, or the segment that sweeps
    /// along an indeterminate one. The bar's background is its track.
    fn paint_progress(&self, node: &Node, value: Option<f64>, radius: f32, scene: &mut Scene) {
        let bounds = node.bounds;
        let (offset, width) = match value {
            Some(fraction) => (0.0, bounds.width * fraction as f32),
            None => {
                let segment = bounds.width * 0.4;
                // Out of the box at one end and in at the other, easing
                // at both so that it does not look mechanical.
                let phase = self.busy_phase(Duration::from_millis(1400));
                let eased = if self.reduced_motion {
                    0.5
                } else {
                    phase * phase * (3.0 - 2.0 * phase)
                };
                ((bounds.width + segment) * eased - segment, segment)
            }
        };
        if width <= 0.0 {
            return;
        }
        let x = if node.resolved.rtl() {
            bounds.x + bounds.width - offset - width
        } else {
            bounds.x + offset
        };
        scene.primitives.push(Primitive::Quad {
            rect: Rect { x, width, ..bounds },
            color: self.busy_color(node),
            radius: radius.min(bounds.height / 2.0).min(width / 2.0),
        });
    }

    /// A pill and a thumb at the start edge, the thumb moving to the other
    /// end as `knob` goes from 0 to 1, in both directions of writing.
    fn paint_switch(&self, node: &Node, knob: f32, scene: &mut Scene) {
        let (width, height) = crate::ui::SWITCH_TRACK;
        let content = node.content_box();
        let rtl = node.resolved.rtl();
        let track = Rect {
            x: if rtl {
                content.x + content.width - width
            } else {
                content.x
            },
            y: content.y + (content.height - height) / 2.0,
            width,
            height,
        };
        let (off, on, thumb) = if node.enabled {
            (self.theme.track, self.theme.accent, self.theme.thumb)
        } else {
            (
                self.theme.control_disabled,
                self.theme.text_disabled,
                self.theme.text_muted,
            )
        };
        scene.primitives.push(Primitive::Quad {
            rect: track,
            color: crate::animation::lerp_color(off, on, knob),
            radius: height / 2.0,
        });
        let inset = 3.0;
        let diameter = height - 2.0 * inset;
        let along = if rtl { 1.0 - knob } else { knob };
        scene.primitives.push(Primitive::Quad {
            rect: Rect {
                x: track.x + inset + (width - height) * along,
                y: track.y + inset,
                width: diameter,
                height: diameter,
            },
            color: thumb,
            radius: diameter / 2.0,
        });
    }

    /// The box of a checkbox or the ring of a radio button at the start
    /// edge, drawn rather than taken from a font, so that it is the same
    /// size set and unset and the same on every platform and renderer.
    fn paint_mark(&self, node: &Node, round: bool, set: bool, scene: &mut Scene) {
        let side = crate::ui::MARK;
        let content = node.content_box();
        let rect = Rect {
            x: if node.resolved.rtl() {
                content.x + content.width - side
            } else {
                content.x
            },
            y: content.y + (content.height - side) / 2.0,
            width: side,
            height: side,
        };
        let (rim, fill, tick) = if node.enabled {
            (self.theme.text_muted, self.theme.accent, self.theme.window)
        } else {
            (
                self.theme.text_disabled,
                self.theme.text_disabled,
                self.theme.control_disabled,
            )
        };
        let radius = if round { side / 2.0 } else { 4.0 };
        if set && !round {
            scene.primitives.push(Primitive::Quad {
                rect,
                color: fill,
                radius,
            });
            let point = |dx: f32, dy: f32| glassmere_scene::Point {
                x: rect.x + side * dx,
                y: rect.y + side * dy,
            };
            let mut path = glassmere_scene::Path::new();
            path.move_to(point(0.24, 0.52))
                .line_to(point(0.43, 0.70))
                .line_to(point(0.77, 0.31));
            scene.primitives.push(Primitive::Path {
                path,
                fill: None,
                stroke: Some(glassmere_scene::Stroke {
                    width: 2.0,
                    color: tick,
                }),
            });
            return;
        }
        // A rim of one and a half pixels: the shape, and the same shape
        // inside it in whatever is behind the control.
        let rim_width = 1.5;
        scene.primitives.push(Primitive::Path {
            path: rounded(
                Rect {
                    x: rect.x + rim_width / 2.0,
                    y: rect.y + rim_width / 2.0,
                    width: side - rim_width,
                    height: side - rim_width,
                },
                radius - rim_width / 2.0,
            ),
            fill: None,
            stroke: Some(glassmere_scene::Stroke {
                width: rim_width,
                color: if set { fill } else { rim },
            }),
        });
        if set {
            let dot = side * 0.5;
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: rect.x + (side - dot) / 2.0,
                    y: rect.y + (side - dot) / 2.0,
                    width: dot,
                    height: dot,
                },
                color: fill,
                radius: dot / 2.0,
            });
        }
    }

    /// A chevron at the start edge: pointing along the line when the section
    /// is closed, which is toward its end, and down when it is open.
    fn paint_chevron(&self, node: &Node, open: bool, color: Color, scene: &mut Scene) {
        let content = node.content_box();
        let rtl = node.resolved.rtl();
        let x = if rtl {
            content.x + content.width - 6.0
        } else {
            content.x + 6.0
        };
        let y = content.y + content.height / 2.0;
        let toward_end = if rtl { -1.0 } else { 1.0 };
        let point = |dx: f32, dy: f32| glassmere_scene::Point {
            x: x + dx,
            y: y + dy,
        };
        let mut path = glassmere_scene::Path::new();
        if open {
            path.move_to(point(-4.5, -2.0))
                .line_to(point(0.0, 2.5))
                .line_to(point(4.5, -2.0));
        } else {
            path.move_to(point(-2.0 * toward_end, -4.5))
                .line_to(point(2.5 * toward_end, 0.0))
                .line_to(point(-2.0 * toward_end, 4.5));
        }
        scene.primitives.push(Primitive::Path {
            path,
            fill: None,
            stroke: Some(glassmere_scene::Stroke { width: 1.8, color }),
        });
    }

    /// Dots around a circle, the newest at full strength and each one before
    /// it fainter, so that the trail turns clockwise. It is not mirrored in
    /// right-to-left text: a clock is not.
    fn paint_spinner(&self, node: &Node, scene: &mut Scene) {
        const DOTS: usize = 10;
        let bounds = node.bounds;
        let diameter = bounds.width.min(bounds.height);
        let dot = (diameter / 6.0).max(1.0);
        let orbit = (diameter - dot) / 2.0;
        let (center_x, center_y) = (
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        let color = self.busy_color(node);
        let head = self.busy_phase(Duration::from_millis(900)) * DOTS as f32;
        for index in 0..DOTS {
            let angle = index as f32 / DOTS as f32 * core::f32::consts::TAU;
            let behind = (head - index as f32).rem_euclid(DOTS as f32) / DOTS as f32;
            let strength = 1.0 - 0.85 * behind;
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: center_x + orbit * angle.sin() - dot / 2.0,
                    y: center_y - orbit * angle.cos() - dot / 2.0,
                    width: dot,
                    height: dot,
                },
                color: Color(
                    color.0,
                    color.1,
                    color.2,
                    (f32::from(color.3) * strength) as u8,
                ),
                radius: dot / 2.0,
            });
        }
    }

    /// What a busy indicator is drawn in: the accent, unless its style says.
    fn busy_color(&self, node: &Node) -> Color {
        if node.enabled {
            node.style.foreground.unwrap_or(self.theme.accent)
        } else {
            self.theme.text_disabled
        }
    }

    /// Logical candidate-window anchor, clamped to the active input's content box.
    pub fn ime_cursor_rect(&self) -> Option<Rect> {
        if let Some(caret) = self.area_caret().or_else(|| self.rich_caret()) {
            return Some(caret);
        }
        let node = self.nodes.get(&self.focused?)?;
        let editor = node.editor.as_ref().filter(|_| !node.read_only())?;
        let caret = editor.caret()?;
        let padding = node.style.padding.max(0.0);
        Some(
            caret
                .translated(Point {
                    x: node.bounds.x + padding - editor.scroll_x + editor.align_x,
                    y: node.bounds.y + padding,
                })
                .intersect(node.clip),
        )
    }

    pub fn accepts_text(&self) -> bool {
        self.area_accepts_text()
            || self.rich_accepts_text()
            || self
                .focused
                .and_then(|id| self.nodes.get(&id))
                .is_some_and(|node| node.enabled && node.editor.is_some() && !node.read_only())
    }

    pub fn composing_text(&self) -> bool {
        self.area_composing()
            || self.rich_composing()
            || self
                .focused
                .and_then(|id| self.nodes.get(&id))
                .and_then(|node| node.editor.as_ref())
                .is_some_and(|editor| editor.composing())
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.nodes
            .get(&self.focused?)?
            .editor
            .as_ref()
            .and_then(|editor| editor.selected_text())
    }
}

/// Where a slider's track runs: along the bottom of its box, inset to leave
/// room for the thumb at either end.
pub(super) fn slider_track(bounds: Rect) -> Rect {
    Rect {
        x: bounds.x + 12.0,
        y: bounds.y + bounds.height - 17.0,
        width: (bounds.width - 24.0).max(1.0),
        height: 4.0,
    }
}

/// The outline of a rectangle with round corners; a circle when `radius` is
/// half its side.
fn rounded(rect: Rect, radius: f32) -> glassmere_scene::Path {
    // How far a cubic's handles reach to follow a quarter circle.
    const ARC: f32 = 0.552_284_8;
    let r = radius.min(rect.width / 2.0).min(rect.height / 2.0);
    let k = r * (1.0 - ARC);
    let (left, top) = (rect.x, rect.y);
    let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
    let point = |x: f32, y: f32| glassmere_scene::Point { x, y };
    let mut path = glassmere_scene::Path::new();
    path.move_to(point(left + r, top))
        .line_to(point(right - r, top))
        .cubic_to(
            point(right - k, top),
            point(right, top + k),
            point(right, top + r),
        )
        .line_to(point(right, bottom - r))
        .cubic_to(
            point(right, bottom - k),
            point(right - k, bottom),
            point(right - r, bottom),
        )
        .line_to(point(left + r, bottom))
        .cubic_to(
            point(left + k, bottom),
            point(left, bottom - k),
            point(left, bottom - r),
        )
        .line_to(point(left, top + r))
        .cubic_to(
            point(left, top + k),
            point(left + k, top),
            point(left + r, top),
        )
        .close();
    path
}

/// The kept lists are only worth anything if a frame without a declaration
/// uses them, and the check of them is what shows that they are used.
#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    use crate::{MonospaceMeasurer, TextInput};

    const WINDOW: Size = Size::new(320.0, 200.0);

    #[test]
    #[should_panic(expected = "the kept lists are out of date")]
    fn a_frame_without_a_declaration_holds_the_kept_lists_to_the_nodes() {
        let mut context = Context::new();
        context.run(WINDOW, &mut MonospaceMeasurer, |ui| {
            ui.keyed("name", |ui| {
                ui.text_input(TextInput::new("Name"), "Ada", |_| ());
            });
        });
        assert!(context.preparing.valid);
        let root = context.root();
        context.preparing.editors.push(root);
        context.refresh(WINDOW, &mut MonospaceMeasurer);
    }
}
