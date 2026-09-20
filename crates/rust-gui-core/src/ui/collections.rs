use super::*;
use crate::{
    GridModel, GridState, Length, ListModel, ListResponse, ListState, SortDirection,
    VirtualListOptions, collection::CollectionKind,
};

impl Ui<'_> {
    /// Fixed-height rows with bounded realization. State belongs to the app;
    /// transient row NodeIds are deliberately not stored in the model.
    pub fn virtual_list(
        &mut self,
        key: impl Into<String>,
        model: &mut impl ListModel,
        state: &mut ListState,
        options: VirtualListOptions,
    ) -> ListResponse {
        self.collection(
            key.into(),
            model,
            state,
            options,
            CollectionKind::List,
            |ui, model, index, _| {
                ui.label_styled(
                    "label",
                    model.label(index),
                    Style {
                        width: Length::Percent(1.0),
                        padding: 6.0,
                        ..Style::default()
                    },
                );
            },
        )
    }

    /// Custom row content. Application editing state must live outside the row,
    /// since nodes leaving the realized range are removed.
    pub fn virtual_list_with<M: ListModel>(
        &mut self,
        key: impl Into<String>,
        model: &mut M,
        state: &mut ListState,
        options: VirtualListOptions,
        row: impl FnMut(&mut Ui<'_>, &M, usize, bool),
    ) -> ListResponse {
        self.collection(key.into(), model, state, options, CollectionKind::List, row)
    }

    /// Tree rows with custom content. The model supplies depth, expansion and
    /// parents; Right/Left and disclosure clicks call `set_expanded`.
    pub fn tree_view_with<M: ListModel>(
        &mut self,
        key: impl Into<String>,
        model: &mut M,
        state: &mut ListState,
        options: VirtualListOptions,
        row: impl FnMut(&mut Ui<'_>, &M, usize, bool),
    ) -> ListResponse {
        self.collection(key.into(), model, state, options, CollectionKind::Tree, row)
    }

    pub fn tree_view(
        &mut self,
        key: impl Into<String>,
        model: &mut impl ListModel,
        state: &mut ListState,
        options: VirtualListOptions,
    ) -> ListResponse {
        self.tree_view_with(key, model, state, options, |ui, model, index, _| {
            let prefix = match model.expanded(index) {
                Some(true) => "▾",
                Some(false) => "▸",
                None => " ",
            };
            ui.label_styled(
                "label",
                format!("{prefix}  {}", model.label(index)),
                Style {
                    width: Length::Percent(1.0),
                    padding: 6.0,
                    ..Style::default()
                },
            );
        })
    }

    pub fn data_grid(
        &mut self,
        key: impl Into<String>,
        model: &mut impl GridModel,
        state: &mut GridState,
        options: VirtualListOptions,
    ) -> ListResponse {
        let columns = model.columns().to_vec();
        assert!(
            !columns.is_empty()
                && columns
                    .iter()
                    .all(|column| column.width.is_finite() && column.width > 0.0),
            "grid columns need finite positive widths"
        );
        let mut response = ListResponse::default();
        let total_width = columns.iter().map(|column| column.width).sum::<f32>();
        self.group(
            key.into(),
            Kind::Column,
            Style {
                width: options.style.width,
                gap: 0.0,
                ..Style::default()
            },
            |ui| {
                ui.row(
                    "$header",
                    Style {
                        width: Length::Px(total_width),
                        height: Length::Px(options.row_height),
                        gap: 0.0,
                        ..Style::default()
                    },
                    |ui| {
                        for (index, column) in columns.iter().enumerate() {
                            let direction = state
                                .sort
                                .filter(|(selected, _)| *selected == index)
                                .map(|(_, direction)| direction);
                            let label = format!(
                                "{}{}",
                                column.label,
                                match direction {
                                    Some(SortDirection::Ascending) => " ↑",
                                    Some(SortDirection::Descending) => " ↓",
                                    None => "",
                                }
                            );
                            if ui
                                .button_styled(
                                    index.to_string(),
                                    label,
                                    column.sortable,
                                    Style {
                                        width: Length::Px(column.width),
                                        height: Length::Px(options.row_height),
                                        padding: 6.0,
                                        radius: 0.0,
                                        ..Style::button()
                                    },
                                )
                                .clicked()
                            {
                                let next = if direction == Some(SortDirection::Ascending) {
                                    SortDirection::Descending
                                } else {
                                    SortDirection::Ascending
                                };
                                if model.sort(index, next) {
                                    state.sort = Some((index, next));
                                }
                            }
                            ui.elements.last_mut().unwrap().control = Control::GridCell {
                                row: 0,
                                column: index,
                                header: true,
                                sort: state
                                    .sort
                                    .filter(|(selected, _)| *selected == index)
                                    .map(|(_, direction)| direction),
                            };
                        }
                    },
                );
                response = ui.collection(
                    "$rows".into(),
                    model,
                    &mut state.rows,
                    VirtualListOptions {
                        height: (options.height - options.row_height).max(1.0),
                        ..options.clone()
                    },
                    CollectionKind::Grid,
                    |ui, model, index, _| {
                        ui.row(
                            "$cells",
                            Style {
                                width: Length::Px(total_width),
                                gap: 0.0,
                                ..Style::default()
                            },
                            |ui| {
                                for (column, descriptor) in columns.iter().enumerate() {
                                    ui.label_styled(
                                        column.to_string(),
                                        model.cell(index, column),
                                        Style {
                                            width: Length::Px(descriptor.width),
                                            height: Length::Px(options.row_height),
                                            padding: 6.0,
                                            ..Style::default()
                                        },
                                    );
                                    ui.elements.last_mut().unwrap().control = Control::GridCell {
                                        row: index,
                                        column,
                                        header: false,
                                        sort: None,
                                    };
                                }
                            },
                        );
                    },
                );
            },
        );
        self.elements.last_mut().unwrap().control = Control::Grid {
            label: options.label,
            rows: model.len(),
            columns: columns.len(),
        };
        response
    }

    fn collection<M: ListModel>(
        &mut self,
        key: String,
        model: &mut M,
        state: &mut ListState,
        options: VirtualListOptions,
        kind: CollectionKind,
        mut row: impl FnMut(&mut Ui<'_>, &M, usize, bool),
    ) -> ListResponse {
        let owner = self.context.child(
            self.parent,
            &WidgetKey::Named(key.clone()),
            Kind::Collection,
        );
        let node = owner.and_then(|id| self.context.node(id));
        let input = owner.and_then(|id| self.updates.remove(&id));
        let input = match input {
            Some(ControlUpdate::Collection(input)) if self.enabled => input,
            _ => Vec::new(),
        };
        let response = state.update(
            model,
            input,
            node.and_then(|node| node.scroll.map(|scroll| scroll.offset)),
            &options,
        );
        let count = model.len();
        let selected = state.selected();
        let theme = self.context.theme();
        let (selection, even_row, odd_row) = (theme.selection, theme.row, theme.row_alternate);
        let style = Style {
            height: Length::Px(options.height),
            gap: 0.0,
            padding: 0.0,
            ..options.style.clone()
        };
        self.group(key, Kind::Collection, style, |ui| {
            ui.label_styled(
                "$before",
                "",
                Style {
                    height: Length::Px(response.realized.start as f32 * options.row_height),
                    gap: 0.0,
                    ..Style::default()
                },
            );
            for index in response.realized.clone() {
                let item = model.item_id(index);
                let is_selected = selected == Some(item);
                ui.group(
                    format!("item-{}", item.0),
                    Kind::CollectionRow,
                    Style {
                        width: Length::Percent(1.0),
                        height: Length::Px(options.row_height),
                        gap: 0.0,
                        background: Some(if is_selected {
                            selection
                        } else if index % 2 == 0 {
                            even_row
                        } else {
                            odd_row
                        }),
                        ..Style::default()
                    },
                    |ui| {
                        if kind == CollectionKind::Tree {
                            ui.row(
                                "$indent",
                                Style {
                                    gap: 0.0,
                                    ..Style::default()
                                },
                                |ui| {
                                    ui.label_styled(
                                        "$space",
                                        "",
                                        Style {
                                            width: Length::Px(
                                                (model.depth(index).min(100) * 18) as f32,
                                            ),
                                            ..Style::default()
                                        },
                                    );
                                    row(ui, model, index, is_selected);
                                },
                            );
                        } else {
                            row(ui, model, index, is_selected);
                        }
                    },
                );
                ui.elements.last_mut().unwrap().control = Control::CollectionRow {
                    label: model.label(index),
                    item,
                    index,
                    total: count,
                    selected: is_selected,
                    kind,
                    depth: model.depth(index),
                    expanded: model.expanded(index),
                };
            }
            ui.label_styled(
                "$after",
                "",
                Style {
                    height: Length::Px((count - response.realized.end) as f32 * options.row_height),
                    gap: 0.0,
                    ..Style::default()
                },
            );
        });
        self.elements.last_mut().unwrap().control = Control::Collection {
            label: options.label,
            kind,
            count,
            row_height: options.row_height,
            offset: state.offset,
            selected,
        };
        response
    }
}
