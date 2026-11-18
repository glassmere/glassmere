//! A window that shows a markup file as it is edited, and edits it.
//!
//! `glassmere-designer view.glass` previews the file with the values of its
//! `data` block. Saving the file in any editor updates the preview. In the
//! window, an element is selected in the outline or by picking it in the
//! preview; its text, class and properties are edited on the right, and
//! every change is written back to the file.
pub mod edit;

use glassmere::markup::{
    Element, Markup, MarkupError, MarkupEvent, Node, ShowOptions, Value, Values,
};
use glassmere::{
    App, AreaBuilder, Button, Color, ComboBox, Effects, EventArea, Label, Length, NumberOptions,
    Point, Style, TextInput, Theme, Ui, UiEvent, suggest,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a new file starts as.
pub const STARTER: &str = "data { name: \"world\" }\n\ncolumn { padding: 24; gap: 12\n    label \"Hello, {name}\" { font-size: 24 }\n    input \"Name\" { bind: name }\n}\n";

const SIZES: [(&str, Option<(f32, f32)>); 5] = [
    ("Fill the window", None),
    ("320 x 240", Some((320.0, 240.0))),
    ("480 x 320", Some((480.0, 320.0))),
    ("800 x 480", Some((800.0, 480.0))),
    ("390 x 844", Some((390.0, 844.0))),
];

const PALETTE: [(&str, &str); 12] = [
    ("Label", "label \"Text\""),
    ("Button", "button \"Button\" { on-click: clicked }"),
    ("Text input", "input \"Name\""),
    ("Number input", "number \"Amount\""),
    ("Checkbox", "checkbox \"Option\""),
    ("Switch", "switch \"Option\""),
    ("Slider", "slider \"Value\""),
    ("Progress", "progress \"Progress\" { value: 0.5 }"),
    ("Row", "row { gap: 8 }"),
    ("Column", "column { gap: 8 }"),
    ("Box", "box { padding: 12 }"),
    ("Spacer", "spacer"),
];

/// Offered while a property's name is typed.
const PROPERTIES: [&str; 40] = [
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "padding",
    "gap",
    "grow",
    "shrink",
    "radius",
    "background",
    "foreground",
    "font-size",
    "font-weight",
    "font-family",
    "italic",
    "wrap",
    "opacity",
    "align-items",
    "align-self",
    "justify-content",
    "columns",
    "rows",
    "area",
    "aspect-ratio",
    "shadow",
    "hover.background",
    "pressed.background",
    "enabled",
    "tooltip",
    "key",
    "bind",
    "on-click",
    "on-change",
    "on-toggle",
    "value",
    "checked",
    "placeholder",
    "selectable",
];

#[derive(Clone, Debug)]
pub enum Msg {
    /// From a widget of the previewed view.
    View(MarkupEvent),
    TogglePick,
    PickAt(Point),
    Select(usize),
    Text(String),
    Class(String),
    Property(String, String),
    RemoveProperty(String),
    NewName(String),
    NewValue(String),
    AddProperty,
    Delete,
    Move(bool),
    Palette(usize),
    Add(bool),
    Size(usize),
    ResetData,
    Data(String, Value),
    ClearLog,
}

struct Document {
    source: String,
    /// The last version that parsed. When `error` is set its spans belong to
    /// an older text, and nothing may be edited by them.
    markup: Option<Markup>,
    error: Option<MarkupError>,
    values: Values,
    /// The text the file watcher last handed over.
    delivered: Option<Arc<str>>,
}

impl Document {
    fn load(&mut self, text: &str) {
        self.source = text.to_owned();
        match Markup::parse(text) {
            Ok(markup) => {
                self.values.fill_from(markup.defaults());
                self.markup = Some(markup);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

pub struct Designer {
    path: PathBuf,
    document: RefCell<Document>,
    selected: Cell<Option<usize>>,
    pick: bool,
    /// A press in the preview, resolved to an element by the next view.
    picked_at: Cell<Option<Point>>,
    /// What a field holds while its text does not make a file that parses.
    drafts: HashMap<String, String>,
    problem: Option<String>,
    new_name: String,
    new_value: String,
    log: Vec<String>,
    size: usize,
    palette: usize,
}

impl Designer {
    /// Opens `path`, which must exist.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let mut document = Document {
            source: String::new(),
            markup: None,
            error: None,
            values: Values::new(),
            delivered: None,
        };
        document.load(&std::fs::read_to_string(&path)?);
        Ok(Self {
            path,
            document: RefCell::new(document),
            selected: Cell::new(None),
            pick: false,
            picked_at: Cell::new(None),
            drafts: HashMap::new(),
            problem: None,
            new_name: String::new(),
            new_value: String::new(),
            log: Vec::new(),
            size: 0,
            palette: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file's text as the designer has it.
    pub fn source(&self) -> String {
        self.document.borrow().source.clone()
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected.get()
    }

    /// Changes the text through `change`, given the selected element, and
    /// keeps the result if it parses. Otherwise `draft` stays in `field`.
    fn edit(
        &mut self,
        field: &str,
        draft: String,
        change: impl FnOnce(&str, &Markup, &Element) -> Option<String>,
    ) -> bool {
        let mut document = self.document.borrow_mut();
        if document.error.is_some() {
            return false;
        }
        let Some(markup) = &document.markup else {
            return false;
        };
        let Some(element) = self.selected.get().and_then(|id| markup.element(id)) else {
            return false;
        };
        let Some(text) = change(&document.source, markup, element) else {
            return false;
        };
        match Markup::parse(&text) {
            Ok(markup) => {
                self.problem = std::fs::write(&self.path, &text)
                    .err()
                    .map(|error| format!("{}: {error}", self.path.display()));
                document.values.fill_from(markup.defaults());
                document.markup = Some(markup);
                document.source = text;
                self.drafts.remove(field);
                true
            }
            Err(error) => {
                if !field.is_empty() {
                    self.drafts.insert(field.to_owned(), draft);
                }
                self.problem = Some(error.to_string());
                false
            }
        }
    }
}

/// The list `id` is in, and its place there.
fn siblings(nodes: &[Node], id: usize) -> Option<(&[Node], usize)> {
    for (index, node) in nodes.iter().enumerate() {
        let found = match node {
            Node::Element(element) if element.id == id => return Some((nodes, index)),
            Node::Element(element) => siblings(&element.children, id),
            Node::If(node) => siblings(&node.then, id).or_else(|| siblings(&node.otherwise, id)),
            Node::For(node) => siblings(&node.body, id),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// How many numbered things a subtree holds, itself included.
fn weight(node: &Node) -> usize {
    let (inner, other): (&[Node], &[Node]) = match node {
        Node::Element(element) => (&element.children, &[]),
        Node::If(node) => (&node.then, &node.otherwise),
        Node::For(node) => (&node.body, &[]),
        _ => (&[], &[]),
    };
    1 + inner.iter().chain(other).map(weight).sum::<usize>()
}

impl App for Designer {
    type Message = Msg;

    fn update(&mut self, message: Msg, _effects: &mut Effects<'_, Msg>) {
        match message {
            Msg::View(event) => {
                if !self.document.borrow_mut().values.apply(&event) && !event.name.is_empty() {
                    let args: Vec<String> = event.args.iter().map(Value::to_string).collect();
                    let value = match &event.value {
                        Value::None => String::new(),
                        value => format!(" = {value}"),
                    };
                    self.log
                        .push(format!("{}({}){value}", event.name, args.join(", ")));
                }
            }
            Msg::TogglePick => self.pick = !self.pick,
            Msg::PickAt(point) => {
                if self.pick {
                    self.picked_at.set(Some(point));
                    self.pick = false;
                    self.drafts.clear();
                    self.problem = None;
                }
            }
            Msg::Select(id) => {
                self.selected.set(Some(id));
                self.drafts.clear();
                self.problem = None;
            }
            Msg::Text(text) => {
                self.edit("text", text.clone(), |source, _, element| {
                    Some(edit::set_text(source, element, &text))
                });
            }
            Msg::Class(class) => {
                self.edit("class", class.clone(), |source, _, element| {
                    Some(edit::set_class(source, element, &class))
                });
            }
            Msg::Property(name, value) => {
                self.edit(
                    &format!("property {name}"),
                    value.clone(),
                    |source, _, element| Some(edit::set_property(source, element, &name, &value)),
                );
            }
            Msg::RemoveProperty(name) => {
                self.drafts.remove(&format!("property {name}"));
                self.edit("", String::new(), |source, _, element| {
                    Some(edit::remove_property(source, element, &name))
                });
            }
            Msg::NewName(name) => self.new_name = name,
            Msg::NewValue(value) => self.new_value = value,
            Msg::AddProperty => {
                let (name, value) = (
                    self.new_name.trim().to_owned(),
                    self.new_value.trim().to_owned(),
                );
                if name.is_empty() || value.is_empty() {
                    self.problem = Some("a property needs a name and a value".into());
                    return;
                }
                if self.edit("", String::new(), |source, _, element| {
                    Some(edit::set_property(source, element, &name, &value))
                }) {
                    self.new_name.clear();
                    self.new_value.clear();
                }
            }
            Msg::Delete => {
                if self.edit("", String::new(), |source, _, element| {
                    Some(edit::remove_element(source, element))
                }) {
                    self.selected.set(None);
                }
            }
            Msg::Move(up) => {
                let mut moved = None;
                self.edit("", String::new(), |source, markup, element| {
                    let (list, index) = siblings(markup.nodes(), element.id)?;
                    let other = if up { index.checked_sub(1)? } else { index + 1 };
                    let Node::Element(neighbour) = list.get(other)? else {
                        return None;
                    };
                    // Numbers follow the order of writing, so the moved
                    // element takes its neighbour's or passes its subtree.
                    moved = Some(if up {
                        neighbour.id
                    } else {
                        element.id + weight(&list[other])
                    });
                    Some(edit::swap(source, element, neighbour))
                });
                if let Some(id) = moved {
                    self.selected.set(Some(id));
                }
            }
            Msg::Palette(index) => self.palette = index,
            Msg::Add(inside) => {
                let snippet = PALETTE[self.palette].1;
                self.edit("", String::new(), |source, _, element| {
                    Some(if inside {
                        edit::add_child(source, element, snippet)
                    } else {
                        edit::add_after(source, element, snippet)
                    })
                });
            }
            Msg::Size(index) => self.size = index,
            Msg::ResetData => {
                let mut document = self.document.borrow_mut();
                document.values = document
                    .markup
                    .as_ref()
                    .map(|markup| markup.defaults().clone())
                    .unwrap_or_default();
            }
            Msg::Data(name, value) => {
                self.document.borrow_mut().values.set(&name, value);
            }
            Msg::ClearLog => self.log.clear(),
        }
    }

    fn view(&self, ui: &mut Ui<'_, Msg>) {
        // The file as an editor saved it, when that is not what is here.
        if let Some(text) = ui.watched_text(&self.path) {
            let mut document = self.document.borrow_mut();
            if !document
                .delivered
                .as_ref()
                .is_some_and(|known| Arc::ptr_eq(known, &text))
            {
                if *text != *document.source {
                    document.load(&text);
                }
                document.delivered = Some(text);
            }
        }
        if let Some(point) = self.picked_at.take() {
            self.selected.set(Markup::pick(ui.context(), point));
        }
        let document = self.document.borrow();
        let theme = ui.theme().clone();
        let fill = Style::default()
            .width(Length::Percent(1.0))
            .height(Length::Percent(1.0));
        ui.column(fill.clone(), |ui| {
            self.toolbar(ui, &document, &theme);
            ui.row(fill.clone().grow(1.0).shrink(1.0), |ui| {
                self.outline(ui, &document, &theme);
                self.preview(ui, &document, &theme);
                self.inspector(ui, &document, &theme);
            });
        });
    }
}

fn heading(theme: &Theme) -> Style {
    Style::default()
        .font_size(12.0)
        .font_weight(700)
        .foreground(theme.text_muted)
}

/// The stock field, sized by its container.
fn field() -> Style {
    Style {
        height: Length::Px(38.0),
        padding: 8.0,
        ..Style::button()
    }
    .font_size(14.0)
}

fn panel(theme: &Theme, width: f32) -> Style {
    Style::default()
        .width(Length::Px(width))
        .height(Length::Percent(1.0))
        .background(theme.surface)
}

impl Designer {
    fn toolbar(&self, ui: &mut Ui<'_, Msg>, document: &Document, theme: &Theme) {
        let bar = Style::default()
            .width(Length::Percent(1.0))
            .padding(8.0)
            .gap(8.0)
            .background(theme.surface)
            .align_items(glassmere::Align::Center);
        ui.row(bar, |ui| {
            let name = self.path.file_name().unwrap_or_default().to_string_lossy();
            ui.label(Label::new(name).style(Style::default().font_weight(700)));
            let mut pick = Button::new(if self.pick {
                "Click an element..."
            } else {
                "Pick element"
            })
            .on_click(Msg::TogglePick);
            if self.pick {
                pick = pick.style(Style::button().background(theme.accent));
            }
            ui.button(pick);
            ui.tooltip("Select an element by clicking it in the preview");
            let sizes: Vec<&str> = SIZES.iter().map(|(name, _)| *name).collect();
            ui.combo_box(ComboBox::new("Preview size"), self.size, &sizes, Msg::Size);
            ui.button(Button::new("Reset data").on_click(Msg::ResetData));
            ui.tooltip("Back to the values of the file's data block");
            ui.container(Style::default().grow(1.0), |_| {});
            match &document.error {
                Some(error) => ui.label(
                    Label::new(error.to_string())
                        .style(Style::default().foreground(Color::rgb(248, 113, 113))),
                ),
                None => ui.label(Label::new("Saved changes show at once").style(heading(theme))),
            }
        });
    }

    fn outline(&self, ui: &mut Ui<'_, Msg>, document: &Document, theme: &Theme) {
        fn rows(nodes: &[Node], depth: usize, out: &mut Vec<(usize, String, Option<usize>)>) {
            for node in nodes {
                match node {
                    Node::Element(element) => {
                        let mut name = element.kind.clone();
                        if let Some(class) = &element.class {
                            name = format!("{name}.{class}");
                        }
                        if let Some(text) = &element.text {
                            let short: String = text.raw.chars().take(24).collect();
                            name = format!("{name}  \"{short}\"");
                        }
                        out.push((depth, name, Some(element.id)));
                        rows(&element.children, depth + 1, out);
                    }
                    Node::If(node) => {
                        out.push((depth, format!("if {}", node.condition), None));
                        rows(&node.then, depth + 1, out);
                        if !node.otherwise.is_empty() {
                            out.push((depth, "else".into(), None));
                            rows(&node.otherwise, depth + 1, out);
                        }
                    }
                    Node::For(node) => {
                        out.push((depth, format!("for {} in {}", node.item, node.list), None));
                        rows(&node.body, depth + 1, out);
                    }
                    _ => {}
                }
            }
        }
        let mut list = Vec::new();
        if let Some(markup) = &document.markup {
            rows(markup.nodes(), 0, &mut list);
        }
        ui.column(panel(theme, 250.0).padding(8.0).gap(6.0), |ui| {
            ui.label(Label::new("OUTLINE").style(heading(theme)));
            ui.scroll(
                Style::default()
                    .width(Length::Percent(1.0))
                    .grow(1.0)
                    .shrink(1.0),
                |ui| {
                    ui.column(
                        Style::default().width(Length::Percent(1.0)).gap(1.0),
                        |ui| {
                            for (index, (depth, name, id)) in list.into_iter().enumerate() {
                                ui.keyed(index.to_string(), |ui| {
                                    ui.row(Style::default().width(Length::Percent(1.0)), |ui| {
                                        ui.container(
                                            Style::default().width(Length::Px(depth as f32 * 14.0)),
                                            |_| {},
                                        );
                                        let Some(id) = id else {
                                            ui.label(
                                                Label::new(name).style(heading(theme).padding(5.0)),
                                            );
                                            return;
                                        };
                                        let chosen = self.selected.get() == Some(id);
                                        let style = Style::button()
                                            .padding(5.0)
                                            .radius(4.0)
                                            .grow(1.0)
                                            .font_size(13.0)
                                            .justify_content(glassmere::Justify::Start)
                                            .background(if chosen {
                                                theme.selection
                                            } else {
                                                theme.surface
                                            });
                                        ui.button(
                                            Button::new(name)
                                                .style(style)
                                                .on_click(Msg::Select(id)),
                                        );
                                    });
                                });
                            }
                        },
                    );
                },
            );
        });
    }

    fn preview(&self, ui: &mut Ui<'_, Msg>, document: &Document, theme: &Theme) {
        let stage = Style::default()
            .grow(1.0)
            .shrink(1.0)
            .height(Length::Percent(1.0))
            .padding(if SIZES[self.size].1.is_some() {
                24.0
            } else {
                0.0
            })
            .background(theme.window);
        ui.scroll(stage, |ui| {
            let frame = match SIZES[self.size].1 {
                Some((width, height)) => Style::default()
                    .width(Length::Px(width))
                    .height(Length::Px(height))
                    .background(theme.surface),
                None => Style::default()
                    .width(Length::Percent(1.0))
                    .height(Length::Percent(1.0)),
            };
            let mut area = EventArea::pointer().capturing();
            if self.pick {
                area = area.consuming();
            }
            let pick = self.pick;
            let watch = AreaBuilder::new(area).on_event(move |event| match event {
                UiEvent::PointerDown(point) if pick => Some(Msg::PickAt(point)),
                _ => None,
            });
            ui.event_area(frame, watch, |ui| {
                let Some(markup) = &document.markup else {
                    ui.label("Nothing to show until the file parses.");
                    return;
                };
                let mut options = ShowOptions::default().selected(self.selected.get());
                if let Some(folder) = self.path.parent() {
                    options = options.base_dir(folder);
                }
                markup.show_with(ui, &document.values, &options, Msg::View);
            });
        });
    }

    fn inspector(&self, ui: &mut Ui<'_, Msg>, document: &Document, theme: &Theme) {
        let wide = Style::default().width(Length::Percent(1.0));
        ui.scroll(panel(theme, 320.0), |ui| {
            ui.column(wide.clone().padding(10.0).gap(8.0), |ui| {
                ui.label(Label::new("ELEMENT").style(heading(theme)));
                let element = match (&document.markup, &document.error) {
                    (Some(markup), None) => self.selected.get().and_then(|id| markup.element(id)),
                    _ => None,
                };
                match element {
                    Some(element) => self.element(ui, element, theme),
                    None if document.error.is_some() => {
                        ui.label(
                            "The file has an error. It can be edited here again once it parses.",
                        );
                    }
                    None => ui.label(
                        Label::new("Select an element in the outline, or pick one in the preview.")
                            .style(wide.clone().wrap(glassmere::TextWrap::Word)),
                    ),
                }
                if let Some(problem) = &self.problem {
                    ui.label(
                        Label::new(problem.as_str()).style(
                            wide.clone()
                                .wrap(glassmere::TextWrap::Word)
                                .foreground(Color::rgb(248, 113, 113)),
                        ),
                    );
                }

                ui.label(Label::new("DATA").style(heading(theme)));
                let names = document
                    .markup
                    .as_ref()
                    .map(Markup::references)
                    .unwrap_or_default();
                for name in names {
                    ui.keyed(format!("data {name}"), |ui| {
                        let to = name.clone();
                        if !matches!(
                            document.values.get(&name),
                            Some(Value::Bool(_) | Value::List(_) | Value::Record(_))
                        ) {
                            ui.label(
                                Label::new(name.as_str()).style(Style::default().font_size(12.0)),
                            );
                        }
                        match document.values.get(&name) {
                            Some(Value::Bool(value)) => {
                                ui.checkbox(name.as_str(), *value, move |value| {
                                    Msg::Data(to.clone(), value.into())
                                });
                            }
                            Some(Value::Number(value)) => {
                                ui.number_input(
                                    name.as_str(),
                                    *value,
                                    NumberOptions::default(),
                                    move |value| Msg::Data(to.clone(), value.into()),
                                );
                            }
                            Some(value @ (Value::List(_) | Value::Record(_))) => {
                                ui.label(Label::new(format!("{name}: {value}")).style(
                                    wide.clone().wrap(glassmere::TextWrap::Word).font_size(13.0),
                                ));
                            }
                            other => {
                                let text = other.map(Value::to_string).unwrap_or_default();
                                ui.text_input(
                                    TextInput::new(name.as_str())
                                        .style(field().width(Length::Percent(1.0))),
                                    &text,
                                    move |text| Msg::Data(to.clone(), text.into()),
                                );
                            }
                        }
                    });
                }

                ui.row(
                    wide.clone().gap(8.0).align_items(glassmere::Align::Center),
                    |ui| {
                        ui.label(Label::new("EVENTS").style(heading(theme).grow(1.0)));
                        ui.button(
                            Button::new("Clear")
                                .enabled(!self.log.is_empty())
                                .on_click(Msg::ClearLog),
                        );
                    },
                );
                let from = self.log.len().saturating_sub(8);
                for (index, line) in self.log.iter().enumerate().skip(from) {
                    ui.keyed(format!("event {index}"), |ui| {
                        ui.label(Label::new(line.as_str()).style(Style::default().font_size(13.0)));
                    });
                }
            });
        });
    }

    fn element(&self, ui: &mut Ui<'_, Msg>, element: &Element, theme: &Theme) {
        let wide = Style::default().width(Length::Percent(1.0));
        let draft = |field: &str, saved: &str| {
            self.drafts
                .get(field)
                .cloned()
                .unwrap_or_else(|| saved.to_owned())
        };
        ui.label(format!("{}, line {}", element.kind, element.line));
        let text = draft(
            "text",
            element.text.as_ref().map_or("", |text| text.raw.as_str()),
        );
        let caption = Style::default().font_size(12.0);
        ui.label(Label::new("Text").style(caption.clone()));
        ui.text_input(
            TextInput::new("Text").style(field().width(Length::Percent(1.0))),
            &text,
            Msg::Text,
        );
        let class = draft("class", element.class.as_deref().unwrap_or(""));
        ui.label(Label::new("Class").style(caption.clone()));
        ui.text_input(
            TextInput::new("Class").style(field().width(Length::Percent(1.0))),
            &class,
            Msg::Class,
        );

        for property in &element.properties {
            ui.keyed(format!("property {}", property.name), |ui| {
                ui.row(
                    wide.clone().gap(6.0).align_items(glassmere::Align::Center),
                    |ui| {
                        let name = property.name.clone();
                        let value = draft(&format!("property {name}"), &property.value);
                        ui.label(
                            Label::new(name.as_str())
                                .style(caption.clone().width(Length::Px(96.0))),
                        );
                        ui.text_input(
                            TextInput::new(name.as_str())
                                .style(field().width(Length::Px(60.0)).grow(1.0).shrink(1.0)),
                            &value,
                            move |value| Msg::Property(name.clone(), value),
                        );
                        ui.button(
                            Button::new("Remove")
                                .style(Style::button().padding(6.0).font_size(12.0))
                                .on_click(Msg::RemoveProperty(property.name.clone())),
                        );
                    },
                );
            });
        }

        ui.label(Label::new("ADD A PROPERTY").style(heading(theme)));
        ui.autocomplete(
            "Property name",
            &self.new_name,
            &suggest(&self.new_name, &PROPERTIES, 8),
            Msg::NewName,
        );
        ui.row(wide.clone().gap(6.0), |ui| {
            ui.text_input(
                TextInput::new("Property value")
                    .style(field().width(Length::Px(60.0)).grow(1.0).shrink(1.0))
                    .on_submit(Msg::AddProperty),
                &self.new_value,
                Msg::NewValue,
            );
            ui.button(Button::new("Add").on_click(Msg::AddProperty));
        });

        ui.label(Label::new("ARRANGE").style(heading(theme)));
        ui.row(wide.clone().gap(6.0), |ui| {
            ui.button(Button::new("Move up").on_click(Msg::Move(true)));
            ui.button(Button::new("Move down").on_click(Msg::Move(false)));
            ui.button(Button::new("Delete").on_click(Msg::Delete));
        });
        let kinds: Vec<&str> = PALETTE.iter().map(|(name, _)| *name).collect();
        ui.combo_box(
            ComboBox::new("New element"),
            self.palette,
            &kinds,
            Msg::Palette,
        );
        ui.row(wide.gap(6.0), |ui| {
            ui.button(Button::new("Add inside").on_click(Msg::Add(true)));
            ui.button(Button::new("Add after").on_click(Msg::Add(false)));
        });
    }
}
