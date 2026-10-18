//! Syntax highlighting for `Ui::code_editor` through syntect's bundled
//! Sublime Text grammars and themes.
use glassmere_core::{HighlightSpan, Highlighter, TextDocument};
use glassmere_scene::Color;
use std::collections::HashMap;
use std::sync::OnceLock;
use syntect::highlighting::{
    FontStyle, HighlightState, Highlighter as ThemeHighlighter, RangedHighlightIterator, ThemeSet,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

/// Parser state is kept at the start of every `CHECKPOINT`th line.
const CHECKPOINT: usize = 32;
/// Lines parsed per declaration while catching up to a far-away line.
const BUDGET: usize = 1500;
const SPAN_CACHE: usize = 4000;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn themes() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

type State = (ParseState, HighlightState);

/// Incremental highlighter. Parsing a line needs the state left by all lines
/// before it, so state checkpoints are kept and only lines after an edit are
/// parsed again. Jumping far ahead in a large file catches up over several
/// frames (`pending` is true meanwhile) instead of stalling one.
pub struct SyntectHighlighter {
    syntax: usize,
    theme: ThemeHighlighter<'static>,
    /// State at the start of line `index * CHECKPOINT`.
    checkpoints: Vec<State>,
    /// State at the start of `cursor.0`, continuing the last parsed line.
    cursor: Option<(usize, State)>,
    spans: HashMap<usize, Vec<HighlightSpan>>,
    revision: u64,
    /// Catch-up lines left in this declaration.
    budget: usize,
    pending: bool,
}

impl SyntectHighlighter {
    /// `language` is a file extension, a token or a syntax name such as `rs`,
    /// `json` or `Rust`; unknown languages highlight as plain text. Uses
    /// `base16-ocean.dark` or `InspiredGitHub`.
    pub fn new(language: &str, dark: bool) -> Self {
        let theme = if dark {
            "base16-ocean.dark"
        } else {
            "InspiredGitHub"
        };
        Self::with_theme(language, theme).expect("bundled themes exist")
    }

    /// `None` when the theme name is not one of `themes()`.
    pub fn with_theme(language: &str, theme: &str) -> Option<Self> {
        let theme = themes().themes.get(theme)?;
        let mut highlighter = Self {
            syntax: 0,
            theme: ThemeHighlighter::new(theme),
            checkpoints: Vec::new(),
            cursor: None,
            spans: HashMap::new(),
            revision: 0,
            budget: BUDGET,
            pending: false,
        };
        highlighter.set_language(language);
        Some(highlighter)
    }

    pub fn set_language(&mut self, language: &str) {
        let set = syntaxes();
        let syntax = set
            .find_syntax_by_token(language)
            .or_else(|| set.find_syntax_by_name(language))
            .unwrap_or_else(|| set.find_syntax_plain_text());
        self.syntax = set
            .syntaxes()
            .iter()
            .position(|candidate| candidate.name == syntax.name)
            .unwrap_or(0);
        self.reset(0);
    }

    pub fn language(&self) -> &'static str {
        &syntaxes().syntaxes()[self.syntax].name
    }

    pub fn languages() -> Vec<&'static str> {
        syntaxes()
            .syntaxes()
            .iter()
            .map(|syntax| syntax.name.as_str())
            .collect()
    }

    pub fn themes() -> Vec<&'static str> {
        themes().themes.keys().map(String::as_str).collect()
    }

    fn initial(&self) -> State {
        (
            ParseState::new(&syntaxes().syntaxes()[self.syntax]),
            HighlightState::new(&self.theme, ScopeStack::new()),
        )
    }

    /// Forgets everything from `line` on.
    fn reset(&mut self, line: usize) {
        self.checkpoints.truncate(line / CHECKPOINT + 1);
        if line == 0 {
            self.checkpoints.clear();
        }
        if self.cursor.as_ref().is_some_and(|cursor| cursor.0 > line) {
            self.cursor = None;
        }
        self.spans.retain(|cached, _| *cached < line);
    }

    /// Parses `line` from `state`, leaving `state` at the start of the next line.
    fn parse(&self, document: &TextDocument, line: usize, state: &mut State) -> Vec<HighlightSpan> {
        let mut text = document.line(line);
        let len = text.len();
        text.push('\n');
        let Ok(ops) = state.0.parse_line(&text, syntaxes()) else {
            return Vec::new();
        };
        let mut spans: Vec<HighlightSpan> = Vec::new();
        for (style, _, range) in
            RangedHighlightIterator::new(&mut state.1, &ops, &text, &self.theme)
        {
            let range = range.start.min(len)..range.end.min(len);
            if range.is_empty() {
                continue;
            }
            let color = Color(
                style.foreground.r,
                style.foreground.g,
                style.foreground.b,
                style.foreground.a,
            );
            let weight = style.font_style.contains(FontStyle::BOLD).then_some(700);
            let italic = style.font_style.contains(FontStyle::ITALIC);
            match spans.last_mut() {
                Some(last)
                    if last.range.end == range.start
                        && last.color == color
                        && last.weight == weight
                        && last.italic == italic =>
                {
                    last.range.end = range.end;
                }
                _ => spans.push(HighlightSpan {
                    range,
                    color,
                    weight,
                    italic,
                }),
            }
        }
        spans
    }
}

impl Highlighter for SyntectHighlighter {
    fn begin(&mut self, _document: &TextDocument) {
        self.budget = BUDGET;
        self.pending = false;
    }

    fn highlight(&mut self, document: &TextDocument, line: usize) -> Vec<HighlightSpan> {
        if let Some(first) = document.changed_since(self.revision) {
            self.reset(first);
        }
        self.revision = document.revision();
        if let Some(spans) = self.spans.get(&line) {
            return spans.clone();
        }
        // Continue from the last parsed line when it is the closest start.
        let checkpoint = (line / CHECKPOINT).min(self.checkpoints.len().saturating_sub(1));
        let (mut at, mut state) = match self.cursor.take() {
            Some((next, state)) if next <= line && next >= checkpoint * CHECKPOINT => (next, state),
            _ => match self.checkpoints.get(checkpoint) {
                Some(state) => (checkpoint * CHECKPOINT, state.clone()),
                None => (0, self.initial()),
            },
        };
        while at < line {
            if at % CHECKPOINT == 0 && at / CHECKPOINT == self.checkpoints.len() {
                self.checkpoints.push(state.clone());
            }
            if self.budget == 0 {
                // Resume here next frame; the line stays plain until then.
                self.cursor = Some((at, state));
                self.pending = true;
                return Vec::new();
            }
            self.parse(document, at, &mut state);
            at += 1;
            self.budget -= 1;
        }
        if at % CHECKPOINT == 0 && at / CHECKPOINT == self.checkpoints.len() {
            self.checkpoints.push(state.clone());
        }
        let spans = self.parse(document, line, &mut state);
        self.cursor = Some((line + 1, state));
        if self.spans.len() >= SPAN_CACHE {
            self.spans.clear();
        }
        self.spans.insert(line, spans.clone());
        spans
    }

    fn pending(&self) -> bool {
        self.pending
    }
}
