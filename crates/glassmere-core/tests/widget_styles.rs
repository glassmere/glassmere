//! A checkbox, a radio button and a tab strip that take a style of their own,
//! so they need not be as tall as the defaults.
mod support;

use glassmere_core::app::Action;
use glassmere_core::{
    Checkbox, Context, Event, Length, MonospaceMeasurer, Radio, Style, Tab, TabStyle, Ui,
};
use glassmere_scene::Size;
use support::{Driven, act, center};

const ROOT: Size = Size::new(500.0, 300.0);

#[derive(Default)]
struct Page {
    on: bool,
    mode: usize,
}

fn compact() -> Style {
    Style::default().padding(2.0)
}

fn view(styled: bool) -> impl Fn(&Page, &mut Ui<'_, Action<Page>>) {
    move |page, ui| {
        let flip = |on: bool| act(move |s: &mut Page| s.on = on);
        let pick = |option: usize| act(move |s: &mut Page| s.mode = option);
        ui.keyed("box", |ui| {
            if styled {
                ui.checkbox_with(Checkbox::new("Notify").style(compact()), page.on, flip);
            } else {
                ui.checkbox("Notify", page.on, flip);
            }
        });
        ui.keyed("radio", |ui| {
            if styled {
                ui.radio_with(Radio::new("Quiet").style(compact()), page.mode, 1, pick);
            } else {
                ui.radio("Quiet", page.mode, 1, pick);
            }
        });
    }
}

fn height(driven: &Driven<Page>, key: &str) -> f32 {
    driven
        .context
        .node(driven.context.find(&[key]).unwrap())
        .unwrap()
        .bounds
        .height
}

#[test]
fn a_styled_checkbox_and_radio_are_as_short_as_their_style_says() {
    let plain = Driven::new(ROOT, Page::default(), view(false));
    let styled = Driven::new(ROOT, Page::default(), view(true));
    for key in ["box", "radio"] {
        // 12 of padding on each side against 2.
        let difference = height(&plain, key) - height(&styled, key);
        assert!(
            (difference - 20.0).abs() < 0.01,
            "{key}: {} against {}",
            height(&plain, key),
            height(&styled, key)
        );
    }
}

#[test]
fn a_styled_checkbox_and_radio_still_send_their_messages() {
    let mut driven = Driven::new(ROOT, Page::default(), view(true));
    for key in ["box", "radio"] {
        let at = center(&driven.context, &[key]);
        driven.context.event(Event::PointerMoved(at));
        driven.context.event(Event::PointerDown);
        driven.context.event(Event::PointerUp);
        driven.frame();
    }
    assert!(driven.state().on);
    assert_eq!(driven.state().mode, 1);
}

#[test]
fn the_short_forms_are_the_default_look() {
    // `checkbox_with` of a bare label is `checkbox`.
    let mut with = Driven::new(ROOT, Page::default(), |page, ui| {
        ui.keyed("box", |ui| {
            ui.checkbox_with("Notify", page.on, |on| act(move |s: &mut Page| s.on = on));
        });
    });
    let plain = Driven::new(ROOT, Page::default(), view(false));
    assert_eq!(height(&with, "box"), height(&plain, "box"));
    with.frame();
}

fn tabs(styles: Option<TabStyle>) -> Context {
    let mut context = Context::new();
    context.run(ROOT, &mut MonospaceMeasurer, |ui: &mut Ui<'_, ()>| {
        let tabs = [Tab::new("One"), Tab::new("Two"), Tab::new("Three")];
        ui.keyed("tabs", |ui| match &styles {
            Some(styles) => ui.tabs_styled(1, &tabs, styles, |_| (), |_, _| {}),
            None => ui.tabs(1, &tabs, |_| (), |_, _| {}),
        });
    });
    context
}

fn tab_height(context: &Context, index: usize) -> f32 {
    context
        .node(
            context
                .find(&["tabs", "$strip", &index.to_string()])
                .unwrap(),
        )
        .unwrap()
        .bounds
        .height
}

#[test]
fn tabs_take_a_style_for_the_selected_tab_and_one_for_the_rest() {
    let default = tabs(None);
    assert!(
        tab_height(&default, 0) > 30.0,
        "{}",
        tab_height(&default, 0)
    );
    let styled = tabs(Some(TabStyle::new(
        Style::default().height(Length::Px(24.0)),
        Style::default().height(Length::Px(30.0)),
    )));
    assert_eq!(tab_height(&styled, 0), 24.0);
    assert_eq!(tab_height(&styled, 2), 24.0);
    // The strip's second tab is the selected one.
    assert_eq!(tab_height(&styled, 1), 30.0);
}
