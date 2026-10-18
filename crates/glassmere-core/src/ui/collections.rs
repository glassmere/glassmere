use super::*;
use super::{DragSourceBuilder, DropTargetBuilder};
use crate::{
    GridModel, GridMsg, GridState, Length, ListModel, ListState, SortDirection, VirtualListOptions,
    collection::{After, CollectionInput, CollectionKind, FilterNote, GridNode, NodeEdit},
};
use std::rc::Rc;

impl<'a, Msg: 'static> Ui<'a, Msg> {
    /// A list whose selection is sent as [`GridMsg::Select`], Enter on a row
    /// as [`GridMsg::Activate`], and a selection the model lost as
    /// [`GridMsg::Deselect`]. The list keeps no state of its own that its view
    /// changes.
    #[track_caller]
    pub fn virtual_list(
        &mut self,
        model: &impl ListModel,
        state: &ListState,
        options: VirtualListOptions,
        on: impl Fn(GridMsg) -> Msg + 'static,
    ) {
        self.virtual_list_with(model, state, options, on, |ui, model, index, _| {
            ui.label_at(
                "label",
                model.label(index),
                Style {
                    width: Length::Percent(1.0),
                    padding: 6.0,
                    ..Style::default()
                },
            );
        });
    }

    /// [`virtual_list`](Self::virtual_list) with the content of a row
    /// declared by `row`, from the model, the row's index and whether it is
    /// selected. The row's key is up to the closure.
    #[track_caller]
    pub fn virtual_list_with<M: ListModel>(
        &mut self,
        model: &M,
        state: &ListState,
        options: VirtualListOptions,
        on: impl Fn(GridMsg) -> Msg + 'static,
        row: impl FnMut(&mut Ui<'_, Msg>, &M, usize, bool),
    ) {
        let key = self.next_key();
        let on: Rc<dyn Fn(GridMsg) -> Msg> = Rc::new(on);
        let nav = self.rows(key, model, state, options, CollectionKind::List, row);
        self.set_handler(rows_handler(on, nav));
    }

    /// A tree whose selection and disclosure, by pointer or keyboard, are sent
    /// as [`GridMsg::Select`] and [`GridMsg::Expand`], with the activation and
    /// the lost selection of [`virtual_list`](Self::virtual_list).
    #[track_caller]
    pub fn tree_view(
        &mut self,
        model: &impl ListModel,
        state: &ListState,
        options: VirtualListOptions,
        on: impl Fn(GridMsg) -> Msg + 'static,
    ) {
        self.tree_view_with(model, state, options, on, |ui, model, index, _| {
            let prefix = match model.expanded(index) {
                Some(true) => "▾",
                Some(false) => "▸",
                None => " ",
            };
            ui.label_at(
                "label",
                format!("{prefix}  {}", model.label(index)),
                Style {
                    width: Length::Percent(1.0),
                    padding: 6.0,
                    ..Style::default()
                },
            );
        });
    }

    /// [`tree_view`](Self::tree_view) with the content of a row
    /// declared by `row`, after the indent; the disclosure mark is the
    /// closure's to draw.
    #[track_caller]
    pub fn tree_view_with<M: ListModel>(
        &mut self,
        model: &M,
        state: &ListState,
        options: VirtualListOptions,
        on: impl Fn(GridMsg) -> Msg + 'static,
        row: impl FnMut(&mut Ui<'_, Msg>, &M, usize, bool),
    ) {
        let key = self.next_key();
        let on: Rc<dyn Fn(GridMsg) -> Msg> = Rc::new(on);
        let nav = self.rows(key, model, state, options, CollectionKind::Tree, row);
        self.set_handler(rows_handler(on, nav));
    }

    /// A virtual grid whose changes are sent as [`GridMsg`]s, shown from
    /// `state`, which the view does not change. Dragging a header's end edge
    /// sends `ResizeColumn`, a header clicked sends `Sort`, a header dropped
    /// on another sends `MoveColumn`, the header menu sends `HideColumn`,
    /// `ShowAllColumns`, `ResetColumns` or `FreezeColumns`, a
    /// row chosen by pointer or keyboard sends `Select`, and typing in a
    /// filter field sends `Filter`, which the grid turns into
    /// `FiltersApplied` once typing pauses; filter text the application set
    /// itself applies at once. Committing a cell edit sends `Edit`, and Enter
    /// on a cell that cannot be edited sends `Activate`.
    ///
    /// The cell cursor, the sideways scroll and the cell being edited belong
    /// to the grid's node, not to `state`. An edit stays open, showing its
    /// draft, until the model's value for the cell differs from what it was
    /// when the edit began, so a `set_cell` that refuses the value leaves the
    /// editor open. Report the refusal with [`GridState::refuse_edit`] and the
    /// editor shows it and accepts another commit; Escape cancels the edit.
    /// Messages that follow a commit (the edit itself, the move to the next
    /// row) are sent one batch later.
    #[track_caller]
    pub fn data_grid(
        &mut self,
        model: &impl GridModel,
        state: &GridState,
        options: VirtualListOptions,
        on: impl Fn(GridMsg) -> Msg + 'static,
    ) {
        /// Width of the resize handle at the end of each header.
        const SIZER: f32 = 5.0;
        /// What one "Wider" or "Narrower" changes.
        const WIDTH_STEP: f32 = 20.0;
        let on: Rc<dyn Fn(GridMsg) -> Msg> = Rc::new(on);
        let key = self.next_key();
        let columns = model.columns().to_vec();
        if columns.is_empty()
            || !columns
                .iter()
                .all(|column| column.width.is_finite() && column.width > 0.0)
        {
            self.misuse("grid columns need finite positive widths".to_owned());
            return;
        }
        let count = columns.len();
        let visible = state.columns.visible(count);
        let widths: Vec<f32> = visible
            .iter()
            .map(|column| state.columns.width(*column, columns[*column].width))
            .collect();
        let rtl = self.context.direction() == crate::Direction::Rtl;
        // Menu entries name physical directions; the start is on the right in
        // a right-to-left grid.
        let (toward_start, toward_end) = if rtl {
            ("glassmere-grid-move-right", "glassmere-grid-move-left")
        } else {
            ("glassmere-grid-move-left", "glassmere-grid-move-right")
        };
        let (toward_start, toward_end) = (
            self.context.tr(toward_start, &[]),
            self.context.tr(toward_end, &[]),
        );
        let invalid_number = self.context.tr("glassmere-number-invalid", &[]);
        let domain = format!("grid-column:{}", key.describe());
        let filterable = columns.iter().any(|column| column.filterable);
        let rows = self
            .context
            .child(self.parent, &key, Kind::Column)
            .and_then(|grid| {
                self.context.child(
                    Some(grid),
                    &WidgetKey::Named("$rows".into()),
                    Kind::Collection,
                )
            });

        // The cursor, the sideways scroll and the open editor are the grid
        // node's. What only a grid understands arrives in its inbox.
        let mut grid = rows
            .map(|rows| self.context.take_grid(rows))
            .unwrap_or_default();
        let selected = state.rows.selected();
        // What the next batch is to send: the edits, and the row to go to.
        let mut sends = Vec::new();
        let mut inputs = std::mem::take(&mut grid.inbox);
        if !self.enabled {
            inputs.clear();
        }
        if !visible.contains(&grid.cursor) {
            grid.cursor = visible[0];
        }
        let editable = columns
            .get(grid.cursor)
            .is_some_and(|column| column.editor.is_some());
        let cursor_before = grid.cursor;
        // Where the keyboard goes once this declaration is reconciled.
        let (mut focus_editor, mut focus_rows) = (false, false);
        for input in inputs {
            let mut start = None;
            match input {
                // Enter opens an editable cell; elsewhere it activates the row.
                CollectionInput::Activate if editable => start = Some(None),
                CollectionInput::Activate => {
                    if let Some(item) = selected
                        && grid.edit.is_none()
                    {
                        sends.push(CollectionInput::Activated(item));
                    }
                }
                CollectionInput::Cell { item, position } => {
                    let Some(&column) = visible.get(position) else {
                        continue;
                    };
                    // A second press on the current cell opens it.
                    let again =
                        grid.edit.is_none() && grid.cursor == column && grid.shown == Some(item);
                    grid.cursor = column;
                    start = again.then_some(None);
                }
                CollectionInput::Column(by) if grid.edit.is_none() => {
                    let at = visible
                        .iter()
                        .position(|column| *column == grid.cursor)
                        .unwrap_or(0);
                    grid.cursor = visible[at.saturating_add_signed(by).min(visible.len() - 1)];
                }
                CollectionInput::ScrollX(delta) => {
                    grid.scroll_x += if rtl { -delta } else { delta };
                }
                CollectionInput::Edit => start = Some(None),
                CollectionInput::Type(text) => start = Some(Some(text)),
                CollectionInput::EditKey { key, shift } => {
                    let step = if shift { -1 } else { 1 };
                    if key == crate::Key::Escape {
                        grid.edit = None;
                        focus_rows = true;
                    } else {
                        // Enter and Tab send the value first; a refused value
                        // keeps the editor open and the cursor where it is.
                        let after = if key == crate::Key::Enter {
                            After::Row(step)
                        } else {
                            After::Column(step)
                        };
                        if commit_edit(
                            model,
                            &mut grid,
                            &columns,
                            &invalid_number,
                            after,
                            &mut sends,
                            self.context,
                        ) {
                            finish_edit(after, &mut grid, &visible, &mut sends, &mut focus_rows);
                        }
                    }
                }
                _ => {}
            }
            if let Some(typed) = start
                && grid.edit.is_none()
            {
                focus_editor |= begin_edit(
                    self.context,
                    model,
                    &mut grid,
                    &columns,
                    selected,
                    typed,
                    &mut sends,
                );
            }
        }
        // A refusal the application reported answers the edit that waits for
        // it: the editor stays, shows why, and can be committed again.
        if state.refusals != grid.refusals_seen {
            grid.refusals_seen = state.refusals;
            if let (Some(edit), Some((item, column, message))) = (&mut grid.edit, &state.refusal)
                && edit.waiting.is_some()
                && edit.item == *item
                && edit.column == *column
            {
                edit.error = Some(message.clone());
                edit.waiting = None;
            }
        }
        // An edit the model has taken is over.
        if let Some(edit) = &grid.edit
            && let Some(after) = edit.waiting
            && model
                .index_of(edit.item)
                .is_none_or(|row| model.value(row, edit.column) != edit.original)
        {
            grid.edit = None;
            finish_edit(after, &mut grid, &visible, &mut sends, &mut focus_rows);
        }

        // Filters: typing waits for a pause, which the kernel times; text the
        // application set applies at once. The view reports what is pending.
        let wanted = state.wanted_filters();
        let pending = wanted.as_slice() != state.applied_filters();
        let mut typed_filter = false;
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
            &mut grid.scroll_x,
            (grid.cursor != cursor_before).then_some(grid.cursor),
            rtl,
        );
        let rows_focused = rows.is_some_and(|rows| self.context.focused() == Some(rows));
        let theme = self.context.theme();
        let (cursor_fill, error_fill) = (
            theme.control_hover,
            glassmere_scene::Color::rgb(120, 40, 48),
        );
        let layout = Rc::new(state.columns.clone());
        let visible_shared = Rc::new(visible.clone());
        // Set by the editor in its cell, applied after the rows are declared:
        // a choice was picked, or the keyboard left the cell.
        let (mut picked, mut left) = (false, false);
        let sorted = state.sort;
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
                        let descriptor = &columns[column];
                        let direction = sorted
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
                        let resize_label = named("glassmere-grid-resize");
                        let item = |label: &str, enabled| crate::MenuItem {
                            enabled,
                            ..crate::MenuItem::new(label)
                        };
                        let flags = [
                            position > 0,
                            position + 1 < visible.len(),
                            width < crate::GridColumns::MAX_WIDTH,
                            width > crate::GridColumns::MIN_WIDTH,
                            visible.len() > 1,
                            visible.len() < count,
                            true,
                            state.columns.frozen != position + 1,
                            state.columns.frozen > 0,
                        ];
                        let menu = [
                            item(&toward_start, flags[0]),
                            item(&toward_end, flags[1]),
                            item(&text("glassmere-grid-wider"), flags[2]),
                            item(&text("glassmere-grid-narrower"), flags[3]),
                            item(&named("glassmere-grid-hide"), flags[4]),
                            item(&text("glassmere-grid-show-all"), flags[5]),
                            item(&text("glassmere-grid-reset"), flags[6]),
                            item(&named("glassmere-grid-freeze"), flags[7]),
                            item(&text("glassmere-grid-unfreeze"), flags[8]),
                        ];
                        // A pick sends what it does to the columns.
                        let (on_pick, layout, visible_now) =
                            (on.clone(), layout.clone(), visible_shared.clone());
                        let default_width = descriptor.width;
                        let pick = crate::app::Handler::Map(Box::new(move |update| {
                            let ControlUpdate::Choice(index) = update else {
                                return None;
                            };
                            if !flags.get(index).copied().unwrap_or(false) {
                                return None;
                            }
                            let width = layout.width(column, default_width);
                            let message = match index {
                                0 | 1 => {
                                    let mut next = (*layout).clone();
                                    next.shift(count, column, if index == 0 { -1 } else { 1 });
                                    GridMsg::MoveColumn(next.order)
                                }
                                2 => GridMsg::ResizeColumn(column, width + WIDTH_STEP),
                                3 => GridMsg::ResizeColumn(column, width - WIDTH_STEP),
                                4 => GridMsg::HideColumn(column),
                                5 => GridMsg::ShowAllColumns,
                                6 => GridMsg::ResetColumns,
                                7 => {
                                    let at = visible_now.iter().position(|other| *other == column);
                                    GridMsg::FreezeColumns(at.map_or(0, |at| at + 1))
                                }
                                _ => GridMsg::FreezeColumns(0),
                            };
                            Some(on_pick(message))
                        }));
                        let domain = &domain;
                        ui.at(column.to_string()).context_menu_with(
                            &menu,
                            cell.clone(),
                            pick,
                            |ui| {
                                ui.row_at("$cell", cell.clone(), |ui| {
                                    // Unnamed, so pointer-only: the menu is the keyboard path.
                                    let payload = crate::DragPayload::Item {
                                        domain: domain.clone(),
                                        id: crate::ItemId(column as u64),
                                    };
                                    let grip = cell.clone().width(Length::Px(width - SIZER));
                                    // The drop's handler follows: it takes only this
                                    // grid's columns.
                                    let target = DropTargetBuilder::new("", crate::DropKind::Item);
                                    ui.at("$drop").drop_target(grip.clone(), target, |ui| {
                                        let source = DragSourceBuilder::new("", payload);
                                        ui.at("$drag").drag_source(grip, source, |ui| {
                                            let sort = Style {
                                                padding: 6.0,
                                                radius: 0.0,
                                                ..Style::button()
                                            }
                                            .width(Length::Px(width - SIZER))
                                            .height(Length::Px(options.row_height));
                                            ui.button_at("$sort", label, descriptor.sortable, sort);
                                            if descriptor.sortable {
                                                let next = if direction
                                                    == Some(SortDirection::Ascending)
                                                {
                                                    SortDirection::Descending
                                                } else {
                                                    SortDirection::Ascending
                                                };
                                                ui.set_handler(crate::app::Handler::Message(on(
                                                    GridMsg::Sort(column, next),
                                                )));
                                            }
                                            ui.set_control(Control::GridCell {
                                                row: 0,
                                                column: position,
                                                header: true,
                                                sort: direction,
                                            });
                                        });
                                    });
                                    let arranged = state.columns.arranged(count);
                                    let (drop_domain, visible_now, on_drop) =
                                        (domain.clone(), visible.clone(), on.clone());
                                    ui.set_handler(crate::app::Handler::Map(Box::new(
                                        move |update| match update {
                                            ControlUpdate::Drop(crate::DragPayload::Item {
                                                domain: from,
                                                id,
                                            }) if from == drop_domain => {
                                                Some(on_drop(GridMsg::MoveColumn(moved_order(
                                                    &arranged,
                                                    &visible_now,
                                                    id.0 as usize,
                                                    column,
                                                ))))
                                            }
                                            _ => None,
                                        },
                                    )));
                                    ui.push(Element {
                                        style: Style::default()
                                            .width(Length::Px(SIZER))
                                            .height(Length::Px(options.row_height)),
                                        enabled: ui.enabled,
                                        control: Control::Sizer {
                                            label: resize_label.clone(),
                                            size: width,
                                            min: crate::GridColumns::MIN_WIDTH,
                                            max: crate::GridColumns::MAX_WIDTH,
                                        },
                                        ..Element::new(
                                            WidgetKey::Named("$size".into()),
                                            Kind::Divider,
                                        )
                                    });
                                    let on_resize = on.clone();
                                    ui.set_handler(crate::app::Handler::Map(Box::new(
                                        move |update| match update {
                                            ControlUpdate::Number(size) => Some(on_resize(
                                                GridMsg::ResizeColumn(column, size as f32),
                                            )),
                                            _ => None,
                                        },
                                    )));
                                });
                            },
                        );
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
                                typed_filter |= ui
                                    .context
                                    .child(
                                        ui.parent,
                                        &WidgetKey::Named(column.to_string()),
                                        Kind::TextInput,
                                    )
                                    .is_some_and(|id| ui.context.text_typed(id));
                                let mut shown =
                                    state.filters.get(&column).cloned().unwrap_or_default();
                                ui.text_input_at(
                                    column.to_string(),
                                    ui.context.tr(
                                        "glassmere-grid-filter",
                                        &[("name", descriptor.label.as_str().into())],
                                    ),
                                    &mut shown,
                                    &ui.context.tr("glassmere-filter-placeholder", &[]),
                                    field,
                                );
                                let on_filter = on.clone();
                                ui.set_handler(crate::app::Handler::Field {
                                    change: Some(Box::new(move |update| match update {
                                        ControlUpdate::Text(text) => {
                                            Some(on_filter(GridMsg::Filter { column, text }))
                                        }
                                        _ => None,
                                    })),
                                    submit: None,
                                });
                            } else {
                                ui.label_at(
                                    column.to_string(),
                                    "",
                                    field.background(glassmere_scene::Color::TRANSPARENT),
                                );
                            }
                        },
                    );
                }
                let header_rows = if filterable { 2.0 } else { 1.0 };
                let nav = ui.rows(
                    WidgetKey::named("$rows"),
                    model,
                    &state.rows,
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
                                let cell = Style {
                                    width: Length::Px(width),
                                    height: Length::Px(options.row_height),
                                    padding: 6.0,
                                    ..Style::default()
                                };
                                let cursor = grid.cursor;
                                if let Some(edit) = grid
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
                                            let (response, changed) = ui.combo_box_at(
                                                "$edit",
                                                descriptor.label.clone(),
                                                &mut edit.choice,
                                                &choices,
                                                style,
                                            );
                                            // Picking a choice is the whole edit.
                                            picked |= changed;
                                            response
                                        }
                                        _ => {
                                            ui.text_input_at(
                                                "$edit",
                                                descriptor.label.clone(),
                                                &mut edit.text,
                                                "",
                                                style,
                                            )
                                            .0
                                        }
                                    };
                                    if let Some(error) = &edit.error {
                                        ui.tooltip(error.clone());
                                    }
                                    if focus_editor {
                                        ui.request_focus("$edit");
                                    }
                                    // Leaving the cell any other way sends the value too.
                                    left |= edit.focused && !response.focused;
                                    edit.focused |= response.focused;
                                    return;
                                }
                                let current = is_selected && column == cursor && rows_focused;
                                ui.label_at(
                                    column.to_string(),
                                    model.cell(index, column),
                                    Style {
                                        background: current.then_some(cursor_fill),
                                        ..cell
                                    },
                                );
                                ui.set_control(Control::GridCell {
                                    row: index,
                                    column: position,
                                    header: false,
                                    sort: None,
                                });
                            },
                        );
                    },
                );
                ui.set_handler(rows_handler(on.clone(), nav));
                plan.scrollbar(ui, rtl);
                if focus_rows || std::mem::take(&mut grid.refocus) {
                    ui.request_focus("$rows");
                }
            },
        );
        if picked || left {
            let after = if picked {
                After::Refocus
            } else {
                After::Nothing
            };
            if commit_edit(
                model,
                &mut grid,
                &columns,
                &invalid_number,
                after,
                &mut sends,
                self.context,
            ) {
                // The combo box that had the keyboard is gone with the editor:
                // the next declaration hands the keyboard back to the rows.
                finish_edit(after, &mut grid, &visible, &mut sends, &mut focus_rows);
                if picked {
                    self.request_redraw();
                }
            }
        }
        let queued = !sends.is_empty();
        // Filters this grid already sent and the application has not applied
        // are not sent again, and do not keep frames coming.
        let mut waiting = pending;
        if let Some(rows) = rows {
            if pending {
                if typed_filter || self.context.grid_filters_waiting(rows) {
                    self.context
                        .note_grid_filters(rows, FilterNote::Waiting(wanted));
                } else if self.context.grid_filters_sent(rows, &wanted) {
                    self.context
                        .note_grid_filters(rows, FilterNote::Sent(wanted));
                    waiting = false;
                } else {
                    // Set from code: the model hears about it at once.
                    self.context
                        .queue_grid_input(rows, CollectionInput::FiltersDue);
                    self.context
                        .note_grid_filters(rows, FilterNote::Sent(wanted));
                }
            } else {
                self.context.note_grid_filters(rows, FilterNote::Applied);
            }
            for input in sends.drain(..) {
                self.context.queue_grid_input(rows, input);
            }
            grid.shown = selected;
            self.context.put_grid(rows, grid);
        }
        // Frames until what was queued has been sent, and until the delay of
        // typed filters is over.
        if waiting || queued {
            self.request_redraw();
        }
        self.set_control(Control::Grid {
            label: options.label,
            rows: model.len(),
            columns: visible.len(),
        });
    }

    /// Declares the rows of a collection. The caller gives the collection its
    /// handler right after, with `set_handler(rows_handler(..))`, since that
    /// applies to the element declared last.
    fn rows<M: ListModel>(
        &mut self,
        key: WidgetKey,
        model: &M,
        state: &ListState,
        options: VirtualListOptions,
        kind: CollectionKind,
        row: impl FnMut(&mut Ui<'_, Msg>, &M, usize, bool),
    ) -> Nav {
        if let Err(message) = options.check(model.len()) {
            self.misuse(message.to_owned());
            return Nav::default();
        }
        let owner = self.context.child(self.parent, &key, Kind::Collection);
        let mut native = owner
            .and_then(|id| self.context.node(id))
            .and_then(|node| node.scroll.map(|scroll| scroll.offset))
            .unwrap_or(state.scroll_offset());
        // The top row stays in place when the model reorders, unless the user
        // scrolled since the last declaration.
        if let Some(owner) = owner
            && let Some(anchor) = self.context.list_anchor(owner)
            && anchor.offset == native
            && let Some(index) = model.index_of(anchor.item)
        {
            native = index as f32 * options.row_height + anchor.within;
        }
        let top = (model.len() as f64 * f64::from(options.row_height) - f64::from(options.height))
            .max(0.0) as f32;
        let mut offset = native.clamp(0.0, top);
        // A row the user or the application just moved to, or asked to
        // reveal, is scrolled into view once.
        if let Some(owner) = owner
            && self.context.reveal_selection(owner, state.reveal_target())
            && let Some(index) = state.reveal_target().0.and_then(|id| model.index_of(id))
        {
            let first = index as f32 * options.row_height;
            let last = first + options.row_height;
            if first < offset {
                offset = first;
            }
            if last > offset + options.height {
                offset = last - options.height;
            }
            offset = offset.clamp(0.0, top);
        }
        if let Some(owner) = owner {
            let first = (offset / options.row_height).floor() as usize;
            self.context.set_list_anchor(
                owner,
                (first < model.len()).then(|| crate::collection::ListAnchor {
                    item: model.item_id(first),
                    within: offset - first as f32 * options.row_height,
                    offset,
                }),
            );
            // A selection the model no longer has is for the application to
            // drop: the next batch sends it, once per lost id.
            let lost = state.selected().filter(|&id| model.index_of(id).is_none());
            if self.context.report_lost_selection(owner, lost) {
                self.context
                    .queue_grid_input(owner, CollectionInput::Deselect);
                self.request_redraw();
            }
        }
        let nav = Nav::new(model, state.selected(), &options);
        let window = Window {
            selected: state.selected(),
            offset,
            realized: realized_rows(model.len(), offset, &options),
        };
        self.rows_body(key, model, window, options, kind, row);
        nav
    }

    /// The rows of a collection, as `window` shows them.
    fn rows_body<M: ListModel>(
        &mut self,
        key: WidgetKey,
        model: &M,
        window: Window,
        options: VirtualListOptions,
        kind: CollectionKind,
        mut row: impl FnMut(&mut Ui<'_, Msg>, &M, usize, bool),
    ) {
        let count = model.len();
        let selected = window.selected;
        let theme = self.context.theme();
        let (selection, even_row, odd_row) = (theme.selection, theme.row, theme.row_alternate);
        let style = Style {
            height: Length::Px(options.height),
            gap: 0.0,
            padding: 0.0,
            ..options.style.clone()
        };
        self.group(key, Kind::Collection, style, |ui| {
            ui.label_at(
                "$before",
                "",
                Style {
                    height: Length::Px(window.realized.start as f32 * options.row_height),
                    gap: 0.0,
                    ..Style::default()
                },
            );
            for index in window.realized.clone() {
                let item = model.item_id(index);
                let is_selected = selected == Some(item);
                ui.group(
                    WidgetKey::named(format!("item-{}", item.0)),
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
                            ui.row_at(
                                "$indent",
                                Style {
                                    gap: 0.0,
                                    ..Style::default()
                                },
                                |ui| {
                                    ui.label_at(
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
                ui.set_control(Control::CollectionRow {
                    label: model.label(index),
                    item,
                    index,
                    total: count,
                    selected: is_selected,
                    kind,
                    depth: model.depth(index),
                    expanded: model.expanded(index),
                });
            }
            ui.label_at(
                "$after",
                "",
                Style {
                    height: Length::Px((count - window.realized.end) as f32 * options.row_height),
                    gap: 0.0,
                    ..Style::default()
                },
            );
        });
        self.set_control(Control::Collection {
            label: options.label,
            kind,
            count,
            row_height: options.row_height,
            offset: window.offset,
            selected,
        });
    }
}

/// What a rows declaration shows: the selected row, the scroll offset and the
/// rows realized.
struct Window {
    selected: Option<crate::ItemId>,
    offset: f32,
    realized: std::ops::Range<usize>,
}

/// The rows a list shows at `offset`, with its overscan.
pub(crate) fn realized_rows(
    len: usize,
    offset: f32,
    options: &VirtualListOptions,
) -> std::ops::Range<usize> {
    let first = (offset / options.row_height).floor() as usize;
    let start = first.saturating_sub(options.overscan).min(len);
    let end = (((offset + options.height) / options.row_height).ceil() as usize)
        .saturating_add(options.overscan)
        .min(len);
    start..end
}

/// What a collection's rows need to turn keys into messages without the
/// model: the rows around the selection, and the first and the last.
#[derive(Default)]
struct Nav {
    len: usize,
    current: Option<usize>,
    /// Index of `entries[0]`.
    base: usize,
    entries: Vec<NavEntry>,
    first: Option<crate::ItemId>,
    last: Option<crate::ItemId>,
}

struct NavEntry {
    id: crate::ItemId,
    expanded: Option<bool>,
    parent: Option<crate::ItemId>,
}

impl Nav {
    /// A page either side of the selection is enough for any key.
    fn new(
        model: &impl ListModel,
        selected: Option<crate::ItemId>,
        options: &VirtualListOptions,
    ) -> Self {
        let len = model.len();
        let current = selected.and_then(|id| model.index_of(id));
        let page = (options.height / options.row_height).ceil() as usize + 1;
        let centre = current.unwrap_or(0);
        let base = centre.saturating_sub(page).min(len);
        let end = centre.saturating_add(page + 1).min(len);
        Self {
            len,
            current,
            base,
            entries: (base..end)
                .map(|index| {
                    let id = model.item_id(index);
                    NavEntry {
                        id,
                        expanded: model.expanded(index),
                        parent: model.parent(id),
                    }
                })
                .collect(),
            first: (len > 0).then(|| model.item_id(0)),
            last: len.checked_sub(1).map(|index| model.item_id(index)),
        }
    }

    fn entry(&self, index: usize) -> Option<&NavEntry> {
        self.entries.get(index.checked_sub(self.base)?)
    }

    /// The row at `index`, or the nearest one known.
    fn id_at(&self, index: usize) -> Option<crate::ItemId> {
        if index == 0 {
            return self.first;
        }
        if index + 1 == self.len {
            return self.last;
        }
        let known = self.base..self.base + self.entries.len();
        self.entry(index.clamp(known.start, known.end.checked_sub(1)?))
            .map(|entry| entry.id)
    }

    fn position(&self, id: crate::ItemId) -> Option<usize> {
        let at = self.entries.iter().position(|entry| entry.id == id)?;
        Some(self.base + at)
    }
}

/// The handler of a collection's rows: selection, disclosure, the keys that
/// move them, the filter delay and a committed edit, each input sent as its
/// own message in the order it arrived.
fn rows_handler<M: 'static>(on: Rc<dyn Fn(GridMsg) -> M>, nav: Nav) -> crate::app::Handler<M> {
    crate::app::Handler::Each(Box::new(move |update, out| {
        let ControlUpdate::Collection(inputs) = update else {
            return;
        };
        let mut current = nav.current;
        let select = |index: usize, current: &mut Option<usize>, out: &mut Vec<M>| {
            if let Some(id) = nav.id_at(index) {
                *current = Some(index);
                out.push(on(GridMsg::Select(id)));
            }
        };
        for input in inputs {
            match input {
                CollectionInput::Select(row) => {
                    current = nav.position(row).or(current);
                    out.push(on(GridMsg::Select(row)));
                }
                CollectionInput::Move(delta) if nav.len > 0 => {
                    let last = nav.len - 1;
                    let target = current.map_or(if delta < 0 { last } else { 0 }, |index| {
                        index.saturating_add_signed(delta).min(last)
                    });
                    select(target, &mut current, out);
                }
                CollectionInput::First if nav.len > 0 => select(0, &mut current, out),
                CollectionInput::Last if nav.len > 0 => select(nav.len - 1, &mut current, out),
                CollectionInput::Toggle(row) => {
                    if let Some(index) = nav.position(row)
                        && let Some(open) = nav.entry(index).and_then(|entry| entry.expanded)
                    {
                        current = Some(index);
                        out.push(on(GridMsg::Select(row)));
                        out.push(on(GridMsg::Expand { row, open: !open }));
                    }
                }
                CollectionInput::SetExpanded { item, open } => {
                    current = nav.position(item).or(current);
                    out.push(on(GridMsg::Select(item)));
                    out.push(on(GridMsg::Expand { row: item, open }));
                }
                CollectionInput::Expand(expand) => {
                    let Some(index) = current else { continue };
                    let Some(entry) = nav.entry(index) else {
                        continue;
                    };
                    match (expand, entry.expanded) {
                        (true, Some(false)) => out.push(on(GridMsg::Expand {
                            row: entry.id,
                            open: true,
                        })),
                        (true, Some(true)) if index + 1 < nav.len => {
                            select(index + 1, &mut current, out)
                        }
                        (false, Some(true)) => out.push(on(GridMsg::Expand {
                            row: entry.id,
                            open: false,
                        })),
                        (false, _) => {
                            if let Some(parent) = entry.parent {
                                current = nav.position(parent);
                                out.push(on(GridMsg::Select(parent)));
                            }
                        }
                        _ => {}
                    }
                }
                CollectionInput::Activate => {
                    if let Some(entry) = current.and_then(|index| nav.entry(index)) {
                        out.push(on(GridMsg::Activate(entry.id)));
                    }
                }
                CollectionInput::Activated(row) => out.push(on(GridMsg::Activate(row))),
                CollectionInput::Deselect => out.push(on(GridMsg::Deselect)),
                CollectionInput::FiltersDue => out.push(on(GridMsg::FiltersApplied)),
                CollectionInput::EditValue {
                    item,
                    column,
                    value,
                } => out.push(on(GridMsg::Edit {
                    row: item,
                    column,
                    value,
                })),
                _ => {}
            }
        }
    }))
}

/// Opens the editor of the current cell, or sends the toggled value of a
/// bool cell. `typed` replaces the content of a text or number cell. Returns
/// whether an editor opened and wants the keyboard.
fn begin_edit(
    context: &crate::Context,
    model: &impl GridModel,
    grid: &mut GridNode,
    columns: &[crate::GridColumn],
    selected: Option<crate::ItemId>,
    typed: Option<String>,
    sends: &mut Vec<CollectionInput>,
) -> bool {
    let column = grid.cursor;
    let Some(item) = selected else {
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
    let original = value.clone();
    let (text, choice) = match (editor, value) {
        (crate::CellEditor::Bool, value) => {
            let next = !matches!(value, crate::CellValue::Bool(true));
            sends.push(CollectionInput::EditValue {
                item,
                column,
                value: crate::CellValue::Bool(next),
            });
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
    grid.edit = Some(NodeEdit {
        item,
        column,
        text: typed.unwrap_or_else(|| text.clone()),
        choice,
        error: None,
        focused: false,
        start_text: text,
        start_choice: choice,
        original,
        waiting: None,
    });
    true
}

/// Sends the open editor's value. Returns whether the edit is over: there was
/// nothing to store, or the value is what the cell already held. Otherwise it
/// stays open until the model's value for the cell changes (`finish_edit`
/// then runs), or the user cancels; a number that does not parse keeps it
/// open with the reason.
fn commit_edit(
    model: &impl GridModel,
    grid: &mut GridNode,
    columns: &[crate::GridColumn],
    // What to say about text that is not a number, in the user's language.
    invalid: &str,
    after: After,
    sends: &mut Vec<CollectionInput>,
    context: &crate::Context,
) -> bool {
    let Some(edit) = &mut grid.edit else {
        return true;
    };
    if edit.waiting.is_some() {
        return false;
    }
    let (Some(_), Some(editor)) = (
        model.index_of(edit.item),
        columns
            .get(edit.column)
            .and_then(|column| column.editor.as_ref()),
    ) else {
        // The row or the column went away while it was being edited.
        grid.edit = None;
        return true;
    };
    if edit.text == edit.start_text && edit.choice == edit.start_choice {
        grid.edit = None;
        return true;
    }
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
    sends.push(CollectionInput::EditValue {
        item: edit.item,
        column: edit.column,
        value,
    });
    edit.error = None;
    edit.waiting = Some(after);
    false
}

/// What follows an edit that ended.
fn finish_edit(
    after: After,
    grid: &mut GridNode,
    visible: &[usize],
    sends: &mut Vec<CollectionInput>,
    focus_rows: &mut bool,
) {
    match after {
        After::Nothing => {}
        After::Row(step) => {
            sends.push(CollectionInput::Move(step));
            *focus_rows = true;
        }
        After::Column(step) => {
            let at = visible
                .iter()
                .position(|column| *column == grid.cursor)
                .unwrap_or(0);
            grid.cursor = visible[at.saturating_add_signed(step).min(visible.len() - 1)];
            *focus_rows = true;
        }
        After::Refocus => grid.refocus = true,
    }
}

/// Where a moved column goes in the display order `arranged`: toward the end
/// it lands after `onto`, toward the start before it.
fn moved_order(arranged: &[usize], visible: &[usize], moved: usize, onto: usize) -> Vec<usize> {
    let place = |column| visible.iter().position(|other| *other == column);
    if moved == onto || place(moved).is_none() || place(onto).is_none() {
        return arranged.to_vec();
    }
    let before = if place(moved) < place(onto) {
        place(onto).and_then(|at| visible.get(at + 1).copied())
    } else {
        Some(onto)
    };
    let mut order: Vec<usize> = arranged.iter().copied().filter(|c| *c != moved).collect();
    let at = before
        .and_then(|before| order.iter().position(|other| *other == before))
        .unwrap_or(order.len());
    order.insert(at, moved);
    order
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
        // Keep the current cell in view when the cursor moved to it. A column
        // wider than the view shows its leading edge.
        let mut start = 0.0;
        for (_, column, width) in rest {
            if reveal == Some(*column) {
                *scroll_x = scroll_x.max(start + width - view).min(start);
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
    fn scrollbar<Msg: 'static>(&self, ui: &mut Ui<'_, Msg>, rtl: bool) {
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
        ui.row_at("$hbar", line.clone(), |ui| {
            ui.label_at("$frozen", "", line.clone().width(Length::Px(frozen)));
            ui.container_at("$track", line.clone().width(Length::Px(self.view)), |ui| {
                let thumb = Style::default()
                    .offset(x, 1.0)
                    .width(Length::Px(thumb))
                    .height(Length::Px(4.0))
                    .radius(2.0)
                    .background(color);
                ui.container_at("$thumb", thumb, |_| {});
            });
        });
    }
}

/// Declares one cell of a line: at a position, of a column, at a width.
type SpreadCell<'c, Msg> = dyn FnMut(&mut Ui<'_, Msg>, usize, usize, f32) + 'c;

/// One line of a grid: the frozen cells, then the window onto the scrolling
/// ones. The header, the filter row and every body row use it, so they line up.
fn spread<Msg: 'static>(
    ui: &mut Ui<'_, Msg>,
    key: &str,
    height: f32,
    plan: &ColumnPlan,
    cell: &mut SpreadCell<'_, Msg>,
) {
    let line = Style::default().height(Length::Px(height)).gap(0.0);
    ui.row_at(key, line.clone(), |ui| {
        for &(position, column, width) in &plan.frozen {
            cell(ui, position, column, width);
        }
        ui.container_at(
            "$scrolling",
            line.clone().width(Length::Px(plan.view)),
            |ui| {
                ui.row_at("$strip", line.clone().offset(plan.strip_x, 0.0), |ui| {
                    for &(position, column, width) in &plan.realized {
                        cell(ui, position, column, width);
                    }
                });
            },
        );
    });
}
