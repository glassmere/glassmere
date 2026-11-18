//! Works out what an expression stands for.
use crate::ast::{Expr, Operator, Piece};
use crate::{Value, Values};
use std::cmp::Ordering;

/// The names an expression can see: the loop and component variables around
/// it, innermost last, then the application's values.
pub(crate) struct Scope<'a> {
    values: Option<&'a Values>,
    pub locals: Vec<(String, Value)>,
}

impl<'a> Scope<'a> {
    pub fn new(values: &'a Values) -> Self {
        Self {
            values: Some(values),
            locals: Vec::new(),
        }
    }

    /// For values that may not name anything, as in a `data` block.
    pub fn constant() -> Self {
        Self {
            values: None,
            locals: Vec::new(),
        }
    }

    pub fn lookup(&self, path: &[String]) -> Value {
        let (first, rest) = path.split_first().expect("a path has a name");
        let local = self.locals.iter().rev().find(|(name, _)| name == first);
        let root = match local {
            Some((_, value)) => Some(value),
            None => self.values.and_then(|values| values.get(first)),
        };
        root.and_then(|root| rest.iter().try_fold(root, |value, name| value.field(name)))
            .cloned()
            .unwrap_or_default()
    }

    pub fn text(&self, pieces: &[Piece]) -> String {
        let mut text = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(part) => text.push_str(part),
                Piece::Expr(expr) => text.push_str(&self.eval(expr).to_string()),
            }
        }
        text
    }

    pub fn eval(&self, expr: &Expr) -> Value {
        match expr {
            Expr::Literal(value) => value.clone(),
            Expr::Path(path) => self.lookup(path),
            Expr::Template(pieces) => Value::Text(self.text(pieces)),
            Expr::List(items) => Value::List(items.iter().map(|item| self.eval(item)).collect()),
            Expr::Record(fields) => Value::Record(
                fields
                    .iter()
                    .map(|(name, value)| (name.clone(), self.eval(value)))
                    .collect(),
            ),
            Expr::Not(inner) => Value::Bool(!self.eval(inner).truthy()),
            Expr::Negate(inner) => self
                .eval(inner)
                .number()
                .map_or(Value::None, |number| Value::Number(-number)),
            Expr::Length(inner) => Value::Number(match self.eval(inner) {
                Value::List(items) => items.len() as f64,
                Value::Text(text) => text.chars().count() as f64,
                Value::Record(fields) => fields.len() as f64,
                _ => 0.0,
            }),
            Expr::Choice(test, then, otherwise) => {
                if self.eval(test).truthy() {
                    self.eval(then)
                } else {
                    self.eval(otherwise)
                }
            }
            Expr::Binary(Operator::And, left, right) => {
                Value::Bool(self.eval(left).truthy() && self.eval(right).truthy())
            }
            Expr::Binary(Operator::Or, left, right) => {
                Value::Bool(self.eval(left).truthy() || self.eval(right).truthy())
            }
            Expr::Binary(operator, left, right) => {
                binary(*operator, self.eval(left), self.eval(right))
            }
        }
    }
}

fn binary(operator: Operator, left: Value, right: Value) -> Value {
    let order = || match (&left, &right) {
        (Value::Text(left), Value::Text(right)) => Some(left.cmp(right)),
        _ => left.number()?.partial_cmp(&right.number()?),
    };
    let arithmetic = |apply: fn(f64, f64) -> f64| match (left.number(), right.number()) {
        (Some(left), Some(right)) => Value::Number(apply(left, right)),
        _ => Value::None,
    };
    match operator {
        Operator::Equal => Value::Bool(left == right),
        Operator::NotEqual => Value::Bool(left != right),
        Operator::Less => Value::Bool(order() == Some(Ordering::Less)),
        Operator::LessEqual => {
            Value::Bool(matches!(order(), Some(Ordering::Less | Ordering::Equal)))
        }
        Operator::Greater => Value::Bool(order() == Some(Ordering::Greater)),
        Operator::GreaterEqual => {
            Value::Bool(matches!(order(), Some(Ordering::Greater | Ordering::Equal)))
        }
        // Text on either side joins; two numbers add.
        Operator::Add => match (&left, &right) {
            (Value::Number(left), Value::Number(right)) => Value::Number(left + right),
            _ => Value::Text(format!("{left}{right}")),
        },
        Operator::Subtract => arithmetic(|left, right| left - right),
        Operator::Multiply => arithmetic(|left, right| left * right),
        Operator::Divide => arithmetic(|left, right| left / right),
        Operator::Remainder => arithmetic(|left, right| left % right),
        Operator::And | Operator::Or => unreachable!("decided before both sides are known"),
    }
}
