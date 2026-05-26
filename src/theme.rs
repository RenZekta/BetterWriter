use egui::{Color32, Stroke};

#[derive(Clone, Copy, Debug)]
pub struct EditorTheme {
    pub sheet_background: Color32,
    pub panel_background: Color32,
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

impl Default for EditorTheme {
    fn default() -> Self {
        Self {
            sheet_background: Color32::from_rgb(244, 243, 238),
            panel_background: Color32::from_rgb(31, 35, 39),
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
}

impl EditorTheme {
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
