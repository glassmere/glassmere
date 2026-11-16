use crate::{Markup, MarkupError, MarkupEvent, ShowOptions, Values};
use glassmere_core::{Label, Style, Ui};
use glassmere_scene::Color;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A markup file that is read again whenever it changes, through
/// [`Ui::watched_text`]: save the file and the running app shows it.
///
/// A file that no longer parses keeps showing its last good version, under
/// a line that says what is wrong.
#[derive(Debug)]
pub struct LiveMarkup {
    path: PathBuf,
    state: RefCell<State>,
}

#[derive(Debug, Default)]
struct State {
    /// The text the rest was read from.
    text: Option<Arc<str>>,
    markup: Option<Arc<Markup>>,
    error: Option<MarkupError>,
}

impl LiveMarkup {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            state: RefCell::default(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What is wrong with the file as it is now.
    pub fn error(&self) -> Option<MarkupError> {
        self.state.borrow().error.clone()
    }

    /// The last version that parsed. `None` until the host has delivered the
    /// file, which takes a frame, and while its first version does not parse.
    pub fn markup(&self) -> Option<Arc<Markup>> {
        self.state.borrow().markup.clone()
    }

    /// Reads the file if it changed, without showing it.
    #[track_caller]
    pub fn refresh<M: 'static>(&self, ui: &mut Ui<'_, M>) {
        let Some(text) = ui.watched_text(&self.path) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        if state
            .text
            .as_ref()
            .is_some_and(|known| Arc::ptr_eq(known, &text))
        {
            return;
        }
        match Markup::parse(&text) {
            Ok(markup) => {
                state.markup = Some(Arc::new(markup));
                state.error = None;
            }
            Err(error) => state.error = Some(error),
        }
        state.text = Some(text);
    }

    /// Declares the view, with images relative to the file's folder.
    #[track_caller]
    pub fn show<M: 'static>(
        &self,
        ui: &mut Ui<'_, M>,
        values: &Values,
        on: impl Fn(MarkupEvent) -> M + 'static,
    ) {
        let options = ShowOptions {
            base_dir: self.path.parent().map(Path::to_path_buf),
            ..ShowOptions::default()
        };
        self.show_with(ui, values, &options, on);
    }

    #[track_caller]
    pub fn show_with<M: 'static>(
        &self,
        ui: &mut Ui<'_, M>,
        values: &Values,
        options: &ShowOptions,
        on: impl Fn(MarkupEvent) -> M + 'static,
    ) {
        self.refresh(ui);
        if let Some(error) = self.error() {
            let name = self.path.file_name().unwrap_or_default().to_string_lossy();
            ui.label(
                Label::new(format!("{name}: {error}")).style(
                    Style::default()
                        .padding(8.0)
                        .background(Color::rgb(127, 29, 29))
                        .foreground(Color::WHITE),
                ),
            );
        }
        if let Some(markup) = self.markup() {
            markup.show_with(ui, values, options, on);
        }
    }
}
