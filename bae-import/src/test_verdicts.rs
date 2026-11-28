//! Verdicts a test stores on a scanned candidate without running
//! identification, for the tests of this crate and the crates above it.

use crate::folder_scanner::FolderCandidate;
use crate::tables::{ImportTables, NewImportCandidateVerdict};

/// Store on `candidate` the verdict of a run that found one MusicBrainz
/// release, `release_id` of the release group `group_id`, and picked none.
pub async fn store_found_verdict(
    tables: &ImportTables,
    candidate: &FolderCandidate,
    release_id: &str,
    group_id: &str,
) {
    let release = bae_catalogs::search::MetadataResult {
        source: bae_catalogs::catalog::LookupCatalog::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist")),
        year: None,
        labels: Vec::new(),
        area: None,
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: bae_types::pressing::StatedMedia::Undescribed,
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some(group_id.to_string()),
        album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
        source_tracks: None,
        document_failure: None,
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    };
    let verdict = bae_identify::TerminalVerdict::Found {
        findings: bae_identify::Findings {
            matches: vec![release],
            pressings: vec![0],
            narrowed_out: bae_identify::NarrowedOut::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: candidate.files.track_count(),
        ledger: None,
    };
    let stored = tables
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: candidate.files.content_hash(),
            file_edit_revision: candidate.file_edit_revision,
            candidate_key: candidate.key(),
            verdict,
            signals: bae_identify::signals::Signals {
                pane: bae_identify::signals::PaneSignals {
                    origin: bae_catalogs::AudioOrigin::default(),
                    disc_id: bae_identify::signals::DiscIdSignal::Absent,
                    barcode: bae_identify::signals::BarcodeSignal::Absent,
                    text: bae_identify::signals::TextSignal::Settled {
                        catalogs: Vec::new(),
                        free_text: Vec::new(),
                    },
                    isrcs: Vec::new(),
                    track_titles: Vec::new(),
                },
                text_pool: Vec::new(),
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .expect("the verdict's write runs");
    assert!(stored, "the verdict lands on the scanned candidate");
}
