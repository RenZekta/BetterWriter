//! Standard note lengths: a note value with up to two augmentation dots, and
//! recognizing one from a length in ticks.

use super::project::{DurationChoice, TICKS_PER_QUARTER, Tick};

/// Most augmentation dots a length can carry.
pub const MAX_DOTS: u8 = 2;

/// Ticks of a note value (1 = whole, 2 = half, 4 = quarter, ...) with `dots`
/// augmentation dots, or `None` when that length isn't a whole number of
/// ticks. Each dot adds half of the previous addition.
pub fn dotted_ticks(value: u32, dots: u8) -> Option<Tick> {
    if value == 0 || dots > MAX_DOTS {
        return None;
    }
    let plain = TICKS_PER_QUARTER * 4 / value as Tick;
    let numerator = plain * ((1 << (dots + 1)) - 1);
    let denominator = 1 << dots;
    (numerator % denominator == 0).then_some(numerator / denominator)
}

/// Every standard length as `(ticks, note value, dots)`, longest first.
/// Note values run from whole to 128th.
pub fn standard_lengths() -> Vec<(Tick, u32, u8)> {
    let mut lengths = Vec::new();
    for value in [1_u32, 2, 4, 8, 16, 32, 64, 128] {
        for dots in 0..=MAX_DOTS {
            if let Some(ticks) = dotted_ticks(value, dots) {
                lengths.push((ticks, value, dots));
            }
        }
    }
    lengths.sort_by(|a, b| b.0.cmp(&a.0));
    lengths
}

/// The note value and dots whose length is exactly `ticks`.
pub fn detect_length(ticks: Tick) -> Option<(u32, u8)> {
    standard_lengths()
        .into_iter()
        .find(|(length, _, _)| *length == ticks)
        .map(|(_, value, dots)| (value, dots))
}

impl DurationChoice {
    /// The note value this choice stands for (1 = whole, 4 = quarter, ...).
    pub fn note_value(self) -> u32 {
        match self {
            DurationChoice::Whole => 1,
            DurationChoice::Half => 2,
            DurationChoice::Quarter => 4,
            DurationChoice::Eighth => 8,
            DurationChoice::Sixteenth => 16,
            DurationChoice::ThirtySecond => 32,
        }
    }

    /// Length with `dots` augmentation dots (clamped to `MAX_DOTS`).
    pub fn ticks_dotted(self, dots: u8) -> Tick {
        dotted_ticks(self.note_value(), dots.min(MAX_DOTS)).unwrap_or_else(|| self.ticks())
    }

    /// The choice and dots that make exactly `ticks`, if there are any.
    pub fn detect(ticks: Tick) -> Option<(DurationChoice, u8)> {
        let (value, dots) = detect_length(ticks)?;
        DurationChoice::ALL
            .into_iter()
            .find(|choice| choice.note_value() == value)
            .map(|choice| (choice, dots))
    }

    /// The longest choice-with-dots that fits in `room` ticks.
    pub fn longest_fitting(room: Tick) -> Option<(DurationChoice, u8)> {
        DurationChoice::ALL
            .into_iter()
            .flat_map(|choice| (0..=MAX_DOTS).map(move |dots| (choice, dots)))
            .filter(|(choice, dots)| choice.ticks_dotted(*dots) <= room)
            .max_by_key(|(choice, dots)| choice.ticks_dotted(*dots))
    }

    /// Label such as `1/4`, `1/4.` or `1/4..`.
    pub fn label_dotted(self, dots: u8) -> String {
        format!("{}{}", self.label(), ".".repeat(dots.min(MAX_DOTS) as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q: Tick = TICKS_PER_QUARTER;

    #[test]
    fn each_dot_adds_half_of_what_came_before() {
        assert_eq!(dotted_ticks(4, 0), Some(Q));
        assert_eq!(dotted_ticks(4, 1), Some(Q * 3 / 2));
        assert_eq!(dotted_ticks(4, 2), Some(Q * 7 / 4));
        assert_eq!(dotted_ticks(1, 1), Some(Q * 6));
        assert_eq!(dotted_ticks(1, 2), Some(Q * 7));
        assert_eq!(dotted_ticks(8, 2), Some(Q / 2 * 7 / 4));
    }

    #[test]
    fn lengths_that_are_not_whole_ticks_do_not_exist() {
        assert_eq!(dotted_ticks(128, 2), None);
        assert_eq!(dotted_ticks(4, 3), None);
        assert_eq!(dotted_ticks(0, 0), None);
    }

    #[test]
    fn standard_lengths_are_distinct_and_longest_first() {
        let lengths = standard_lengths();
        assert!(lengths.windows(2).all(|pair| pair[0].0 > pair[1].0));
        assert_eq!(lengths.first().map(|l| (l.1, l.2)), Some((1, 2)));
    }

    #[test]
    fn a_length_in_ticks_is_recognized_with_its_dots() {
        assert_eq!(detect_length(Q), Some((4, 0)));
        assert_eq!(detect_length(Q * 3 / 2), Some((4, 1)));
        assert_eq!(detect_length(Q * 7 / 4), Some((4, 2)));
        assert_eq!(detect_length(Q * 3), Some((2, 1)));
        assert_eq!(detect_length(Q * 6), Some((1, 1)));
        assert_eq!(detect_length(Q / 4 * 3 / 2), Some((16, 1)));
        assert_eq!(detect_length(Q + 1), None);
        assert_eq!(detect_length(0), None);
    }

    #[test]
    fn choices_round_trip_through_their_dotted_lengths() {
        for choice in DurationChoice::ALL {
            for dots in 0..=MAX_DOTS {
                let ticks = choice.ticks_dotted(dots);
                assert_eq!(DurationChoice::detect(ticks), Some((choice, dots)));
            }
        }
        assert_eq!(DurationChoice::Quarter.ticks_dotted(9), Q * 7 / 4);
        assert_eq!(DurationChoice::detect(Q / 32), None, "128th is not a choice");
    }

    #[test]
    fn the_longest_fitting_length_can_be_dotted() {
        assert_eq!(
            DurationChoice::longest_fitting(Q * 3 / 2),
            Some((DurationChoice::Quarter, 1))
        );
        assert_eq!(
            DurationChoice::longest_fitting(Q * 3 / 2 - 1),
            Some((DurationChoice::Quarter, 0))
        );
        assert_eq!(
            DurationChoice::longest_fitting(Q * 100),
            Some((DurationChoice::Whole, 2))
        );
        assert_eq!(DurationChoice::longest_fitting(Q / 8 - 1), None);
    }

    #[test]
    fn labels_show_one_character_per_dot() {
        assert_eq!(DurationChoice::Quarter.label_dotted(0), "1/4");
        assert_eq!(DurationChoice::Quarter.label_dotted(1), "1/4.");
        assert_eq!(DurationChoice::Eighth.label_dotted(2), "1/8..");
        assert_eq!(DurationChoice::Whole.label_dotted(1), "1.");
    }
}
