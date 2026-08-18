use egui::{Color32, Stroke};

#[derive(Clone, Copy, Debug)]
pub struct EditorTheme {
    pub sheet_background: Color32,
    pub panel_background: Color32,
    pub menu_bar_background: Color32,
    pub palette_background: Color32,
    pub fretboard_background: Color32,
    pub track_table_background: Color32,
    /// The area behind the "page" (the sheet itself is `sheet_background`).
    pub canvas_backdrop: Color32,
    pub notation_foreground: Color32,
    pub muted_foreground: Color32,
    pub staff_line_color: Color32,
    pub bar_line_color: Color32,
    pub accent_color: Color32,
    pub playhead_color: Color32,
    pub rest_color: Color32,
    pub note_fill: Color32,
    pub note_stroke: Color32,
}

impl EditorTheme {
    /// The original "sheet music on dark chrome" look — light/cream paper
    /// surrounded by dark toolbars and panels, similar to Guitar
    /// Pro/TuxGuitar. Used for `AppThemeMode::Bright`; also this app's
    /// original, and only, appearance before theme switching existed.
    pub fn bright() -> Self {
        Self {
            sheet_background: Color32::from_rgb(244, 243, 238),
            panel_background: Color32::from_rgb(31, 35, 39),
            menu_bar_background: Color32::from_rgb(238, 238, 238),
            palette_background: Color32::from_rgb(235, 235, 235),
            fretboard_background: Color32::from_rgb(10, 10, 10),
            track_table_background: Color32::from_rgb(239, 239, 239),
            canvas_backdrop: Color32::from_rgb(247, 247, 247),
            notation_foreground: Color32::from_rgb(30, 32, 34),
            muted_foreground: Color32::from_rgb(104, 111, 116),
            staff_line_color: Color32::from_rgb(170, 174, 171),
            bar_line_color: Color32::from_rgb(95, 102, 105),
            accent_color: Color32::from_rgb(0, 132, 124),
            playhead_color: Color32::from_rgb(210, 58, 48),
            rest_color: Color32::from_rgb(98, 101, 104),
            note_fill: Color32::from_rgb(255, 255, 252),
            note_stroke: Color32::from_rgb(33, 37, 41),
        }
    }

    /// A fully dark variant — the sheet itself is dark too, not just the
    /// surrounding chrome. Used for `AppThemeMode::Dark`, and as the
    /// fallback when `AppThemeMode::System` can't determine the OS
    /// preference.
    pub fn dark() -> Self {
        Self {
            sheet_background: Color32::from_rgb(35, 37, 40),
            panel_background: Color32::from_rgb(24, 26, 29),
            menu_bar_background: Color32::from_rgb(28, 30, 33),
            palette_background: Color32::from_rgb(30, 32, 35),
            fretboard_background: Color32::from_rgb(8, 8, 9),
            track_table_background: Color32::from_rgb(28, 30, 33),
            canvas_backdrop: Color32::from_rgb(20, 21, 23),
            notation_foreground: Color32::from_rgb(226, 227, 224),
            muted_foreground: Color32::from_rgb(150, 155, 158),
            staff_line_color: Color32::from_rgb(92, 96, 94),
            bar_line_color: Color32::from_rgb(140, 145, 148),
            accent_color: Color32::from_rgb(64, 196, 184),
            playhead_color: Color32::from_rgb(235, 100, 90),
            rest_color: Color32::from_rgb(158, 161, 164),
            note_fill: Color32::from_rgb(52, 55, 59),
            note_stroke: Color32::from_rgb(214, 216, 219),
        }
    }

    pub fn staff_stroke(self) -> Stroke {
        Stroke::new(1.0, self.staff_line_color)
    }

    pub fn bar_stroke(self) -> Stroke {
        Stroke::new(1.5, self.bar_line_color)
    }

    pub fn note_stroke(self) -> Stroke {
        Stroke::new(1.25, self.note_stroke)
    }
}

impl Default for EditorTheme {
    fn default() -> Self {
        Self::bright()
    }
}
