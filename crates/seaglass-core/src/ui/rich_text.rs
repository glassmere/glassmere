use super::*;
use crate::RichText;

impl<'a, M: 'static> Ui<'a, M> {
    /// A read-only block of styled runs that wraps as one paragraph. A link
    /// clicked or activated with Enter sends its target as `on_link(url)`;
    /// arrow keys move between the links of a focused block.
    #[track_caller]
    pub fn rich_text(
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
            text: text.plain(),
            style,
            enabled: self.enabled,
            control: Control::RichText {
                runs: text.runs.clone(),
                links: text.runs.iter().filter(|run| run.link.is_some()).count(),
            },
            ..Element::new(key, Kind::RichText)
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
