use glassmere_scene::{Color, FontFamily};

/// Design tokens every built-in control paints with. Applications read them
/// through `Ui::theme` for their own surfaces and switch them at runtime with
/// `Context::set_theme`; a switch invalidates paint, and layout only when fonts differ.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub window: Color,
    pub surface: Color,
    pub popup: Color,
    pub control: Color,
    pub control_hover: Color,
    pub control_pressed: Color,
    pub control_disabled: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_disabled: Color,
    pub accent: Color,
    pub focus: Color,
    /// Selected rows and highlighted popup choices.
    pub selection: Color,
    /// Text selection in the focused editor.
    pub highlight: Color,
    /// Text selection in an unfocused editor.
    pub selection_inactive: Color,
    pub row: Color,
    pub row_alternate: Color,
    pub track: Color,
    pub thumb: Color,
    pub scrollbar: Color,
    /// Translucent cover behind modal dialogs.
    pub scrim: Color,
    pub font_family: FontFamily,
    pub font_size: f32,
}

/// Token names accepted by stylesheets and `Theme::set_token`.
pub const TOKENS: [&str; 21] = [
    "window",
    "surface",
    "popup",
    "control",
    "control-hover",
    "control-pressed",
    "control-disabled",
    "text",
    "text-muted",
    "text-disabled",
    "accent",
    "focus",
    "selection",
    "highlight",
    "selection-inactive",
    "row",
    "row-alternate",
    "track",
    "thumb",
    "scrollbar",
    "scrim",
];

impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "dark".into(),
            window: Color::rgb(15, 23, 34),
            surface: Color::rgb(24, 35, 49),
            popup: Color::rgb(38, 51, 67),
            control: Color::rgb(45, 66, 84),
            control_hover: Color::rgb(56, 94, 110),
            control_pressed: Color::rgb(28, 115, 112),
            control_disabled: Color::rgb(36, 43, 54),
            text: Color::rgb(226, 232, 240),
            text_muted: Color::rgb(140, 154, 171),
            text_disabled: Color::rgb(114, 128, 145),
            accent: Color::rgb(103, 232, 206),
            focus: Color::rgb(103, 232, 206),
            selection: Color::rgb(22, 105, 99),
            highlight: Color::rgb(35, 107, 150),
            selection_inactive: Color::rgb(65, 78, 94),
            row: Color::rgb(25, 36, 49),
            row_alternate: Color::rgb(20, 30, 42),
            track: Color::rgb(95, 111, 132),
            thumb: Color::WHITE,
            scrollbar: Color::rgb(100, 116, 139),
            scrim: Color(2, 6, 23, 150),
            font_family: FontFamily::SansSerif,
            font_size: 16.0,
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light".into(),
            window: Color::rgb(246, 247, 249),
            surface: Color::rgb(255, 255, 255),
            popup: Color::rgb(255, 255, 255),
            control: Color::rgb(226, 232, 240),
            control_hover: Color::rgb(203, 213, 225),
            control_pressed: Color::rgb(148, 163, 184),
            control_disabled: Color::rgb(241, 245, 249),
            text: Color::rgb(15, 23, 42),
            text_muted: Color::rgb(100, 116, 139),
            text_disabled: Color::rgb(148, 163, 184),
            accent: Color::rgb(13, 148, 136),
            focus: Color::rgb(13, 148, 136),
            selection: Color::rgb(204, 251, 241),
            highlight: Color::rgb(191, 219, 254),
            selection_inactive: Color::rgb(226, 232, 240),
            row: Color::rgb(255, 255, 255),
            row_alternate: Color::rgb(241, 245, 249),
            track: Color::rgb(203, 213, 225),
            thumb: Color::rgb(13, 148, 136),
            scrollbar: Color::rgb(148, 163, 184),
            scrim: Color(15, 23, 42, 90),
            font_family: FontFamily::SansSerif,
            font_size: 16.0,
        }
    }

    fn slot(&mut self, name: &str) -> Option<&mut Color> {
        Some(match name {
            "window" => &mut self.window,
            "surface" => &mut self.surface,
            "popup" => &mut self.popup,
            "control" => &mut self.control,
            "control-hover" => &mut self.control_hover,
            "control-pressed" => &mut self.control_pressed,
            "control-disabled" => &mut self.control_disabled,
            "text" => &mut self.text,
            "text-muted" => &mut self.text_muted,
            "text-disabled" => &mut self.text_disabled,
            "accent" => &mut self.accent,
            "focus" => &mut self.focus,
            "selection" => &mut self.selection,
            "highlight" => &mut self.highlight,
            "selection-inactive" => &mut self.selection_inactive,
            "row" => &mut self.row,
            "row-alternate" => &mut self.row_alternate,
            "track" => &mut self.track,
            "thumb" => &mut self.thumb,
            "scrollbar" => &mut self.scrollbar,
            "scrim" => &mut self.scrim,
            _ => return None,
        })
    }

    pub fn token(&self, name: &str) -> Option<Color> {
        Some(match name {
            "window" => self.window,
            "surface" => self.surface,
            "popup" => self.popup,
            "control" => self.control,
            "control-hover" => self.control_hover,
            "control-pressed" => self.control_pressed,
            "control-disabled" => self.control_disabled,
            "text" => self.text,
            "text-muted" => self.text_muted,
            "text-disabled" => self.text_disabled,
            "accent" => self.accent,
            "focus" => self.focus,
            "selection" => self.selection,
            "highlight" => self.highlight,
            "selection-inactive" => self.selection_inactive,
            "row" => self.row,
            "row-alternate" => self.row_alternate,
            "track" => self.track,
            "thumb" => self.thumb,
            "scrollbar" => self.scrollbar,
            "scrim" => self.scrim,
            _ => return None,
        })
    }

    /// Returns false for an unknown token name.
    pub fn set_token(&mut self, name: &str, color: Color) -> bool {
        match self.slot(name) {
            Some(slot) => {
                *slot = color;
                true
            }
            None => false,
        }
    }
}
