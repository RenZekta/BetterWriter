//! Turning keys played on a MIDI keyboard into tab positions.

/// A string and fret for each key, in the order given; `None` for a key no
/// string reaches. Keys are placed from the lowest up, each on the unused
/// string where it needs the lowest fret, so a chord falls into the usual
/// open shapes and no two notes share a string.
///
/// `tuning` lists the strings highest first, as a track does.
pub fn assign_strings(tuning: &[u8], fret_count: u8, keys: &[u8]) -> Vec<Option<(usize, u8)>> {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by_key(|&index| keys[index]);
    let mut used = vec![false; tuning.len()];
    let mut placed = vec![None; keys.len()];
    for index in order {
        let key = i32::from(keys[index]);
        let best = tuning
            .iter()
            .enumerate()
            .filter(|&(string, _)| !used[string])
            .filter_map(|(string, &open)| {
                let fret = key - i32::from(open);
                (0..=i32::from(fret_count))
                    .contains(&fret)
                    .then_some((string, fret as u8))
            })
            .min_by_key(|&(string, fret)| (fret, std::cmp::Reverse(string)));
        if let Some((string, fret)) = best {
            used[string] = true;
            placed[index] = Some((string, fret));
        }
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

    #[test]
    fn a_single_key_takes_the_lowest_fret() {
        assert_eq!(assign_strings(&STANDARD, 24, &[40]), vec![Some((5, 0))]);
        assert_eq!(assign_strings(&STANDARD, 24, &[60]), vec![Some((1, 1))]);
        assert_eq!(assign_strings(&STANDARD, 24, &[41]), vec![Some((5, 1))]);
    }

    #[test]
    fn an_open_chord_falls_into_its_shape() {
        // E2 B2 E3 G#3 B3 E4.
        let keys = [40, 47, 52, 56, 59, 64];
        assert_eq!(
            assign_strings(&STANDARD, 24, &keys),
            vec![
                Some((5, 0)),
                Some((4, 2)),
                Some((3, 2)),
                Some((2, 1)),
                Some((1, 0)),
                Some((0, 0)),
            ]
        );
    }

    #[test]
    fn the_result_follows_the_order_the_keys_came_in() {
        let placed = assign_strings(&STANDARD, 24, &[64, 40]);
        assert_eq!(placed, vec![Some((0, 0)), Some((5, 0))]);
    }

    #[test]
    fn strings_are_not_shared() {
        // Two E4s: the second has to move to the B string's 5th fret.
        let placed = assign_strings(&STANDARD, 24, &[64, 64]);
        assert_eq!(placed, vec![Some((0, 0)), Some((1, 5))]);
    }

    #[test]
    fn keys_out_of_reach_are_left_out() {
        assert_eq!(assign_strings(&STANDARD, 24, &[39, 89]), vec![None, None]);
        assert_eq!(assign_strings(&STANDARD, 12, &[64 + 13]), vec![None]);
    }
}
