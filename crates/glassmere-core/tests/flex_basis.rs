//! `basis` and `flex`: what `grow` and `shrink` start from.
use glassmere_core::{Context, Length, MonospaceMeasurer, Style, StyleSheet, Ui};
use glassmere_scene::Size;

/// The widths of three children in a row 300 wide, with no gap.
fn widths(styles: [Style; 3]) -> [f32; 3] {
    let mut context = Context::new();
    context.run(
        Size::new(400.0, 200.0),
        &mut MonospaceMeasurer,
        |ui: &mut Ui<'_>| {
            ui.keyed("row", |ui| {
                ui.row(Style::default().width(Length::Px(300.0)).gap(0.0), |ui| {
                    for (name, style) in ["a", "b", "c"].into_iter().zip(&styles) {
                        ui.keyed(name, |ui| ui.container(style.clone(), |_| {}));
                    }
                })
            });
        },
    );
    ["a", "b", "c"].map(|name| {
        context
            .node(context.find(&["row", name]).unwrap())
            .unwrap()
            .bounds
            .width
    })
}

fn item(width: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(10.0))
}

fn near(a: [f32; 3], b: [f32; 3]) {
    for (a, b) in a.iter().zip(b) {
        assert!((a - b).abs() < 0.01, "{a:?} against {b:?}");
    }
}

#[test]
fn grow_starts_from_the_size_so_children_share_what_is_left() {
    // The default basis: 100 and 20 and 60 leave 120, 60 more for each of the two.
    near(
        widths([item(100.0).grow(1.0), item(20.0).grow(1.0), item(60.0)]),
        [160.0, 80.0, 60.0],
    );
}

#[test]
fn a_basis_of_zero_shares_everything_by_the_factor_whatever_is_inside() {
    // `flex: 1` and `flex: 2` beside a fixed 60: 240 shared one to two.
    near(
        widths([item(100.0).flex(1.0), item(20.0).flex(2.0), item(60.0)]),
        [80.0, 160.0, 60.0],
    );
    // The same by hand.
    near(
        widths([
            item(100.0).grow(1.0).basis(Length::Px(0.0)),
            item(20.0).grow(2.0).basis(Length::Px(0.0)),
            item(60.0),
        ]),
        [80.0, 160.0, 60.0],
    );
}

#[test]
fn a_basis_wins_over_the_width_as_the_size_to_start_from() {
    // 50 and 100 and nothing: the 150 left goes half and half.
    near(
        widths([
            item(5.0).grow(1.0).basis(Length::Px(50.0)),
            item(5.0).grow(1.0).basis(Length::Px(100.0)),
            item(0.0),
        ]),
        [125.0, 175.0, 0.0],
    );
}

#[test]
fn shrink_starts_from_the_basis_too() {
    // 200 and 200 in 300, shrinking by their factors from the basis.
    near(
        widths([
            item(5.0).shrink(1.0).basis(Length::Px(200.0)),
            item(5.0).shrink(1.0).basis(Length::Px(200.0)),
            item(0.0),
        ]),
        [150.0, 150.0, 0.0],
    );
}

#[test]
fn a_style_that_sets_no_basis_is_laid_out_as_before() {
    assert_eq!(Style::default().basis, Length::Auto);
    near(
        widths([item(100.0), item(50.0), item(25.0)]),
        [100.0, 50.0, 25.0],
    );
}

#[test]
fn a_stylesheet_has_basis_and_flex() {
    let sheet = StyleSheet::parse(
        "class share {\nflex: 2\n}\nclass start {\nbasis: 120\ngrow: 1\n}\nclass pct {\nbasis: 25%\n}",
    )
    .unwrap();
    let share = sheet.class("share").unwrap();
    assert_eq!(
        (share.grow, share.shrink, share.basis),
        (2.0, 1.0, Length::Px(0.0))
    );
    let start = sheet.class("start").unwrap();
    assert_eq!((start.grow, start.basis), (1.0, Length::Px(120.0)));
    assert_eq!(sheet.class("pct").unwrap().basis, Length::Percent(0.25));
    assert!(StyleSheet::parse("class bad {\nflex: lots\n}").is_err());
}
