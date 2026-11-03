//! The order a folder's tracks play in.

use super::*;

/// Reorder the audio among `proposed`, which is in name order, into the
/// order it plays, each file keeping to the folder of the release it is read
/// from: by the disc and track numbers its tags state, else the ones its name
/// starts with ([`TrackNumbers::or_file_name`]), a file stating none after
/// those that do, and files the numbers do not tell apart in name order. The
/// audio takes the places audio holds in name order, so every other file —
/// a track sheet, an image, a log — keeps the place its name gives it.
pub(super) fn lay_audio_in_play_order(
    proposed: &mut [(ScannedFile, ProposedRole)],
    tagged: &HashMap<String, TrackNumbers>,
    parts: &[ReleasePart],
) {
    let slots: Vec<usize> = proposed
        .iter()
        .enumerate()
        .filter(|(_, (_, role))| *role == ProposedRole::Audio)
        .map(|(index, _)| index)
        .collect();
    // A release read from several folders keeps each folder's audio together,
    // the folders in the order of its parts.
    let mut audio: Vec<(Option<usize>, TrackNumbers, (ScannedFile, ProposedRole))> = slots
        .iter()
        .map(|&slot| {
            let entry = proposed[slot].clone();
            let rank = part_of(parts, &entry.0.relative_path);
            let numbers = tagged
                .get(&entry.0.relative_path)
                .expect("every audio file's tags were read with its audio facts")
                .or_file_name(&entry.0.relative_path);
            (rank, numbers, entry)
        })
        .collect();
    // Stable, so files the numbers do not tell apart stay in name order.
    audio.sort_by(|(left_rank, left, _), (right_rank, right, _)| {
        left_rank.cmp(right_rank).then_with(|| left.play_order(right))
    });
    for (slot, (_, _, entry)) in slots.into_iter().zip(audio) {
        proposed[slot] = entry;
    }
}

#[cfg(test)]
#[path = "play_order_tests.rs"]
mod tests;
