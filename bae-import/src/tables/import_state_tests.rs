//! The folder decisions as the store keeps them, and the candidate-state
//! fixtures the import tables' tests share.

use crate::folder_scanner::CategorizedFiles;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use crate::tables::NewImportCandidateVerdict;
use crate::{
    ArtistAssignment, AudioFile, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
    TrackArtistAssignments,
};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::credits::ArtistCredit;
use bae_catalogs::search::MetadataResult;
use bae_identify::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use bae_identify::{
    DiscIdStepView, Findings, IdentifyRunView, LookupView, NarrowedOut, TerminalVerdict,
};
use bae_records::DbArtist;
use bae_store::test_fixtures::empty_db;
use bae_store::test_fixtures::fixed_now;
use bae_store::Store;
use std::path::Path;
use std::path::PathBuf;

/// The candidate as a caller reads it back after a fixture stored it: no file
/// decisions edited yet, and whatever metadata revision the caller just
/// observed. Tests that deliberately submit a stale or advanced
/// `file_edit_revision` write the record out in full instead.
pub(crate) fn as_read(content_hash: &str, metadata_revision: u64) -> crate::CandidateAsRead {
    crate::CandidateAsRead {
        content_hash: content_hash.to_string(),
        file_edit_revision: 0,
        metadata_revision,
    }
}

/// The stored draft and the metadata revision it stands at: what a file
/// decision that reshapes no track lays over the folder.
pub(crate) async fn current_draft(db: &Store, content_hash: &str) -> (u64, crate::CandidateDraft) {
    let tables = crate::tables::ImportTables::new(db.clone());
    let preparation = tables
        .load_candidate_preparation(content_hash)
        .await
        .unwrap()
        .and_then(crate::tables::CandidatePreparationRead::stored)
        .expect("the candidate has a stored preparation");
    (preparation.metadata_revision, preparation.metadata.draft)
}

/// The sample verdict's ledger: a disc ID read off a rip log that named its
/// release.
pub(crate) fn sample_ledger() -> IdentifyRunView {
    IdentifyRunView {
        providers: vec![LookupCatalog::MusicBrainz],
        disc_id: DiscIdStepView::Read {
            discs: vec![bae_identify::DiscIdRow {
                disc_id: "disc-1".to_string(),
                lookup: LookupView::Found {
                    count: 1,
                    groups: bae_identify::release_group::group_results(
                        bae_identify::release_group::unranked(vec![sample_match()]),
                        None,
                    ),
                },
            }],
        },
        barcode: bae_identify::BarcodeStepView::Absent,
        catalog: bae_identify::CatalogStepView::NoneFound,
        isrc: bae_identify::IsrcStepView::Absent,
        search: bae_identify::SearchStepView::NotNeeded,
    }
}

pub(crate) fn sample_match() -> MetadataResult {
    MetadataResult {
        source: LookupCatalog::MusicBrainz,
        release_id: "rel-1".to_string(),
        title: "Album".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist")),
        year: Some(1999),
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Label"), Some("CAT-1"))],
        area: Some(bae_types::pressing::area("US")),
        status: None,
        packaging: None,
        discogs_details: Vec::new(),
        barcodes: Vec::new(),
        media: bae_types::pressing::StatedMedia::PerMedium(vec![Some(
            bae_types::pressing::Medium::Cd,
        )]),
        links: Vec::new(),
        cover_art: None,
        source_group_id: Some("group-1".to_string()),
        album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
        source_tracks: Some(bae_catalogs::search::SourceTracks::Listed { count: 2 }),
        document_failure: None,
        // No catalog release of it is stored, so a stored verdict reads none.
        album_first_year: None,
        track_titles: Vec::new(),
        notes: Vec::new(),
    }
}

pub(crate) fn sample_verdict() -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: sample_findings(),
        track_count: 11,
        ledger: Some(sample_ledger()),
    }
}

pub(crate) fn sample_findings() -> Findings {
    Findings {
        matches: vec![sample_match()],
        pressings: vec![0],
        narrowed_out: NarrowedOut::default(),
        medium_conflict: None,
        named_notes: vec![bae_identify::NamedNote {
            release: bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "rel-1"),
            note: "Pressed By Plant Name".to_string(),
        }],
    }
}

/// Settled signals with nothing found.
pub(crate) fn sample_signals() -> bae_identify::signals::Signals {
    bae_identify::signals::Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: bae_identify::signals::DiscIdSignal::Absent,
        barcode: bae_identify::signals::BarcodeSignal::Absent,
        text: bae_identify::signals::TextSignal::Settled {
            catalogs: Vec::new(),
            free_text: Vec::new(),
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// The row with a draft picking `release_id`, as a run that settled on it
/// writes.
pub(crate) fn concluding(
    mut row: NewImportCandidateVerdict,
    release_id: &str,
) -> NewImportCandidateVerdict {
    row.pick = Some(crate::tables::VerdictPick {
        link: release_link(release_id),
        metadata: crate::CandidateMetadataDraft {
            draft: candidate_draft("", ""),
            provenance: Some(release_pick(release_id)),
            cover: None,
        },
    });
    row
}

pub(crate) fn new_candidate_row(
    content_hash: &str,
    folder_path: &Path,
    verdict: &TerminalVerdict,
) -> NewImportCandidateVerdict {
    NewImportCandidateVerdict {
        content_hash: content_hash.to_string(),
        file_edit_revision: 0,
        candidate_key: crate::CandidateKey::folder(folder_path),
        verdict: verdict.clone(),
        signals: sample_signals(),
        pick: None,
        releases: Vec::new(),
    }
}

/// Store MusicBrainz release `release_id` as the setup of a write that names
/// it, which then finds it stored.
pub(crate) async fn fetched(db: &Store, release_id: &str) {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.save_source_release(&fetched_release(release_id)).await.unwrap();
}

/// The catalog release [`fetched`] stores.
pub(crate) fn fetched_release(release_id: &str) -> bae_catalogs::source_release::SourceRelease {
    bae_catalogs::payloads::ReleasePayloads::for_test(
        bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            release_id,
        ),
        serde_json::json!({
            "id": release_id,
            "title": "Album",
            "artist-credit": [{ "name": "Artist" }],
            "media": [],
            "cover-art-archive": { "front": false, "darkened": false }
        })
        .to_string(),
        Vec::new(),
    )
    .extract()
    .unwrap()
}

pub(crate) fn release_link(release_id: &str) -> bae_identify::PressingLink {
    bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            release_id.to_string(),
        ),
        partners: vec![],
    }
}

pub(crate) fn release_pick(release_id: &str) -> crate::MetadataProvenance {
    crate::MetadataProvenance::ExternalRelease {
        record: bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            release_id.to_string(),
        ),
    }
}

pub(crate) fn pane_candidate() -> CategorizedFiles {
    track_files_candidate(&[("01 Track.flac", 111), ("CDImage.flac", 222)])
}

pub(crate) fn pane_candidate_path() -> PathBuf {
    host_root("/music").join("Album")
}

pub(crate) fn pane_candidate_key() -> crate::CandidateKey {
    crate::CandidateKey::folder(&pane_candidate_path())
}

pub(crate) fn settled_signals() -> Signals {
    Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// Store a verdict for `hash` carrying `signals`, and say whether it landed.
pub(crate) async fn store_verdict_with_signals(db: &Store, hash: &str, signals: Signals) -> bool {
    crate::tables::ImportTables::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: hash.to_string(),
            file_edit_revision: 0,
            candidate_key: pane_candidate_key(),
            verdict: sample_verdict(),
            signals,
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap()
}

pub(crate) fn edited_row(id: &str, title: &str, file: Option<AudioFile>) -> RawTrackEdit {
    RawTrackEdit {
        id: id.to_string(),
        title: title.to_string(),
        artist_assignments: TrackArtistAssignments::Explicit(vec![crate::Credited::plain(
            credit_named("Artist Name"),
        )]),
        file,
    }
}

pub(crate) fn credit_named(name: &str) -> ArtistAssignment {
    ArtistAssignment::Credit {
        credit: ArtistCredit {
            credited_as: None,
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: None,
        },
    }
}

pub(crate) fn existing_artist() -> DbArtist {
    DbArtist {
        id: bae_util::test_ids::test_uuid("library-artist"),
        name: "Library Artist".to_string(),
        sort_name: Some("Artist, Library".to_string()),
        discogs_artist_id: Some("discogs-library".to_string()),
        musicbrainz_artist_id: Some("mb-library".to_string()),
        created_at: fixed_now(),
    }
}

/// The candidate the pane controls address, as a scan of `root` reads it:
/// one track and a `cover.jpg` beside it.
pub(crate) fn pane_candidate_item(root: &Path) -> crate::folder_scanner::ScanItem {
    let mut item = scanned_candidate(root, "Album");
    let crate::folder_scanner::ScanItem::Valid(candidate) = &mut item else {
        unreachable!("the fixture creates a valid candidate")
    };
    candidate.files.files.push(crate::folder_scanner::CandidateFile {
        file: crate::folder_scanner::ScannedFile::new(
            PathBuf::from("cover.jpg"),
            "cover.jpg".to_string(),
            456,
            1,
        ),
        role: crate::folder_scanner::FileRole::Artwork,
    });
    item
}

/// [`pane_candidate_item`] stored by a scan of `/music`.
pub(crate) async fn stored_pane_candidate(db: &Store) -> (CategorizedFiles, String) {
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/music");
    let item = pane_candidate_item(&root);
    let crate::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture creates a valid candidate")
    };
    let files = candidate.files.clone();
    let hash = files.content_hash();
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &item)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    (files, hash)
}

pub(crate) async fn store_candidate_state(
    db: &Store,
    files: &CategorizedFiles,
    folder_path: &Path,
) -> String {
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = folder_path.parent().expect("the candidate fixture has a watched root");
    let name = folder_path
        .file_name()
        .expect("the candidate fixture has a folder name")
        .to_str()
        .expect("the candidate fixture's folder name is text");
    let item = crate::folder_scanner::ScanItem::Valid(candidate_with(
        root,
        name,
        files.clone(),
        crate::folder_scanner::ReleaseFileScope::Direct,
    ));
    let hash = files.content_hash();
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(root, generation, &item)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    hash
}

/// `metadata_draft` as a candidate stores it.
pub(crate) fn candidate_draft(title: &str, artist: &str) -> crate::CandidateDraft {
    crate::pane::draft_of_edit(metadata_draft(title, artist))
}

pub(crate) fn metadata_draft(title: &str, artist: &str) -> RawReleaseEdit {
    RawReleaseEdit {
        album_title: title.to_string(),
        album_artist_assignments: if artist.is_empty() {
            Vec::new()
        } else {
            vec![crate::Credited::plain(credit_named(artist))]
        },
        album_year: String::new(),
        pressing: RawPressingEdit {
            year: String::new(),
            labels: Vec::new(),
            facts: Default::default(),
            barcodes: Vec::new(),
        },
        tracks: vec![RawTrackEdit {
            id: "candidate-track-0".to_string(),
            title: "Track title".to_string(),
            artist_assignments: TrackArtistAssignments::AlbumArtists,
            file: Some(AudioFile::Standalone { file_id: "01 Track.flac".into() }),
        }],
    }
}

/// A twelve-track sheet written against a WAV, the FLAC it was encoded to,
/// and the rip log.
pub(crate) fn walkthrough_folder() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(
        bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac"),
        tmp.path().join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(fixtures.join("tests/fixtures/logs/test_album.log"), tmp.path().join("rip.log"))
        .unwrap();
    let mut cue =
        String::from("PERFORMER \"Test Artist\"\nTITLE \"Album\"\nFILE \"cd.wav\" WAVE\n");
    for track in 1..=12 {
        cue.push_str(&format!(
            "  TRACK {track:02} AUDIO\n    TITLE \"Track {track:02}\"\n    INDEX 01 00:{:02}:00\n",
            track - 1,
        ));
    }
    std::fs::write(tmp.path().join("cd.cue"), cue).unwrap();
    tmp
}

/// Store `verdict` for a fresh candidate and read it back, with the
/// listing of each match row in position order.
pub(crate) async fn stored_and_read(verdict: &TerminalVerdict) -> (TerminalVerdict, Vec<String>) {
    let (db, hash, _tmp) = stored_verdict(verdict).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let loaded = tables.load_import_candidate_states().await.unwrap();
    let read =
        loaded[&hash].identify.as_ref().expect("a stored verdict reads back").verdict.clone();
    let listings = db
        .read(move |sql| {
            sql.query(
                "SELECT release_id || ' ' || listing FROM import_candidate_match \
                 WHERE content_hash = ? ORDER BY position",
                [hash],
                |row| row.get::<_, String>(0),
            )
            .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap();
    (read, listings)
}

/// A library holding one candidate's `verdict`, as the verdict write stores
/// it: the library, the candidate's content hash and the directory the
/// library lives in.
pub(crate) async fn stored_verdict(
    verdict: &TerminalVerdict,
) -> (Store, String, tempfile::TempDir) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::tables::ImportTables::new(db.clone()).store_verdict(&row).await.unwrap();
    (db, hash, tmp)
}

/// Damage the candidate `content_hash`'s draft as no write leaves it: it
/// says nobody wrote it while it is read from a source, so reading the
/// candidate fails as a damaged row.
pub(crate) async fn disown_the_draft(db: &Store, content_hash: &str) {
    bae_store::test_fixtures::exec(
        db,
        "UPDATE import_candidate_edit SET author = 'nobody' WHERE content_hash = ?1",
        &[content_hash],
    )
    .await;
}

/// The import pane's controls over one candidate: its writers as the handle
/// calls them, for the candidate the scan lists at `key` under `root`, each
/// composed against the candidate as it stands when the control is used.
pub(crate) struct PaneControls {
    preparations: crate::tables::ImportTables,
    tables: crate::tables::ImportTables,
    root: PathBuf,
    key: crate::CandidateKey,
}

impl PaneControls {
    pub(crate) fn new(db: &Store, root: &Path, key: &crate::CandidateKey) -> Self {
        Self {
            preparations: crate::tables::ImportTables::new(db.clone()),
            tables: crate::tables::ImportTables::new(db.clone()),
            root: root.to_path_buf(),
            key: key.clone(),
        }
    }

    /// The controls over the scanned candidate `candidate`.
    pub(crate) fn of(db: &Store, candidate: &crate::folder_scanner::FolderCandidate) -> Self {
        Self::new(db, &candidate.watched_folder_path, &candidate.key())
    }

    /// The controls over [`stored_pane_candidate`]'s candidate.
    pub(crate) fn of_pane_candidate(db: &Store) -> Self {
        Self::new(db, &host_root("/music"), &pane_candidate_key())
    }

    /// The candidate `content_hash` as the pane reads it now.
    async fn read(&self, content_hash: &str) -> crate::CandidateAsRead {
        let state = self
            .tables
            .load_import_candidate_state(content_hash)
            .await
            .unwrap()
            .expect("the pane is drawn over a stored candidate");
        crate::CandidateAsRead {
            content_hash: content_hash.to_string(),
            file_edit_revision: state.file_edits.revision,
            metadata_revision: state.metadata_revision,
        }
    }

    /// Type `value` into the album field `field`.
    pub(crate) async fn type_field(
        &self,
        content_hash: &str,
        field: crate::CandidateEditField,
        value: &str,
    ) -> Result<u64, crate::ImportError> {
        self.set_field(
            content_hash,
            crate::DraftFieldEdit::Text { field, value: value.to_string() },
        )
        .await
    }

    /// Type or choose one album field.
    pub(crate) async fn set_field(
        &self,
        content_hash: &str,
        edit: crate::DraftFieldEdit,
    ) -> Result<u64, crate::ImportError> {
        let read = self.read(content_hash).await;
        self.preparations
            .set_field_prepared(&self.root, &self.key, content_hash, read.file_edit_revision, edit)
            .await
    }

    /// Choose `cover` as the candidate's cover.
    pub(crate) async fn choose_cover(
        &self,
        content_hash: &str,
        cover: &crate::cover_art::PreparedCover,
    ) -> Result<u64, crate::ImportError> {
        let read = self.read(content_hash).await;
        self.preparations.set_prepared_cover(&self.root, &self.key, &read, cover).await
    }

    /// Set the album's artists to `assignments`.
    pub(crate) async fn set_album_artists(
        &self,
        content_hash: &str,
        assignments: &[crate::Credited],
    ) -> Result<u64, crate::ImportError> {
        let read = self.read(content_hash).await;
        self.preparations
            .set_album_artists_prepared(&self.root, &self.key, &read, assignments)
            .await
    }

    /// Change one row of the mapping table to `row`.
    pub(crate) async fn edit_track(
        &self,
        content_hash: &str,
        row: &crate::RawTrackEdit,
    ) -> Result<u64, crate::ImportError> {
        let read = self.read(content_hash).await;
        self.preparations.set_track_edit_prepared(&self.root, &self.key, &read, row).await
    }
}
