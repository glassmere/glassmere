use accesskit::{
    Action, ActionData, ActionRequest, Node, NodeId, Rect, Role, ScrollUnit, TextDirection,
    TextPosition, TextSelection, TreeId, TreeInfo, TreeUpdate,
};
use glassmere_core::{
    Action as CoreAction, Context, EditorPoint, EditorText, NodeId as CoreNodeId, Role as CoreRole,
};
use std::ops::Range;

/// Text runs take ids no node of the tree has: the top bit, the editor they
/// belong to and their number in it.
const RUN_ID: u64 = 1 << 63;
const RUN_BITS: u32 = 24;

/// The platform tree indexes the characters and word starts of a run in a
/// byte, so a longer line is cut into several runs.
const RUN_CHARACTERS: usize = 255;

/// A stretch of the characters of one line of an editor.
struct Run {
    line: usize,
    characters: Range<usize>,
}

fn runs(text: &EditorText) -> Vec<Run> {
    let mut runs = Vec::new();
    for (line, entry) in text.lines.iter().enumerate() {
        let count = entry.characters.len();
        let mut from = 0;
        loop {
            let to = (from + RUN_CHARACTERS).min(count);
            runs.push(Run {
                line,
                characters: from..to,
            });
            if to == count {
                break;
            }
            from = to;
        }
    }
    runs
}

fn run_id(owner: CoreNodeId, index: usize) -> NodeId {
    NodeId(RUN_ID | owner.0 << RUN_BITS | index as u64)
}

/// Where the run nodes of `owner` are, as the platform expects a selection.
fn text_position(
    owner: CoreNodeId,
    text: &EditorText,
    runs: &[Run],
    point: EditorPoint,
) -> Option<TextPosition> {
    let of_paragraph = |line: &&glassmere_core::EditorLine| line.paragraph == point.paragraph;
    let line = text
        .lines
        .iter()
        .rposition(|line| of_paragraph(&line) && line.start <= point.byte)
        .or_else(|| text.lines.iter().rposition(|line| of_paragraph(&line)))?;
    let entry = &text.lines[line];
    let offset = point.byte.saturating_sub(entry.start);
    let (mut index, mut at) = (0, 0);
    for character in &entry.characters {
        if at >= offset {
            break;
        }
        at += character.len;
        index += 1;
    }
    let count = entry.characters.len();
    let run = runs.iter().position(|run| {
        run.line == line
            && index >= run.characters.start
            && (index < run.characters.end || run.characters.end == count)
    })?;
    Some(TextPosition {
        node: run_id(owner, run),
        character_index: index - runs[run].characters.start,
    })
}

/// The place in the editor a position of the platform stands for.
fn editor_point(
    owner: CoreNodeId,
    text: &EditorText,
    runs: &[Run],
    position: TextPosition,
) -> Option<EditorPoint> {
    let id = position.node.0;
    if id & RUN_ID == 0 || (id & !RUN_ID) >> RUN_BITS != owner.0 {
        return None;
    }
    let run = runs.get((id & ((1 << RUN_BITS) - 1)) as usize)?;
    let entry = &text.lines[run.line];
    let index = (run.characters.start + position.character_index).min(entry.characters.len());
    Some(EditorPoint {
        paragraph: entry.paragraph,
        byte: entry.start
            + entry.characters[..index]
                .iter()
                .map(|character| character.len)
                .sum::<usize>(),
    })
}

/// The text run nodes of an editor and the selection in them.
fn text_runs(
    owner: CoreNodeId,
    text: &EditorText,
    scale: f64,
) -> (Vec<(NodeId, Node)>, Option<TextSelection>) {
    let runs = runs(text);
    let nodes = runs
        .iter()
        .enumerate()
        .map(|(index, run)| {
            let entry = &text.lines[run.line];
            let characters = &entry.characters[run.characters.clone()];
            // Byte offset of each character of the line, and the end.
            let mut starts = Vec::with_capacity(entry.characters.len() + 1);
            let mut at = 0;
            for character in &entry.characters {
                starts.push(at);
                at += character.len;
            }
            starts.push(at);
            let bytes = starts[run.characters.start]..starts[run.characters.end];
            let mut node = Node::new(Role::TextRun);
            node.set_value(&entry.text[bytes.clone()]);
            node.set_character_lengths(
                characters
                    .iter()
                    .map(|character| character.len as u8)
                    .collect::<Vec<_>>(),
            );
            let bounds = entry.bounds;
            let (first, last) = characters.iter().fold((f32::MAX, f32::MIN), |(a, b), c| {
                (a.min(c.position), b.max(c.position + c.width))
            });
            let (first, last) = if characters.is_empty() {
                (0.0, 0.0)
            } else {
                (first, last)
            };
            // Positions are measured from the edge a line starts at.
            let (left, right) = if entry.rtl {
                (
                    bounds.x + bounds.width - last,
                    bounds.x + bounds.width - first,
                )
            } else {
                (bounds.x + first, bounds.x + last)
            };
            node.set_bounds(Rect {
                x0: f64::from(left) * scale,
                y0: f64::from(bounds.y) * scale,
                x1: f64::from(right) * scale,
                y1: f64::from(bounds.y + bounds.height) * scale,
            });
            node.set_character_positions(
                characters
                    .iter()
                    .map(|c| ((c.position - first) as f64 * scale) as f32)
                    .collect::<Vec<_>>(),
            );
            node.set_character_widths(
                characters
                    .iter()
                    .map(|c| (f64::from(c.width) * scale) as f32)
                    .collect::<Vec<_>>(),
            );
            node.set_word_starts(
                entry
                    .word_starts
                    .iter()
                    .filter_map(|start| starts.binary_search(start).ok())
                    .filter(|character| run.characters.contains(character))
                    .map(|character| (character - run.characters.start) as u8)
                    .collect::<Vec<_>>(),
            );
            node.set_text_direction(if entry.rtl {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            });
            if index > 0 && runs[index - 1].line == run.line {
                node.set_previous_on_line(run_id(owner, index - 1));
            }
            if runs
                .get(index + 1)
                .is_some_and(|next| next.line == run.line)
            {
                node.set_next_on_line(run_id(owner, index + 1));
            }
            (run_id(owner, index), node)
        })
        .collect();
    let selection = text_position(owner, text, &runs, text.anchor)
        .zip(text_position(owner, text, &runs, text.focus));
    (
        nodes,
        selection.map(|(anchor, focus)| TextSelection { anchor, focus }),
    )
}

pub(crate) fn tree(context: &Context, title: &str, scale: f64) -> TreeUpdate {
    let root = context.root();
    let snapshots = context.snapshots();
    let popups: Vec<_> = snapshots
        .iter()
        .filter(|node| node.is_popup)
        .map(|node| node.id)
        .collect();
    let focus = context
        .focused()
        .and_then(|id| {
            context
                .node(id)
                .and_then(|node| node.active_descendant)
                .or(Some(id))
        })
        .unwrap_or(root);
    let nodes = snapshots
        .into_iter()
        .flat_map(|snapshot| {
            let (runs, selection) = snapshot
                .editor_text
                .as_ref()
                .map(|text| text_runs(snapshot.id, text, scale))
                .unwrap_or_default();
            let role = if snapshot.id == root {
                Role::Window
            } else {
                match snapshot.semantics.role {
                    CoreRole::Group => Role::GenericContainer,
                    CoreRole::Label => Role::Label,
                    CoreRole::Button | CoreRole::MenuButton => Role::Button,
                    CoreRole::ScrollView => Role::ScrollView,
                    CoreRole::TextInput => Role::TextInput,
                    CoreRole::MultilineTextInput => Role::MultilineTextInput,
                    CoreRole::PasswordInput => Role::PasswordInput,
                    CoreRole::Checkbox => Role::CheckBox,
                    CoreRole::Radio => Role::RadioButton,
                    CoreRole::Slider => Role::Slider,
                    CoreRole::ComboBox => Role::ComboBox,
                    CoreRole::Menu => Role::Menu,
                    CoreRole::ListBox => Role::ListBox,
                    CoreRole::Option => Role::ListBoxOption,
                    CoreRole::MenuItem => Role::MenuItem,
                    CoreRole::Tree => Role::Tree,
                    CoreRole::TreeItem => Role::TreeItem,
                    CoreRole::Grid => Role::Grid,
                    CoreRole::Row => Role::Row,
                    CoreRole::RowGroup => Role::RowGroup,
                    CoreRole::Cell => Role::Cell,
                    CoreRole::ColumnHeader => Role::ColumnHeader,
                    CoreRole::Dialog => Role::Dialog,
                    CoreRole::TabList => Role::TabList,
                    CoreRole::Tab => Role::Tab,
                    CoreRole::TabPanel => Role::TabPanel,
                    CoreRole::Splitter => Role::Splitter,
                    CoreRole::Image => Role::Image,
                    CoreRole::SpinButton => Role::SpinButton,
                    CoreRole::Status => Role::Status,
                    CoreRole::Alert => Role::Alert,
                    other => {
                        debug_assert!(false, "no AccessKit role for {other:?}");
                        Role::GenericContainer
                    }
                }
            };
            let mut node = Node::new(role);
            let bounds = snapshot.bounds;
            node.set_bounds(Rect {
                x0: f64::from(bounds.x) * scale,
                y0: f64::from(bounds.y) * scale,
                x1: f64::from(bounds.x + bounds.width) * scale,
                y1: f64::from(bounds.y + bounds.height) * scale,
            });
            // Popups escape their owner's bounds and intermediate layout groups.
            // Only viewport/scroll containers clip the platform subtree.
            if snapshot.id == root || snapshot.scroll.is_some() || !runs.is_empty() {
                node.set_clips_children();
            }
            node.set_children(
                snapshot
                    .children
                    .iter()
                    .filter(|id| !popups.contains(id))
                    .chain(popups.iter().filter(|_| snapshot.id == root))
                    .map(|id| NodeId(id.0))
                    .chain(runs.iter().map(|(id, _)| *id))
                    .collect::<Vec<_>>(),
            );
            if let Some(selection) = selection {
                node.set_text_selection(selection);
                if !snapshot.semantics.disabled {
                    node.add_action(Action::SetTextSelection);
                }
            }
            if snapshot.id == root {
                node.set_label(title);
            } else if role == Role::Label {
                node.set_value(snapshot.semantics.label);
            } else if !snapshot.semantics.label.is_empty() {
                node.set_label(snapshot.semantics.label);
            }
            if let Some(live) = snapshot.live {
                node.set_live(match live {
                    glassmere_core::Live::Polite => accesskit::Live::Polite,
                    glassmere_core::Live::Assertive => accesskit::Live::Assertive,
                });
            }
            if snapshot.semantics.disabled {
                node.set_disabled();
            }
            if snapshot.modal {
                node.set_modal();
            }
            if let Some(description) = &snapshot.description {
                node.set_description(description.clone());
            }
            // `Axis` is the direction panes are laid out in; the separator
            // between side-by-side panes is a vertical bar.
            if let Some(axis) = snapshot.orientation {
                node.set_orientation(match axis {
                    glassmere_core::Axis::Horizontal => accesskit::Orientation::Vertical,
                    glassmere_core::Axis::Vertical => accesskit::Orientation::Horizontal,
                });
            }
            if let Some(info) = snapshot.collection {
                if matches!(role, Role::ListBox | Role::ListBoxOption) {
                    node.set_size_of_set(info.total);
                    if info.item.is_some() {
                        node.set_position_in_set(info.index + 1);
                    }
                }
                if role == Role::TreeItem {
                    node.set_level(info.level);
                }
                if let Some(columns) = info.columns {
                    node.set_column_count(columns);
                    node.set_row_count(info.total);
                }
                if let Some(column) = info.column {
                    node.set_column_index(column);
                    if role == Role::Cell {
                        node.set_row_index(info.index);
                    }
                }
                if role == Role::Row {
                    node.set_row_index(info.index);
                }
                if let Some(direction) = info.sort {
                    node.set_sort_direction(match direction {
                        glassmere_core::SortDirection::Ascending => {
                            accesskit::SortDirection::Ascending
                        }
                        glassmere_core::SortDirection::Descending => {
                            accesskit::SortDirection::Descending
                        }
                    });
                }
            }
            if let Some(value) = snapshot.value {
                node.set_value(value);
            }
            if let Some(drag) = snapshot.drag {
                let mut actions = Vec::new();
                if drag.source {
                    actions.push(accesskit::CustomAction {
                        id: 1,
                        description: "Pick up".into(),
                    });
                }
                if drag.can_drop {
                    actions.push(accesskit::CustomAction {
                        id: 2,
                        description: "Drop here".into(),
                    });
                }
                if !actions.is_empty() {
                    node.set_custom_actions(actions);
                    node.add_action(Action::CustomAction);
                }
            }
            if let Some(checked) = snapshot.checked {
                node.set_toggled(if checked {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                });
            }
            if let Some(selected) = snapshot.selected {
                if role == Role::RadioButton {
                    node.set_toggled(if selected {
                        accesskit::Toggled::True
                    } else {
                        accesskit::Toggled::False
                    });
                } else {
                    node.set_selected(selected);
                }
            }
            if let Some(descendant) = snapshot.active_descendant {
                node.set_active_descendant(NodeId(descendant.0));
            }
            if snapshot.expanded == Some(true) {
                node.set_controls(
                    snapshot
                        .children
                        .iter()
                        .map(|id| NodeId(id.0))
                        .collect::<Vec<_>>(),
                );
            }
            if let Some(numeric) = snapshot.numeric {
                node.set_numeric_value(numeric.value);
                node.set_min_numeric_value(numeric.min);
                node.set_max_numeric_value(numeric.max);
                node.set_numeric_value_step(numeric.step);
                node.set_numeric_value_jump(numeric.step * 10.0);
            }
            if let Some(expanded) = snapshot.expanded {
                node.set_expanded(expanded);
                if matches!(
                    snapshot.semantics.role,
                    CoreRole::ComboBox | CoreRole::MenuButton
                ) {
                    node.set_has_popup(if role == Role::ComboBox {
                        accesskit::HasPopup::Listbox
                    } else {
                        accesskit::HasPopup::Menu
                    });
                }
                if !snapshot.semantics.disabled {
                    node.add_action(if expanded {
                        Action::Collapse
                    } else {
                        Action::Expand
                    });
                }
            }
            if snapshot.semantics.focusable
                || (!snapshot.semantics.disabled
                    && snapshot.collection.is_some_and(|info| info.item.is_some()))
            {
                node.add_action(Action::Focus);
            }
            if !snapshot.semantics.disabled {
                if matches!(
                    role,
                    Role::Button
                        | Role::CheckBox
                        | Role::RadioButton
                        | Role::ComboBox
                        | Role::ListBoxOption
                        | Role::MenuItem
                        | Role::TreeItem
                        | Role::Row
                        | Role::ColumnHeader
                        | Role::Tab
                ) {
                    node.add_action(Action::Click);
                }
                if matches!(
                    role,
                    Role::TextInput
                        | Role::PasswordInput
                        | Role::Slider
                        | Role::Splitter
                        | Role::SpinButton
                ) {
                    node.add_action(Action::SetValue);
                }
                if matches!(role, Role::Slider | Role::SpinButton) {
                    node.add_action(Action::Increment);
                    node.add_action(Action::Decrement);
                }
            }
            if snapshot.id != root {
                node.add_action(Action::ScrollIntoView);
            }
            if let Some(scroll) = snapshot.scroll {
                node.set_scroll_y(f64::from(scroll.offset) * scale);
                node.set_scroll_y_min(0.0);
                node.set_scroll_y_max(f64::from(scroll.maximum) * scale);
                if scroll.maximum_x > 0.0 {
                    node.set_scroll_x(f64::from(scroll.offset_x) * scale);
                    node.set_scroll_x_min(0.0);
                    node.set_scroll_x_max(f64::from(scroll.maximum_x) * scale);
                }
                node.add_action(Action::ScrollUp);
                node.add_action(Action::ScrollDown);
                if scroll.maximum_x > 0.0 {
                    node.add_action(Action::ScrollLeft);
                    node.add_action(Action::ScrollRight);
                }
                node.add_action(Action::SetScrollOffset);
            }
            std::iter::once((NodeId(snapshot.id.0), node)).chain(runs)
        })
        .collect();
    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(NodeId(root.0))),
        tree_id: TreeId::ROOT,
        focus: NodeId(focus.0),
    }
}

pub(crate) fn action(context: &mut Context, request: ActionRequest, scale: f64) -> bool {
    if request.target_tree != TreeId::ROOT {
        return false;
    }
    let target = CoreNodeId(request.target_node.0);
    let action = match request.action {
        Action::Focus => CoreAction::Focus,
        Action::Click => CoreAction::Activate,
        Action::SetValue => match request.data {
            Some(ActionData::Value(value)) => CoreAction::SetText(value.to_string()),
            Some(ActionData::NumericValue(value)) => CoreAction::SetNumber(value),
            _ => return false,
        },
        Action::SetTextSelection => {
            let Some(ActionData::SetTextSelection(selection)) = request.data else {
                return false;
            };
            let Some(text) = context.node(target).and_then(|node| node.editor_text) else {
                return false;
            };
            let runs = runs(&text);
            let (Some(anchor), Some(focus)) = (
                editor_point(target, &text, &runs, selection.anchor),
                editor_point(target, &text, &runs, selection.focus),
            ) else {
                return false;
            };
            CoreAction::SetSelection { anchor, focus }
        }
        Action::Increment => CoreAction::Increment,
        Action::Decrement => CoreAction::Decrement,
        Action::Expand => CoreAction::Expand,
        Action::Collapse => CoreAction::Collapse,
        Action::CustomAction => match request.data {
            Some(ActionData::CustomAction(1)) => CoreAction::StartDrag,
            Some(ActionData::CustomAction(2)) => CoreAction::Drop,
            _ => return false,
        },
        Action::ScrollIntoView => CoreAction::ScrollIntoView,
        Action::ScrollDown | Action::ScrollUp => {
            let Some(node) = context.node(target) else {
                return false;
            };
            let amount = match request.data {
                Some(ActionData::ScrollUnit(ScrollUnit::Item)) => 40.0,
                _ => node.bounds.height * 0.9,
            };
            CoreAction::ScrollBy(if request.action == Action::ScrollUp {
                -amount
            } else {
                amount
            })
        }
        Action::ScrollLeft | Action::ScrollRight => {
            let Some(node) = context.node(target) else {
                return false;
            };
            let amount = match request.data {
                Some(ActionData::ScrollUnit(ScrollUnit::Item)) => 40.0,
                _ => node.bounds.width * 0.9,
            };
            CoreAction::ScrollByX(if request.action == Action::ScrollLeft {
                -amount
            } else {
                amount
            })
        }
        Action::SetScrollOffset => match request.data {
            Some(ActionData::SetScrollOffset(point)) => {
                // One request carries both axes; apply the horizontal one first.
                context.action(target, CoreAction::ScrollToX((point.x / scale) as f32));
                CoreAction::ScrollTo((point.y / scale) as f32)
            }
            _ => return false,
        },
        _ => return false,
    };
    context.action(target, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glassmere_core::{
        Button, CodeEditorOptions, ItemId, Length, ListModel, ListState, MonospaceMeasurer, Style,
        TextDocument, TextEdit, TextInput, TreeItem, TreeModel, VirtualListOptions,
    };
    use glassmere_scene::Size;

    fn node_in(update: &TreeUpdate, id: NodeId) -> &Node {
        &update.nodes.iter().find(|(key, _)| *key == id).unwrap().1
    }

    /// The text runs of an editor, in order.
    fn runs_in<'a>(update: &'a TreeUpdate, editor: &Node) -> Vec<(NodeId, &'a Node)> {
        editor
            .children()
            .iter()
            .map(|id| (*id, node_in(update, *id)))
            .filter(|(_, node)| node.role() == Role::TextRun)
            .collect()
    }

    fn field_view(ui: &mut glassmere_core::Ui<'_>, text: &str) {
        ui.keyed("field", |ui| ui.text_input("Field", text, |_| ()));
        ui.keyed("secret", |ui| {
            ui.text_input(TextInput::new("Secret").password(), "hunter2", |_| ())
        });
    }

    fn run_fields(context: &mut Context, text: &str) {
        context.run(Size::new(600.0, 200.0), &mut MonospaceMeasurer, |ui| {
            field_view(ui, text)
        });
    }

    #[test]
    fn a_text_input_carries_text_runs_with_caret_and_selection() {
        let mut context = Context::new();
        run_fields(&mut context, "héllo wörld");
        let field = context.find(&["field"]).unwrap();
        context.action(field, CoreAction::Focus);
        context.event(glassmere_core::Event::Text(TextEdit::Select {
            anchor: 1,
            cursor: 8,
        }));
        run_fields(&mut context, "héllo wörld");
        let update = tree(&context, "Fields", 2.0);
        let editor = node_in(&update, NodeId(field.0));
        assert_eq!(editor.role(), Role::TextInput);
        assert!(editor.supports_action(Action::SetTextSelection));
        let runs = runs_in(&update, editor);
        assert_eq!(runs.len(), 1);
        let (id, run) = runs[0];
        assert_eq!(run.value(), Some("héllo wörld"));
        // The bytes of each character: é and ö take two.
        assert_eq!(run.character_lengths(), [1, 2, 1, 1, 1, 1, 1, 2, 1, 1, 1]);
        assert_eq!(run.word_starts(), [0, 6]);
        assert_eq!(run.text_direction(), Some(TextDirection::LeftToRight));
        let positions = run.character_positions().unwrap();
        let widths = run.character_widths().unwrap();
        assert_eq!(positions[0], 0.0);
        for index in 1..positions.len() {
            assert!((positions[index] - positions[index - 1] - widths[index - 1]).abs() < 0.01);
        }
        // Bounds are in physical pixels, like the positions.
        let bounds = run.bounds().unwrap();
        let end = f64::from(positions[10] + widths[10]);
        assert!((bounds.x1 - bounds.x0 - end).abs() < 0.01);
        let selection = editor.text_selection().unwrap();
        assert_eq!(
            selection.anchor,
            TextPosition {
                node: id,
                character_index: 1
            }
        );
        assert_eq!(
            selection.focus,
            TextPosition {
                node: id,
                character_index: 7
            }
        );
    }

    #[test]
    fn the_platform_can_move_the_caret_of_a_text_input() {
        let mut context = Context::new();
        run_fields(&mut context, "héllo wörld");
        let field = context.find(&["field"]).unwrap();
        let update = tree(&context, "Fields", 1.0);
        let run = runs_in(&update, node_in(&update, NodeId(field.0)))[0].0;
        let at = |character_index| TextPosition {
            node: run,
            character_index,
        };
        let request = |anchor, focus| ActionRequest {
            action: Action::SetTextSelection,
            target_tree: TreeId::ROOT,
            target_node: NodeId(field.0),
            data: Some(ActionData::SetTextSelection(TextSelection {
                anchor,
                focus,
            })),
        };
        assert!(action(&mut context, request(at(3), at(8)), 1.0));
        run_fields(&mut context, "héllo wörld");
        // Character 3 starts at byte 4, character 8 at byte 10.
        let selection = context.node(field).unwrap().text_selection.unwrap();
        assert_eq!((selection.anchor, selection.cursor), (4, 10));
        assert_eq!(tree(&context, "Fields", 1.0).focus, NodeId(field.0));
        // A position in some other node's text is not ours to move to.
        let stranger = TextPosition {
            node: NodeId(run.0 + (1 << 24)),
            character_index: 0,
        };
        assert!(!action(&mut context, request(stranger, stranger), 1.0));
    }

    #[test]
    fn a_long_line_is_cut_into_linked_runs_the_platform_can_index() {
        let mut context = Context::new();
        let text = "a".repeat(300);
        run_fields(&mut context, &text);
        let field = context.find(&["field"]).unwrap();
        let update = tree(&context, "Fields", 1.0);
        let editor = node_in(&update, NodeId(field.0));
        let runs = runs_in(&update, editor);
        let lengths: Vec<_> = runs
            .iter()
            .map(|(_, run)| run.character_lengths().len())
            .collect();
        assert_eq!(lengths, [255, 45]);
        assert_eq!(runs[0].1.next_on_line(), Some(runs[1].0));
        assert_eq!(runs[1].1.previous_on_line(), Some(runs[0].0));
        assert_eq!(runs[0].1.previous_on_line(), None);
        assert_eq!(runs[1].1.value().map(str::len), Some(45));
        // One word, which starts in the first run only.
        assert_eq!(runs[0].1.word_starts(), [0]);
        assert!(runs[1].1.word_starts().is_empty());
        // The caret is at the end, in the second run.
        let selection = editor.text_selection().unwrap();
        assert_eq!(selection.focus.node, runs[1].0);
        assert_eq!(selection.focus.character_index, 45);
        // The second run starts where the first ends.
        let (first, second) = (runs[0].1.bounds().unwrap(), runs[1].1.bounds().unwrap());
        assert!((first.x1 - second.x0).abs() < 0.01);
        // Character 10 of the second run is character 265 of the line.
        let at = TextPosition {
            node: runs[1].0,
            character_index: 10,
        };
        let request = ActionRequest {
            action: Action::SetTextSelection,
            target_tree: TreeId::ROOT,
            target_node: NodeId(field.0),
            data: Some(ActionData::SetTextSelection(TextSelection {
                anchor: at,
                focus: at,
            })),
        };
        assert!(action(&mut context, request, 1.0));
        run_fields(&mut context, &text);
        let selection = context.node(field).unwrap().text_selection.unwrap();
        assert_eq!((selection.anchor, selection.cursor), (265, 265));
    }

    #[test]
    fn right_to_left_text_is_positioned_from_the_right_edge() {
        let mut context = Context::new();
        // Alef, bet, gimel, a space and Latin letters, which read left to right.
        run_fields(&mut context, "\u{5d0}\u{5d1}\u{5d2} abc");
        let field = context.find(&["field"]).unwrap();
        let update = tree(&context, "Fields", 1.0);
        let runs = runs_in(&update, node_in(&update, NodeId(field.0)));
        assert_eq!(runs.len(), 1);
        let run = runs[0].1;
        assert_eq!(run.text_direction(), Some(TextDirection::RightToLeft));
        assert_eq!(run.character_lengths(), [2, 2, 2, 1, 1, 1, 1]);
        let positions = run.character_positions().unwrap();
        let widths = run.character_widths().unwrap();
        let advance = widths[0];
        for (index, position) in positions[..4].iter().enumerate() {
            assert!(
                (position - advance * index as f32).abs() < 0.01,
                "{positions:?}"
            );
        }
        // The Latin run is reversed as a whole, so its first letter is the
        // furthest from the right edge.
        assert!(positions[4] > positions[5] && positions[5] > positions[6]);
        assert!((positions[4] - advance * 6.0).abs() < 0.01, "{positions:?}");
    }

    #[test]
    fn a_password_exposes_no_text_runs_or_selection() {
        let mut context = Context::new();
        run_fields(&mut context, "name");
        let secret = context.find(&["secret"]).unwrap();
        let update = tree(&context, "Fields", 1.0);
        let editor = node_in(&update, NodeId(secret.0));
        assert_eq!(editor.role(), Role::PasswordInput);
        assert!(runs_in(&update, editor).is_empty());
        assert!(editor.text_selection().is_none());
        assert!(!editor.supports_action(Action::SetTextSelection));
        assert!(!editor.value().unwrap_or_default().contains('h'));
        // Only the other field has text runs.
        let field = context.find(&["field"]).unwrap();
        let runs = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::TextRun)
            .count();
        assert_eq!(
            runs,
            runs_in(&update, node_in(&update, NodeId(field.0))).len()
        );
    }

    #[test]
    fn a_text_area_is_multiline_with_a_run_per_line_and_the_primary_selection() {
        let mut document = TextDocument::from_text("h\u{e9}llo\nw\u{f6}rld\n\u{5d0}\u{5d1} end");
        // From the é on the first line to the r of the second.
        document.set_selection(1, 8);
        let mut context = Context::new();
        let view = |ui: &mut glassmere_core::Ui<'_>| {
            ui.keyed("notes", |ui| {
                ui.text_area(
                    &document,
                    CodeEditorOptions {
                        label: "Notes".into(),
                        style: Style::default()
                            .width(Length::Px(400.0))
                            .height(Length::Px(200.0)),
                        ..CodeEditorOptions::default()
                    },
                    |_| (),
                )
            });
        };
        context.run(Size::new(600.0, 400.0), &mut MonospaceMeasurer, view);
        let notes = context.find(&["notes"]).unwrap();
        let update = tree(&context, "Notes", 1.0);
        let editor = node_in(&update, NodeId(notes.0));
        assert_eq!(editor.role(), Role::MultilineTextInput);
        assert!(editor.supports_action(Action::SetTextSelection));
        let runs = runs_in(&update, editor);
        let values: Vec<_> = runs.iter().map(|(_, run)| run.value().unwrap()).collect();
        assert_eq!(
            values,
            ["h\u{e9}llo\n", "w\u{f6}rld\n", "\u{5d0}\u{5d1} end"]
        );
        // A line break is a character, and so is counted.
        assert_eq!(runs[0].1.character_lengths(), [1, 2, 1, 1, 1, 1]);
        assert_eq!(runs[2].1.character_lengths(), [2, 2, 1, 1, 1, 1]);
        assert_eq!(runs[2].1.text_direction(), Some(TextDirection::RightToLeft));
        // Lines are stacked.
        let tops: Vec<_> = runs
            .iter()
            .map(|(_, run)| run.bounds().unwrap().y0)
            .collect();
        assert!(tops[0] < tops[1] && tops[1] < tops[2], "{tops:?}");
        let selection = editor.text_selection().unwrap();
        assert_eq!(
            selection.anchor,
            TextPosition {
                node: runs[0].0,
                character_index: 1
            }
        );
        assert_eq!(
            selection.focus,
            TextPosition {
                node: runs[1].0,
                character_index: 2
            }
        );
    }

    #[test]
    fn expanded_state_reaches_combo_boxes_and_tree_rows_with_children() {
        let item = |id, parent: Option<u64>| TreeItem {
            id: ItemId(id),
            parent: parent.map(ItemId),
            label: format!("Node {id}"),
        };
        let mut model =
            TreeModel::new(vec![item(1, None), item(2, Some(1)), item(3, None)]).unwrap();
        let state = ListState::default();
        let mut context = Context::new();
        let declare = |context: &mut Context, model: &TreeModel| {
            context.run(Size::new(500.0, 500.0), &mut MonospaceMeasurer, |ui| {
                ui.keyed("combo", |ui| {
                    ui.combo_box("Theme", 0, &["Dark", "Light"], |_| ())
                });
                ui.keyed("tree", |ui| {
                    ui.tree_view(
                        model,
                        &state,
                        VirtualListOptions {
                            height: 200.0,
                            row_height: 30.0,
                            ..Default::default()
                        },
                        |_| (),
                    )
                });
            });
        };
        declare(&mut context, &model);
        let combo = context.find(&["combo"]).unwrap();
        let parent = context.find(&["tree", "item-1"]).unwrap();
        let leaf = context.find(&["tree", "item-3"]).unwrap();
        let update = tree(&context, "Tree", 1.0);
        let expanded = |id: CoreNodeId| node_in(&update, NodeId(id.0)).is_expanded();
        assert_eq!(expanded(combo), Some(false));
        assert_eq!(expanded(parent), Some(false));
        assert!(node_in(&update, NodeId(parent.0)).supports_action(Action::Expand));
        assert_eq!(expanded(leaf), None);

        context.action(combo, CoreAction::Expand);
        model.set_expanded(ItemId(1), true);
        declare(&mut context, &model);
        declare(&mut context, &model);
        let update = tree(&context, "Tree", 1.0);
        let expanded = |id: CoreNodeId| node_in(&update, NodeId(id.0)).is_expanded();
        assert_eq!(expanded(combo), Some(true));
        assert!(node_in(&update, NodeId(combo.0)).supports_action(Action::Collapse));
        assert_eq!(expanded(parent), Some(true));
        assert_eq!(expanded(leaf), None);
    }

    #[test]
    fn controls_expose_values_actions_and_unclipped_popup_focus() {
        let mut context = Context::new();
        // The values are what the view shows; nothing in this test sends a message.
        let view = |ui: &mut glassmere_core::Ui<'_>| {
            ui.keyed("input", |ui| ui.text_input("Name", "Name", |_| ()));
            ui.keyed("check", |ui| ui.checkbox("Check", false, |_| ()));
            ui.keyed("radio", |ui| ui.radio("Radio", 0, 0, |_| ()));
            ui.keyed("slider", |ui| {
                ui.slider("Volume", 50.0, 0.0..=100.0, 5.0, |_| ())
            });
            ui.keyed("combo", |ui| {
                ui.combo_box("Theme", 0, &["Dark", "Light"], |_| ())
            });
            ui.enabled(false, |ui| {
                ui.keyed("disabled", |ui| ui.text_input("Disabled", "", |_| ()));
            });
        };
        context.run(Size::new(600.0, 800.0), &mut MonospaceMeasurer, view);
        let get = |context: &Context, key: &str| context.find(&[key]).unwrap();
        let update = tree(&context, "Controls", 1.0);
        let node = |key| {
            &update
                .nodes
                .iter()
                .find(|(id, _)| id.0 == get(&context, key).0)
                .unwrap()
                .1
        };
        assert_eq!(node("input").role(), Role::TextInput);
        assert_eq!(node("input").value(), Some("Name"));
        assert!(node("input").supports_action(Action::SetValue));
        assert!(!node("disabled").supports_action(Action::SetValue));
        assert_eq!(node("check").toggled(), Some(accesskit::Toggled::False));
        assert_eq!(node("radio").toggled(), Some(accesskit::Toggled::True));
        assert_eq!(node("slider").numeric_value(), Some(50.0));
        assert_eq!(node("slider").numeric_value_step(), Some(5.0));
        let owner = get(&context, "combo");
        assert!(action(
            &mut context,
            ActionRequest {
                action: Action::Expand,
                target_tree: TreeId::ROOT,
                target_node: NodeId(owner.0),
                data: None
            },
            1.0
        ));
        context.run(Size::new(600.0, 800.0), &mut MonospaceMeasurer, view);
        let panel = context.find(&["combo", "$popup"]).unwrap();
        let choice = context.find(&["combo", "$popup", "0"]).unwrap();
        let update = tree(&context, "Controls", 1.0);
        let node = |id| &update.nodes.iter().find(|(key, _)| key.0 == id).unwrap().1;
        assert!(node(context.root().0).children().contains(&NodeId(panel.0)));
        assert!(!node(owner.0).children().contains(&NodeId(panel.0)));
        assert_eq!(node(owner.0).active_descendant(), Some(NodeId(choice.0)));
        assert_eq!(update.focus, NodeId(choice.0));
        assert_eq!(node(choice.0).role(), Role::ListBoxOption);
        assert_eq!(node(choice.0).is_selected(), Some(true));
    }

    #[test]
    fn a_view_that_overflows_sideways_reports_and_takes_horizontal_scrolling() {
        let mut context = Context::new();
        context.run(Size::new(200.0, 200.0), &mut MonospaceMeasurer, |ui| {
            let sized = |width: f32, height: f32| Style {
                width: Length::Px(width),
                height: Length::Px(height),
                ..Style::default()
            };
            ui.keyed("strip", |ui| {
                ui.scroll(sized(100.0, 60.0), |ui| {
                    ui.keyed("wide", |ui| {
                        ui.button(Button::new("Wide").enabled(true).style(sized(400.0, 30.0)))
                    });
                })
            });
            ui.keyed("plain", |ui| {
                ui.scroll(sized(100.0, 60.0), |ui| {
                    ui.keyed("fits", |ui| ui.button("Fits"));
                })
            });
        });
        let strip = context.find(&["strip"]).unwrap();
        let plain = context.find(&["plain"]).unwrap();
        let update = tree(&context, "Test window", 2.0);
        let node = |id: CoreNodeId| {
            &update
                .nodes
                .iter()
                .find(|(key, _)| key.0 == id.0)
                .unwrap()
                .1
        };
        assert_eq!(node(strip).scroll_x_max(), Some(600.0));
        assert!(node(strip).supports_action(Action::ScrollRight));
        assert_eq!(node(plain).scroll_x_max(), None);
        assert!(!node(plain).supports_action(Action::ScrollRight));
        let request = |action, data| ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: NodeId(strip.0),
            data,
        };
        let item = Some(ActionData::ScrollUnit(ScrollUnit::Item));
        assert!(action(
            &mut context,
            request(Action::ScrollRight, item.clone()),
            2.0
        ));
        assert_eq!(context.node(strip).unwrap().scroll.unwrap().offset_x, 40.0);
        assert!(action(&mut context, request(Action::ScrollLeft, item), 2.0));
        let point = ActionData::SetScrollOffset(accesskit::Point::new(100.0, 0.0));
        action(
            &mut context,
            request(Action::SetScrollOffset, Some(point)),
            2.0,
        );
        assert_eq!(context.node(strip).unwrap().scroll.unwrap().offset_x, 50.0);
    }

    #[test]
    fn maps_stable_ids_disabled_state_scaled_bounds_and_scroll_actions() {
        let mut context = Context::new();
        context.run(Size::new(200.0, 200.0), &mut MonospaceMeasurer, |ui| {
            ui.keyed("list", |ui| {
                ui.scroll(
                    Style {
                        height: Length::Px(60.0),
                        ..Style::default()
                    },
                    |ui| {
                        ui.keyed("disabled", |ui| {
                            ui.button(
                                Button::new("Disabled")
                                    .enabled(false)
                                    .style(Style::button()),
                            )
                        });
                        ui.keyed("active", |ui| ui.button("Active"));
                    },
                )
            });
        });
        let disabled = context.find(&["list", "disabled"]).unwrap();
        let active = context.find(&["list", "active"]).unwrap();
        let list = context.find(&["list"]).unwrap();
        let update = tree(&context, "Test window", 2.0);
        let node = |id: CoreNodeId| {
            &update
                .nodes
                .iter()
                .find(|(key, _)| key.0 == id.0)
                .unwrap()
                .1
        };
        assert_eq!(node(context.root()).role(), Role::Window);
        assert!(node(disabled).is_disabled());
        assert!(!node(disabled).supports_action(Action::Click));
        assert!(node(active).supports_action(Action::Click));
        assert_eq!(
            node(active).bounds().unwrap().y0,
            f64::from(context.node(active).unwrap().bounds.y) * 2.0
        );
        assert!(node(list).scroll_y_max().unwrap() > 0.0);
        assert!(action(
            &mut context,
            ActionRequest {
                action: Action::SetScrollOffset,
                target_tree: TreeId::ROOT,
                target_node: NodeId(list.0),
                data: Some(ActionData::SetScrollOffset(accesskit::Point::new(
                    0.0, 20.0
                ))),
            },
            2.0
        ));
        assert_eq!(context.node(list).unwrap().scroll.unwrap().offset, 10.0);
        assert!(action(
            &mut context,
            ActionRequest {
                action: Action::Focus,
                target_tree: TreeId::ROOT,
                target_node: NodeId(active.0),
                data: None,
            },
            2.0
        ));
        assert_eq!(tree(&context, "Test window", 2.0).focus, NodeId(active.0));
    }
}
