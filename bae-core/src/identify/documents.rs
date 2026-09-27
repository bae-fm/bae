//! The full documents of the rows a run offers, read before it settles.
//!
//! A search result states less than its release's document: a Discogs result
//! names only its first label, no search result lists a tracklist, and a
//! result may leave out the barcode its document states. Every
//! record of every offered row is fetched in full and stored where a pick
//! reads it (see [`crate::import::service::prepare_release`]), and what the
//! document states replaces what the result stated before the rows are ranked
//! for the last time. A document that cannot be read leaves its record as the
//! result stated it, with why it could not be read; the run settles on what
//! it did read.

use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::MetadataRef;
use crate::pressing::ReleaseLabel;
use crate::signals::LookupFailure;

/// Where a run is with its offered rows' documents.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentReading {
    /// Waiting for the lookups and the album links to settle.
    Pending,
    /// The offered records are being fetched.
    Reading,
    /// Fetched, record by record; empty when nothing was offered.
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
    /// The tracks it lists for the audio being identified.
    pub source_tracks: SourceTracks,
}

impl ReleaseDocument {
    /// What `release` states, its tracklist read against `track_lengths_ms`.
    pub(crate) fn of(
        release: &crate::import::source_release::SourceRelease,
        track_lengths_ms: &[u64],
    ) -> Self {
        Self {
            labels: release.pressing().labels.clone(),
            barcode: release.pressing().barcode.clone(),
            source_tracks: release.source_tracks_for_audio(track_lengths_ms),
        }
    }
}

impl DocumentReading {
    /// The documents read so far, empty until they are.
    fn read(&self) -> &[ReleaseReading] {
        match self {
            Self::Read(read) => read,
            Self::Pending | Self::Reading => &[],
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
