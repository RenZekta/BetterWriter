//! Vector glyphs for the edition palette. Every icon is drawn with the egui
//! painter on a 24x24 design grid and scaled to the button, so icons follow
//! the theme colors and stay sharp at any size.

use std::f32::consts::PI;

use eframe::egui::{self, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteIcon {
    TimeSignature,
    KeySignature,
    Clef,
    TripletFeel,
    RepeatOpen,
    RepeatClose,
    AddBarAfter,
    InsertBarBefore,
    DeleteBar,
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    Dotted,
    DoubleDotted,
    Tuplet,
    Tie,
    Fermata,
    Dynamic(&'static str),
    Crescendo,
    Decrescendo,
    Ghost,
    Dead,
    Accent,
    HeavyAccent,
    Staccato,
    LetRing,
    PalmMute,
    Vibrato,
    Slide,
    HammerOn,
    Tapping,
    Slap,
    Pop,
    FadeIn,
    Bend,
    Harmonic,
    Trill,
    Grace,
    TremoloBar,
    Text,
    Chord,
}

/// Paints `icon` centered in `rect` using `color` for every stroke and fill.
pub fn paint(painter: &Painter, rect: Rect, icon: PaletteIcon, color: Color32) {
    let scale = rect.width().min(rect.height()) / 24.0;
    let center = rect.center();
    let ink = Ink {
        painter,
        center,
        scale,
        color,
    };
    match icon {
        PaletteIcon::TimeSignature => {
            ink.text(0.0, -5.5, "4", 12.0, false);
            ink.text(0.0, 5.5, "4", 12.0, false);
        }
        PaletteIcon::KeySignature => {
            ink.text(-3.5, 0.0, "b", 14.0, false);
            ink.text(4.5, 0.0, "#", 14.0, false);
        }
        PaletteIcon::Clef => ink.text(0.0, 0.0, "G", 17.0, true),
        PaletteIcon::TripletFeel => {
            ink.note(-5.0, Value::Eighth, 0.8);
            ink.note(4.0, Value::Eighth, 0.8);
            ink.text(0.0, -8.0, "3", 8.0, true);
        }
        PaletteIcon::RepeatOpen => ink.repeat_sign(true),
        PaletteIcon::RepeatClose => ink.repeat_sign(false),
        PaletteIcon::AddBarAfter => {
            ink.bar_outline();
            ink.plus(7.5, 7.0, 3.0);
        }
        PaletteIcon::InsertBarBefore => {
            ink.bar_outline();
            ink.plus(-7.5, 7.0, 3.0);
        }
        PaletteIcon::DeleteBar => {
            ink.bar_outline();
            ink.cross(0.0, 0.5, 3.2);
        }
        PaletteIcon::Whole => ink.note(0.0, Value::Whole, 1.0),
        PaletteIcon::Half => ink.note(0.0, Value::Half, 1.0),
        PaletteIcon::Quarter => ink.note(0.0, Value::Quarter, 1.0),
        PaletteIcon::Eighth => ink.note(0.0, Value::Eighth, 1.0),
        PaletteIcon::Sixteenth => ink.note(0.0, Value::Sixteenth, 1.0),
        PaletteIcon::ThirtySecond => ink.note(0.0, Value::ThirtySecond, 1.0),
        PaletteIcon::Dotted => ink.dot(0.0, 2.0, 2.0),
        PaletteIcon::DoubleDotted => {
            ink.dot(-3.5, 2.0, 2.0);
            ink.dot(3.5, 2.0, 2.0);
        }
        PaletteIcon::Tuplet => {
            ink.line(&[(-8.0, -1.0), (-8.0, -5.0), (-3.0, -5.0)]);
            ink.line(&[(3.0, -5.0), (8.0, -5.0), (8.0, -1.0)]);
            ink.text(0.0, -5.0, "3", 10.0, true);
            ink.head(-4.0, 6.0, true, 0.8);
            ink.head(4.0, 6.0, true, 0.8);
        }
        PaletteIcon::Tie => {
            ink.curve((-8.0, -1.0), (-4.0, 6.0), (4.0, 6.0), (8.0, -1.0));
            ink.dot(-8.0, -3.5, 1.6);
            ink.dot(8.0, -3.5, 1.6);
        }
        PaletteIcon::Fermata => {
            ink.curve((-8.0, 3.0), (-8.0, -9.0), (8.0, -9.0), (8.0, 3.0));
            ink.dot(0.0, 0.0, 1.8);
        }
        PaletteIcon::Dynamic(label) => {
            let size = if label.len() > 2 { 10.0 } else { 12.0 };
            ink.text(0.0, 0.5, label, size, true);
        }
        PaletteIcon::Crescendo | PaletteIcon::FadeIn => {
            ink.line(&[(8.0, -4.5), (-8.0, 0.0), (8.0, 4.5)]);
        }
        PaletteIcon::Decrescendo => {
            ink.line(&[(-8.0, -4.5), (8.0, 0.0), (-8.0, 4.5)]);
        }
        PaletteIcon::Ghost => {
            ink.head(0.0, 0.0, true, 0.9);
            ink.text(-8.0, 0.0, "(", 15.0, false);
            ink.text(8.0, 0.0, ")", 15.0, false);
        }
        PaletteIcon::Dead => ink.cross(0.0, 0.0, 5.0),
        PaletteIcon::Accent => ink.line(&[(-5.0, -5.0), (5.0, 0.0), (-5.0, 5.0)]),
        PaletteIcon::HeavyAccent => ink.line(&[(-5.0, 5.5), (0.0, -6.0), (5.0, 5.5)]),
        PaletteIcon::Staccato => {
            ink.dot(0.0, -6.0, 1.7);
            ink.head(0.0, 4.0, true, 1.0);
        }
        PaletteIcon::LetRing => ink.text(0.0, 0.0, "L.R.", 9.0, false),
        PaletteIcon::PalmMute => ink.text(0.0, 0.0, "P.M.", 9.0, false),
        PaletteIcon::Vibrato => {
            let points: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    (-9.0 + 18.0 * t, (t * 3.0 * 2.0 * PI).sin() * 3.0)
                })
                .collect();
            ink.line(&points);
        }
        PaletteIcon::Slide => {
            ink.line(&[(-7.0, 5.0), (7.0, -5.0)]);
            ink.dot(-7.0, 5.0, 1.5);
            ink.dot(7.0, -5.0, 1.5);
        }
        PaletteIcon::HammerOn => {
            ink.curve((-7.0, 1.0), (-4.0, -8.0), (4.0, -8.0), (7.0, 1.0));
            ink.text(0.0, 6.0, "H", 9.0, false);
        }
        PaletteIcon::Tapping => ink.text(0.0, 0.0, "T", 15.0, false),
        PaletteIcon::Slap => ink.text(0.0, 0.0, "S", 15.0, false),
        PaletteIcon::Pop => ink.text(0.0, 0.0, "P", 15.0, false),
        PaletteIcon::Bend => {
            ink.curve((-6.0, 6.0), (2.0, 6.0), (6.0, 2.0), (6.0, -5.0));
            ink.line(&[(3.0, -3.0), (6.0, -7.0), (9.0, -3.0)]);
        }
        PaletteIcon::Harmonic => {
            let diamond = [(0.0, -6.0), (5.0, 0.0), (0.0, 6.0), (-5.0, 0.0)];
            ink.polygon(&diamond, false);
        }
        PaletteIcon::Trill => ink.text(0.0, 0.0, "tr", 14.0, true),
        PaletteIcon::Grace => {
            ink.note(0.0, Value::Eighth, 0.75);
            ink.line(&[(-1.0, -2.5), (6.0, -7.5)]);
        }
        PaletteIcon::TremoloBar => {
            ink.line(&[(-8.0, -3.0), (-3.0, -3.0), (0.0, 4.0), (3.0, -3.0), (8.0, -3.0)]);
        }
        PaletteIcon::Text => ink.text(0.0, 0.0, "T", 15.0, true),
        PaletteIcon::Chord => ink.chord_grid(),
    }
}

#[derive(Clone, Copy)]
enum Value {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl Value {
    fn flags(self) -> usize {
        match self {
            Value::Eighth => 1,
            Value::Sixteenth => 2,
            Value::ThirtySecond => 3,
            _ => 0,
        }
    }
}

/// Drawing context for one icon: converts design-grid offsets (relative to
/// the icon's center) into canvas positions and issues the shapes.
struct Ink<'a> {
    painter: &'a Painter,
    center: Pos2,
    scale: f32,
    color: Color32,
}

impl Ink<'_> {
    fn pos(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(self.center.x + x * self.scale, self.center.y + y * self.scale)
    }

    fn stroke(&self) -> Stroke {
        Stroke::new((1.4 * self.scale).max(1.0), self.color)
    }

    fn line(&self, points: &[(f32, f32)]) {
        let points = points.iter().map(|&(x, y)| self.pos(x, y)).collect();
        self.painter.add(Shape::line(points, self.stroke()));
    }

    fn polygon(&self, points: &[(f32, f32)], filled: bool) {
        let points: Vec<Pos2> = points.iter().map(|&(x, y)| self.pos(x, y)).collect();
        if filled {
            self.painter
                .add(Shape::convex_polygon(points, self.color, Stroke::NONE));
        } else {
            self.painter.add(Shape::closed_line(points, self.stroke()));
        }
    }

    fn curve(&self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)) {
        let steps = 14;
        let points: Vec<(f32, f32)> = (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let u = 1.0 - t;
                let mix = |a: f32, b: f32, c: f32, d: f32| {
                    u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
                };
                (mix(p0.0, p1.0, p2.0, p3.0), mix(p0.1, p1.1, p2.1, p3.1))
            })
            .collect();
        self.line(&points);
    }

    fn dot(&self, x: f32, y: f32, radius: f32) {
        self.painter
            .circle_filled(self.pos(x, y), radius * self.scale, self.color);
    }

    fn cross(&self, x: f32, y: f32, half: f32) {
        self.line(&[(x - half, y - half), (x + half, y + half)]);
        self.line(&[(x - half, y + half), (x + half, y - half)]);
    }

    fn plus(&self, x: f32, y: f32, half: f32) {
        self.line(&[(x - half, y), (x + half, y)]);
        self.line(&[(x, y - half), (x, y + half)]);
    }

    fn text(&self, x: f32, y: f32, text: &str, size: f32, italic: bool) {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: FontId::proportional(size * self.scale),
                color: self.color,
                italics: italic,
                ..Default::default()
            },
        );
        let galley = self.painter.layout_job(job);
        let top_left = self.pos(x, y) - galley.size() * 0.5;
        self.painter.galley(top_left, galley, self.color);
    }

    /// A slightly tilted oval, the shape of an engraved notehead.
    fn head(&self, x: f32, y: f32, filled: bool, size: f32) {
        let (rx, ry) = (4.3 * size, 3.0 * size);
        let tilt = -0.35_f32;
        let (sin, cos) = tilt.sin_cos();
        let points: Vec<(f32, f32)> = (0..28)
            .map(|i| {
                let t = i as f32 / 28.0 * 2.0 * PI;
                let (ex, ey) = (rx * t.cos(), ry * t.sin());
                (x + ex * cos - ey * sin, y + ex * sin + ey * cos)
            })
            .collect();
        self.polygon(&points, filled);
    }

    fn note(&self, x: f32, value: Value, size: f32) {
        let head_y = 6.0 * size;
        match value {
            Value::Whole => {
                self.head(x, 0.0, false, 1.3 * size);
                return;
            }
            Value::Half => self.head(x, head_y, false, size),
            _ => self.head(x, head_y, true, size),
        }
        let stem_x = x + 4.0 * size;
        let stem_top = head_y - 15.0 * size;
        self.line(&[(stem_x, head_y - 0.5), (stem_x, stem_top)]);
        for flag in 0..value.flags() {
            let y = stem_top + flag as f32 * 3.2 * size;
            self.curve(
                (stem_x, y),
                (stem_x + 0.5, y + 3.5 * size),
                (stem_x + 6.0 * size, y + 4.0 * size),
                (stem_x + 4.0 * size, y + 9.0 * size),
            );
        }
    }

    fn bar_outline(&self) {
        self.line(&[(-8.0, -6.0), (-8.0, 6.0)]);
        self.line(&[(8.0, -6.0), (8.0, 6.0)]);
        for y in [-3.0, 0.0, 3.0] {
            self.line(&[(-8.0, y), (8.0, y)]);
        }
    }

    fn repeat_sign(&self, open: bool) {
        let side = if open { -1.0 } else { 1.0 };
        let thick = self.pos(side * 3.5, 0.0);
        let width = 2.4 * self.scale;
        self.painter.rect_filled(
            Rect::from_center_size(thick, Vec2::new(width, 16.0 * self.scale)),
            0.0,
            self.color,
        );
        self.line(&[(side * 7.0, -8.0), (side * 7.0, 8.0)]);
        let dots_x = -side * 1.5;
        self.dot(dots_x, -2.5, 1.3);
        self.dot(dots_x, 2.5, 1.3);
    }

    fn chord_grid(&self) {
        for i in 0..4 {
            let x = -6.0 + i as f32 * 4.0;
            self.line(&[(x, -7.0), (x, 7.0)]);
        }
        for i in 0..4 {
            let y = -7.0 + i as f32 * 4.67;
            self.line(&[(-6.0, y), (6.0, y)]);
        }
        self.dot(-2.0, -2.5, 1.6);
        self.dot(2.0, 2.0, 1.6);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_counts_follow_note_value() {
        assert_eq!(Value::Whole.flags(), 0);
        assert_eq!(Value::Quarter.flags(), 0);
        assert_eq!(Value::Eighth.flags(), 1);
        assert_eq!(Value::Sixteenth.flags(), 2);
        assert_eq!(Value::ThirtySecond.flags(), 3);
    }
}
