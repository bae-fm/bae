//! The watched folders and the folder decisions as the store keeps them, and
//! the candidate-state fixtures the import tables' tests share.

use crate::folder_scanner::CategorizedFiles;
use crate::tables::tests::*;
use crate::tables::NewImportCandidateVerdict;
use crate::{
    ArtistAssignment, AudioFile, ImportFailure, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
    TrackArtistAssignments,
};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::credits::ArtistCredit;
use bae_catalogs::search::MetadataResult;
use bae_db::test_fixtures::fixed_now;
use bae_db::test_fixtures::temp_db;
use bae_db::watched_path::host_root;
use bae_identify::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};
use bae_identify::{
    DiscIdStepView, Findings, IdentifyRunView, LookupView, NarrowedOut, TerminalVerdict,
};
use std::path::Path;
use std::path::PathBuf;

use bae_db::test_fixtures::empty_db;
use bae_db::{Database, DbArtist};
use rusqlite::params;

/// Two overlapping folders added at once: the overlap check and the insert
/// are one decision, so exactly one of them is watched and the other is
/// refused. With the check read ahead of the write, both saw an empty list and
/// both were stored — a store `load_watched_import_folders` then refuses to
/// read.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn overlapping_folders_added_at_once_keep_only_one() {
    for _ in 0..20 {
        let (db, _tmp) = temp_db().await;
        let tables = crate::tables::ImportTables::new(db.clone());
        let outer = host_root("/music");
        let inner = host_root("/music/inner");
        let (first, second) = tokio::join!(
            tables.add_watched_import_folder(&outer),
            tables.add_watched_import_folder(&inner),
        );

        let added = [&first, &second].iter().filter(|result| matches!(result, Ok(true))).count();
        let refused = [&first, &second]
            .iter()
            .filter(|result| {
                matches!(result, Err(error) if error.to_string().contains("cannot overlap"))
            })
            .count();
        assert_eq!(
            (added, refused),
            (1, 1),
            "one folder is watched and the overlapping one refused: {first:?} / {second:?}"
        );
        tables.load_watched_import_folders().await.expect("the stored folders do not overlap");
    }
}

/// Whether a folder is watched is the answer for that folder alone: the
/// watched folder is, a folder inside it or beside it is not.
#[tokio::test]
async fn a_folder_is_watched_only_by_its_own_key() {
    let (db, _tmp) = temp_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let watched = host_root("/music");
    tables.add_watched_import_folder(&watched).await.unwrap();

    assert!(tables.is_watched_import_folder(&watched).await.unwrap());
    assert!(!tables.is_watched_import_folder(&host_root("/music/inner")).await.unwrap());
    assert!(!tables.is_watched_import_folder(&host_root("/musical")).await.unwrap());
}

/// The watched folder above a folder is the one holding it, never the folder
/// itself, and the watched folders below one are those inside it, in their
/// order, never one beside it whose name only begins with its own.
#[tokio::test]
async fn the_watched_folders_above_and_below_a_folder_are_read_by_path() {
    let (db, _tmp) = temp_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for folder in ["/music/b", "/music/a", "/musical"] {
        tables.add_watched_import_folder(&host_root(folder)).await.unwrap();
    }

    assert_eq!(
        tables.watched_import_folder_above(&host_root("/music/a/disc")).await.unwrap(),
        Some(host_root("/music/a"))
    );
    assert_eq!(tables.watched_import_folder_above(&host_root("/music/a")).await.unwrap(), None);
    assert_eq!(
        tables.watched_import_folders_below(&host_root("/music")).await.unwrap(),
        [host_root("/music/b"), host_root("/music/a")]
    );
    assert_eq!(
        tables.watched_import_folders_below(&host_root("/music/a")).await.unwrap(),
        Vec::<String>::new()
    );
}

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
pub(crate) async fn current_draft(
    db: &Database,
    content_hash: &str,
) -> (u64, crate::CandidateDraft) {
    let tables = crate::tables::ImportTables::new(db.clone());
    let preparation = tables
        .load_import_candidate_preparation(content_hash)
        .await
        .unwrap()
        .and_then(crate::tables::DbPreparationRead::prepared)
        .expect("the candidate has a stored preparation");
    (preparation.metadata_revision, preparation.draft)
}

/// Store the person's answer for how one folder reads, alone: the row a
/// fixture needs beside the entries it writes itself. The service never stores
/// a decision without the candidates it gives — that is
/// `commit_folder_reading`.
pub(crate) async fn store_user_folder_decision(
    db: &Database,
    key: &crate::folder_scanner::FolderReleaseDecisionKey,
    decision: crate::folder_scanner::FolderReleaseDecision,
) -> Result<(), bae_db::DbError> {
    let key = key.clone();
    let grouping = crate::CandidateKey::grouping(&format!(
        "{}/{}",
        key.watched_folder_path.display(),
        key.relative_folder.text()
    ));
    db.call(move |sql| {
        crate::tables::folder_scans::store_folder_reading(
            sql,
            &key,
            decision,
            crate::folder_scanner::FolderReleaseDecisionAuthor::User,
            &grouping,
        )
    })
    .await
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
        artist: Some("Artist".to_string()),
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
            assets: crate::CandidatePreparedAssets::default(),
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
pub(crate) async fn fetched(db: &Database, release_id: &str) {
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
pub(crate) async fn store_verdict_with_signals(
    db: &Database,
    hash: &str,
    signals: Signals,
) -> bool {
    crate::CandidatePreparations::new(db.clone())
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

pub(crate) async fn stored_pane_candidate(db: &Database) -> (CategorizedFiles, String) {
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/music");
    let item = scanned_candidate(&root, "Album");
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
    db: &Database,
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

pub(crate) fn artist_identity_failure(discogs: &DbArtist, musicbrainz: &DbArtist) -> ImportFailure {
    ImportFailure {
        reason: crate::ImportFailureReason::error("the artist identities disagree"),
        failed_at: fixed_now(),
        identity_conflict: Some(crate::IdentityConflict::Artist(bae_db::ArtistIdentityConflict {
            incoming_artist_name: "Artist One".to_string(),
            discogs_artist_id: "discogs-1".to_string(),
            musicbrainz_artist_id: "mb-1".to_string(),
            discogs_artist: discogs.clone().into(),
            musicbrainz_artist: musicbrainz.clone().into(),
        })),
    }
}

/// Store `verdict` for a fresh candidate and read it back, with the
/// listing of each match row in position order.
pub(crate) async fn stored_and_read(verdict: &TerminalVerdict) -> (TerminalVerdict, Vec<String>) {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = new_candidate_row(&hash, &host_root("/music/Some Album"), verdict);
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();
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
            .map_err(bae_db::DbError::from)
        })
        .await
        .unwrap();
    (read, listings)
}

#[tokio::test]
async fn watched_root_overlap_uses_paths_not_sql_patterns() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for root in ["/music/100%", "/music/name_value"] {
        assert!(tables.add_watched_import_folder(&host_root(root)).await.unwrap());
    }
    for child in ["/music/100%/child", "/music/name_value/child"] {
        let error = tables.add_watched_import_folder(&host_root(child)).await.unwrap_err();
        assert!(error.to_string().contains("cannot overlap"), "{child}");
    }
}

#[tokio::test]
async fn watched_root_order_survives_middle_removal_and_later_add() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for root in ["/one", "/two", "/three"] {
        tables.add_watched_import_folder(&host_root(root)).await.unwrap();
    }
    tables.remove_watched_import_folders(vec![host_root("/two")], None).await.unwrap();
    tables.add_watched_import_folder(&host_root("/four")).await.unwrap();
    let paths: Vec<_> = tables
        .load_watched_import_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(paths, vec![host_root("/one"), host_root("/three"), host_root("/four")]);
}

/// Every spelling of a watched folder is the same row, keyed by the canonical
/// spelling.
#[tokio::test]
async fn watched_root_spellings_settle_on_one_row() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let canonical = host_root("/music/rips");
    assert!(tables.add_watched_import_folder(&canonical).await.unwrap());

    // The drive-lettered, forward-slashed form a `bae://import` link and a
    // `file://` folder drop give on Windows.
    #[cfg(windows)]
    const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
    #[cfg(not(windows))]
    const URL_SPELLINGS: &[&str] = &[];

    let spellings =
        [host_root("/music/rips/"), host_root("/music//rips"), host_root("/music/./rips")];

    for spelling in
        spellings.iter().map(PathBuf::as_path).chain(URL_SPELLINGS.iter().map(std::path::Path::new))
    {
        assert!(
            !tables.add_watched_import_folder(spelling).await.unwrap(),
            "{} is the folder already watched, not a new one",
            spelling.display()
        );
    }
    let paths: Vec<_> = tables
        .load_watched_import_folders()
        .await
        .unwrap()
        .into_iter()
        .map(|folder| folder.path)
        .collect();
    assert_eq!(paths, vec![canonical]);
}

/// `..` is refused: resolving it without the filesystem is wrong across a
/// symlink.
#[tokio::test]
async fn watched_root_rejects_a_path_climbing_out_of_itself() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let path = host_root("/music/../rips");
    assert!(tables.add_watched_import_folder(&path).await.is_err(), "{}", path.display());
}

/// Folder keys below the root that are not canonical — a doubled separator,
/// a `.` — fail the reads of the root's folders. They are spelled with the
/// host's separator, which the reads take the range below the root by.
#[tokio::test]
async fn corrupt_folder_keys_fail_when_loaded() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/mounted/library");
    tables.add_watched_import_folder(&root).await.unwrap();
    let sep = std::path::MAIN_SEPARATOR;
    let stored_root = root.display().to_string();
    let doubled = format!("{stored_root}{sep}a{sep}{sep}b");
    let dotted = format!("{stored_root}{sep}a{sep}.{sep}b");
    assert!(tables
        .set_import_candidate_skipped(std::path::Path::new(&doubled), true)
        .await
        .is_err());
    db.call(move |conn| {
        conn.execute("INSERT INTO skipped_import_candidates VALUES (?)", [doubled.into_bytes()])?;
        conn.execute(
            "INSERT INTO release_grouping (key, anchor_folder, combined, author) \
                 VALUES (CAST('grouping:corrupt' AS BLOB), ?, 1, 'user')",
            [dotted.into_bytes()],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(tables.load_skipped_import_candidates(&root).await.is_err());
    assert!(tables.load_folder_release_decisions(&root).await.is_err());
}

/// The add refuses a root inside another, so stored rows that overlap are
/// corrupt and fail to load.
#[tokio::test]
async fn overlapping_stored_roots_fail_to_load() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let outer = host_root("/music");
    let inner = host_root("/music/artist");
    tables.add_watched_import_folder(&outer).await.unwrap();
    let error = tables.add_watched_import_folder(&inner).await.unwrap_err();
    assert!(error.to_string().contains("cannot overlap"), "{error}");
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO watched_import_folders (path, position) VALUES (?, 1)",
            params![coven::path_bytes(&inner)],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let error = tables.load_watched_import_folders().await.unwrap_err();
    assert!(error.to_string().contains("cannot overlap"), "{error}");
}
