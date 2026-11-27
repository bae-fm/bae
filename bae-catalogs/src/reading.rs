//! Reading a catalog release against a candidate's audio, once.
//!
//! Identification's documents and tracklist fit, a person's pick, the draft a
//! pick or an unattended identification writes, the shared-album offer, and
//! the library's re-identify all lay a release onto audio through
//! [`SourceRelease::read`]: it chooses which of the release's mediums the
//! audio is a rip of and, for a Discogs tracklist, how its index and heading
//! rows lay out, together, and reads the chosen tracks into values. A
//! person's pick of a release that covers less than the audio reads through
//! [`SourceRelease::read_picked`], which also chooses the audio its tracks
//! are. Nothing else lays a tracklist out.

use crate::catalog::{Catalog, LookupCatalog};
use crate::medium_coverage::{candidate_sets, Fit, MediumCoverage, MediumShape};
use crate::release_values::ReleaseValues;
use crate::search::SourceTracks;
use crate::source_release::SourceRelease;
use crate::track_lengths::lengths_disagree;
use crate::AudioOrigin;
use crate::CatalogError;
use std::collections::BTreeSet;

/// What each of the audio's tracks measures, in play order. Never empty: a
/// release has a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredLengths(Vec<u64>);

impl MeasuredLengths {
    /// `lengths`, unless there are none.
    pub fn new(lengths: Vec<u64>) -> Option<Self> {
        (!lengths.is_empty()).then_some(Self(lengths))
    }

    pub(crate) fn as_slice(&self) -> &[u64] {
        &self.0
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    fn count(&self) -> u32 {
        u32::try_from(self.0.len()).expect("a release's track count fits u32")
    }
}

/// The audio a catalog release is read against: its tracks' lengths, where
/// its files say it came from, and the Disc IDs its rips hash to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadAudio {
    lengths: MeasuredLengths,
    origin: AudioOrigin,
    /// Every Disc ID the audio's rip logs and sheets hash to. Leaving the
    /// Disc IDs out of a run's lookups changes what the catalogs are asked,
    /// not what the folder's files are, so they are read here either way.
    disc_ids: Vec<String>,
}

impl ReadAudio {
    /// Audio of these lengths, from `origin`, whose rips hash to `disc_ids`.
    pub fn new(lengths: MeasuredLengths, origin: AudioOrigin, disc_ids: Vec<String>) -> Self {
        Self { lengths, origin, disc_ids }
    }

    /// How many tracks the audio holds.
    pub fn track_count(&self) -> u32 {
        self.lengths.count()
    }

    /// Audio of `lengths` whose files prove nothing and hash to no Disc ID.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn of_lengths(lengths: &[u64]) -> Self {
        Self {
            lengths: MeasuredLengths::new(lengths.to_vec()).expect("the test audio has tracks"),
            origin: AudioOrigin::default(),
            disc_ids: Vec::new(),
        }
    }
}

/// Why a release cannot be read onto the audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadingRefusal {
    /// No set of its mediums holds as many tracks as the audio, however its
    /// tracklist lays out. `release` is how many it lists in all.
    TrackCount { folder: u32, release: u32 },
    /// Several sets of its mediums fit the audio equally — by Disc ID,
    /// carrier and lengths — and read as different tracks. `mediums` is every
    /// medium of those sets, as the release numbers them, ascending.
    Tied { mediums: Vec<u32> },
    /// The tracks it lays onto the audio cannot be read into values: a track
    /// with no title, an album credited to no one.
    Unreadable { catalog: Catalog, detail: String },
}

/// A release a person picked, read onto the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct PickedReading {
    pub values: ReleaseValues,
    /// The audio's tracks the pick reads onto, as ascending positions among
    /// them in play order: `values`' tracks are these, in order. All of them
    /// for a release that holds the audio, and for one that lists no tracks.
    pub matched: Vec<usize>,
}

/// What a reading says the release lists for the audio, by count: as many
/// tracks as the audio when a set of its mediums fits it, tied with another
/// or not; as many as the release lists in all when none does; unreadable
/// when the tracks it lays onto the audio cannot be read; and nothing for a
/// release that lists no tracks.
pub fn listing(reading: &Result<ReleaseValues, ReadingRefusal>, audio: &ReadAudio) -> SourceTracks {
    match reading {
        Ok(values) if values.tracks.is_empty() => SourceTracks::Nothing,
        Ok(_) | Err(ReadingRefusal::Tied { .. }) => {
            SourceTracks::Listed { count: audio.lengths.count() }
        }
        Err(ReadingRefusal::TrackCount { release, .. }) => SourceTracks::Listed { count: *release },
        Err(ReadingRefusal::Unreadable { .. }) => SourceTracks::Unreadable,
    }
}

impl SourceRelease {
    /// What this release says the audio's tracks are: the mediums the audio
    /// is a rip of, laid onto it, with every credit, role and work it states.
    /// A release that lists no tracks says nothing about them, and reads as
    /// its album and pressing alone.
    ///
    /// The covered mediums are a set whose tracks, laid out against the
    /// audio, number exactly as many as its tracks. The folder's Disc IDs
    /// decide between such sets first, then the carriers the audio's files
    /// rule out, then the lengths (see `Fit`). Sets tied on all of those
    /// that read the same values — the two layers of a hybrid SACD — are one
    /// answer; sets that read differently are refused, naming their mediums,
    /// and never settled by which comes first.
    pub fn read(&self, audio: &ReadAudio) -> Result<ReleaseValues, ReadingRefusal> {
        let shapes = self.medium_shapes(audio);
        if shapes.iter().all(|shape| shape.counts.is_empty()) {
            return self.values_over(&MediumCoverage::none(), audio).map_err(unreadable);
        }
        let measured = audio.lengths.as_slice();
        let mut best: Vec<MediumCoverage> = Vec::new();
        let mut best_fit: Option<Fit> = None;
        for set in candidate_sets(&shapes, audio.lengths.len()) {
            let Some(stated) = self.laid_out_lengths(&set, measured) else {
                continue;
            };
            let fit = Fit::of(&shapes, &set, &audio.origin, &stated, measured);
            match best_fit {
                Some(held) if fit > held => {}
                Some(held) if fit == held => best.push(set),
                _ => {
                    best_fit = Some(fit);
                    best = vec![set];
                }
            }
        }
        let mut readings = best.iter().map(|set| self.values_over(set, audio).map_err(unreadable));
        let Some(first) = readings.next() else {
            return Err(ReadingRefusal::TrackCount {
                folder: audio.lengths.count(),
                release: u32::try_from(self.listed_track_count())
                    .expect("a release's track count fits u32"),
            });
        };
        let mut readings = readings.peekable();
        if readings.peek().is_none() || readings.all(|other| other == first) {
            return first;
        }
        Err(tied(best.iter()))
    }

    /// What a person's pick of this release says the audio's tracks are. A
    /// release whose mediums hold the audio's tracks reads as [`Self::read`]
    /// does. One that covers less is read onto part of the audio: of the sets
    /// of its mediums that list fewer tracks than the audio holds, those
    /// listing the most are laid, in play order, each onto the audio whose
    /// lengths it matches best, and `Fit` decides between them. Refused as
    /// [`Self::read`] refuses when every set lists more tracks than the audio
    /// holds.
    pub fn read_picked(&self, audio: &ReadAudio) -> Result<PickedReading, ReadingRefusal> {
        match self.read(audio) {
            Ok(values) => Ok(PickedReading { values, matched: (0..audio.lengths.len()).collect() }),
            Err(refusal @ ReadingRefusal::TrackCount { .. }) => {
                self.read_covering_less(audio).unwrap_or(Err(refusal))
            }
            Err(refusal) => Err(refusal),
        }
    }

    /// The reading of the mediums that cover the most of the audio while
    /// listing fewer tracks than it holds; `None` when no set of mediums does.
    fn read_covering_less(
        &self,
        audio: &ReadAudio,
    ) -> Option<Result<PickedReading, ReadingRefusal>> {
        let shapes = self.medium_shapes(audio);
        let measured = audio.lengths.as_slice();
        for count in (1..measured.len()).rev() {
            let mut best: Vec<(MediumCoverage, Vec<usize>)> = Vec::new();
            let mut best_fit: Option<Fit> = None;
            for set in candidate_sets(&shapes, count) {
                let Some(stated) = self.laid_out_lengths(&set, &measured[..count]) else {
                    continue;
                };
                let matched = matching_audio(&stated, measured);
                let chosen: Vec<u64> = matched.iter().map(|&track| measured[track]).collect();
                // A Discogs layout of this many tracks is chosen again for
                // the audio they matched.
                let Some(stated) = self.laid_out_lengths(&set, &chosen) else {
                    continue;
                };
                let fit = Fit::of(&shapes, &set, &audio.origin, &stated, &chosen);
                match best_fit {
                    Some(held) if fit > held => {}
                    Some(held) if fit == held => best.push((set, matched)),
                    _ => {
                        best_fit = Some(fit);
                        best = vec![(set, matched)];
                    }
                }
            }
            let mut readings = best.iter().map(|(set, matched)| {
                let part = ReadAudio {
                    lengths: MeasuredLengths(
                        matched.iter().map(|&track| measured[track]).collect(),
                    ),
                    origin: audio.origin.clone(),
                    disc_ids: audio.disc_ids.clone(),
                };
                self.values_over(set, &part)
                    .map(|values| PickedReading { values, matched: matched.clone() })
                    .map_err(unreadable)
            });
            let Some(first) = readings.next() else {
                continue;
            };
            let mut readings = readings.peekable();
            if readings.peek().is_none() || readings.all(|other| other == first) {
                return Some(first);
            }
            return Some(Err(tied(best.iter().map(|(set, _)| set))));
        }
        None
    }

    /// Each medium as the choice weighs it.
    fn medium_shapes(&self, audio: &ReadAudio) -> Vec<MediumShape> {
        self.mediums
            .iter()
            .map(|medium| MediumShape {
                counts: match self.release.catalog {
                    LookupCatalog::MusicBrainz if medium.entries.is_empty() => BTreeSet::new(),
                    LookupCatalog::MusicBrainz => BTreeSet::from([medium.entries.len()]),
                    // A layout that gives no tracks covers nothing.
                    LookupCatalog::Discogs => crate::discogs_mapper::layout_counts(&medium.entries)
                        .into_iter()
                        .filter(|&count| count > 0)
                        .collect(),
                },
                carrier: medium.medium,
                holds_folder_disc: medium
                    .disc_ids
                    .iter()
                    .any(|disc_id| audio.disc_ids.contains(disc_id)),
            })
            .collect()
    }

    /// The lengths the covered mediums state for each of the audio's tracks,
    /// once laid out against `measured`, or `None` when no layout of them
    /// gives exactly as many tracks.
    fn laid_out_lengths(
        &self,
        coverage: &MediumCoverage,
        measured: &[u64],
    ) -> Option<Vec<Option<u64>>> {
        let stated: Vec<Option<u64>> = match self.release.catalog {
            LookupCatalog::MusicBrainz => self
                .covered_mediums(coverage)
                .flat_map(|medium| &medium.entries)
                .map(|entry| entry.duration_ms)
                .collect(),
            LookupCatalog::Discogs => crate::discogs_mapper::process_tracklist(
                &self.covered_entries(coverage),
                Some(measured),
            )
            .iter()
            .map(|track| track.duration_ms)
            .collect(),
        };
        (stated.len() == measured.len()).then_some(stated)
    }

    /// The covered mediums' tracks, as their catalog's reader reads them.
    fn values_over(
        &self,
        coverage: &MediumCoverage,
        audio: &ReadAudio,
    ) -> Result<ReleaseValues, CatalogError> {
        match self.release.catalog {
            LookupCatalog::MusicBrainz => crate::musicbrainz_mapper::values(self, coverage),
            LookupCatalog::Discogs => {
                crate::discogs_mapper::values(self, coverage, Some(audio.lengths.as_slice()))
            }
        }
    }

    /// How many tracks the release lists in all, every index and heading
    /// read out to its sub-tracks.
    fn listed_track_count(&self) -> usize {
        self.mediums
            .iter()
            .map(|medium| match self.release.catalog {
                LookupCatalog::MusicBrainz => medium.entries.len(),
                LookupCatalog::Discogs => *crate::discogs_mapper::layout_counts(&medium.entries)
                    .last()
                    .expect("every tracklist has a layout, if only of no tracks"),
            })
            .sum()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl SourceRelease {
    /// Every medium's tracks, each index and heading read out to its
    /// sub-tracks: what a reader's test reads with no audio to lay out on.
    pub fn every_medium_values(&self) -> Result<ReleaseValues, CatalogError> {
        let every = MediumCoverage::all(self.mediums.len());
        match self.release.catalog {
            LookupCatalog::MusicBrainz => crate::musicbrainz_mapper::values(self, &every),
            LookupCatalog::Discogs => crate::discogs_mapper::values(self, &every, None),
        }
    }
}

/// The refusal of sets of mediums that fit alike and read differently,
/// naming every medium of `sets`.
fn tied<'a>(sets: impl Iterator<Item = &'a MediumCoverage>) -> ReadingRefusal {
    ReadingRefusal::Tied {
        mediums: sets
            .flat_map(MediumCoverage::medium_numbers)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

/// Which of the `measured` audio's tracks each of the tracks stating
/// `stated` lengths is, where there are fewer of those: ascending positions,
/// in order, that leave the fewest lengths disagreeing and then the smallest
/// summed distance, taking the earlier audio where positions fit alike.
fn matching_audio(stated: &[Option<u64>], measured: &[u64]) -> Vec<usize> {
    assert!(stated.len() <= measured.len(), "no more tracks than the audio holds");
    // (disagreeing, summed distance) of one track on one audio track. A track
    // stating no length fits every one alike.
    let cost = |track: usize, audio: usize| match stated[track] {
        Some(length) => (
            usize::from(lengths_disagree(length, measured[audio])),
            length.abs_diff(measured[audio]),
        ),
        None => (0, 0),
    };
    let add = |a: (usize, u64), b: (usize, u64)| (a.0 + b.0, a.1 + b.1);
    // `best[track][audio]`: the least cost of laying the tracks from `track`
    // on onto the audio from `audio` on, `None` where too little is left.
    let mut best = vec![vec![None; measured.len() + 1]; stated.len() + 1];
    best[stated.len()].fill(Some((0, 0)));
    for track in (0..stated.len()).rev() {
        for audio in (0..measured.len()).rev() {
            let taken = best[track + 1][audio + 1].map(|rest| add(cost(track, audio), rest));
            let skipped = best[track][audio + 1];
            best[track][audio] = taken.into_iter().chain(skipped).min();
        }
    }
    let mut audio = 0;
    (0..stated.len())
        .map(|track| {
            while best[track + 1][audio + 1].map(|rest| add(cost(track, audio), rest))
                != best[track][audio]
            {
                audio += 1;
            }
            audio += 1;
            audio - 1
        })
        .collect()
}

/// A catalog reader's failure: always its release's own data.
fn unreadable(error: CatalogError) -> ReadingRefusal {
    match error {
        CatalogError::SourceData { catalog, detail } => {
            ReadingRefusal::Unreadable { catalog, detail }
        }
        other => unreachable!("a catalog reader fails only on its release's own data: {other}"),
    }
}

#[cfg(test)]
#[path = "reading_tests.rs"]
mod tests;
