use glassmere_core::Label;
use glassmere_core::{
    Align, AutoFlow, Context, GridArea, Justify, Length, MonospaceMeasurer, NodeId, Style,
    StyleSheet, Track, TrackSize, Ui,
};
use glassmere_scene::{Primitive, Rect, Size};

const ROOT: Size = Size::new(500.0, 400.0);

fn frame(context: &mut Context, view: impl FnOnce(&mut Ui<'_>)) -> glassmere_core::FrameStats {
    context.run(ROOT, &mut MonospaceMeasurer, view)
}

fn bounds(context: &Context, path: &[&str]) -> Rect {
    context.node(find(context, path)).unwrap().bounds
}

/// A child's rectangle relative to its grid's origin.
fn within(context: &Context, path: &[&str]) -> Rect {
    let grid = bounds(context, &path[..path.len() - 1]);
    let child = bounds(context, path);
    Rect {
        x: child.x - grid.x,
        y: child.y - grid.y,
        ..child
    }
}

fn find(context: &Context, path: &[&str]) -> NodeId {
    context
        .find(path)
        .unwrap_or_else(|| panic!("missing {path:?}"))
}

fn fixed(width: f32, height: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Px(height))
}

fn assert_rect(actual: Rect, x: f32, y: f32, width: f32, height: f32) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.51;
    assert!(
        close(actual.x, x)
            && close(actual.y, y)
            && close(actual.width, width)
            && close(actual.height, height),
        "expected ({x}, {y}, {width}×{height}), got {actual:?}"
    );
}

#[test]
fn grid_auto_places_children_and_honors_explicit_areas_and_implicit_rows() {
    let mut context = Context::new();
    let view = |ui: &mut Ui<'_>| {
        ui.keyed("grid", |ui| {
            ui.grid(
                Style::default()
                    .width(Length::Percent(1.0))
                    .columns(vec![Track::Fr(1.0), Track::Px(100.0), Track::Auto])
                    .auto_rows(vec![Track::Px(30.0)])
                    .gap(10.0),
                |ui| {
                    for key in ["a", "b", "c", "d", "e"] {
                        ui.keyed(key, |ui| ui.label(Label::new(key).style(fixed(50.0, 20.0))));
                    }
                    ui.keyed("wide", |ui| {
                        ui.label(
                            Label::new("wide").style(
                                Style::default()
                                    .width(Length::Percent(1.0))
                                    .height(Length::Px(20.0))
                                    .area(GridArea::span(1, 3, 3, 1)),
                            ),
                        )
                    });
                },
            )
        });
    };
    frame(&mut context, view);
    // Column widths: auto = 50 (content), fixed 100, fr = 500 - 150 - 2 gaps.
    assert_rect(bounds(&context, &["grid", "a"]), 0.0, 0.0, 50.0, 20.0);
    assert_rect(bounds(&context, &["grid", "b"]), 340.0, 0.0, 50.0, 20.0);
    assert_rect(bounds(&context, &["grid", "c"]), 450.0, 0.0, 50.0, 20.0);
    // Implicit rows are 30 tall; the second row starts after one gap.
    assert_rect(bounds(&context, &["grid", "d"]), 0.0, 40.0, 50.0, 20.0);
    assert_rect(bounds(&context, &["grid", "e"]), 340.0, 40.0, 50.0, 20.0);
    // The explicitly placed child spans all three columns on row three.
    assert_rect(bounds(&context, &["grid", "wide"]), 0.0, 80.0, 500.0, 20.0);
    assert_rect(bounds(&context, &["grid"]), 0.0, 0.0, 500.0, 110.0);
    // A grid is a group for accessibility and reuses nodes like any container.
    let stats = frame(&mut context, view);
    assert_eq!((stats.created, stats.layout_passes), (0, 0));
}

#[test]
fn repeat_minmax_and_percent_tracks_size_as_specified() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("grid", |ui| {
            ui.grid(
                Style::default()
                    .width(Length::Px(400.0))
                    .gap(0.0)
                    .columns(vec![
                        Track::repeat(2, vec![Track::Fr(1.0)]),
                        Track::MinMax(TrackSize::Px(50.0), TrackSize::Px(80.0)),
                        Track::Percent(0.25),
                    ]),
                |ui| {
                    for key in ["a", "b", "c", "d"] {
                        ui.keyed(key, |ui| {
                            ui.label(
                                Label::new(key).style(Style::default().height(Length::Px(10.0))),
                            )
                        });
                    }
                },
            )
        });
    });
    // CSS track sizing: 25% = 100, minmax(50, 80) grows to its 80 maximum while
    // free space remains, and the two fr tracks split the remaining 220. Items
    // without an explicit width stretch to their cell.
    assert_rect(bounds(&context, &["grid", "a"]), 0.0, 0.0, 110.0, 10.0);
    assert_rect(bounds(&context, &["grid", "b"]), 110.0, 0.0, 110.0, 10.0);
    assert_rect(bounds(&context, &["grid", "c"]), 220.0, 0.0, 80.0, 10.0);
    assert_rect(bounds(&context, &["grid", "d"]), 300.0, 0.0, 100.0, 10.0);
}

fn cells(ui: &mut Ui<'_>, keys: &[&str]) {
    for key in keys {
        ui.keyed(*key, |ui| {
            ui.label(Label::new(*key).style(Style::default().height(Length::Px(10.0))))
        });
    }
}

#[test]
fn row_and_column_gaps_are_separate_and_gap_sets_both() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("grid", |ui| {
            ui.grid(
                Style::default()
                    .width(Length::Px(400.0))
                    .columns(vec![Track::repeat(2, vec![Track::Px(100.0)])])
                    .gap(8.0)
                    .row_gap(5.0)
                    .column_gap(20.0),
                |ui| cells(ui, &["a", "b", "c", "d"]),
            )
        });
        ui.keyed("both", |ui| {
            ui.grid(
                Style::default()
                    .width(Length::Px(400.0))
                    .columns(vec![Track::repeat(2, vec![Track::Px(100.0)])])
                    .row_gap(5.0)
                    .gap(8.0),
                |ui| cells(ui, &["a", "b", "c"]),
            )
        });
    });
    assert_rect(bounds(&context, &["grid", "b"]), 120.0, 0.0, 100.0, 10.0);
    assert_rect(bounds(&context, &["grid", "c"]), 0.0, 15.0, 100.0, 10.0);
    assert_rect(bounds(&context, &["grid", "d"]), 120.0, 15.0, 100.0, 10.0);
    // `gap` after `row_gap` replaces it, so both gaps are 8.
    assert_rect(within(&context, &["both", "b"]), 108.0, 0.0, 100.0, 10.0);
    assert_rect(within(&context, &["both", "c"]), 0.0, 18.0, 100.0, 10.0);
}

#[test]
fn auto_fill_keeps_empty_tracks_and_auto_fit_collapses_them() {
    let mut context = Context::new();
    let grid = |tracks: Vec<Track>| {
        Style::default()
            .width(Length::Px(400.0))
            .gap(0.0)
            .columns(tracks)
    };
    let fluid = || vec![Track::MinMax(TrackSize::Px(100.0), TrackSize::Fr(1.0))];
    frame(&mut context, |ui| {
        ui.keyed("fill", |ui| {
            ui.grid(grid(vec![Track::auto_fill(fluid())]), |ui| {
                cells(ui, &["a", "b", "c", "d", "e"])
            })
        });
        ui.keyed("fit", |ui| {
            ui.grid(grid(vec![Track::auto_fit(fluid())]), |ui| {
                cells(ui, &["a", "b"])
            })
        });
        ui.keyed("fill-two", |ui| {
            ui.grid(grid(vec![Track::auto_fill(fluid())]), |ui| {
                cells(ui, &["a", "b"])
            })
        });
    });
    // 400 wide holds four 100 tracks; the fifth child wraps to a second row.
    assert_rect(within(&context, &["fill", "d"]), 300.0, 0.0, 100.0, 10.0);
    assert_rect(within(&context, &["fill", "e"]), 0.0, 10.0, 100.0, 10.0);
    // Two children leave two empty tracks: auto-fill keeps them, auto-fit
    // collapses them and lets the two 1fr tracks take the whole width.
    assert_rect(
        within(&context, &["fill-two", "b"]),
        100.0,
        0.0,
        100.0,
        10.0,
    );
    assert_rect(within(&context, &["fit", "a"]), 0.0, 0.0, 200.0, 10.0);
    assert_rect(within(&context, &["fit", "b"]), 200.0, 0.0, 200.0, 10.0);
}

#[test]
fn counted_repeat_takes_minmax_tracks() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("grid", |ui| {
            ui.grid(
                Style::default()
                    .width(Length::Px(400.0))
                    .gap(0.0)
                    .columns(vec![
                        Track::Px(100.0),
                        Track::repeat(
                            2,
                            vec![Track::MinMax(TrackSize::Px(50.0), TrackSize::Fr(1.0))],
                        ),
                    ]),
                |ui| cells(ui, &["a", "b", "c"]),
            )
        });
    });
    assert_rect(bounds(&context, &["grid", "b"]), 100.0, 0.0, 150.0, 10.0);
    assert_rect(bounds(&context, &["grid", "c"]), 250.0, 0.0, 150.0, 10.0);
}

#[test]
fn dense_flow_fills_holes_that_sparse_flow_leaves() {
    let mut context = Context::new();
    let span = |columns| {
        Style::default()
            .height(Length::Px(10.0))
            .area(GridArea::auto_span(columns, 1))
    };
    let view = |flow: AutoFlow| {
        move |ui: &mut Ui<'_>| {
            ui.keyed("grid", |ui| {
                ui.grid(
                    Style::default()
                        .width(Length::Px(300.0))
                        .gap(0.0)
                        .columns(vec![Track::repeat(3, vec![Track::Px(100.0)])])
                        .auto_flow(flow),
                    |ui| {
                        for (key, columns) in [("a", 2), ("b", 2), ("c", 1)] {
                            ui.keyed(key, |ui| ui.label(Label::new(key).style(span(columns))));
                        }
                    },
                )
            });
        }
    };
    frame(&mut context, view(AutoFlow::Row));
    // `b` does not fit beside `a`, so it starts row two and `c` follows it.
    assert_rect(bounds(&context, &["grid", "b"]), 0.0, 10.0, 200.0, 10.0);
    assert_rect(bounds(&context, &["grid", "c"]), 200.0, 10.0, 100.0, 10.0);
    frame(&mut context, view(AutoFlow::RowDense));
    // Dense packing sends `c` back to the hole beside `a`.
    assert_rect(bounds(&context, &["grid", "b"]), 0.0, 10.0, 200.0, 10.0);
    assert_rect(bounds(&context, &["grid", "c"]), 200.0, 0.0, 100.0, 10.0);
}

#[test]
fn column_flow_fills_columns_before_rows() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("grid", |ui| {
            ui.grid(
                Style::default()
                    .width(Length::Px(400.0))
                    .gap(0.0)
                    .rows(vec![Track::repeat(2, vec![Track::Px(10.0)])])
                    .auto_columns(vec![Track::Px(100.0)])
                    .auto_flow(AutoFlow::ColumnDense),
                |ui| cells(ui, &["a", "b", "c", "d"]),
            )
        });
    });
    assert_rect(bounds(&context, &["grid", "b"]), 0.0, 10.0, 100.0, 10.0);
    assert_rect(bounds(&context, &["grid", "c"]), 100.0, 0.0, 100.0, 10.0);
    assert_rect(bounds(&context, &["grid", "d"]), 100.0, 10.0, 100.0, 10.0);
}

#[test]
fn children_are_placed_in_the_named_areas_of_the_template() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("page", |ui| {
            ui.grid(
                Style::default()
                    .width(Length::Px(400.0))
                    .gap(0.0)
                    .align_items(Align::Stretch)
                    .columns(vec![Track::Px(100.0), Track::Fr(1.0)])
                    .rows(vec![Track::Px(30.0), Track::Px(50.0), Track::Px(20.0)])
                    .areas(["head head", "side body", "foot ."]),
                |ui| {
                    // Declared out of template order on purpose.
                    for name in ["foot", "body", "side", "head"] {
                        ui.keyed(name, |ui| {
                            ui.label(
                                Label::new(name).style(Style::default().area_name(name.to_owned())),
                            )
                        });
                    }
                },
            )
        });
    });
    assert_rect(bounds(&context, &["page", "head"]), 0.0, 0.0, 400.0, 30.0);
    assert_rect(bounds(&context, &["page", "side"]), 0.0, 30.0, 100.0, 50.0);
    assert_rect(
        bounds(&context, &["page", "body"]),
        100.0,
        30.0,
        300.0,
        50.0,
    );
    assert_rect(bounds(&context, &["page", "foot"]), 0.0, 80.0, 100.0, 20.0);
}

#[test]
#[should_panic(expected = "same number of cells")]
fn a_ragged_area_template_is_misuse() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.grid(Style::default().areas(["a a", "b"]), |_| {});
    });
}

#[test]
#[should_panic(expected = "not a rectangle")]
fn an_l_shaped_area_is_misuse() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.grid(Style::default().areas(["a a", "a b"]), |_| {});
    });
}

#[test]
fn min_max_and_aspect_ratio_constrain_sizes() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("page", |ui| {
            ui.column(Style::default().width(Length::Percent(1.0)), |ui| {
                ui.keyed("min", |ui| {
                    ui.container(
                        Style::default()
                            .width(Length::Percent(0.5))
                            .min_width(Length::Px(300.0))
                            .height(Length::Px(10.0)),
                        |_| {},
                    )
                });
                ui.keyed("max", |ui| {
                    ui.container(
                        Style::default()
                            .width(Length::Percent(1.0))
                            .max_width(Length::Px(100.0))
                            .height(Length::Px(10.0)),
                        |_| {},
                    )
                });
                ui.keyed("aspect", |ui| {
                    ui.container(
                        Style::default().width(Length::Px(200.0)).aspect_ratio(2.0),
                        |_| {},
                    )
                });
                ui.keyed("tall", |ui| {
                    ui.container(
                        Style::default()
                            .height(Length::Px(50.0))
                            .aspect_ratio(0.5)
                            .max_height(Length::Px(40.0)),
                        |_| {},
                    )
                });
            })
        });
    });
    assert_rect(bounds(&context, &["page", "min"]), 0.0, 0.0, 300.0, 10.0);
    assert_rect(bounds(&context, &["page", "max"]), 0.0, 22.0, 100.0, 10.0);
    assert_rect(
        bounds(&context, &["page", "aspect"]),
        0.0,
        44.0,
        200.0,
        100.0,
    );
    // max-height wins over the specified height; width follows the ratio.
    let tall = bounds(&context, &["page", "tall"]);
    assert_eq!(tall.height, 40.0);
    assert_eq!(tall.width, 20.0);
}

#[test]
fn alignment_and_distribution_apply_to_rows_columns_and_children() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("page", |ui| {
            ui.column(
                Style::default().width(Length::Percent(1.0)).gap(0.0),
                |ui| {
                    ui.keyed("centered", |ui| {
                        ui.row(
                            Style::default()
                                .width(Length::Percent(1.0))
                                .height(Length::Px(100.0))
                                .align_items(Align::Center)
                                .justify_content(Justify::SpaceBetween)
                                .gap(0.0),
                            |ui| {
                                ui.keyed("short", |ui| {
                                    ui.label(Label::new("s").style(fixed(50.0, 20.0)))
                                });
                                ui.keyed("tall", |ui| {
                                    ui.label(Label::new("t").style(fixed(50.0, 40.0)))
                                });
                                ui.keyed("end", |ui| {
                                    ui.label(
                                        Label::new("e")
                                            .style(fixed(50.0, 20.0).align_self(Align::End)),
                                    )
                                });
                            },
                        )
                    });
                    ui.keyed("stretched", |ui| {
                        ui.row(
                            Style::default()
                                .width(Length::Percent(1.0))
                                .height(Length::Px(60.0))
                                .align_items(Align::Stretch)
                                .justify_content(Justify::Center)
                                .gap(0.0),
                            |ui| {
                                ui.keyed("fill", |ui| {
                                    ui.label(
                                        Label::new("f")
                                            .style(Style::default().width(Length::Px(100.0))),
                                    )
                                });
                            },
                        )
                    });
                    ui.keyed("vertical", |ui| {
                        ui.column(
                            Style::default()
                                .width(Length::Px(100.0))
                                .height(Length::Px(90.0))
                                .justify_content(Justify::End)
                                .align_items(Align::Center)
                                .gap(0.0),
                            |ui| {
                                ui.keyed("bottom", |ui| {
                                    ui.label(Label::new("b").style(fixed(20.0, 30.0)))
                                });
                            },
                        )
                    });
                },
            )
        });
    });
    assert_rect(
        bounds(&context, &["page", "centered", "short"]),
        0.0,
        40.0,
        50.0,
        20.0,
    );
    assert_rect(
        bounds(&context, &["page", "centered", "tall"]),
        225.0,
        30.0,
        50.0,
        40.0,
    );
    assert_rect(
        bounds(&context, &["page", "centered", "end"]),
        450.0,
        80.0,
        50.0,
        20.0,
    );
    assert_rect(
        bounds(&context, &["page", "stretched", "fill"]),
        200.0,
        100.0,
        100.0,
        60.0,
    );
    assert_rect(
        bounds(&context, &["page", "vertical", "bottom"]),
        40.0,
        220.0,
        20.0,
        30.0,
    );
}

#[test]
fn children_shrink_only_when_asked_and_respect_minimums() {
    let mut context = Context::new();
    let row = |shrink: f32, minimum: Length| {
        move |ui: &mut Ui<'_>| {
            ui.keyed("row", |ui| {
                ui.row(
                    Style::default().width(Length::Percent(1.0)).gap(0.0),
                    |ui| {
                        ui.keyed("first", |ui| {
                            ui.label(
                                Label::new("1")
                                    .style(fixed(400.0, 10.0).shrink(shrink).min_width(minimum)),
                            )
                        });
                        ui.keyed("second", |ui| {
                            ui.label(Label::new("2").style(fixed(400.0, 10.0).shrink(shrink)))
                        });
                    },
                )
            });
        }
    };
    frame(&mut context, row(0.0, Length::Auto));
    assert_rect(bounds(&context, &["row", "first"]), 0.0, 0.0, 400.0, 10.0);
    assert_rect(
        bounds(&context, &["row", "second"]),
        400.0,
        0.0,
        400.0,
        10.0,
    );
    frame(&mut context, row(1.0, Length::Auto));
    assert_rect(bounds(&context, &["row", "first"]), 0.0, 0.0, 250.0, 10.0);
    assert_rect(
        bounds(&context, &["row", "second"]),
        250.0,
        0.0,
        250.0,
        10.0,
    );
    frame(&mut context, row(1.0, Length::Px(300.0)));
    assert_rect(bounds(&context, &["row", "first"]), 0.0, 0.0, 300.0, 10.0);
    assert_rect(
        bounds(&context, &["row", "second"]),
        300.0,
        0.0,
        200.0,
        10.0,
    );
}

#[test]
fn baseline_alignment_lines_up_the_text_of_different_sizes() {
    let mut context = Context::new();
    frame(&mut context, |ui| {
        ui.keyed("row", |ui| {
            ui.row(
                Style::default().align_items(Align::Baseline).gap(4.0),
                |ui| {
                    ui.keyed("small", |ui| {
                        ui.label(Label::new("small").style(Style::default().font_size(12.0)))
                    });
                    ui.keyed("large", |ui| {
                        ui.label(Label::new("large").style(Style::default().font_size(32.0)))
                    });
                    ui.keyed("padded", |ui| {
                        ui.label(
                            Label::new("pad").style(Style::default().font_size(16.0).padding(10.0)),
                        )
                    });
                },
            )
        });
    });
    let small = bounds(&context, &["row", "small"]);
    let large = bounds(&context, &["row", "large"]);
    let padded = bounds(&context, &["row", "padded"]);
    assert!(large.height > small.height);
    // The synthetic measurer puts the baseline at four fifths of the size,
    // and padding pushes it down with the text.
    let baseline = |rect: Rect, size: f32, padding: f32| rect.y + padding + size * 0.8;
    let line = baseline(large, 32.0, 0.0);
    let same = |value: f32| (value - line).abs() < 0.01;
    assert!(same(baseline(small, 12.0, 0.0)), "{small:?} against {line}");
    assert!(same(baseline(padded, 16.0, 10.0)), "{padded:?}");
    assert_eq!(large.y, 0.0, "the lowest baseline stays where it is");
    assert!(
        small.y > 0.0 && padded.y > 0.0,
        "the others move down to the shared line"
    );
    // Bottoms no longer line up, because a baseline is not a bottom.
    assert!(small.y + small.height != large.y + large.height);
}

#[test]
fn stylesheets_express_layout_constraints_tracks_and_alignment() {
    let sheet = StyleSheet::parse(
        r#"
        class card {
            min-width: 120; max-width: 50%; min-height: auto; max-height: 300
            aspect-ratio: 1.5
            shrink: 1
            align-items: center; justify-content: space-between
            align-self: end; justify-self: stretch
        }
        class gallery {
            columns: repeat(2, 1fr) minmax(100, 1fr) auto 25%
            rows: 40 auto
            auto-rows: minmax(30, auto)
        }
        class hero { area: 1 2 3 1 }
        class cell { area: 2 3 }
        class fluid {
            columns: repeat(auto-fill, minmax(120, 1fr)) repeat(auto-fit, 80)
            row-gap: 4; column-gap: 8
            auto-flow: row dense
            areas: "head head" "side ."
        }
        class both { row-gap: 4; gap: 10 }
        class head { area: head }
        class flowing { area: auto 2 3 1 }
        class down { auto-flow: column }
        "#,
    )
    .unwrap();
    let card = sheet.class("card").unwrap();
    assert_eq!(card.min_width, Length::Px(120.0));
    assert_eq!(card.max_width, Length::Percent(0.5));
    assert_eq!(card.min_height, Length::Auto);
    assert_eq!(card.max_height, Length::Px(300.0));
    assert_eq!(card.aspect_ratio, Some(1.5));
    assert_eq!(card.shrink, 1.0);
    assert_eq!(card.align_items, Align::Center);
    assert_eq!(card.justify_content, Justify::SpaceBetween);
    assert_eq!(card.align_self, Some(Align::End));
    assert_eq!(card.justify_self, Some(Align::Stretch));
    let gallery = sheet.class("gallery").unwrap();
    assert_eq!(
        gallery.columns,
        vec![
            Track::Repeat(2, vec![Track::Fr(1.0)]),
            Track::MinMax(TrackSize::Px(100.0), TrackSize::Fr(1.0)),
            Track::Auto,
            Track::Percent(0.25),
        ]
    );
    assert_eq!(gallery.rows, vec![Track::Px(40.0), Track::Auto]);
    assert_eq!(
        gallery.auto_rows,
        vec![Track::MinMax(TrackSize::Px(30.0), TrackSize::Auto)]
    );
    assert_eq!(
        sheet.class("hero").unwrap().area,
        Some(GridArea::span(1, 2, 3, 1))
    );
    assert_eq!(
        sheet.class("cell").unwrap().area,
        Some(GridArea::cell(2, 3))
    );
    let fluid = sheet.class("fluid").unwrap();
    assert_eq!(
        fluid.columns,
        vec![
            Track::AutoFill(vec![Track::MinMax(
                TrackSize::Px(120.0),
                TrackSize::Fr(1.0)
            )]),
            Track::AutoFit(vec![Track::Px(80.0)]),
        ]
    );
    assert_eq!((fluid.row_gap, fluid.column_gap), (Some(4.0), Some(8.0)));
    assert_eq!(fluid.auto_flow, AutoFlow::RowDense);
    assert_eq!(fluid.areas, vec!["head head", "side ."]);
    let both = sheet.class("both").unwrap();
    assert_eq!(
        (both.gap, both.row_gap, both.column_gap),
        (10.0, None, None)
    );
    let head = sheet.class("head").unwrap();
    assert_eq!((head.area, head.area_name.as_deref()), (None, Some("head")));
    assert_eq!(
        sheet.class("flowing").unwrap().area,
        Some(GridArea::span(0, 2, 3, 1))
    );
    assert_eq!(sheet.class("down").unwrap().auto_flow, AutoFlow::Column);
    let error = |text: &str| StyleSheet::parse(text).unwrap_err().message;
    assert!(error("class a { columns: repeat(some, 1fr) }").contains("invalid repeat count"));
    assert!(error("class a { auto-flow: dense column row }").contains("auto-flow is"));
    assert!(error("class a { areas: a b }").contains("quoted rows"));
    assert!(error("class a { areas: \"a a\" \"b\" }").contains("same number of cells"));
    assert!(error("class a { areas: \"a b\" \"b b\" }").contains("not a rectangle"));
    assert!(error("class a { columns: 1fr bogus }").contains("invalid number"));
    assert!(error("class a { columns: repeat(2, repeat(2, 1fr)) }").contains("nested repeat"));
    assert!(error("class a { align-items: middle }").contains("unknown alignment"));
    assert!(error("class a { justify-content: around }").contains("unknown justify-content"));
    assert!(error("class a { area: 1 }").contains("area needs"));
}

#[test]
fn a_container_tells_its_content_how_much_room_it_had() {
    let mut context = Context::new();
    let mut seen = Vec::new();
    let declare = |context: &mut Context, width: f32, seen: &mut Vec<f32>| {
        context.run(Size::new(width, 200.0), &mut MonospaceMeasurer, |ui| {
            ui.keyed("page", |ui| {
                ui.responsive(
                    Style::default().width(Length::Percent(1.0)).padding(10.0),
                    |ui, room| {
                        seen.push(room);
                        // A narrow page stacks what a wide one puts in a row.
                        if room >= 300.0 {
                            ui.keyed("wide", |ui| {
                                ui.row(Style::default(), |ui| {
                                    ui.keyed("a", |ui| ui.label("a"));
                                    ui.keyed("b", |ui| ui.label("b"));
                                })
                            });
                        } else {
                            ui.keyed("narrow", |ui| {
                                ui.column(Style::default(), |ui| {
                                    ui.keyed("a", |ui| ui.label("a"));
                                })
                            });
                        }
                    },
                )
            });
        });
    };
    // Nothing has been laid out on the first frame, so the content is told
    // nothing and the narrow shape appears.
    declare(&mut context, 500.0, &mut seen);
    assert_eq!(seen, [0.0]);
    assert!(context.find(&["page", "narrow"]).is_some());

    // The second frame knows the room and changes shape.
    declare(&mut context, 500.0, &mut seen);
    assert_eq!(seen[1], 480.0);
    assert!(context.find(&["page", "wide"]).is_some());

    // And it changes back when the window is made smaller.
    declare(&mut context, 200.0, &mut seen);
    declare(&mut context, 200.0, &mut seen);
    assert_eq!(seen[3], 180.0);
    assert!(context.find(&["page", "narrow"]).is_some());
}

/// Where a line of text sits in a box larger than it is. The properties a
/// container places its children with place a leaf's own text, which is what
/// centres a glyph in a caption button.
#[test]
fn a_text_leaf_is_placed_in_its_box_by_the_same_properties_a_container_uses() {
    let mut context = Context::new();
    let boxed = |justify: Justify, align: Align| Style {
        width: Length::Px(200.0),
        height: Length::Px(100.0),
        justify_content: justify,
        align_items: align,
        padding: 0.0,
        ..Style::default()
    };
    let text = |context: &Context| {
        context
            .scene()
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                Primitive::Text { rect, text, .. } if text == "x" => Some(*rect),
                _ => None,
            })
            .expect("the text")
    };

    frame(&mut context, |ui| {
        ui.keyed("one", |ui| {
            ui.label(Label::new("x").style(boxed(Justify::Start, Align::Start)))
        });
    });
    let start = text(&context);
    assert_eq!((start.x, start.y), (0.0, 0.0));

    frame(&mut context, |ui| {
        ui.keyed("one", |ui| {
            ui.label(Label::new("x").style(boxed(Justify::Center, Align::Center)))
        });
    });
    let centre = text(&context);
    assert!(centre.x > 0.0 && centre.x < 100.0, "{centre:?}");
    assert!(centre.y > 0.0 && centre.y < 50.0, "{centre:?}");

    frame(&mut context, |ui| {
        ui.keyed("one", |ui| {
            ui.label(Label::new("x").style(boxed(Justify::End, Align::End)))
        });
    });
    // At the end the text is moved by all the room the box has spare, which
    // is twice what centring moves it by.
    let end = text(&context);
    assert!((end.x - centre.x * 2.0).abs() < 0.01, "{end:?}");
    assert!((end.y - centre.y * 2.0).abs() < 0.01, "{end:?}");

    // Padding still comes first: the box the text is placed in is inside it.
    frame(&mut context, |ui| {
        ui.keyed("one", |ui| {
            ui.label(Label::new("x").style(Style {
                padding: 10.0,
                ..boxed(Justify::Center, Align::Center)
            }))
        });
    });
    let padded = text(&context);
    assert!((padded.x - centre.x).abs() < 0.01, "{padded:?}");
    assert!((padded.y - centre.y).abs() < 0.01, "{padded:?}");
}
