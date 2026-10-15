use super::*;
use crate::RichText;

impl<'a, M: 'static> Ui<'a, M> {
    /// A read-only block of styled runs that wraps as one paragraph. Returns
    /// the target of a link once when it is clicked or activated with Enter;
    /// arrow keys move between the links of a focused block.
    #[track_caller]
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
            handler: None,
            seq: 0,
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

    /// [`Ui::rich_text`] that sends the target of a followed link as
    /// `on_link(url)`, in the batch the link was followed in.
    #[track_caller]
    pub fn rich_text_msg(
        &mut self,
        text: &RichText,
        style: Style,
        on_link: impl Fn(String) -> M + 'static,
    ) -> Response<'a> {
        let links: Vec<String> = text
            .runs
            .iter()
            .filter_map(|run| run.link.clone())
            .collect();
        let key = self.next_key();
        let response = self.push(Element {
            handler: None,
            seq: 0,
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
        self.set_handler(crate::app::Handler::Map(Box::new(
            move |update| match update {
                ControlUpdate::Choice(index) => links.get(index).cloned().map(&on_link),
                _ => None,
            },
        )));
        response
    }
}
