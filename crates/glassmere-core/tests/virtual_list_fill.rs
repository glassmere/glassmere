//! A virtual list that fills the room its parent leaves.
use glassmere_core::{
    Context, ItemId, Length, ListModel, ListState, MonospaceMeasurer, Style, Ui, VirtualListOptions,
};
use glassmere_scene::Size;

struct Rows(usize);

impl ListModel for Rows {
    fn len(&self) -> usize {
        self.0
    }
    fn item_id(&self, index: usize) -> ItemId {
        ItemId(index as u64)
    }
    fn index_of(&self, id: ItemId) -> Option<usize> {
        ((id.0 as usize) < self.0).then_some(id.0 as usize)
    }
    fn label(&self, index: usize) -> String {
        format!("Row {index}")
    }
}

fn options(fill: bool) -> VirtualListOptions {
    let options = VirtualListOptions {
        row_height: 30.0,
        ..VirtualListOptions::default()
    };
    if fill { options.filling() } else { options }
}

/// A window-filling column: a 40 px bar, then the list.
fn declare(context: &mut Context, window: Size, fill: bool, state: &ListState) {
    context.run(window, &mut MonospaceMeasurer, |ui: &mut Ui<'_, ()>| {
        ui.keyed("page", |ui| {
            ui.column(
                Style::default()
                    .width(Length::Percent(1.0))
                    .height(Length::Px(window.height))
                    .gap(0.0),
                |ui| {
                    ui.keyed("bar", |ui| {
                        ui.container(Style::default().height(Length::Px(40.0)), |_| {})
                    });
                    ui.keyed("list", |ui| {
                        ui.virtual_list(&Rows(100_000), state, options(fill), |_| ())
                    });
                },
            );
        });
    });
}

fn list_height(context: &Context) -> f32 {
    context
        .node(context.find(&["page", "list"]).unwrap())
        .unwrap()
        .bounds
        .height
}

fn realized(context: &Context) -> usize {
    (0..200)
        .filter(|index| {
            context
                .find(&["page", "list", &format!("item-{index}")])
                .is_some()
        })
        .count()
}

#[test]
fn it_is_as_tall_as_the_room_its_parent_leaves() {
    let state = ListState::default();
    let mut context = Context::new();
    declare(&mut context, Size::new(500.0, 400.0), true, &state);
    assert_eq!(list_height(&context), 360.0);
    // Without `fill` it is `height` tall whatever the room is.
    let mut fixed = Context::new();
    declare(&mut fixed, Size::new(500.0, 400.0), false, &state);
    assert_eq!(list_height(&fixed), 320.0);
}

#[test]
fn it_follows_the_window_up_and_down_and_never_leaves_a_gap() {
    let state = ListState::default();
    let mut context = Context::new();
    for height in [400.0, 900.0, 250.0, 700.0] {
        // A resize declares again, and so does what that changed.
        declare(&mut context, Size::new(500.0, height), true, &state);
        declare(&mut context, Size::new(500.0, height), true, &state);
        assert_eq!(list_height(&context), height - 40.0, "window {height}");
        // The last row that is not clipped is realized.
        let shown = ((height - 40.0) / 30.0).ceil() as usize;
        assert!(
            realized(&context) >= shown,
            "{height}: {}",
            realized(&context)
        );
        assert!(
            context
                .find(&["page", "list", &format!("item-{}", shown - 1)])
                .is_some()
        );
    }
}

#[test]
fn it_declares_rows_for_the_window_and_no_more() {
    let state = ListState::default();
    let mut context = Context::new();
    declare(&mut context, Size::new(500.0, 600.0), true, &state);
    declare(&mut context, Size::new(500.0, 600.0), true, &state);
    // 560 px of list is 19 rows; the window's 600 px is 20, and the overscan
    // adds two: nowhere near the 100,000 of the model.
    assert!(realized(&context) <= 25, "{}", realized(&context));
}

#[test]
fn revealing_the_last_row_puts_it_inside_the_viewport_that_was_laid_out() {
    let mut state = ListState::default();
    let mut context = Context::new();
    let window = Size::new(500.0, 400.0);
    let declare_few = |context: &mut Context, state: &ListState| {
        context.run(window, &mut MonospaceMeasurer, |ui: &mut Ui<'_, ()>| {
            ui.keyed("page", |ui| {
                ui.column(
                    Style::default()
                        .width(Length::Percent(1.0))
                        .height(Length::Px(window.height))
                        .gap(0.0),
                    |ui| {
                        ui.keyed("bar", |ui| {
                            ui.container(Style::default().height(Length::Px(40.0)), |_| {})
                        });
                        ui.keyed("list", |ui| {
                            ui.virtual_list(&Rows(50), state, options(true), |_| ())
                        });
                    },
                );
            });
        });
    };
    declare_few(&mut context, &state);
    declare_few(&mut context, &state);
    state.select(Some(ItemId(49)));
    for _ in 0..3 {
        declare_few(&mut context, &state);
    }
    let list = context
        .node(context.find(&["page", "list"]).unwrap())
        .unwrap()
        .bounds;
    let last = context
        .node(context.find(&["page", "list", "item-49"]).unwrap())
        .unwrap()
        .bounds;
    assert!(
        last.y + last.height <= list.y + list.height + 0.5,
        "{last:?} in {list:?}"
    );
    assert!(last.y >= list.y - 0.5);
}
