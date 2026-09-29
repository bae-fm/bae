//! The full documents of the rows a run offers, read before it settles.
//!
//! A search result states less than its release's document: a Discogs result
//! names only its first label, no search result lists a tracklist, and a
//! result may leave out the barcode its document states. Every record of
//! every offered row is fetched in full and stored where a pick reads it (see
//! `crate::import::service::prepare_release`), and what the document states
//! replaces what the result stated before the rows are ranked again. A row
//! the documents raise to the top is read in turn, so the run settles once
//! every offered row's records are read — or once more rows are offered than
//! `MOST_ROWS_READ`, which it leaves for the person to pick among. A document that cannot be read
//! leaves its record as the result stated it, with why it could not be read;
//! the run settles on what it did read.

use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::MetadataRef;
use crate::pressing::ReleaseLabel;
use crate::signals::LookupFailure;

/// The most rows tied at the top a run reads the full documents of. Each row
/// read is at least one request to a catalog that answers about one a second,
/// and past this many rows the folder has not told the pressings apart — the
/// documents rarely leave one standing — so the person picks among them, which
/// reads only the row picked. A run with more rows tied reads none, and no
/// fact only a document states ranks its rows.
pub(crate) const MOST_ROWS_READ: usize = 5;

/// Where a run is with its offered rows' documents.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentReading {
    /// Waiting for the lookups and the album links to settle.
    Pending,
    /// Offered records not read yet are being fetched; these were read
    /// before.
    Reading(Vec<ReleaseReading>),
    /// Fetched, record by record: every record of every row offered as they
    /// rank with them, unless more rows are offered than `MOST_ROWS_READ`.
    /// Empty when nothing was offered, or too much was.
    Read(Vec<ReleaseReading>),
}

/// One record's document, or why it could not be fetched.
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseReading {
    pub release: MetadataRef,
    pub document: Result<ReleaseDocument, LookupFailure>,
}

/// What a release's document states that ranking reads beyond its result.
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseDocument {
    /// Every label the release is on, with its catalog number.
    pub labels: Vec<ReleaseLabel>,
    /// The barcode the document states, as it prints it.
    pub barcode: Option<String>,
    /// The year the release's album first came out, as its group or master
    /// states it.
    pub album_first_year: Option<i32>,
    /// The tracks it lists for the audio being identified.
    pub source_tracks: SourceTracks,
    /// Their titles, in order; empty where a track has none.
    pub track_titles: Vec<String>,
    /// What it writes about which pressing it is, in free text.
    pub notes: Vec<String>,
}

impl ReleaseDocument {
    /// What `release` states, its tracklist read against `track_lengths_ms`.
    pub(crate) fn of(
        release: &crate::import::source_release::SourceRelease,
        track_lengths_ms: &[u64],
    ) -> Self {
        let titles = release.track_titles_for_audio(track_lengths_ms);
        Self {
            labels: release.pressing().labels.clone(),
            barcode: release.pressing().barcode.clone(),
            album_first_year: release.metadata.album.first_year,
            source_tracks: SourceTracks::of_titles(&titles),
            track_titles: titles
                .into_iter()
                .collect::<Option<_>>()
                .unwrap_or_default(),
            notes: release.notes.clone(),
        }
    }
}

impl DocumentReading {
    /// The documents read so far, empty until they are.
    pub(crate) fn read(&self) -> &[ReleaseReading] {
        match self {
            Self::Reading(read) | Self::Read(read) => read,
            Self::Pending => &[],
        }
    }

    /// `result` with what its document states in place of what the result
    /// stated, or with why its document could not be read. The document
    /// keeps one barcode where a Discogs result lists every code the sleeve
    /// carries, so its barcode joins the result's rather than replacing them.
    pub(crate) fn apply(&self, result: &mut MetadataResult) {
        let reading = self.read().iter().find(|reading| {
            reading.release.catalog == result.source && reading.release.key == result.release_id
        });
        match reading.map(|reading| &reading.document) {
            Some(Ok(document)) => {
                result.labels = document.labels.clone();
                result.source_tracks = Some(document.source_tracks.clone());
                result.album_first_year = document.album_first_year;
                result.track_titles = document.track_titles.clone();
                result.notes = document.notes.clone();
                if let Some(barcode) = &document.barcode {
                    let key = crate::barcode::comparison_key(barcode).ok();
                    let stated = result.barcodes.iter().any(|stated| {
                        stated == barcode
                            || (key.is_some() && crate::barcode::comparison_key(stated).ok() == key)
                    });
                    if !stated {
                        result.barcodes.push(barcode.clone());
                    }
                }
            }
            Some(Err(failure)) => result.document_failure = Some(failure.clone()),
            None => {}
        }
    }
}
