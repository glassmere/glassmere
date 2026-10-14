use super::*;
use crate::RichText;

impl Ui<'_> {
    /// A read-only block of styled runs that wraps as one paragraph. Returns
    /// the target of a link once when it is clicked or activated with Enter;
    /// arrow keys move between the links of a focused block.
    pub fn rich_text(&mut self, text: &RichText, style: Style) -> Option<String> {
        let key = self.next_key();
        let followed = match self.take_update(&key, Kind::RichText) {
            Some(ControlUpdate::Choice(index)) => text
                .runs
                .iter()
                .filter_map(|run| run.link.clone())
                .nth(index),
            _ => None,
        };
        self.push(Element {
            key,
            kind: Kind::RichText,
            text: text.plain(),
            style,
            enabled: self.enabled,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control: Control::RichText {
                runs: text.runs.clone(),
                links: text.runs.iter().filter(|run| run.link.is_some()).count(),
            },
        });
        followed
    }
}
