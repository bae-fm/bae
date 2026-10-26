//! Reading a catalog release against a candidate's audio, once.
//!
//! Identification's documents and tracklist fit, a person's pick, the draft a
//! pick or an unattended identification writes, the shared-album offer, and
//! the library's re-identify all lay a release onto audio through
//! [`SourceRelease::read`]: it chooses which of the release's mediums the
//! audio is a rip of and, for a Discogs tracklist, how its index and heading
//! rows lay out, together, and reads the chosen tracks into values. Nothing
//! else lays a tracklist out.

use crate::import::medium_coverage::{candidate_sets, Fit, MediumCoverage, MediumShape};
use crate::import::release_values::ReleaseValues;
use crate::import::search::SourceTracks;
use crate::import::source_release::{not_fetched, SourceRelease};
use crate::import::ImportError;
use crate::catalog::Catalog;
use crate::signals::AudioOrigin;
use std::collections::BTreeSet;

/// What each of the audio's tracks measures, in play order. Never empty: a
/// release has a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeasuredLengths(Vec<u64>);

impl MeasuredLengths {
    /// `lengths`, unless there are none.
    pub(crate) fn new(lengths: Vec<u64>) -> Option<Self> {
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
    pub(crate) lengths: MeasuredLengths,
    pub(crate) origin: AudioOrigin,
    /// Every Disc ID the audio's rip logs and sheets hash to. Leaving the
    /// Disc IDs out of a run's lookups changes what the catalogs are asked,
    /// not what the folder's files are, so they are read here either way.
    pub(crate) disc_ids: Vec<String>,
}

impl ReadAudio {
    /// Audio of these lengths, whose rip logs and sheets read as `rip`.
    pub(crate) fn of_rip(lengths: MeasuredLengths, rip: crate::import::discid::RipReading) -> Self {
        Self {
            lengths,
            origin: rip.origin,
            disc_ids: rip
                .disc_id
                .computed()
                .into_iter()
                .map(|disc| disc.disc_id)
                .collect(),
        }
    }

    /// Audio of `lengths` whose files prove nothing and hash to no Disc ID.
    #[cfg(test)]
    pub(crate) fn of_lengths(lengths: &[u64]) -> Self {
        Self {
            lengths: MeasuredLengths::new(lengths.to_vec()).expect("the test audio has tracks"),
            origin: AudioOrigin::default(),
            disc_ids: Vec::new(),
        }
    }

    /// A candidate's audio, as its scan stored each file and as its rip logs
    /// and track sheets read now.
    pub(crate) fn of_files(
        files: &crate::import::folder_scanner::CategorizedFiles,
    ) -> Result<Self, ImportError> {
        let durations = crate::import::probe::source_durations(files)?;
        let lengths = crate::import::audio_layout::audio_durations(files, &durations)?;
        let lengths = MeasuredLengths::new(lengths).ok_or_else(|| ImportError::Internal {
            detail: "a candidate with no audio has nothing to read a release against".to_string(),
        })?;
        Ok(Self::of_rip(
            lengths,
            crate::import::discid::read_rip_artifacts(files),
        ))
    }
}

/// Why a release cannot be read onto the audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadingRefusal {
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

impl From<ReadingRefusal> for ImportError {
    fn from(refusal: ReadingRefusal) -> Self {
        match refusal {
            ReadingRefusal::TrackCount { folder, release } => Self::MetadataTrackCount {
                folder_tracks: folder,
                release_tracks: release,
            },
            ReadingRefusal::Tied { mediums } => Self::TiedMediums { mediums },
            ReadingRefusal::Unreadable { catalog, detail } => Self::SourceData { catalog, detail },
        }
    }
}

/// What a reading says the release lists for the audio, by count: as many
/// tracks as the audio when a set of its mediums fits it, tied with another
/// or not; as many as the release lists in all when none does; unreadable
/// when the tracks it lays onto the audio cannot be read; and nothing for a
/// release that lists no tracks.
pub(crate) fn listing(
    reading: &Result<ReleaseValues, ReadingRefusal>,
    audio: &ReadAudio,
) -> SourceTracks {
    match reading {
        Ok(values) if values.tracks.is_empty() => SourceTracks::Nothing,
        Ok(_) | Err(ReadingRefusal::Tied { .. }) => SourceTracks::Listed {
            count: audio.lengths.count(),
        },
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
    /// rule out, then the lengths (see [`Fit`]). Sets tied on all of those
    /// that read the same values — the two layers of a hybrid SACD — are one
    /// answer; sets that read differently are refused, naming their mediums,
    /// and never settled by which comes first.
    pub(crate) fn read(&self, audio: &ReadAudio) -> Result<ReleaseValues, ReadingRefusal> {
        let shapes = self.medium_shapes(audio);
        if shapes.iter().all(|shape| shape.counts.is_empty()) {
            return self
                .values_over(&MediumCoverage::none(), audio)
                .map_err(unreadable);
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
        let mut readings = best
            .iter()
            .map(|set| self.values_over(set, audio).map_err(unreadable));
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
        Err(ReadingRefusal::Tied {
            mediums: best
                .iter()
                .flat_map(MediumCoverage::medium_numbers)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
    }

    /// Each medium as the choice weighs it.
    fn medium_shapes(&self, audio: &ReadAudio) -> Vec<MediumShape> {
        self.mediums
            .iter()
            .map(|medium| MediumShape {
                counts: match self.release.catalog {
                    Catalog::MusicBrainz if medium.entries.is_empty() => BTreeSet::new(),
                    Catalog::MusicBrainz => BTreeSet::from([medium.entries.len()]),
                    // A layout that gives no tracks covers nothing.
                    Catalog::Discogs => crate::import::discogs_mapper::layout_counts(&medium.entries)
                        .into_iter()
                        .filter(|&count| count > 0)
                        .collect(),
                    other => not_fetched(other),
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
            Catalog::MusicBrainz => self
                .covered_mediums(coverage)
                .flat_map(|medium| &medium.entries)
                .map(|entry| entry.duration_ms)
                .collect(),
            Catalog::Discogs => crate::import::discogs_mapper::process_tracklist(
                &self.covered_entries(coverage),
                Some(measured),
            )
            .iter()
            .map(|track| track.duration_ms)
            .collect(),
            other => not_fetched(other),
        };
        (stated.len() == measured.len()).then_some(stated)
    }

    /// The covered mediums' tracks, as their catalog's reader reads them.
    fn values_over(
        &self,
        coverage: &MediumCoverage,
        audio: &ReadAudio,
    ) -> Result<ReleaseValues, ImportError> {
        match self.release.catalog {
            Catalog::MusicBrainz => crate::import::musicbrainz_mapper::values(self, coverage),
            Catalog::Discogs => crate::import::discogs_mapper::values(
                self,
                coverage,
                Some(audio.lengths.as_slice()),
            ),
            other => not_fetched(other),
        }
    }

    /// How many tracks the release lists in all, every index and heading
    /// read out to its sub-tracks.
    fn listed_track_count(&self) -> usize {
        self.mediums
            .iter()
            .map(|medium| match self.release.catalog {
                Catalog::MusicBrainz => medium.entries.len(),
                Catalog::Discogs => *crate::import::discogs_mapper::layout_counts(&medium.entries)
                    .last()
                    .expect("every tracklist has a layout, if only of no tracks"),
                other => not_fetched(other),
            })
            .sum()
    }
}

#[cfg(test)]
impl SourceRelease {
    /// Every medium's tracks, each index and heading read out to its
    /// sub-tracks: what a reader's test reads with no audio to lay out on.
    pub(crate) fn every_medium_values(&self) -> Result<ReleaseValues, ImportError> {
        let every = MediumCoverage::all(self.mediums.len());
        match self.release.catalog {
            Catalog::MusicBrainz => crate::import::musicbrainz_mapper::values(self, &every),
            Catalog::Discogs => crate::import::discogs_mapper::values(self, &every, None),
            other => not_fetched(other),
        }
    }
}

/// A catalog reader's failure: always its release's own data.
fn unreadable(error: ImportError) -> ReadingRefusal {
    match error {
        ImportError::SourceData { catalog, detail } => ReadingRefusal::Unreadable { catalog, detail },
        other => unreachable!("a catalog reader fails only on its release's own data: {other}"),
    }
}

#[cfg(test)]
#[path = "catalog_reading_tests.rs"]
mod tests;
