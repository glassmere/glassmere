use std::collections::BTreeMap;
use std::fmt;

/// What a name in a markup file stands for.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    None,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Value>),
    Record(BTreeMap<String, Value>),
}

impl Value {
    /// `None`, `false`, zero, and empty text and lists are false.
    pub fn truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0,
            Self::Text(text) => !text.is_empty(),
            Self::List(items) => !items.is_empty(),
            Self::Record(_) => true,
        }
    }

    /// The number this is, or the one its text spells.
    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(f64::from(u8::from(*value))),
            Self::Text(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn list(&self) -> &[Value] {
        match self {
            Self::List(items) => items,
            _ => &[],
        }
    }

    /// The field of a record, or the item of a list when `name` is an index.
    pub fn field(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Record(fields) => fields.get(name),
            Self::List(items) => items.get(name.parse::<usize>().ok()?),
            _ => None,
        }
    }

    fn field_mut(&mut self, name: &str) -> Option<&mut Value> {
        if let Self::List(items) = self {
            return items.get_mut(name.parse::<usize>().ok()?);
        }
        if !matches!(self, Self::Record(_)) {
            *self = Self::Record(BTreeMap::new());
        }
        let Self::Record(fields) = self else {
            unreachable!()
        };
        Some(fields.entry(name.to_owned()).or_default())
    }

    /// A record from `(name, value)` pairs.
    pub fn record<N: Into<String>, V: Into<Value>>(
        fields: impl IntoIterator<Item = (N, V)>,
    ) -> Self {
        Self::Record(
            fields
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        )
    }
}

/// As the value reads in text: whole numbers without a fraction, `None` as
/// nothing.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => Ok(()),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Number(value) if value.fract() == 0.0 && value.abs() < 1e15 => {
                write!(f, "{}", *value as i64)
            }
            Self::Number(value) => write!(f, "{value}"),
            Self::Text(text) => f.write_str(text),
            Self::List(items) => {
                f.write_str("[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Self::Record(fields) => {
                f.write_str("{")?;
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                f.write_str("}")
            }
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}
impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(items: Vec<T>) -> Self {
        Self::List(items.into_iter().map(Into::into).collect())
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::None, Into::into)
    }
}
macro_rules! number {
    ($($type:ty),*) => {$(
        impl From<$type> for Value {
            fn from(value: $type) -> Self {
                Self::Number(value as f64)
            }
        }
    )*};
}
number!(f64, f32, i32, i64, u32, u64, usize);

/// The values a markup file reads, by name. `a.b` is the field `b` of the
/// record `a`, and `a.0` the first item of the list `a`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Values {
    root: Value,
}

impl Values {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `path`, making the records on the way to it. An index past the
    /// end of a list sets nothing.
    pub fn set(&mut self, path: &str, value: impl Into<Value>) -> &mut Self {
        let mut target = Some(&mut self.root);
        for name in path.split('.') {
            target = target.and_then(|value| value.field_mut(name));
        }
        if let Some(target) = target {
            *target = value.into();
        }
        self
    }

    pub fn get(&self, path: &str) -> Option<&Value> {
        path.split('.')
            .try_fold(&self.root, |value, name| value.field(name))
    }

    /// The top-level names and their values, in order of name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        let fields = match &self.root {
            Value::Record(fields) => Some(fields),
            _ => None,
        };
        fields
            .into_iter()
            .flatten()
            .map(|(name, value)| (name.as_str(), value))
    }

    /// Stores what a widget with `bind` changed. False when the event is not
    /// from one, and the application has to act on it itself.
    pub fn apply(&mut self, event: &MarkupEvent) -> bool {
        match &event.bind {
            Some(path) => {
                self.set(path, event.value.clone());
                true
            }
            None => false,
        }
    }

    /// Takes every top-level value of `other` that this one does not have.
    pub fn fill_from(&mut self, other: &Values) {
        for (name, value) in other.iter() {
            if self.get(name).is_none() {
                self.set(name, value.clone());
            }
        }
    }
}

/// What a widget of a markup view sent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkupEvent {
    /// The name after `on-click:` and the like. Empty when the widget has
    /// only `bind`, or no handler at all.
    pub name: String,
    /// The arguments of `on-click: remove(item.id)`, as they were when the
    /// view was declared.
    pub args: Vec<Value>,
    /// The widget's new value: a checkbox's state, a field's text, a
    /// slider's number, the index chosen. `None` for a click.
    pub value: Value,
    /// The path after `bind:`, which [`Values::apply`] stores `value` at.
    pub bind: Option<String>,
}
