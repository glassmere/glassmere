use super::*;
use crate::{
    GridModel, GridState, Length, ListModel, ListResponse, ListState, SortDirection,
    VirtualListOptions,
    collection::{CollectionInput, CollectionKind},
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

    /// A virtual grid over a model. `state.columns` holds what the user did to
    /// the columns. Dragging the end edge of a header resizes its column,
    /// dragging a header onto another moves it there, and each header has a
    /// context menu (right-click, Shift+F10 or the Menu key) that moves,
    /// resizes, hides and restores columns from the keyboard.
    pub fn data_grid(
        &mut self,
        key: impl Into<String>,
        model: &mut impl GridModel,
        state: &mut GridState,
        options: VirtualListOptions,
    ) -> ListResponse {
        /// Width of the resize handle at the end of each header.
        const SIZER: f32 = 5.0;
        /// What one "Wider" or "Narrower" changes.
        const WIDTH_STEP: f32 = 20.0;
        enum HeaderAction {
            Sort(usize),
            Resize(usize, f32),
            /// The first column was dropped on the second.
            Drop(usize, usize),
            Menu(usize, usize),
        }
        let key = key.into();
        let columns = model.columns().to_vec();
        assert!(
            !columns.is_empty()
                && columns
                    .iter()
                    .all(|column| column.width.is_finite() && column.width > 0.0),
            "grid columns need finite positive widths"
        );
        let count = columns.len();
        let visible = state.columns.visible(count);
        let widths: Vec<f32> = visible
            .iter()
            .map(|column| state.columns.width(*column, columns[*column].width))
            .collect();
        let rtl = self.context.direction() == crate::Direction::Rtl;
        // Menu entries name physical directions; the start is on the right in
        // a right-to-left grid.
        let (toward_start, toward_end) = if self.context.direction() == crate::Direction::Rtl {
            ("rust-gui-grid-move-right", "rust-gui-grid-move-left")
        } else {
            ("rust-gui-grid-move-left", "rust-gui-grid-move-right")
        };
        let (toward_start, toward_end) = (
            self.context.tr(toward_start, &[]),
            self.context.tr(toward_end, &[]),
        );
        let invalid_number = self.context.tr("rust-gui-number-invalid", &[]);
        let domain = format!("grid-column:{key}");
        let mut response = ListResponse::default();
        let mut actions = Vec::new();

        // Filters. Typing waits for a pause; text set from code applies at once.
        let filterable = columns.iter().any(|column| column.filterable);
        let mut typed_filter = false;
        let wanted = |state: &GridState| -> Vec<(usize, String)> {
            state
                .filters
                .iter()
                .filter(|(column, _)| {
                    columns
                        .get(**column)
                        .is_some_and(|column| column.filterable)
                })
                .map(|(column, text)| (*column, text.trim().to_owned()))
                .filter(|(_, text)| !text.is_empty())
                .collect()
        };

        // Cell cursor and in-place editing. The rows' input queue also carries
        // what only a grid understands; take that out before the rows see the rest.
        let rows = self
            .context
            .child(self.parent, &WidgetKey::Named(key.clone()), Kind::Column)
            .and_then(|grid| {
                self.context.child(
                    Some(grid),
                    &WidgetKey::Named("$rows".into()),
                    Kind::Collection,
                )
            });
        let mut inputs = Vec::new();
        if let Some(rows) = rows
            && let Some(ControlUpdate::Collection(queue)) = self.updates.get_mut(&rows)
        {
            let editable = columns
                .get(state.cursor)
                .is_some_and(|column| column.editor.is_some());
            for input in std::mem::take(queue) {
                match input {
                    // Enter opens an editable cell; elsewhere it activates the row.
                    CollectionInput::Activate if editable => inputs.push(CollectionInput::Edit),
                    input if input.for_grid() => inputs.push(input),
                    input => queue.push(input),
                }
            }
        }
        if !self.enabled {
            inputs.clear();
        }
        if !visible.contains(&state.cursor) {
            state.cursor = visible[0];
        }
        let cursor_before = state.cursor;
        let mut edited = None;
        // Where the keyboard goes once this declaration is reconciled.
        let (mut focus_editor, mut focus_rows) = (false, false);
        let mut row_moves = Vec::new();
        for input in inputs {
            let mut start = None;
            match input {
                CollectionInput::Cell { item, position } => {
                    let Some(&column) = visible.get(position) else {
                        continue;
                    };
                    // A second press on the current cell opens it.
                    let again = state.edit.is_none()
                        && state.cursor == column
                        && state.rows.selected() == Some(item);
                    state.cursor = column;
                    start = again.then_some(None);
                }
                CollectionInput::Column(by) if state.edit.is_none() => {
                    let at = visible
                        .iter()
                        .position(|column| *column == state.cursor)
                        .unwrap_or(0);
                    state.cursor = visible[at.saturating_add_signed(by).min(visible.len() - 1)];
                }
                CollectionInput::ScrollX(delta) => {
                    state.scroll_x += if rtl { -delta } else { delta };
                }
                CollectionInput::Edit => start = Some(None),
                CollectionInput::Type(text) => start = Some(Some(text)),
                CollectionInput::EditKey { key, shift } => {
                    let step = if shift { -1 } else { 1 };
                    match key {
                        crate::Key::Escape => {
                            state.edit = None;
                            focus_rows = true;
                        }
                        // Enter and Tab store the value first; a refused value
                        // keeps the editor open and the cursor where it is.
                        _ if !commit_edit(
                            model,
                            state,
                            &columns,
                            &mut edited,
                            &invalid_number,
                            self.context,
                        ) => {}
                        crate::Key::Enter => {
                            row_moves.push(CollectionInput::Move(step));
                            focus_rows = true;
                        }
                        _ => {
                            let at = visible
                                .iter()
                                .position(|column| *column == state.cursor)
                                .unwrap_or(0);
                            state.cursor =
                                visible[at.saturating_add_signed(step).min(visible.len() - 1)];
                            focus_rows = true;
                        }
                    }
                }
                _ => {}
            }
            if let Some(typed) = start
                && state.edit.is_none()
            {
                focus_editor |=
                    begin_edit(self.context, model, state, &columns, typed, &mut edited);
            }
        }
        if let Some(rows) = rows
            && !row_moves.is_empty()
        {
            match self
                .updates
                .entry(rows)
                .or_insert_with(|| ControlUpdate::Collection(Vec::new()))
            {
                ControlUpdate::Collection(queue) => queue.extend(row_moves),
                _ => unreachable!("a collection only queues collection input"),
            }
        }
        // Which columns exist this frame. The first declaration does not know
        // its width yet and shows everything; the next one knows.
        let viewport = rows
            .and_then(|rows| self.context.node(rows))
            .map(|node| node.bounds.width);
        if viewport.is_none() {
            self.request_redraw();
        }
        let plan = ColumnPlan::new(
            &visible,
            &widths,
            state.columns.frozen,
            viewport.unwrap_or(f32::INFINITY),
            &mut state.scroll_x,
            (state.cursor != cursor_before).then_some(state.cursor),
            rtl,
        );
        let rows_focused = rows.is_some_and(|rows| self.context.focused() == Some(rows));
        let theme = self.context.theme();
        let (cursor_fill, error_fill) =
            (theme.control_hover, rust_gui_scene::Color::rgb(120, 40, 48));
        // Set by the editor in its cell, applied after the rows are declared:
        // a choice was picked, or the keyboard left the cell.
        let (mut picked, mut left) = (false, false);
        self.group(
            key,
            Kind::Column,
            Style {
                width: options.style.width,
                gap: 0.0,
                ..Style::default()
            },
            |ui| {
                spread(
                    ui,
                    "$header",
                    options.row_height,
                    &plan,
                    &mut |ui, position, column, width| {
                        {
                            let descriptor = &columns[column];
                            let direction = state
                                .sort
                                .filter(|(selected, _)| *selected == column)
                                .map(|(_, direction)| direction);
                            let label = format!(
                                "{}{}",
                                descriptor.label,
                                match direction {
                                    Some(SortDirection::Ascending) => " ↑",
                                    Some(SortDirection::Descending) => " ↓",
                                    None => "",
                                }
                            );
                            let cell = Style {
                                width: Length::Px(width),
                                height: Length::Px(options.row_height),
                                gap: 0.0,
                                ..Style::default()
                            };
                            let text = |key: &str| ui.context.tr(key, &[]);
                            let named = |key: &str| {
                                ui.context
                                    .tr(key, &[("name", descriptor.label.as_str().into())])
                            };
                            let resize_label = named("rust-gui-grid-resize");
                            let item = |label: &str, enabled| crate::MenuItem {
                                enabled,
                                ..crate::MenuItem::new(label)
                            };
                            let menu = [
                                item(&toward_start, position > 0),
                                item(&toward_end, position + 1 < visible.len()),
                                item(
                                    &text("rust-gui-grid-wider"),
                                    width < crate::GridColumns::MAX_WIDTH,
                                ),
                                item(
                                    &text("rust-gui-grid-narrower"),
                                    width > crate::GridColumns::MIN_WIDTH,
                                ),
                                item(&named("rust-gui-grid-hide"), visible.len() > 1),
                                item(&text("rust-gui-grid-show-all"), visible.len() < count),
                                item(&text("rust-gui-grid-reset"), true),
                                item(
                                    &named("rust-gui-grid-freeze"),
                                    state.columns.frozen != position + 1,
                                ),
                                item(&text("rust-gui-grid-unfreeze"), state.columns.frozen > 0),
                            ];
                            let actions = &mut actions;
                            let domain = &domain;
                            let chosen =
                                ui.context_menu(column.to_string(), &menu, cell.clone(), |ui| {
                                    ui.row("$cell", cell.clone(), |ui| {
                                        // Unnamed, so pointer-only: the menu is the keyboard path.
                                        let payload = crate::DragPayload::Item {
                                            domain: domain.clone(),
                                            id: crate::ItemId(column as u64),
                                        };
                                        let grip = cell.clone().width(Length::Px(width - SIZER));
                                        let dropped = ui.drop_target(
                                            "$drop",
                                            "",
                                            crate::DropKind::Item,
                                            grip.clone(),
                                            |ui| {
                                                ui.drag_source("$drag", "", payload, grip, |ui| {
                                                    let sort = Style {
                                                        padding: 6.0,
                                                        radius: 0.0,
                                                        ..Style::button()
                                                    }
                                                    .width(Length::Px(width - SIZER))
                                                    .height(Length::Px(options.row_height));
                                                    if ui
                                                        .button_styled(
                                                            "$sort",
                                                            label,
                                                            descriptor.sortable,
                                                            sort,
                                                        )
                                                        .clicked()
                                                    {
                                                        actions.push(HeaderAction::Sort(column));
                                                    }
                                                    ui.elements.last_mut().unwrap().control =
                                                        Control::GridCell {
                                                            row: 0,
                                                            column: position,
                                                            header: true,
                                                            sort: direction,
                                                        };
                                                });
                                            },
                                        );
                                        if let Some(crate::DragPayload::Item { domain: from, id }) =
                                            dropped
                                            && from == *domain
                                        {
                                            actions.push(HeaderAction::Drop(id.0 as usize, column));
                                        }
                                        if let Some(ControlUpdate::Number(size)) =
                                            ui.take_update("$size", Kind::Divider)
                                        {
                                            actions.push(HeaderAction::Resize(column, size as f32));
                                        }
                                        ui.push(Element {
                                            key: WidgetKey::Named("$size".into()),
                                            kind: Kind::Divider,
                                            text: String::new(),
                                            style: Style::default()
                                                .width(Length::Px(SIZER))
                                                .height(Length::Px(options.row_height)),
                                            enabled: ui.enabled,
                                            children: Vec::new(),
                                            custom: None,
                                            tooltip: None,
                                            control: Control::Sizer {
                                                label: resize_label.clone(),
                                                size: width,
                                                min: crate::GridColumns::MIN_WIDTH,
                                                max: crate::GridColumns::MAX_WIDTH,
                                            },
                                        });
                                    });
                                });
                            if let Some(chosen) = chosen {
                                actions.push(HeaderAction::Menu(column, chosen));
                            }
                        }
                    },
                );
                if filterable {
                    spread(
                        ui,
                        "$filters",
                        options.row_height,
                        &plan,
                        &mut |ui, _, column, width| {
                            let descriptor = &columns[column];
                            let field = Style {
                                padding: 4.0,
                                radius: 0.0,
                                ..Style::button()
                            }
                            .width(Length::Px(width))
                            .height(Length::Px(options.row_height));
                            if descriptor.filterable {
                                let text = state.filters.entry(column).or_default();
                                typed_filter |= ui
                                    .text_input_styled(
                                        column.to_string(),
                                        ui.context.tr(
                                            "rust-gui-grid-filter",
                                            &[("name", descriptor.label.as_str().into())],
                                        ),
                                        text,
                                        &ui.context.tr("rust-gui-filter-placeholder", &[]),
                                        field,
                                    )
                                    .changed;
                            } else {
                                ui.label_styled(
                                    column.to_string(),
                                    "",
                                    field.background(rust_gui_scene::Color::TRANSPARENT),
                                );
                            }
                        },
                    );
                }
                let header_rows = if filterable { 2.0 } else { 1.0 };
                response = ui.collection(
                    "$rows".into(),
                    model,
                    &mut state.rows,
                    VirtualListOptions {
                        height: (options.height
                            - options.row_height * header_rows
                            - plan.bar_height())
                        .max(1.0),
                        ..options.clone()
                    },
                    CollectionKind::Grid,
                    |ui, model, index, is_selected| {
                        let item = model.item_id(index);
                        spread(
                            ui,
                            "$cells",
                            options.row_height,
                            &plan,
                            &mut |ui, position, column, width| {
                                {
                                    let cell = Style {
                                        width: Length::Px(width),
                                        height: Length::Px(options.row_height),
                                        padding: 6.0,
                                        ..Style::default()
                                    };
                                    if let Some(edit) = state
                                        .edit
                                        .as_mut()
                                        .filter(|edit| edit.item == item && edit.column == column)
                                    {
                                        // The editor takes the cell's place.
                                        let descriptor = &columns[column];
                                        let mut style = Style {
                                            padding: 4.0,
                                            radius: 0.0,
                                            ..Style::button()
                                        }
                                        .width(Length::Px(width))
                                        .height(Length::Px(options.row_height));
                                        if edit.error.is_some() {
                                            style.background = Some(error_fill);
                                        }
                                        let response = match &descriptor.editor {
                                            Some(crate::CellEditor::Choice(choices)) => {
                                                let choices: Vec<&str> =
                                                    choices.iter().map(String::as_str).collect();
                                                let response = ui.combo_box_styled(
                                                    "$edit",
                                                    descriptor.label.clone(),
                                                    &mut edit.choice,
                                                    &choices,
                                                    style,
                                                );
                                                // Picking a choice is the whole edit.
                                                picked |= response.changed;
                                                response
                                            }
                                            _ => ui.text_input_styled(
                                                "$edit",
                                                descriptor.label.clone(),
                                                &mut edit.text,
                                                "",
                                                style,
                                            ),
                                        };
                                        if let Some(error) = &edit.error {
                                            ui.tooltip(error.clone());
                                        }
                                        if focus_editor {
                                            ui.request_focus("$edit");
                                        }
                                        // Leaving the cell any other way stores the value too.
                                        left |= edit.focused && !response.focused;
                                        edit.focused |= response.focused;
                                        return;
                                    }
                                    let current =
                                        is_selected && column == state.cursor && rows_focused;
                                    ui.label_styled(
                                        column.to_string(),
                                        model.cell(index, column),
                                        Style {
                                            background: current.then_some(cursor_fill),
                                            ..cell
                                        },
                                    );
                                    ui.elements.last_mut().unwrap().control = Control::GridCell {
                                        row: index,
                                        column: position,
                                        header: false,
                                        sort: None,
                                    };
                                }
                            },
                        );
                    },
                );
                plan.scrollbar(ui, rtl);
                if focus_rows || std::mem::take(&mut state.refocus) {
                    ui.request_focus("$rows");
                }
            },
        );
        if (picked || left)
            && commit_edit(
                model,
                state,
                &columns,
                &mut edited,
                &invalid_number,
                self.context,
            )
            && picked
        {
            // The combo box that had the keyboard is gone with the editor:
            // the next declaration hands the keyboard back to the rows.
            state.refocus = true;
            self.request_redraw();
        }
        response.edited = edited;
        // The model hears about filter text once typing pauses, and at once
        // when the app changed it.
        let now = self.context.now();
        if typed_filter {
            state.filter_due = Some(now + crate::FILTER_DELAY);
        }
        let wanted = wanted(state);
        if wanted != state.applied_filters {
            if state.filter_due.is_none_or(|due| now >= due) {
                let pairs: Vec<(usize, &str)> = wanted
                    .iter()
                    .map(|(column, text)| (*column, text.as_str()))
                    .collect();
                model.filter(&pairs);
                state.applied_filters = wanted;
                state.filter_due = None;
            }
            // ponytail: frames until the delay is over; a context timer would
            // let the window sleep through it.
            self.request_redraw();
        } else {
            state.filter_due = None;
        }
        self.elements.last_mut().unwrap().control = Control::Grid {
            label: options.label,
            rows: model.len(),
            columns: visible.len(),
        };
        // The header only recorded what happened; the state changes here,
        // outside the closures that were reading it.
        for action in actions {
            match action {
                HeaderAction::Sort(column) => {
                    let ascending = state.sort == Some((column, SortDirection::Ascending));
                    let next = if ascending {
                        SortDirection::Descending
                    } else {
                        SortDirection::Ascending
                    };
                    if model.sort(column, next) {
                        state.sort = Some((column, next));
                    }
                }
                HeaderAction::Resize(column, width) => state.columns.set_width(column, width),
                HeaderAction::Drop(moved, onto) => {
                    // A column dragged toward the end lands after its target,
                    // one dragged toward the start lands before it.
                    let place = |column| visible.iter().position(|other| *other == column);
                    let before = if place(moved) < place(onto) {
                        place(onto).and_then(|at| visible.get(at + 1).copied())
                    } else {
                        Some(onto)
                    };
                    state.columns.move_before(count, moved, before);
                }
                HeaderAction::Menu(column, chosen) => {
                    let width = state.columns.width(column, columns[column].width);
                    match chosen {
                        0 => drop(state.columns.shift(count, column, -1)),
                        1 => drop(state.columns.shift(count, column, 1)),
                        2 => state.columns.set_width(column, width + WIDTH_STEP),
                        3 => state.columns.set_width(column, width - WIDTH_STEP),
                        4 => drop(state.columns.hide(count, column)),
                        5 => state.columns.show_all(),
                        6 => state.columns.reset(),
                        7 => {
                            let at = visible.iter().position(|other| *other == column);
                            state.columns.frozen = at.map_or(0, |at| at + 1);
                        }
                        _ => state.columns.frozen = 0,
                    }
                }
            }
        }
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

/// Which columns a grid declares this frame and where they go: the frozen
/// ones in place, then a window onto the rest with only the columns that show.
struct ColumnPlan {
    /// Display position, model column and width.
    frozen: Vec<(usize, usize, f32)>,
    realized: Vec<(usize, usize, f32)>,
    /// Width of the window onto the scrolling columns, and of all of them.
    view: f32,
    scrolling: f32,
    scroll_x: f32,
    /// Where the strip of realized columns sits in the window, from its left edge.
    strip_x: f32,
}

impl ColumnPlan {
    fn new(
        visible: &[usize],
        widths: &[f32],
        frozen: usize,
        viewport: f32,
        scroll_x: &mut f32,
        reveal: Option<usize>,
        rtl: bool,
    ) -> Self {
        let columns: Vec<(usize, usize, f32)> = visible
            .iter()
            .zip(widths)
            .enumerate()
            .map(|(position, (column, width))| (position, *column, *width))
            .collect();
        let (frozen, rest) = columns.split_at(frozen.min(columns.len()));
        let frozen_width: f32 = frozen.iter().map(|column| column.2).sum();
        let scrolling: f32 = rest.iter().map(|column| column.2).sum();
        let view = (viewport - frozen_width).clamp(0.0, scrolling);
        // Keep the current cell in view when the cursor moved to it.
        let mut start = 0.0;
        for (_, column, width) in rest {
            if reveal == Some(*column) {
                *scroll_x = scroll_x.min(start).max(start + width - view).min(start);
            }
            start += width;
        }
        *scroll_x = if scroll_x.is_finite() {
            scroll_x.clamp(0.0, (scrolling - view).max(0.0))
        } else {
            0.0
        };
        let (mut start, mut first, mut realized) = (0.0, None, Vec::new());
        for column in rest {
            if start + column.2 > *scroll_x && start < *scroll_x + view {
                first.get_or_insert(start);
                realized.push(*column);
            }
            start += column.2;
        }
        let hidden_before = *scroll_x - first.unwrap_or(0.0);
        let realized_width: f32 = realized.iter().map(|column| column.2).sum();
        Self {
            frozen: frozen.to_vec(),
            realized,
            view,
            scrolling,
            scroll_x: *scroll_x,
            // Offsets are physical: in a right-to-left grid the strip hangs
            // from the right edge of the window.
            strip_x: if rtl {
                view - realized_width + hidden_before
            } else {
                -hidden_before
            },
        }
    }

    fn bar_height(&self) -> f32 {
        if self.scrolling > self.view { 6.0 } else { 0.0 }
    }

    /// The sideways scrollbar under the rows, beside the frozen columns.
    fn scrollbar(&self, ui: &mut Ui<'_>, rtl: bool) {
        if self.bar_height() == 0.0 {
            return;
        }
        let frozen: f32 = self.frozen.iter().map(|column| column.2).sum();
        let thumb = (self.view * self.view / self.scrolling)
            .max(12.0)
            .min(self.view);
        let travelled = (self.view - thumb) * self.scroll_x / (self.scrolling - self.view);
        let x = if rtl {
            self.view - thumb - travelled
        } else {
            travelled
        };
        let color = ui.context.theme().scrollbar;
        let line = Style::default()
            .height(Length::Px(self.bar_height()))
            .gap(0.0);
        ui.row("$hbar", line.clone(), |ui| {
            ui.label_styled("$frozen", "", line.clone().width(Length::Px(frozen)));
            ui.container("$track", line.clone().width(Length::Px(self.view)), |ui| {
                let thumb = Style::default()
                    .offset(x, 1.0)
                    .width(Length::Px(thumb))
                    .height(Length::Px(4.0))
                    .radius(2.0)
                    .background(color);
                ui.container("$thumb", thumb, |_| {});
            });
        });
    }
}

/// One line of a grid: the frozen cells, then the window onto the scrolling
/// ones. The header, the filter row and every body row use it, so they line up.
fn spread(
    ui: &mut Ui<'_>,
    key: &str,
    height: f32,
    plan: &ColumnPlan,
    cell: &mut dyn FnMut(&mut Ui<'_>, usize, usize, f32),
) {
    let line = Style::default().height(Length::Px(height)).gap(0.0);
    ui.row(key, line.clone(), |ui| {
        for &(position, column, width) in &plan.frozen {
            cell(ui, position, column, width);
        }
        ui.container(
            "$scrolling",
            line.clone().width(Length::Px(plan.view)),
            |ui| {
                ui.row("$strip", line.clone().offset(plan.strip_x, 0.0), |ui| {
                    for &(position, column, width) in &plan.realized {
                        cell(ui, position, column, width);
                    }
                });
            },
        );
    });
}

/// Opens the editor of the current cell, or toggles it when it is a bool.
/// `typed` replaces the content of a text or number cell. Returns whether an
/// editor opened and wants the keyboard.
fn begin_edit(
    context: &crate::Context,
    model: &mut impl GridModel,
    state: &mut GridState,
    columns: &[crate::GridColumn],
    typed: Option<String>,
    edited: &mut Option<(crate::ItemId, usize)>,
) -> bool {
    let column = state.cursor;
    let Some(item) = state.rows.selected() else {
        return false;
    };
    let (Some(row), Some(editor)) = (
        model.index_of(item),
        columns
            .get(column)
            .and_then(|column| column.editor.as_ref()),
    ) else {
        return false;
    };
    let value = model.value(row, column);
    let (text, choice) = match (editor, value) {
        (crate::CellEditor::Bool, value) => {
            let next = !matches!(value, crate::CellValue::Bool(true));
            if model
                .set_cell(row, column, crate::CellValue::Bool(next))
                .is_ok()
            {
                *edited = Some((item, column));
            }
            return false;
        }
        (crate::CellEditor::Choice(choices), value) => {
            let choice = match value {
                crate::CellValue::Choice(choice) => choice,
                _ => 0,
            };
            (String::new(), choice.min(choices.len().saturating_sub(1)))
        }
        // The draft is the bare number: the unit is shown, not typed.
        (crate::CellEditor::Number(options), crate::CellValue::Number(number)) => {
            (options.bare_in(context, number), 0)
        }
        (_, crate::CellValue::Text(text)) => (text, 0),
        _ => (model.cell(row, column), 0),
    };
    state.edit = Some(crate::collection::GridEdit {
        item,
        column,
        text: typed.unwrap_or(text),
        choice,
        error: None,
        focused: false,
    });
    true
}

/// Hands the open editor's value to the model. Returns whether the edit is
/// over: the model took the value, or there was nothing to store.
fn commit_edit(
    model: &mut impl GridModel,
    state: &mut GridState,
    columns: &[crate::GridColumn],
    edited: &mut Option<(crate::ItemId, usize)>,
    // What to say about text that is not a number, in the user's language.
    invalid: &str,
    context: &crate::Context,
) -> bool {
    let Some(edit) = &mut state.edit else {
        return true;
    };
    let (Some(row), Some(editor)) = (
        model.index_of(edit.item),
        columns
            .get(edit.column)
            .and_then(|column| column.editor.as_ref()),
    ) else {
        // The row or the column went away while it was being edited.
        state.edit = None;
        return true;
    };
    let value = match editor {
        crate::CellEditor::Number(options) => match options.parse_in(context, &edit.text) {
            Some(number) => crate::CellValue::Number(number),
            None => {
                edit.error = Some(invalid.to_owned());
                return false;
            }
        },
        crate::CellEditor::Choice(_) => crate::CellValue::Choice(edit.choice),
        _ => crate::CellValue::Text(edit.text.clone()),
    };
    match model.set_cell(row, edit.column, value) {
        Ok(()) => {
            *edited = Some((edit.item, edit.column));
            state.edit = None;
            true
        }
        Err(error) => {
            edit.error = Some(error);
            false
        }
    }
}
