//! A small text format for named style classes and theme tokens.
//!
//! ```text
//! // comments start with //
//! theme dark            // optional base; tokens overlay whichever theme is active
//! token accent #1a9c8f
//!
//! class primary {
//!     background: #1a9c8f
//!     foreground: #ffffff
//!     padding: 12
//!     radius: 8
//!     font-size: 16
//!     font-weight: 700
//!     hover.background: #22b0a0
//!     pressed.background: #157d72
//! }
//! class danger extends primary { background: #c0392b }
//! ```
//!
//! Properties: width, height (`auto`, `50%`, `120`), padding, gap, grow, radius,
//! background, foreground (`#rrggbb` or `#rrggbbaa`), font-size, font-weight,
//! font-family (`sans`, `serif`, `monospace`, `"Name"`), italic, wrap (`word`,
//! `none`) and `hover.`/`pressed.`/`focused.`/`disabled.` prefixed background,
//! foreground and radius. One property per line or `;`-separated.
use crate::{Length, Paint, Style, Theme};
use rust_gui_scene::{Color, FontFamily, TextWrap};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleSheet {
    /// Base theme declared by the sheet, applied when the sheet is installed.
    pub theme: Option<Theme>,
    /// Token overrides applied on top of whichever theme is active.
    pub tokens: Vec<(String, Color)>,
    classes: HashMap<String, Style>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleSheetError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for StyleSheetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}
impl std::error::Error for StyleSheetError {}

impl StyleSheet {
    pub fn class(&self, name: &str) -> Option<&Style> {
        self.classes.get(name)
    }

    pub fn insert(&mut self, name: impl Into<String>, style: Style) {
        self.classes.insert(name.into(), style);
    }

    pub fn classes(&self) -> impl Iterator<Item = (&str, &Style)> {
        self.classes
            .iter()
            .map(|(name, style)| (name.as_str(), style))
    }

    /// The sheet's base theme, or `current`, with the sheet's tokens applied.
    pub fn apply_to(&self, current: &Theme) -> Theme {
        let mut theme = self.theme.clone().unwrap_or_else(|| current.clone());
        for (name, color) in &self.tokens {
            theme.set_token(name, *color);
        }
        theme
    }

    pub fn parse(text: &str) -> Result<Self, StyleSheetError> {
        let mut sheet = Self::default();
        let mut current: Option<(String, Style)> = None;
        let fail = |line: usize, message: String| Err(StyleSheetError { line, message });
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let content = raw.split("//").next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            if let Some((name, style)) = current.as_mut() {
                let body = content.strip_suffix('}');
                let closes = body.is_some();
                for property in body.unwrap_or(content).split(';') {
                    let property = property.trim();
                    if property.is_empty() {
                        continue;
                    }
                    let Some((key, value)) = property.split_once(':') else {
                        return fail(
                            line,
                            format!("expected `property: value` in class `{name}`"),
                        );
                    };
                    apply(style, key.trim(), value.trim())
                        .map_err(|message| StyleSheetError { line, message })?;
                }
                if closes {
                    let (name, style) = current.take().unwrap();
                    sheet.classes.insert(name, style);
                }
                continue;
            }
            let mut words = content.split_whitespace();
            match words.next() {
                Some("theme") => {
                    sheet.theme = Some(match words.next() {
                        Some("dark") => Theme::dark(),
                        Some("light") => Theme::light(),
                        other => {
                            return fail(
                                line,
                                format!("unknown theme {other:?}; use dark or light"),
                            );
                        }
                    });
                }
                Some("token") => {
                    let (Some(name), Some(value)) = (words.next(), words.next()) else {
                        return fail(line, "expected `token name #rrggbb`".into());
                    };
                    let color =
                        color(value).map_err(|message| StyleSheetError { line, message })?;
                    if !crate::TOKENS.contains(&name) {
                        return fail(line, format!("unknown token `{name}`"));
                    }
                    sheet.tokens.push((name.to_owned(), color));
                }
                Some("class") => {
                    let Some(name) = words.next() else {
                        return fail(line, "expected a class name".into());
                    };
                    let name = name.trim_end_matches('{').to_owned();
                    let mut style = Style::default();
                    let mut rest: Vec<&str> = words.collect();
                    if rest.first() == Some(&"extends") {
                        let Some(base) = rest.get(1) else {
                            return fail(line, "expected a base class after `extends`".into());
                        };
                        let base = base.trim_end_matches('{');
                        style =
                            sheet
                                .classes
                                .get(base)
                                .cloned()
                                .ok_or_else(|| StyleSheetError {
                                    line,
                                    message: format!("unknown base class `{base}`"),
                                })?;
                        rest.drain(..2);
                    }
                    let inline = rest.join(" ");
                    let inline = inline.trim();
                    let inline = inline.strip_prefix('{').unwrap_or(inline).trim();
                    if let Some(body) = inline.strip_suffix('}') {
                        for property in body.split(';') {
                            let property = property.trim();
                            if property.is_empty() {
                                continue;
                            }
                            let Some((key, value)) = property.split_once(':') else {
                                return fail(
                                    line,
                                    format!("expected `property: value` in class `{name}`"),
                                );
                            };
                            apply(&mut style, key.trim(), value.trim())
                                .map_err(|message| StyleSheetError { line, message })?;
                        }
                        sheet.classes.insert(name, style);
                    } else if inline.is_empty() {
                        current = Some((name, style));
                    } else {
                        return fail(line, "class bodies start with `{` on the same line".into());
                    }
                }
                Some(other) => {
                    return fail(line, format!("unknown directive `{other}`"));
                }
                None => {}
            }
        }
        if let Some((name, _)) = current {
            return fail(
                text.lines().count(),
                format!("class `{name}` is missing `}}`"),
            );
        }
        Ok(sheet)
    }
}

fn apply(style: &mut Style, key: &str, value: &str) -> Result<(), String> {
    if let Some((state, property)) = key.split_once('.') {
        let states = style.states.get_or_insert_with(Default::default);
        let paint: &mut Paint = match state {
            "hover" => &mut states.hover,
            "pressed" => &mut states.pressed,
            "focused" => &mut states.focused,
            "disabled" => &mut states.disabled,
            _ => return Err(format!("unknown state `{state}`")),
        };
        match property {
            "background" => paint.background = Some(color(value)?),
            "foreground" => paint.foreground = Some(color(value)?),
            "radius" => paint.radius = Some(number(value)?),
            _ => return Err(format!("state `{state}` cannot set `{property}`")),
        }
        return Ok(());
    }
    match key {
        "width" => style.width = length(value)?,
        "height" => style.height = length(value)?,
        "padding" => style.padding = number(value)?,
        "gap" => style.gap = number(value)?,
        "grow" => style.grow = number(value)?,
        "radius" => style.radius = number(value)?,
        "background" => style.background = Some(color(value)?),
        "foreground" => style.foreground = Some(color(value)?),
        "font-size" => style.font_size = Some(number(value)?),
        "font-weight" => {
            let weight: u16 = value
                .parse()
                .map_err(|_| format!("invalid font weight `{value}`"))?;
            if !(100..=900).contains(&weight) {
                return Err(format!("font weight `{value}` is outside 100..=900"));
            }
            style.font_weight = Some(weight);
        }
        "font-family" => style.font_family = Some(match value {
            "sans" | "sans-serif" => FontFamily::SansSerif,
            "serif" => FontFamily::Serif,
            "monospace" | "mono" => FontFamily::Monospace,
            quoted => FontFamily::Named(
                quoted
                    .strip_prefix('"')
                    .and_then(|name| name.strip_suffix('"'))
                    .ok_or_else(|| {
                        format!(
                            "font family `{value}` must be sans, serif, monospace or a quoted name"
                        )
                    })?
                    .to_owned(),
            ),
        }),
        "italic" => {
            style.italic = Some(match value {
                "true" => true,
                "false" => false,
                _ => return Err(format!("italic must be true or false, got `{value}`")),
            })
        }
        "wrap" => {
            style.wrap = match value {
                "word" => TextWrap::Word,
                "none" => TextWrap::None,
                _ => return Err(format!("wrap must be word or none, got `{value}`")),
            }
        }
        _ => return Err(format!("unknown property `{key}`")),
    }
    Ok(())
}

fn number(value: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("invalid number `{value}`"))
}

fn length(value: &str) -> Result<Length, String> {
    if value == "auto" {
        return Ok(Length::Auto);
    }
    if let Some(percent) = value.strip_suffix('%') {
        return Ok(Length::Percent(number(percent)? / 100.0));
    }
    Ok(Length::Px(number(value)?))
}

fn color(value: &str) -> Result<Color, String> {
    let hex = value
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6 || hex.len() == 8)
        .ok_or_else(|| format!("color `{value}` must be #rrggbb or #rrggbbaa"))?;
    let channel = |index: usize| {
        u8::from_str_radix(&hex[index..index + 2], 16)
            .map_err(|_| format!("color `{value}` has invalid hex digits"))
    };
    Ok(Color(
        channel(0)?,
        channel(2)?,
        channel(4)?,
        if hex.len() == 8 { channel(6)? } else { 255 },
    ))
}
