//! The import list's shared test fixtures: a library whose rows a test
//! writes as SQL, as the placement's triggers and the list's reads take them,
//! and the list read the way a surface reads it; and candidates scanned and
//! looked up through the import tables' own writes.

use super::*;
use crate::folder_scanner::{FolderCandidate, ScanItem};
use crate::selection::SelectionChange;
use crate::tables::tests::*;
use crate::FolderReleaseDecisionKey;
use crate::PendingFilterEntry;
use crate::{
    ImportListId, ImportListItem, ImportListOrder, ImportListProjection, ImportListRequest,
    ImportListView, PendingFilter, TriageTab,
};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_identify::TerminalVerdict;
use bae_records::test_fixtures::live_query::RELEASE_ID;
use bae_store::test_fixtures::{empty_db, exec, exec_batch};
use bae_store::LibraryPageWindow;

/// A database with one watched folder, its scan begun, and helpers that spell
/// the rows each test writes.
pub(crate) struct Library {
    pub(crate) db: bae_store::Store,
    pub(crate) _tmp: tempfile::TempDir,
    pub(crate) root: std::path::PathBuf,
}

impl Library {
    pub(crate) async fn new() -> Self {
        let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
        let root = crate::tables::watched_path::host_root("/music");
        let library = Self { db, _tmp: tmp, root };
        library.run(&library.watch(&library.root, 0)).await;
        library
    }

    /// Watch `root` at `position`, its scan begun.
    pub(crate) fn watch(&self, root: &std::path::Path, position: i64) -> String {
        format!(
            "INSERT INTO watched_import_folders (path, position) VALUES ({root}, {position});
             INSERT INTO folder_scan_roots (watched_folder_path, generation, status, volume)
             VALUES ({root}, 1, 'scanning', 'local');",
            root = sql_blob(&coven::path_bytes(root)),
        )
    }

    /// The folder at `display_path` below the watched folder.
    pub(crate) fn path(&self, display_path: &str) -> std::path::PathBuf {
        folder_below(&self.root, display_path)
    }

    /// The key of the release scanned at `display_path`.
    pub(crate) fn key(&self, display_path: &str) -> crate::CandidateKey {
        crate::CandidateKey::folder(&self.path(display_path))
    }

    /// The folder at `display_path` as a SQL literal of its bytes.
    pub(crate) fn sql(&self, display_path: &str) -> String {
        sql_blob(&coven::path_bytes(&self.path(display_path)))
    }

    /// The watched folder as a SQL literal of its bytes.
    pub(crate) fn sql_root(&self) -> String {
        sql_blob(&coven::path_bytes(&self.root))
    }

    /// Store the scan's release at `display_path` as `kind`.
    pub(crate) fn scan_as(&self, display_path: &str, kind: &str) -> String {
        self.scan_folder(std::path::Path::new(display_path), kind)
    }

    /// Store the scan's release at the folder `relative` below the watched
    /// folder as `kind`.
    pub(crate) fn scan_folder(&self, relative: &std::path::Path, kind: &str) -> String {
        let folder = self.folder(relative);
        let relative = crate::RelativeFolder::of(relative).unwrap();
        let name = coven::path_text(std::path::Path::new(folder.file_name().unwrap()));
        let hash = match kind {
            "invalid" => "NULL".to_string(),
            _ => format!("'{}'", hash(&relative.text())),
        };
        format!(
            "INSERT INTO scan_candidate
                 (watched_folder_path, path, generation, kind, name, relative_folder, folder,
                  file_root, scope, content_hash)
             VALUES ({root}, {path}, 1, '{kind}', '{name}', {relative}, {path},
                     {path}, 'direct', {hash});",
            root = self.sql_root(),
            path = sql_blob(&coven::path_bytes(&folder)),
            relative = sql_blob(relative.as_bytes()),
        )
    }

    /// The folder `relative` below the watched folder.
    pub(crate) fn folder(&self, relative: &std::path::Path) -> std::path::PathBuf {
        crate::tables::watched_path::folder_below(
            &self.root,
            &crate::RelativeFolder::of(relative).unwrap(),
        )
    }

    /// Store the scan's settled release at `display_path`.
    pub(crate) fn scan(&self, display_path: &str) -> String {
        self.scan_as(display_path, "valid")
    }

    /// Record when the scans first saw the folder at `display_path`.
    pub(crate) fn discovered(&self, display_path: &str, first_seen_at: i64) -> String {
        format!(
            "INSERT INTO folder_discovery (folder, first_seen_at, settled)
             VALUES ({}, {first_seen_at}, 1);",
            self.sql(display_path)
        )
    }

    /// The release's state at its files' current revision, with a blank
    /// draft a person owns.
    pub(crate) fn state(&self, display_path: &str) -> String {
        let hash = hash(display_path);
        format!(
            "INSERT INTO import_candidate_state (content_hash, candidate_key)
             VALUES ('{hash}', {path});
             INSERT INTO import_candidate_edit
                 (content_hash, album_title, album_year, year, author, draft_blank, draft_valid)
             VALUES ('{hash}', '', '', '', 'person', 1, 0);",
            path = self.sql(display_path)
        )
    }

    /// Write `title` into the release's draft.
    pub(crate) fn draft(&self, display_path: &str, title: &str) -> String {
        format!(
            "UPDATE import_candidate_edit SET album_title = '{title}', draft_blank = 0
             WHERE content_hash = '{}';",
            hash(display_path)
        )
    }

    /// A stored verdict of `kind` over a folder of `track_count` tracks.
    pub(crate) fn verdict(
        &self,
        display_path: &str,
        kind: &str,
        track_count: Option<i64>,
    ) -> String {
        let failures = match kind {
            "failed" => "'[{\"source\":\"musicbrainz\"}]'",
            _ => "NULL",
        };
        let error = match kind {
            "error" => "'bae broke'",
            _ => "NULL",
        };
        format!(
            "INSERT INTO import_candidate_verdict
                 (content_hash, kind, track_count, failures_json, error_detail, kept_own_draft)
             VALUES ('{}', '{kind}', {}, {failures}, {error}, 0);",
            hash(display_path),
            track_count.map_or("NULL".to_string(), |count| count.to_string()),
        )
    }

    /// One release the verdict offers, in pressing row `pressing`, listing
    /// `source_tracks` (its kind and count).
    pub(crate) fn found_release(
        &self,
        display_path: &str,
        position: i64,
        pressing: i64,
        source_tracks: (Option<&str>, Option<i64>),
    ) -> String {
        let (kind, count) = source_tracks;
        format!(
            "INSERT INTO import_candidate_match
                 (content_hash, position, listing, pressing, source, release_id, title,
                  media_kind, album_links, source_tracks_kind, source_tracks_count)
             VALUES ('{}', {position}, 'offered', {pressing}, 'musicbrainz', 'mb-{position}',
                     'Album', 'undescribed', 'not_asked', {}, {});",
            hash(display_path),
            kind.map_or("NULL".to_string(), |kind| format!("'{kind}'")),
            count.map_or("NULL".to_string(), |count| count.to_string()),
        )
    }

    /// Run `sql`, and check the counts and the search index kept beside the
    /// placement still say what it holds: every write here moves placement
    /// rows, and each must move the counts and the searched text with them.
    pub(crate) async fn run(&self, sql: &str) {
        exec_batch(&self.db, sql).await;
        self.searchable(sql).await;
        let (kept, counted) = self
            .db
            .read(|sql| {
                let rows = |statement: &str| {
                    sql.query(statement, [], |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    })
                };
                let kept = [
                    rows("SELECT tab, item, NULL, count FROM import_list_tab_count")?,
                    rows("SELECT pending_entry, NULL, NULL, count FROM import_list_entry_count")?,
                    rows(
                        "SELECT coven_path_text(watched_folder_path), coven_path_text(group_folder), \
                         pending_entry, count FROM import_list_group_entry_count",
                    )?,
                ];
                let counted = [
                    rows(
                        "SELECT tab, item, NULL, count(*) FROM import_list_placement \
                         GROUP BY tab, item",
                    )?,
                    rows(
                        "SELECT pending_entry, NULL, NULL, count(*) FROM import_list_placement \
                         WHERE pending_entry IS NOT NULL GROUP BY pending_entry",
                    )?,
                    rows(
                        "SELECT coven_path_text(watched_folder_path), group_name, pending_entry, \
                         count(*) FROM import_list_placement \
                         WHERE item = 'candidate' AND group_folder IS NOT NULL \
                         GROUP BY watched_folder_path, group_folder, pending_entry",
                    )?,
                ];
                Ok((kept, counted))
            })
            .await
            .unwrap();
        for (mut kept, mut counted) in kept.into_iter().zip(counted) {
            kept.sort();
            counted.sort();
            assert_eq!(kept, counted, "the counts kept beside the placement after {sql}");
        }
    }

    /// Check the search index holds each release's folded shown text under
    /// its placement row's number, and nothing else; and that every header
    /// says whether its folder is offered to be read as one release.
    pub(crate) async fn searchable(&self, after: &str) {
        let (indexed, placed) = self
            .db
            .read(|sql| {
                let rows = |statement: &str| {
                    sql.query(statement, [], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    })
                };
                Ok((
                    rows("SELECT rowid, title, artists FROM import_list_search ORDER BY rowid")?,
                    rows(
                        "SELECT entry, filter_title, filter_artists FROM import_list_placement \
                         WHERE item <> 'group' ORDER BY entry",
                    )?,
                ))
            })
            .await
            .unwrap();
        assert_eq!(indexed, placed, "the search index after {after}");
        let unjudged = self
            .db
            .read(|sql| {
                Ok(sql.query(
                    "SELECT coven_path_text(key) FROM import_list_placement \
                     WHERE item = 'group' AND combinable IS NULL",
                    [],
                    |row| row.get::<_, String>(0),
                )?)
            })
            .await
            .unwrap();
        assert!(
            unjudged.is_empty(),
            "headers saying nothing of combining after {after}: {unjudged:?}"
        );
        exec_batch(
            &self.db,
            "INSERT INTO import_list_search (import_list_search, rank) VALUES ('integrity-check', 1);",
        )
        .await;
    }

    /// Open a list showing `view`.
    pub(crate) async fn open(&self, view: ImportListView) -> ImportListId {
        let tables = crate::tables::ImportTables::new(self.db.clone());
        tables.open_import_list(view).await.unwrap()
    }

    /// One read of `list` through `window`. A searching or folded list's
    /// items read from its tab's rows and by its other read must agree, since
    /// its windows are read by either.
    pub(crate) async fn read_through(
        &self,
        list: ImportListId,
        window: LibraryPageWindow,
    ) -> ImportListProjection {
        let tables = crate::tables::ImportTables::new(self.db.clone());
        let read = tables
            .load_import_list(ImportListRequest {
                list,
                windows: std::iter::once(window).collect(),
            })
            .await
            .unwrap();
        if let Some((walked, other)) = tables.import_list_read_both_ways(list).await.unwrap() {
            assert_eq!(
                walked, other,
                "{:?}: the tab's rows in order against its other read",
                read.summary.narrowing
            );
        }
        read
    }

    /// One read of `list` through one window over every item.
    pub(crate) async fn read(&self, list: ImportListId) -> ImportListProjection {
        self.read_through(list, LibraryPageWindow { offset: 0, limit: u64::MAX }).await
    }

    /// One read of a list opened on `view`.
    pub(crate) async fn shown(&self, view: ImportListView) -> ImportListProjection {
        let list = self.open(view).await;
        self.read(list).await
    }

    /// The items a list opened on `view` shows.
    pub(crate) async fn items(&self, view: ImportListView) -> Vec<String> {
        sequence(&self.shown(view).await)
    }

    /// Change `list`'s selection, and answer with the revision.
    pub(crate) async fn select(&self, list: ImportListId, change: SelectionChange) -> u64 {
        let tables = crate::tables::ImportTables::new(self.db.clone());
        match tables.change_import_list_selection(list, change).await.unwrap() {
            crate::tables::SelectionWrite::Changed { revision } => revision,
            crate::tables::SelectionWrite::RunNotShown => {
                panic!("the run's ends are shown")
            }
        }
    }

    /// The release at `display_path` as this device imported it, its release
    /// written at `created_at` (RFC 3339): what puts it on Done.
    pub(crate) fn imported(&self, display_path: &str, created_at: &str) -> String {
        let album = bae_util::test_ids::test_uuid(&format!("album {display_path}"));
        let release = self.release_id(display_path);
        format!(
            "INSERT INTO albums (id, title, is_compilation, created_at)
             VALUES ('{album}', 'Album Title', 0, '{created_at}');
             INSERT INTO releases (id, album_id, created_at, album_order)
             VALUES ('{release}', '{album}', '{created_at}', 0);
             INSERT INTO local_release_origin (release_id, content_hash)
             VALUES ('{release}', '{hash}');",
            hash = hash(display_path),
        )
    }

    /// The id of the release [`Self::imported`] writes for `display_path`.
    pub(crate) fn release_id(&self, display_path: &str) -> String {
        bae_util::test_ids::test_uuid(&format!("release {display_path}"))
    }

    /// The person's skip of the release at `display_path`.
    pub(crate) fn skip(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO skipped_import_candidates (candidate_path) VALUES ({});",
            self.sql(display_path)
        )
    }

    /// A run of identification `stage` (`queued`, `running`, `finalizing`)
    /// for the release at `display_path`.
    pub(crate) fn identifying(&self, display_path: &str, stage: &str) -> String {
        let (admission, run) = match stage {
            "queued" => ("'automatic'", "NULL"),
            _ => ("NULL", "1"),
        };
        format!(
            "INSERT INTO import_candidate_standing
                 (candidate_key, identification, content_hash, file_edit_revision, admission,
                  identify_run)
             VALUES ({}, '{stage}', '{}', 0, {admission}, {run});",
            self.sql(display_path),
            hash(display_path)
        )
    }

    /// An import `stage` (`queued`, `running`, `writing`) owning the release
    /// at `display_path`.
    pub(crate) fn importing(&self, display_path: &str, stage: &str) -> String {
        format!(
            "INSERT INTO import_candidate_standing (candidate_key, import_step, import_id)
             VALUES ({}, '{stage}', 'import-{display_path}');",
            self.sql(display_path)
        )
    }

    /// The failure the last import of the release at `display_path` left.
    pub(crate) fn import_failed(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO import_candidate_failure (content_hash, error, failed_at)
             VALUES ('{}', 'it broke', '2026-01-01T00:00:00Z');",
            hash(display_path)
        )
    }

    /// The release at `display_path` linked to an album, its pressing
    /// unknown.
    pub(crate) fn linked(&self, display_path: &str) -> String {
        format!(
            "INSERT INTO import_candidate_release_link (content_hash, kind) VALUES ('{h}', 'album');
             INSERT INTO import_candidate_release_link_album (content_hash, kind, catalog, album_id)
             VALUES ('{h}', 'album', 'musicbrainz', 'group-{display_path}');",
            h = hash(display_path),
        )
    }

    /// Mark the draft of the release at `display_path` one that would import.
    pub(crate) fn valid_draft(&self, display_path: &str) -> String {
        format!(
            "UPDATE import_candidate_edit SET draft_valid = 1 WHERE content_hash = '{}';",
            hash(display_path)
        )
    }

    /// The group at `name` under the watched folder.
    pub(crate) fn group_key(&self, name: &str) -> FolderReleaseDecisionKey {
        FolderReleaseDecisionKey {
            watched_folder_path: self.root.clone(),
            relative_folder: crate::RelativeFolder::test(name),
        }
    }

    /// A found verdict over eleven tracks, whose releases agreement kept make
    /// `pressings` pressings, each listing `source_tracks`.
    pub(crate) fn found(
        &self,
        display_path: &str,
        pressings: i64,
        source_tracks: (Option<&str>, Option<i64>),
    ) -> String {
        let mut sql = self.verdict(display_path, "found", Some(11));
        for pressing in 0..pressings {
            sql.push_str(&self.found_release(display_path, pressing, pressing, source_tracks));
        }
        sql
    }

    /// A run of the release at `display_path` whose answer bae could not
    /// write, saying `error`.
    pub(crate) fn not_written(&self, display_path: &str, error: &str) -> String {
        format!(
            "INSERT INTO import_candidate_standing
                 (candidate_key, identification, content_hash, file_edit_revision,
                  save_failure_error)
             VALUES ({}, 'not_written', '{}', 0, '{error}');",
            self.sql(display_path),
            hash(display_path)
        )
    }
}

/// `bytes` as a SQL BLOB literal.
pub(crate) fn sql_blob(bytes: &[u8]) -> String {
    format!("X'{}'", hex::encode(bytes))
}

/// The folder at `display_path` below a watched folder as a SQL literal of its
/// `RelativeFolder` bytes.
pub(crate) fn sql_relative(display_path: &str) -> String {
    sql_blob(crate::RelativeFolder::test(display_path).as_bytes())
}

/// The content hash the fixtures give the release at `display_path`.
pub(crate) fn hash(display_path: &str) -> String {
    format!("hash-{display_path}")
}

/// `tab` in the default order, unfiltered.
pub(crate) fn view(tab: TriageTab) -> ImportListView {
    ImportListView { tab, ..ImportListView::default() }
}

/// The items one read holds, one readable line each: a header by its
/// group's name, a release or a folder that failed validation by its display
/// path.
pub(crate) fn sequence(projection: &ImportListProjection) -> Vec<String> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .map(|item| match item {
            ImportListItem::GroupHeader { group, .. } => format!("group {}", group.name),
            ImportListItem::Candidate { row, .. } => format!("candidate {}", row.display_path),
            ImportListItem::Invalid { candidate, .. } => {
                format!("invalid {}", candidate.relative_folder.text())
            }
        })
        .collect()
}

/// The release row one read holds at `display_path`.
pub(crate) fn row_at<'a>(
    projection: &'a ImportListProjection,
    display_path: &str,
) -> (&'a crate::TriageRow, &'a crate::CandidateLiveState) {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .find_map(|item| match item {
            ImportListItem::Candidate { row, live, .. } if row.display_path == display_path => {
                Some((row, live))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{display_path} is a row of the read"))
}

/// The candidate releases of `display_paths`, each scanned with a state
/// whose blank draft a person owns.
pub(crate) fn scanned_with_states(lib: &Library, display_paths: &[&str]) -> String {
    display_paths
        .iter()
        .map(|display_path| format!("{}{}", lib.scan(display_path), lib.state(display_path)))
        .collect()
}

/// `view` narrowed to `filter`.
pub(crate) fn under(tab: TriageTab, filter: PendingFilter) -> ImportListView {
    ImportListView { pending_filter: filter, ..view(tab) }
}

/// `view` ordered by `order`.
pub(crate) fn ordered(tab: TriageTab, order: ImportListOrder) -> ImportListView {
    ImportListView { order, ..view(tab) }
}

/// The releases a fit of eleven tracks lists.
pub(crate) const FITS: (Option<&str>, Option<i64>) = (Some("listed"), Some(11));

/// Each entry's count of one read, in the menu's order.
pub(crate) fn entry_counts(entries: &[PendingFilterEntry]) -> Vec<(PendingFilter, u32)> {
    entries.iter().map(|entry| (entry.filter, entry.count)).collect()
}

/// Five releases, A to E, each with a state and nothing looked up.
pub(crate) async fn five_rows() -> Library {
    let lib = Library::new().await;
    lib.run(&scanned_with_states(&lib, &["A", "B", "C", "D", "E"])).await;
    lib
}

/// One scanned candidate under a fresh watched root.
pub(crate) async fn scanned(db: &Store, root: &std::path::Path, name: &str) -> FolderCandidate {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    let candidate = candidate(root, name);
    tables
        .save_folder_scan_item(root, generation, &ScanItem::Valid(candidate.clone()))
        .await
        .unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    candidate
}

pub(crate) fn verdict(
    release_id: &str,
    ledger: Option<bae_identify::IdentifyRunView>,
) -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: bae_identify::Findings {
            matches: vec![MetadataResult {
                source: LookupCatalog::MusicBrainz,
                release_id: release_id.to_string(),
                title: "Verdict Album".to_string(),
                artist: Some("Verdict Artist".to_string()),
                year: Some(1999),
                labels: Vec::new(),
                area: None,
                status: None,
                packaging: None,
                discogs_details: Vec::new(),
                barcodes: Vec::new(),
                media: bae_types::pressing::StatedMedia::Undescribed,
                links: Vec::new(),
                cover_art: None,
                source_group_id: Some("group-1".to_string()),
                album_links: bae_catalogs::album_links::AlbumLinks::NotAsked,
                source_tracks: Some(SourceTracks::Listed { count: 1 }),
                document_failure: None,
                album_first_year: None,
                track_titles: Vec::new(),
                notes: Vec::new(),
            }],
            pressings: vec![0],
            narrowed_out: bae_identify::NarrowedOut::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 1,
        ledger,
    }
}

pub(crate) async fn save_verdict(db: &Store, candidate: &FolderCandidate, release_id: &str) {
    save_verdict_with_ledger(db, candidate, release_id, None).await;
}

/// Store a verdict with its recorded ledger and settled signals.
pub(crate) async fn save_verdict_with_ledger(
    db: &Store,
    candidate: &FolderCandidate,
    release_id: &str,
    ledger: Option<bae_identify::IdentifyRunView>,
) {
    store_verdict(db, candidate, verdict(release_id, ledger)).await;
}

/// Store `verdict` with settled signals and no pick.
pub(crate) async fn store_verdict(
    db: &Store,
    candidate: &FolderCandidate,
    verdict: TerminalVerdict,
) {
    assert!(crate::CandidatePreparations::new(db.clone())
        .store_verdict(&NewImportCandidateVerdict {
            content_hash: candidate.files.content_hash(),
            file_edit_revision: 0,
            candidate_key: candidate.key(),
            verdict,
            signals: bae_identify::signals::Signals {
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
            },
            pick: None,
            releases: Vec::new(),
        })
        .await
        .unwrap());
}

pub(crate) fn musicbrainz_release(release_id: &str, title: &str) -> serde_json::Value {
    serde_json::json!({
        "id": release_id,
        "title": title,
        "artist-credit": [{ "name": "Picked Artist", "artist": { "id": "artist-1", "name": "Picked Artist" } }],
        "cover-art-archive": { "count": 0, "artwork": false, "front": false, "back": false, "darkened": false },
        "media": [{ "position": 1, "tracks": [
            { "id": "track-1", "position": 1, "number": "1", "title": "Track One",
              "recording": { "id": "rec-1", "title": "Track One" } }
        ] }],
    })
}

pub(crate) fn rows(projection: &crate::ImportListProjection) -> Vec<crate::TriageRow> {
    projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.clone()),
            _ => None,
        })
        .collect()
}

/// One read of a list: its view, its windows, and the rows it has selected.
pub(crate) struct ListRead {
    pub(crate) view: ImportListView,
    pub(crate) windows: bae_store::LibraryPageWindows,
    pub(crate) selected: Vec<crate::CandidateKey>,
}

/// The first fifty items of `tab`, nothing selected.
pub(crate) async fn request(tab: TriageTab) -> ListRead {
    ListRead {
        view: ImportListView { tab, ..ImportListView::default() },
        windows: std::iter::once(bae_store::LibraryPageWindow { offset: 0, limit: 50 }).collect(),
        selected: Vec::new(),
    }
}

impl ImportTables {
    /// `read` as a list opened for it shows it.
    pub(crate) async fn load_import_list_read(
        &self,
        read: ListRead,
    ) -> Result<crate::ImportListProjection, DbError> {
        let list = self.open_import_list(read.view).await?;
        if !read.selected.is_empty() {
            self.change_import_list_selection(
                list,
                crate::selection::SelectionChange::Replace { keys: read.selected },
            )
            .await?;
        }
        let projection =
            self.load_import_list(crate::ImportListRequest { list, windows: read.windows }).await;
        self.close_import_list(list).await?;
        projection
    }

    /// Every item a list shows, read from its tab's rows in order — Found's
    /// in runs — and another way: from the rows its search finds, or, on
    /// Found, its items sorted with each member in its header's place. The
    /// two reads, which a test holds to agree. `None` for a list on Done or
    /// Skipped that does not search, whose rows are only read in order.
    pub(crate) async fn import_list_read_both_ways(
        &self,
        list: ImportListId,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, DbError> {
        self.store
            .read(move |sql| {
                let row = list_row(&sql, list)?;
                let everything = bae_store::LibraryPageWindow { offset: 0, limit: u64::MAX };
                let keys = |shown| -> Result<Vec<String>, DbError> {
                    Ok(row
                        .items(shown)
                        .window(&sql, &everything, &Places::new())?
                        .0
                        .into_iter()
                        .map(|item| format!("{:?} {}", item.item, hex::encode(&item.key)))
                        .collect())
                };
                let other = match (row.filters_text, row.view.tab) {
                    (true, _) => keys(Shown::Found)?,
                    (false, crate::TriageTab::Pending) => sql
                        .query(
                            &format!(
                                "SELECT i.item, i.key FROM import_list_item AS i {join} \
                             WHERE i.list_id = ? AND i.tab = 'pending' ORDER BY {terms}",
                                join = order::unit_join("i", "unit"),
                                terms = order::sorted_terms(row.view.order, "i", "unit"),
                            ),
                            [list.0],
                            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
                        )?
                        .into_iter()
                        .map(|(item, key)| {
                            Ok(format!("{:?} {}", order::Item::of(&item)?, hex::encode(key)))
                        })
                        .collect::<Result<_, DbError>>()?,
                    (false, _) => return Ok(None),
                };
                Ok(Some((keys(Shown::View)?, other)))
            })
            .await
    }
}

/// The import list's request carries the view and the windows, so every test
/// below states both.
/// A list opened on `tab` in path order, read through `windows`.
pub(crate) async fn list_request(
    db: &Store,
    tab: crate::TriageTab,
    windows: impl IntoIterator<Item = (u64, u64)>,
) -> crate::ImportListRequest {
    let tables = crate::tables::ImportTables::new(db.clone());
    let list = tables
        .open_import_list(crate::ImportListView {
            tab,
            order: crate::ImportListOrder::PathAscending,
            ..crate::ImportListView::default()
        })
        .await
        .unwrap();
    through(list, windows)
}

/// What an import here records beside the release it writes: the folder hash
/// of the files it read.
pub(crate) async fn record_import(db: &Store, content_hash: &str) {
    exec(
        db,
        "INSERT INTO local_release_origin (release_id, content_hash) VALUES (?1, ?2)",
        &[RELEASE_ID, content_hash],
    )
    .await;
}

/// Store `name`'s candidate under `root` as one finished scan of it, and
/// return it.
pub(crate) async fn scan_finished(
    db: &Store,
    root: &std::path::Path,
    name: &str,
) -> crate::folder_scanner::FolderCandidate {
    let tables = crate::tables::ImportTables::new(db.clone());
    let item = scan_candidate(root, name);
    let crate::folder_scanner::ScanItem::Valid(candidate) = &item else {
        unreachable!("the fixture builds a valid candidate");
    };
    let candidate = candidate.clone();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(root), crate::VolumeKind::Local)
        .await
        .unwrap();
    tables.save_folder_scan_item(std::path::Path::new(root), generation, &item).await.unwrap();
    tables.finish_folder_scan(std::path::Path::new(root), generation, None).await.unwrap();
    candidate
}

/// The list `list` read through `windows`.
pub(crate) fn through(
    list: crate::ImportListId,
    windows: impl IntoIterator<Item = (u64, u64)>,
) -> crate::ImportListRequest {
    crate::ImportListRequest {
        list,
        windows: windows
            .into_iter()
            .map(|(offset, limit)| bae_store::LibraryPageWindow { offset, limit })
            .collect(),
    }
}
