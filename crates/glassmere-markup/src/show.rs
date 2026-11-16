//! Declares the widgets of a parsed file.
use crate::ast::{Component, Element, Node, PropertyKind};
use crate::eval::Scope;
use crate::{Markup, MarkupEvent, ShowOptions, Value};
use glassmere_core::{
    Button, Collapsible, ComboBox, ImageFit, ImageOptions, Label, Link, NumberOptions, Progress,
    Shadow, Spinner, Style, Switch, Tab, TextInput, Ui,
};
use std::rc::Rc;

/// How deep components may use each other before one is taken to use itself.
const DEPTH: usize = 32;

pub(crate) struct Shower<'m, M> {
    pub markup: &'m Markup,
    pub scope: Scope<'m>,
    pub on: Rc<dyn Fn(MarkupEvent) -> M>,
    pub options: &'m ShowOptions,
    pub depth: usize,
}

/// The key of the node an element becomes. [`Markup::pick`] reads the
/// element's number back out of it.
fn key(prefix: &str, id: usize) -> String {
    format!("{prefix}gm{id}")
}

impl<M: 'static> Shower<'_, M> {
    /// `prefix` keeps the keys of siblings apart where a loop, a condition or
    /// a component puts the same elements into one parent more than once.
    pub fn nodes(&mut self, ui: &mut Ui<'_, M>, nodes: &[Node], prefix: &str) {
        for node in nodes {
            match node {
                Node::Element(element) => self.element(ui, element, prefix),
                Node::If(node) => {
                    if self.scope.eval(&node.test).truthy() {
                        self.nodes(ui, &node.then, &format!("{prefix}{}t/", node.id));
                    } else {
                        self.nodes(ui, &node.otherwise, &format!("{prefix}{}e/", node.id));
                    }
                }
                Node::For(node) => {
                    let items = self.scope.eval(&node.items);
                    for (index, item) in items.list().iter().enumerate() {
                        let locals = self.scope.locals.len();
                        self.scope.locals.push((node.item.clone(), item.clone()));
                        if let Some(name) = &node.index {
                            self.scope.locals.push((name.clone(), index.into()));
                        }
                        self.nodes(ui, &node.body, &format!("{prefix}{}[{index}]/", node.id));
                        self.scope.locals.truncate(locals);
                    }
                }
            }
        }
    }

    fn value(&self, element: &Element, name: &str) -> Option<Value> {
        element
            .properties
            .iter()
            .find_map(|property| match &property.kind {
                PropertyKind::Value(expr) if property.name == name => Some(self.scope.eval(expr)),
                _ => None,
            })
    }

    fn number(&self, element: &Element, name: &str) -> Option<f64> {
        self.value(element, name)?.number()
    }

    /// The widget's value: what `name` says, or what `bind` names.
    fn current(&self, element: &Element, name: &str) -> Value {
        self.value(element, name)
            .or_else(|| {
                element
                    .properties
                    .iter()
                    .find_map(|property| match &property.kind {
                        PropertyKind::Bind(path) => Some(self.scope.lookup(path)),
                        _ => None,
                    })
            })
            .unwrap_or_default()
    }

    fn text(&self, element: &Element) -> String {
        element
            .text
            .as_ref()
            .map_or_else(String::new, |text| self.scope.text(&text.pieces))
    }

    /// What the widget sends when `name` happens, given its new value.
    fn handler(
        &self,
        element: &Element,
        name: &str,
        bound: bool,
    ) -> impl Fn(Value) -> M + 'static + use<M> {
        let mut event = MarkupEvent::default();
        for property in &element.properties {
            match &property.kind {
                PropertyKind::Event { name: sent, args } if property.name == name => {
                    event.name = sent.clone();
                    event.args = args.iter().map(|arg| self.scope.eval(arg)).collect();
                }
                PropertyKind::Bind(path) if bound => event.bind = Some(path.join(".")),
                _ => {}
            }
        }
        let on = self.on.clone();
        move |value| {
            on(MarkupEvent {
                value,
                ..event.clone()
            })
        }
    }

    fn style(&self, ui: &mut Ui<'_, M>, element: &Element) -> Option<Style> {
        let selected = self.options.selected == Some(element.id);
        if element.class.is_none() && !element.styled && !selected {
            return None;
        }
        let mut style = match &element.class {
            // The class may have been reloaded since the file was read, so
            // the element's own properties go over it each time.
            Some(class) => {
                let mut style = ui.class(class);
                for property in &element.properties {
                    if matches!(property.kind, PropertyKind::Style) {
                        let _ = style.set_property(&property.name, &property.value);
                    }
                }
                style
            }
            None => element.fixed.clone(),
        };
        for property in &element.properties {
            if let PropertyKind::DynamicStyle(pieces) = &property.kind {
                let _ = style.set_property(&property.name, &self.scope.text(pieces));
            }
        }
        if selected {
            let mut ring = Shadow::new(ui.theme().accent, 0.0);
            ring.spread = 2.0;
            ring.inset = true;
            style.shadow = Some(Box::new(ring));
        }
        Some(style)
    }

    fn element(&mut self, ui: &mut Ui<'_, M>, element: &Element, prefix: &str) {
        let key = match self.value(element, "key") {
            Some(name) => format!("{}:{name}", key(prefix, element.id)),
            None => key(prefix, element.id),
        };
        if let Some(component) = self.markup.components.get(&element.kind) {
            self.component(ui, element, component, &key);
            return;
        }
        let enabled = self
            .value(element, "enabled")
            .is_none_or(|value| value.truthy());
        let tooltip = self.value(element, "tooltip");
        ui.enabled(enabled, |ui| {
            ui.keyed(key, |ui| self.widget(ui, element));
        });
        if let Some(tooltip) = tooltip {
            ui.tooltip(tooltip.to_string());
        }
    }

    fn component(
        &mut self,
        ui: &mut Ui<'_, M>,
        element: &Element,
        component: &Component,
        key: &str,
    ) {
        if self.depth >= DEPTH {
            return;
        }
        let locals = component
            .parameters
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let value = self.value(element, name).unwrap_or_else(|| {
                    if index == 0 && element.text.is_some() {
                        Value::Text(self.text(element))
                    } else {
                        Value::None
                    }
                });
                (name.clone(), value)
            })
            .collect();
        let outer = std::mem::replace(&mut self.scope.locals, locals);
        self.depth += 1;
        self.nodes(ui, &component.body, &format!("{key}/"));
        self.depth -= 1;
        self.scope.locals = outer;
    }

    fn widget(&mut self, ui: &mut Ui<'_, M>, element: &Element) {
        let style = self.style(ui, element);
        let text = self.text(element);
        match element.kind.as_str() {
            "column" => ui.column(style.unwrap_or_default(), |ui| {
                self.nodes(ui, &element.children, "");
            }),
            "row" => ui.row(style.unwrap_or_default(), |ui| {
                self.nodes(ui, &element.children, "");
            }),
            "box" => ui.container(style.unwrap_or_default(), |ui| {
                self.nodes(ui, &element.children, "");
            }),
            "grid" => ui.grid(style.unwrap_or_default(), |ui| {
                self.nodes(ui, &element.children, "");
            }),
            "scroll" => ui.scroll(style.unwrap_or_default(), |ui| {
                self.nodes(ui, &element.children, "");
            }),
            "spacer" => {
                let mut style = style.unwrap_or_default();
                if style.grow == 0.0 {
                    style.grow = 1.0;
                }
                ui.container(style, |_| {});
            }
            "label" => {
                let mut label = Label::new(text);
                if let Some(style) = style {
                    label = label.style(style);
                }
                if self
                    .value(element, "selectable")
                    .is_some_and(|value| value.truthy())
                {
                    label = label.selectable();
                }
                ui.label(label);
            }
            "button" => {
                let mut button = Button::new(text)
                    .on_click(self.handler(element, "on-click", false)(Value::None));
                if let Some(style) = style {
                    button = button.style(style);
                }
                ui.button(button);
            }
            "link" => {
                let mut link =
                    Link::new(text).on_click(self.handler(element, "on-click", false)(Value::None));
                if let Some(style) = style {
                    link = link.style(style);
                }
                ui.link(link);
            }
            "checkbox" => {
                let send = self.handler(element, "on-toggle", true);
                let checked = self.current(element, "checked").truthy();
                ui.checkbox(text, checked, move |checked| send(checked.into()));
            }
            "switch" => {
                let send = self.handler(element, "on-toggle", true);
                let mut switch = Switch::new(text);
                if let Some(style) = style {
                    switch = switch.style(style);
                }
                let on = self.current(element, "checked").truthy();
                ui.switch(switch, on, move |on| send(on.into()));
            }
            "radio" => {
                let send = self.handler(element, "on-select", true);
                let option = self.number(element, "option").unwrap_or(0.0) as usize;
                let selected = self
                    .current(element, "selected")
                    .number()
                    .map_or(usize::MAX, |selected| selected as usize);
                ui.radio(text, selected, option, move |option| send(option.into()));
            }
            "slider" => {
                let send = self.handler(element, "on-change", true);
                let minimum = self.number(element, "min").unwrap_or(0.0);
                let maximum = self.number(element, "max").unwrap_or(100.0).max(minimum);
                let step = self.number(element, "step").unwrap_or(1.0);
                let value = self.current(element, "value").number().unwrap_or(minimum);
                ui.slider(
                    text,
                    value.clamp(minimum, maximum),
                    minimum..=maximum,
                    step,
                    move |value| send(value.into()),
                );
            }
            "input" => {
                let send = self.handler(element, "on-change", true);
                let value = self.current(element, "value").to_string();
                let mut input = TextInput::new(text);
                if let Some(placeholder) = self.value(element, "placeholder") {
                    input = input.placeholder(placeholder.to_string());
                }
                if self
                    .value(element, "password")
                    .is_some_and(|value| value.truthy())
                {
                    input = input.password();
                }
                if element
                    .properties
                    .iter()
                    .any(|property| property.name == "on-submit")
                {
                    input = input.on_submit(self.handler(element, "on-submit", false)(
                        value.as_str().into(),
                    ));
                }
                if let Some(style) = style {
                    input = input.style(style);
                }
                ui.text_input(input, &value, move |text| send(text.into()));
            }
            "number" => {
                let send = self.handler(element, "on-change", true);
                let mut options = NumberOptions::default();
                if let Some(minimum) = self.number(element, "min") {
                    options.min = minimum;
                }
                if let Some(maximum) = self.number(element, "max") {
                    options.max = maximum;
                }
                if let Some(step) = self.number(element, "step") {
                    options = options.step(step);
                }
                if let Some(decimals) = self.number(element, "decimals") {
                    options = options.decimals(decimals as usize);
                }
                if let Some(suffix) = self.value(element, "suffix") {
                    options = options.suffix(suffix.to_string());
                }
                if let Some(style) = style {
                    options = options.style(style);
                }
                let value = self.current(element, "value").number().unwrap_or(0.0);
                ui.number_input(text, value, options, move |value| send(value.into()));
            }
            "progress" => {
                let mut progress = Progress::new(text);
                if let Some(style) = style {
                    progress = progress.style(style);
                }
                ui.progress(progress, self.number(element, "value"));
            }
            "spinner" => ui.spinner(Spinner::new(text)),
            "combo" => {
                let send = self.handler(element, "on-choose", true);
                let items: Vec<String> = self
                    .value(element, "items")
                    .unwrap_or_default()
                    .list()
                    .iter()
                    .map(Value::to_string)
                    .collect();
                let items: Vec<&str> = items.iter().map(String::as_str).collect();
                let selected = self.current(element, "selected").number().unwrap_or(0.0) as usize;
                let mut combo = ComboBox::new(text);
                if let Some(style) = style {
                    combo = combo.style(style);
                }
                ui.combo_box(combo, selected, &items, move |index| send(index.into()));
            }
            "image" => {
                let source = self.value(element, "src").unwrap_or_default().to_string();
                let path = match &self.options.base_dir {
                    Some(base) => base.join(&source),
                    None => source.into(),
                };
                let fit = match self
                    .value(element, "fit")
                    .unwrap_or_default()
                    .to_string()
                    .as_str()
                {
                    "cover" => ImageFit::Cover,
                    "fill" => ImageFit::Fill,
                    "none" => ImageFit::None,
                    _ => ImageFit::Contain,
                };
                let options = ImageOptions::new(text)
                    .fit(fit)
                    .style(style.unwrap_or_default());
                ui.image_file(path, options);
            }
            "tabs" => {
                let send = self.handler(element, "on-select", true);
                let pages: Vec<&Element> = element
                    .children
                    .iter()
                    .filter_map(|child| match child {
                        Node::Element(page) => Some(page),
                        _ => None,
                    })
                    .collect();
                let tabs: Vec<Tab> = pages.iter().map(|page| Tab::new(self.text(page))).collect();
                let selected = self.current(element, "selected").number().unwrap_or(0.0) as usize;
                ui.tabs(
                    selected.min(tabs.len().saturating_sub(1)),
                    &tabs,
                    move |index| send(index.into()),
                    |ui, index| {
                        if let Some(page) = pages.get(index) {
                            self.nodes(ui, &page.children, "");
                        }
                    },
                );
            }
            "collapsible" => {
                let send = self.handler(element, "on-toggle", true);
                let mut section = Collapsible::new(text);
                if let Some(style) = style {
                    section = section.style(style);
                }
                let open = self.current(element, "open").truthy();
                ui.collapsible(
                    section,
                    open,
                    move |open| send(open.into()),
                    |ui| {
                        self.nodes(ui, &element.children, "");
                    },
                );
            }
            // The parser lets no other name through.
            _ => {}
        }
    }
}
