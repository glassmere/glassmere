use accesskit::{
    Action, ActionData, ActionRequest, Node, NodeId, Rect, Role, ScrollUnit, Tree, TreeId,
    TreeUpdate,
};
use rust_gui_core::{Action as CoreAction, Context, NodeId as CoreNodeId, Role as CoreRole};

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
        .map(|snapshot| {
            let role = if snapshot.id == root {
                Role::Window
            } else {
                match snapshot.semantics.role {
                    CoreRole::Group => Role::GenericContainer,
                    CoreRole::Label => Role::Label,
                    CoreRole::Button | CoreRole::MenuButton => Role::Button,
                    CoreRole::ScrollView => Role::ScrollView,
                    CoreRole::TextInput => Role::TextInput,
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
            if snapshot.id == root || snapshot.scroll.is_some() {
                node.set_clips_children();
            }
            node.set_children(
                snapshot
                    .children
                    .iter()
                    .filter(|id| !popups.contains(id))
                    .chain(popups.iter().filter(|_| snapshot.id == root))
                    .map(|id| NodeId(id.0))
                    .collect::<Vec<_>>(),
            );
            if snapshot.id == root {
                node.set_label(title);
            } else if role == Role::Label {
                node.set_value(snapshot.semantics.label);
            } else if !snapshot.semantics.label.is_empty() {
                node.set_label(snapshot.semantics.label);
            }
            if snapshot.semantics.disabled {
                node.set_disabled();
            }
            if snapshot.modal {
                node.set_modal();
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
                        rust_gui_core::SortDirection::Ascending => {
                            accesskit::SortDirection::Ascending
                        }
                        rust_gui_core::SortDirection::Descending => {
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
                ) {
                    node.add_action(Action::Click);
                }
                if matches!(role, Role::TextInput | Role::Slider) {
                    node.add_action(Action::SetValue);
                }
                if role == Role::Slider {
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
                node.add_action(Action::ScrollUp);
                node.add_action(Action::ScrollDown);
                node.add_action(Action::SetScrollOffset);
            }
            (NodeId(snapshot.id.0), node)
        })
        .collect();
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(NodeId(root.0))),
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
        Action::SetScrollOffset => match request.data {
            Some(ActionData::SetScrollOffset(point)) => {
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
    use rust_gui_core::{Length, MonospaceMeasurer, Style};
    use rust_gui_scene::Size;

    #[test]
    fn controls_expose_values_actions_and_unclipped_popup_focus() {
        let mut context = Context::new();
        let mut text = "Name".into();
        let mut checked = false;
        let mut radio = 0;
        let mut volume = 50.0;
        let mut selected = 0;
        let mut view = |ui: &mut rust_gui_core::Ui<'_>| {
            ui.text_input("input", "Name", &mut text);
            ui.checkbox("check", "Check", &mut checked);
            ui.radio("radio", "Radio", &mut radio, 0);
            ui.slider("slider", "Volume", &mut volume, 0.0..=100.0, 5.0);
            ui.combo_box("combo", "Theme", &mut selected, &["Dark", "Light"]);
            ui.enabled(false, |ui| {
                ui.text_input("disabled", "Disabled", &mut String::new());
            });
        };
        context.run(Size::new(600.0, 800.0), &mut MonospaceMeasurer, &mut view);
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
        context.run(Size::new(600.0, 800.0), &mut MonospaceMeasurer, &mut view);
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
    fn maps_stable_ids_disabled_state_scaled_bounds_and_scroll_actions() {
        let mut context = Context::new();
        context.run(Size::new(200.0, 200.0), &mut MonospaceMeasurer, |ui| {
            ui.scroll(
                "list",
                Style {
                    height: Length::Px(60.0),
                    ..Style::default()
                },
                |ui| {
                    ui.button_styled("disabled", "Disabled", false, Style::button());
                    ui.button_keyed("active", "Active");
                },
            );
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
