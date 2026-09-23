//! Which of a release's mediums a candidate's audio is a rip of.
//!
//! A catalog describes a whole pressing: every disc of a box, both layers of
//! a hybrid SACD. A folder holds some of that — one disc, the CD layer, three
//! of six — and the tracklist it is read against has to be those mediums'
//! tracks alone. Coverage is chosen from the audio itself: the set of mediums
//! whose track counts add up to the folder's, closest in running time where
//! several do. It is a pure function of the documents and the measured
//! durations, so every projection of one applied source lands on the same
//! mediums.

/// The mediums a candidate holds, as ascending positions into the release's
/// medium list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediumCoverage {
    positions: Vec<usize>,
}

impl MediumCoverage {
    /// Every medium of a release with `medium_count` of them — what audio
    /// nothing has measured is read against.
    pub(crate) fn all(medium_count: usize) -> Self {
        Self {
            positions: (0..medium_count).collect(),
        }
    }

    pub(crate) fn covers(&self, position: usize) -> bool {
        self.positions.binary_search(&position).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn positions(&self) -> &[usize] {
        &self.positions
    }
}

/// Above this many mediums only runs of consecutive ones are tried, since
/// every subset would be too many to score.
const EXHAUSTIVE_MEDIUM_LIMIT: usize = 20;

/// The mediums whose tracks the audio is: `mediums` is each medium's stated
/// track lengths in track order, `audio_durations_ms` the folder's measured
/// lengths in row order.
///
/// An empty `audio_durations_ms` means nothing was measured and every medium
/// is covered. Otherwise the covered set is the one whose track counts sum to
/// the audio's; where several do, the one whose stated lengths lie closest to
/// the measured ones, an unstated length counting as agreement; still tied,
/// the earliest mediums.
///
/// No set summing to the audio's count also covers every medium. This answers
/// which mediums the audio is, and nothing more: a release whose tracks the
/// folder cannot hold is refused where it always was, when metadata is
/// applied to a draft, with both counts named.
pub(crate) fn choose(mediums: &[Vec<Option<u64>>], audio_durations_ms: &[u64]) -> MediumCoverage {
    if audio_durations_ms.is_empty() {
        return MediumCoverage::all(mediums.len());
    }
    let wanted = audio_durations_ms.len();
    let mut best: Option<(Vec<usize>, Score)> = None;
    let mut consider = |positions: &[usize]| {
        let score = score(mediums, positions, audio_durations_ms);
        if best.as_ref().is_none_or(|(_, held)| score < *held) {
            best = Some((positions.to_vec(), score));
        }
    };
    if mediums.len() <= EXHAUSTIVE_MEDIUM_LIMIT {
        let mut chosen = Vec::new();
        subsets_summing_to(mediums, wanted, 0, 0, &mut chosen, &mut consider);
    } else {
        for start in 0..mediums.len() {
            let mut count = 0;
            for (end, medium) in mediums.iter().enumerate().skip(start) {
                count += medium.len();
                if count == wanted {
                    consider(&(start..=end).collect::<Vec<_>>());
                }
                if count >= wanted {
                    break;
                }
            }
        }
    }
    match best {
        Some((positions, _)) => MediumCoverage { positions },
        None => MediumCoverage::all(mediums.len()),
    }
}

/// Every ascending set of medium positions whose track counts sum to
/// `wanted`, in the order that puts earlier mediums first.
fn subsets_summing_to(
    mediums: &[Vec<Option<u64>>],
    wanted: usize,
    from: usize,
    count: usize,
    chosen: &mut Vec<usize>,
    consider: &mut impl FnMut(&[usize]),
) {
    if count == wanted {
        if !chosen.is_empty() {
            consider(chosen);
        }
        return;
    }
    for position in from..mediums.len() {
        let next = count + mediums[position].len();
        if next > wanted {
            continue;
        }
        chosen.push(position);
        subsets_summing_to(mediums, wanted, position + 1, next, chosen, consider);
        chosen.pop();
    }
}

/// Lower is better: the summed distance between stated and measured lengths,
/// then how many lengths went unstated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Score {
    distance_ms: u64,
    unstated: usize,
}

fn score(mediums: &[Vec<Option<u64>>], positions: &[usize], audio_durations_ms: &[u64]) -> Score {
    let stated = positions
        .iter()
        .flat_map(|&position| mediums[position].iter().copied());
    let mut score = Score {
        distance_ms: 0,
        unstated: 0,
    };
    for (stated, &measured) in stated.zip(audio_durations_ms) {
        match stated {
            Some(stated) => score.distance_ms += stated.abs_diff(measured),
            None => score.unstated += 1,
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn medium(lengths: &[u64]) -> Vec<Option<u64>> {
        lengths.iter().map(|&length| Some(length)).collect()
    }

    /// A hybrid SACD lists its CD layer and its SACD layer as two mediums of
    /// the same six tracks; a rip is the CD layer, the first of them.
    #[test]
    fn a_hybrid_sacd_rip_covers_its_first_layer() {
        let layer = medium(&[527_000, 284_000, 333_000, 384_000, 275_000, 401_000]);
        let mediums = vec![layer.clone(), layer.clone()];
        let audio = [527_100, 284_200, 333_000, 384_050, 275_000, 401_300];
        assert_eq!(choose(&mediums, &audio).positions(), &[0]);
    }

    /// One disc of a box is the medium its lengths match, not the first
    /// medium with the same count.
    #[test]
    fn one_disc_of_a_box_covers_the_medium_its_lengths_match() {
        let mediums = vec![
            medium(&[300_000, 300_000]),
            medium(&[200_000, 400_000]),
            medium(&[250_000, 350_000]),
        ];
        assert_eq!(choose(&mediums, &[250_200, 349_900]).positions(), &[2]);
    }

    #[test]
    fn two_discs_of_a_box_cover_both() {
        let mediums = vec![
            medium(&[300_000]),
            medium(&[200_000, 400_000]),
            medium(&[250_000]),
        ];
        assert_eq!(choose(&mediums, &[300_000, 250_000]).positions(), &[0, 2]);
    }

    #[test]
    fn a_whole_release_covers_every_medium() {
        let mediums = vec![medium(&[300_000]), medium(&[200_000, 400_000])];
        assert_eq!(
            choose(&mediums, &[300_000, 200_000, 400_000]).positions(),
            &[0, 1]
        );
    }

    #[test]
    fn unmeasured_audio_covers_every_medium() {
        let mediums = vec![medium(&[300_000]), medium(&[200_000])];
        assert_eq!(choose(&mediums, &[]).positions(), &[0, 1]);
    }

    #[test]
    fn unstated_lengths_lose_to_stated_ones_that_agree() {
        let mediums = vec![vec![None, None], medium(&[200_000, 400_000])];
        assert_eq!(choose(&mediums, &[200_000, 400_000]).positions(), &[1]);
    }

    /// Audio no set of mediums holds is read as the whole release, which is
    /// where the track-count refusal has always been raised.
    #[test]
    fn audio_no_set_of_mediums_holds_covers_every_medium() {
        let mediums = vec![medium(&[300_000, 300_000]), medium(&[200_000, 400_000])];
        assert_eq!(choose(&mediums, &[100_000]).positions(), &[0, 1]);
    }

    /// Past the exhaustive limit only consecutive runs are tried, and a run
    /// is still found.
    #[test]
    fn a_large_box_covers_a_run_of_discs() {
        let mediums: Vec<_> = (0..30)
            .map(|disc| medium(&[100_000 + disc * 1_000, 200_000]))
            .collect();
        let audio = [110_000, 200_000, 111_000, 200_000];
        assert_eq!(choose(&mediums, &audio).positions(), &[10, 11]);
    }
}
