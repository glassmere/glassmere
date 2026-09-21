//! Styled runs for read-only rich text blocks.
use rust_gui_scene::Color;

/// One run of a rich text block. Font size and wrapping belong to the block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// Monospace on a subtle background.
    pub code: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// Makes the run a link; the target is reported when it is activated.
    pub link: Option<String>,
}

impl RichRun {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn code(mut self) -> Self {
        self.code = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    pub fn link(mut self, target: impl Into<String>) -> Self {
        self.link = Some(target.into());
        self
    }
}

/// A block of runs, built fluently:
/// `RichText::new().text("See ").link("the docs", "https://…").text(".")`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub runs: Vec<RichRun>,
}

impl RichText {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn run(mut self, run: RichRun) -> Self {
        if !run.text.is_empty() {
            self.runs.push(run);
        }
        self
    }
    pub fn text(self, text: impl Into<String>) -> Self {
        self.run(RichRun::new(text))
    }
    pub fn bold(self, text: impl Into<String>) -> Self {
        self.run(RichRun::new(text).bold())
    }
    pub fn italic(self, text: impl Into<String>) -> Self {
        self.run(RichRun::new(text).italic())
    }
    pub fn code(self, text: impl Into<String>) -> Self {
        self.run(RichRun::new(text).code())
    }
    pub fn link(self, text: impl Into<String>, target: impl Into<String>) -> Self {
        self.run(RichRun::new(text).link(target))
    }
    /// The text without any styling.
    pub fn plain(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}
