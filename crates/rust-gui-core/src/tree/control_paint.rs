use super::*;

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
    pub(super) fn prepare_controls(&mut self, measurer: &mut impl TextMeasurer) {
        self.prepare_text_areas(measurer);
        self.prepare_rich_text(measurer);
        self.prepare_rich_editors(measurer);
        let ids: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| node.editor.is_some().then_some(*id))
            .collect();
        for id in ids {
            let node = self.nodes.get_mut(&id).unwrap();
            let width = (node.bounds.width - node.style.padding.max(0.0) * 2.0).max(1.0);
            let text_style = node.text_style();
            if node
                .editor
                .as_mut()
                .unwrap()
                .prepare(measurer, text_style, width)
            {
                self.invalidate_paint(id);
            }
        }
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

    pub(super) fn paint_control(&self, id: NodeId, scene: &mut Scene) {
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
        if let Some(editor) = &node.editor {
            let padding = node.style.padding.max(0.0);
            let content = Rect {
                x: node.bounds.x + padding,
                y: node.bounds.y + padding,
                width: (node.bounds.width - padding * 2.0).max(0.0),
                height: (node.bounds.height - padding * 2.0).max(0.0),
            };
            let origin = Point {
                x: content.x - editor.scroll_x,
                y: content.y,
            };
            scene.primitives.push(Primitive::PushClip(content));
            if !editor.composing() {
                for rect in editor.geometry.selection(editor.selection.range()) {
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
            let display = editor.display();
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
                for rect in editor.geometry.selection(range) {
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
        if let Control::Slider {
            value, min, max, ..
        } = node.control
        {
            let track = Rect {
                x: node.bounds.x + 12.0,
                y: node.bounds.y + node.bounds.height - 17.0,
                width: (node.bounds.width - 24.0).max(1.0),
                height: 4.0,
            };
            let fraction = ((value - min) / (max - min)) as f32;
            scene.primitives.push(Primitive::Quad {
                rect: track,
                color: self.theme.track,
                radius: 2.0,
            });
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    width: track.width * fraction,
                    ..track
                },
                color: self.theme.accent,
                radius: 2.0,
            });
            scene.primitives.push(Primitive::Quad {
                rect: Rect {
                    x: track.x + track.width * fraction - 7.0,
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

    /// Logical candidate-window anchor, clamped to the active input's content box.
    pub fn ime_cursor_rect(&self) -> Option<Rect> {
        if let Some(caret) = self.area_caret().or_else(|| self.rich_caret()) {
            return Some(caret);
        }
        let node = self.nodes.get(&self.focused?)?;
        let editor = node.editor.as_ref()?;
        let caret = editor.caret()?;
        let padding = node.style.padding.max(0.0);
        Some(
            caret
                .translated(Point {
                    x: node.bounds.x + padding - editor.scroll_x,
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
                .is_some_and(|node| node.enabled && node.editor.is_some())
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
            .map(|editor| editor.selected_text())
    }
}
