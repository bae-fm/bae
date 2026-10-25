//! The full documents of the rows a run offers, read before it settles.
//!
//! A search result states less than its release's document: a Discogs result
//! names only its first label, no lookup's answer lists the tracklist read
//! against the folder's audio, and a result may leave out the barcode its
//! document states. Every record of every offered row is read and stored
//! where a pick reads it (see `crate::import::service::prepare_release`), as
//! far as what is read here needs: the release's own page, its album's page,
//! and for a MusicBrainz release the Wikidata items and Discogs releases its
//! album statements read. A release a Disc ID lookup returned is read from
//! that answer, which lists everything here but the annotation: its notes are
//! its disambiguation alone, and its own page counts among what the read
//! stopped short of. A pick reads the rest, its own page included, before it
//! applies anything. What the document states replaces what the result stated
//! before the rows are ranked again. A row whose tracklist then holds other
//! tracks than the folder is no longer a match (see
//! `identify::fit::rules_out`), and the rows ranked below it move up; a row
//! the documents raise to the top is read in turn. The run settles once every
//! offered row's records are read, however many rows that is, or once no row
//! is left. A document that cannot be read leaves its record as the result
//! stated it, with why it could not be read; the run settles on what it did
//! read.
//!
//! A MusicBrainz document also names the other catalogs' releases that are
//! the same release, and states what its album is on Discogs (see
//! [`crate::import::album_links`]). The release a document names is how the
//! run looks an offered row up on the catalog it has no record of — see
//! `state::pressings`.

use crate::import::album_links::AlbumLinks;
use crate::import::search::{MetadataResult, SourceTracks};
use crate::import::MetadataRef;
use crate::pressing::ReleaseLabel;
use crate::signals::LookupFailure;

/// Where a run is with its offered rows' documents: each record's document,
/// or why it could not be fetched, in the order they were read.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentReading {
    /// Waiting for the lookups to settle.
    Pending,
    /// Offered records not read yet are being fetched; these were read
    /// before.
    Reading(Vec<ReleaseReading>),
    /// Fetched: every record of every row offered as they rank with them.
    /// Empty when nothing was offered.
    Read(Vec<ReleaseReading>),
}

/// Where a run reads one offered record's document from.
#[derive(Clone, Debug, PartialEq)]
pub enum ReleaseToRead {
    /// Its catalog's release lookup, unless storage holds it.
    ReleaseLookup(MetadataRef),
    /// The Disc ID lookup's answer that returned it, unless storage holds
    /// it: a MusicBrainz release's own document is not asked for again.
    DiscIdAnswer(crate::import::payloads::DiscIdRelease),
}

impl ReleaseToRead {
    /// The record read.
    pub fn release(&self) -> &MetadataRef {
        match self {
            Self::ReleaseLookup(release) => release,
            Self::DiscIdAnswer(answered) => answered.release(),
        }
    }
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
    /// Every barcode the document states, as it prints them.
    pub barcodes: Vec<String>,
    /// The year the release's album first came out, as its group or master
    /// states it.
    pub album_first_year: Option<i32>,
    /// The tracks it lists for the audio being identified.
    pub source_tracks: SourceTracks,
    /// Their titles, in order; empty where the release cannot be read onto
    /// the audio.
    pub track_titles: Vec<String>,
    /// What it writes about which pressing it is, in free text.
    pub notes: Vec<String>,
    /// The releases on other catalogs it names as the same release.
    pub links: Vec<MetadataRef>,
    /// What its documents state its album is on the other lookup catalog;
    /// `NotAsked` for a Discogs release, whose documents state none.
    pub album_links: AlbumLinks,
}

impl ReleaseDocument {
    /// What `release` states, read against the audio being identified.
    pub(crate) fn of(
        release: &crate::import::source_release::SourceRelease,
        audio: &crate::import::catalog_reading::ReadAudio,
    ) -> Self {
        let reading = release.read(audio);
        Self {
            labels: release.pressing().labels.clone(),
            barcodes: release.pressing().barcodes.clone(),
            album_first_year: release.metadata.album.first_year,
            source_tracks: crate::import::catalog_reading::listing(&reading, audio),
            track_titles: match reading {
                Ok(values) => values.tracks.into_iter().map(|track| track.title).collect(),
                Err(_) => Vec::new(),
            },
            notes: release.notes.clone(),
            links: release.links(),
            album_links: release.album_links(),
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

    /// What `release`'s documents state its album is: `Unread` where they
    /// could not be read, `None` where nothing read them.
    pub(crate) fn album_statements(&self, release: &MetadataRef) -> Option<AlbumLinks> {
        self.read()
            .iter()
            .find(|reading| reading.release == *release)
            .map(|reading| match &reading.document {
                Ok(document) => document.album_links.clone(),
                Err(_) => AlbumLinks::Unread,
            })
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
                result.links = document.links.clone();
                for barcode in &document.barcodes {
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
