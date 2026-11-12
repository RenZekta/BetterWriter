//! Fretboard geometry: where fret wires sit, and which string and fret a
//! point on the panel belongs to. Painting and click handling both go through
//! `FretboardFrame`, so what is drawn is exactly what can be clicked.

use eframe::egui::{Pos2, Rect};

const LABEL_WIDTH: f32 = 34.0;
const OPEN_ZONE_WIDTH: f32 = 30.0;
const RIGHT_PAD: f32 = 14.0;
const TOP_PAD: f32 = 10.0;
const NUMBER_STRIP: f32 = 16.0;
const STRING_EDGE: f32 = 7.0;

/// Panel height that keeps strings comfortably clickable for `string_count`.
pub fn panel_height(string_count: usize) -> f32 {
    (TOP_PAD + NUMBER_STRIP + 2.0 * STRING_EDGE + 15.0 * string_count as f32).clamp(112.0, 200.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inlay {
    None,
    Single,
    Double,
}

pub fn inlay_at(fret: u8) -> Inlay {
    match fret % 12 {
        0 if fret > 0 => Inlay::Double,
        3 | 5 | 7 | 9 => Inlay::Single,
        _ => Inlay::None,
    }
}

/// Fret wire positions along the neck as fractions of its length, using the
/// equal-tempered rule so cells shrink toward the body like a real guitar.
#[derive(Clone, Debug)]
pub struct FretLayout {
    wires: Vec<f32>,
}

impl FretLayout {
    pub fn new(fret_count: u8) -> Self {
        let count = fret_count.max(1);
        let distance = |fret: u8| 1.0 - 2.0_f32.powf(-(fret as f32) / 12.0);
        let last = distance(count);
        let wires = (0..=count).map(|fret| distance(fret) / last).collect();
        Self { wires }
    }

    /// Position of the wire at the end of `fret` (wire 0 is the nut).
    pub fn wire(&self, fret: u8) -> f32 {
        self.wires[(fret as usize).min(self.wires.len() - 1)]
    }

    /// The fret whose cell holds neck position `t`; the nut itself and
    /// anything past the last wire belong to no fret.
    pub fn fret_at(&self, t: f32) -> Option<u8> {
        if t <= 0.0 || t > 1.0 {
            return None;
        }
        Some(self.wires.partition_point(|&wire| wire < t) as u8)
    }
}

/// Pixel geometry of the whole fretboard panel.
#[derive(Clone, Debug)]
pub struct FretboardFrame {
    /// The fretted part of the neck, from the nut to the last wire.
    pub board: Rect,
    /// The area left of the nut that stands for the open strings.
    pub open_zone: Rect,
    pub label_right: f32,
    pub layout: FretLayout,
    string_count: usize,
    first_string_y: f32,
    string_gap: f32,
}

impl FretboardFrame {
    pub fn new(panel: Rect, string_count: usize, fret_count: u8) -> Self {
        let string_count = string_count.max(1);
        let board = Rect::from_min_max(
            Pos2::new(
                panel.left() + LABEL_WIDTH + OPEN_ZONE_WIDTH,
                panel.top() + TOP_PAD,
            ),
            Pos2::new(panel.right() - RIGHT_PAD, panel.bottom() - NUMBER_STRIP),
        );
        let open_zone = Rect::from_min_max(
            Pos2::new(panel.left() + LABEL_WIDTH, board.top()),
            Pos2::new(board.left(), board.bottom()),
        );
        let span = board.height() - 2.0 * STRING_EDGE;
        let (first_string_y, string_gap) = if string_count > 1 {
            (board.top() + STRING_EDGE, span / (string_count - 1) as f32)
        } else {
            (board.center().y, span)
        };
        Self {
            board,
            open_zone,
            label_right: panel.left() + LABEL_WIDTH - 6.0,
            layout: FretLayout::new(fret_count),
            string_count,
            first_string_y,
            string_gap,
        }
    }

    pub fn string_gap(&self) -> f32 {
        self.string_gap
    }

    pub fn string_y(&self, string: usize) -> f32 {
        self.first_string_y + string as f32 * self.string_gap
    }

    pub fn wire_x(&self, fret: u8) -> f32 {
        self.board.left() + self.layout.wire(fret) * self.board.width()
    }

    /// Horizontal extent of the clickable cell for `fret`; fret 0 is the open
    /// zone left of the nut.
    pub fn cell_x_range(&self, fret: u8) -> (f32, f32) {
        if fret == 0 {
            (self.open_zone.left(), self.open_zone.right())
        } else {
            (self.wire_x(fret - 1), self.wire_x(fret))
        }
    }

    pub fn cell_center_x(&self, fret: u8) -> f32 {
        let (left, right) = self.cell_x_range(fret);
        (left + right) * 0.5
    }

    pub fn cell_rect(&self, string: usize, fret: u8) -> Rect {
        let (left, right) = self.cell_x_range(fret);
        let y = self.string_y(string);
        let half = self.string_gap * 0.5;
        Rect::from_min_max(Pos2::new(left, y - half), Pos2::new(right, y + half))
    }

    pub fn cell_center(&self, string: usize, fret: u8) -> Pos2 {
        Pos2::new(self.cell_center_x(fret), self.string_y(string))
    }

    /// The `(string, fret)` under `pos`, or `None` outside the playable area.
    pub fn hit(&self, pos: Pos2) -> Option<(usize, u8)> {
        if pos.y < self.board.top() || pos.y > self.board.bottom() {
            return None;
        }
        let string = ((pos.y - self.first_string_y) / self.string_gap)
            .round()
            .clamp(0.0, (self.string_count - 1) as f32) as usize;
        if pos.x >= self.open_zone.left() && pos.x <= self.open_zone.right() {
            return Some((string, 0));
        }
        let t = (pos.x - self.board.left()) / self.board.width();
        self.layout.fret_at(t).map(|fret| (string, fret))
    }

    /// Size used for note markers so they never overlap neighbouring strings.
    pub fn marker_radius(&self) -> f32 {
        (self.string_gap * 0.42).clamp(4.0, 9.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;

    fn frame(strings: usize) -> FretboardFrame {
        FretboardFrame::new(
            Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1000.0, 140.0)),
            strings,
            24,
        )
    }

    #[test]
    fn wires_span_the_neck_and_cells_shrink() {
        let layout = FretLayout::new(24);
        assert_eq!(layout.wire(0), 0.0);
        assert!((layout.wire(24) - 1.0).abs() < 1e-6);
        let first = layout.wire(1) - layout.wire(0);
        let last = layout.wire(24) - layout.wire(23);
        assert!(first > last * 3.0, "first cell {first}, last cell {last}");
        assert!((1..=24).all(|fret| layout.wire(fret) > layout.wire(fret - 1)));
    }

    #[test]
    fn fret_at_uses_the_cell_ending_at_each_wire() {
        let layout = FretLayout::new(12);
        assert_eq!(layout.fret_at(0.0), None);
        assert_eq!(layout.fret_at(layout.wire(1) * 0.5), Some(1));
        assert_eq!(layout.fret_at(layout.wire(1)), Some(1));
        assert_eq!(layout.fret_at(layout.wire(1) + 1e-4), Some(2));
        assert_eq!(layout.fret_at(1.0), Some(12));
        assert_eq!(layout.fret_at(1.01), None);
    }

    #[test]
    fn inlays_follow_the_usual_markers() {
        assert_eq!(inlay_at(0), Inlay::None);
        assert_eq!(inlay_at(5), Inlay::Single);
        assert_eq!(inlay_at(12), Inlay::Double);
        assert_eq!(inlay_at(24), Inlay::Double);
        assert_eq!(inlay_at(10), Inlay::None);
    }

    #[test]
    fn open_zone_is_fret_zero_and_nut_side_cells_follow() {
        let frame = frame(6);
        let y = frame.string_y(2);
        let open = frame.open_zone.center().x;
        assert_eq!(frame.hit(Pos2::new(open, y)), Some((2, 0)));
        let just_past_nut = frame.board.left() + 3.0;
        assert_eq!(frame.hit(Pos2::new(just_past_nut, y)), Some((2, 1)));
    }

    #[test]
    fn clicks_pick_the_nearest_string() {
        let frame = frame(6);
        let x = frame.cell_center_x(5);
        for string in 0..6 {
            assert_eq!(frame.hit(Pos2::new(x, frame.string_y(string))), Some((string, 5)));
        }
        let between = (frame.string_y(1) + frame.string_y(2)) * 0.5 + 1.0;
        assert_eq!(frame.hit(Pos2::new(x, between)), Some((2, 5)));
    }

    #[test]
    fn clicks_outside_the_playable_area_miss() {
        let frame = frame(6);
        let y = frame.string_y(0);
        assert_eq!(frame.hit(Pos2::new(5.0, y)), None, "string label column");
        assert_eq!(frame.hit(Pos2::new(1000.0, y)), None, "right margin");
        assert_eq!(
            frame.hit(Pos2::new(frame.cell_center_x(3), frame.board.bottom() + 8.0)),
            None,
            "fret number strip"
        );
    }

    #[test]
    fn marker_cells_sit_inside_their_hit_area() {
        let frame = frame(6);
        for fret in [0_u8, 1, 12, 24] {
            let centre = frame.cell_center(3, fret);
            assert_eq!(frame.hit(centre), Some((3, fret)));
        }
    }

    #[test]
    fn single_string_instruments_do_not_divide_by_zero() {
        let frame = frame(1);
        assert!(frame.string_y(0).is_finite());
        assert!(frame.hit(Pos2::new(frame.cell_center_x(2), frame.string_y(0))).is_some());
    }

    #[test]
    fn panel_height_grows_with_strings_within_limits() {
        assert_eq!(panel_height(4), 112.0);
        assert!(panel_height(6) > panel_height(4) - 0.1);
        assert_eq!(panel_height(40), 200.0);
    }
}
