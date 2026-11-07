//! The import list's shared test fixtures: a library a test writes through
//! the import's own writes, as the placement's triggers and the list's reads
//! take them, and the list read the way a surface reads it; and candidates
//! scanned and looked up through the import tables' own writes.

use super::*;
use crate::folder_scanner::{FolderCandidate, ScanItem};
use crate::selection::SelectionChange;
use crate::tables::folder_scans::{ScanItemSeed, ScanItemToWrite};
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
use bae_store::test_fixtures::{empty_db, exec_batch};
use bae_store::LibraryPageWindow;

#[path = "import_list_tests/preparations.rs"]
mod preparations;
#[path = "import_list_tests/scans.rs"]
mod scans;
#[path = "import_list_tests/standing.rs"]
mod standing;
pub(crate) use preparations::*;
pub(crate) use standing::*;

/// One step a test writes: a call into the import's own writes, run when the
/// write it belongs to runs.
type Step = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

/// What a test writes, through the writes the import makes: each step in
/// order, and nothing until [`Library::run`] or [`Library::moved`] runs it.
#[must_use = "a write does nothing until it runs"]
pub(crate) struct Write(Vec<Step>);

impl Write {
    fn step(step: impl std::future::Future<Output = ()> + Send + 'static) -> Self {
        Self(vec![Box::pin(step)])
    }

    /// The write that writes nothing.
    pub(crate) fn nothing() -> Self {
        Self(Vec::new())
    }
}

impl std::ops::Add for Write {
    type Output = Write;

    fn add(mut self, then: Write) -> Write {
        self.0.extend(then.0);
        self
    }
}

impl std::iter::Sum for Write {
    fn sum<I: Iterator<Item = Write>>(writes: I) -> Write {
        writes.fold(Write::nothing(), |all, write| all + write)
    }
}

/// A database with one watched folder, its scan begun, and helpers that make
/// each write a test needs as the import makes it.
pub(crate) struct Library {
    pub(crate) db: bae_store::Store,
    pub(crate) _tmp: Option<tempfile::TempDir>,
    pub(crate) root: std::path::PathBuf,
}

impl Library {
    pub(crate) async fn new() -> Self {
        let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
        let mut library = Self::on(db).await;
        library._tmp = Some(tmp);
        library
    }

    /// The library over `db`, a device's store, its watched folder added and
    /// its scan begun.
    pub(crate) async fn on(db: bae_store::Store) -> Self {
        let root = crate::tables::watched_path::host_root("/music");
        let tables = ImportTables::new(db.clone());
        tables.add_watched_import_folder(&root).await.unwrap();
        tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
        Self { db, _tmp: None, root }
    }

    /// The library's records, which an import writes its releases into.
    pub(crate) fn records(&self) -> bae_records::Records {
        bae_records::test_fixtures::test_records(&self.db)
    }

    fn tables(&self) -> ImportTables {
        ImportTables::new(self.db.clone())
    }

    /// How many entries each tab holds now.
    pub(crate) async fn tab_counts(&self) -> crate::TriageTabCounts {
        self.tables().load_tab_counts().await.unwrap()
    }

    fn preparations(&self) -> crate::tables::ImportTables {
        crate::tables::ImportTables::new(self.db.clone())
    }

    /// The folder at `display_path` below the watched folder.
    pub(crate) fn path(&self, display_path: &str) -> std::path::PathBuf {
        folder_below(&self.root, display_path)
    }

    /// The key of the release scanned at `display_path`.
    pub(crate) fn key(&self, display_path: &str) -> crate::CandidateKey {
        crate::CandidateKey::folder(&self.path(display_path))
    }

    /// The folder `relative` below the watched folder.
    pub(crate) fn folder(&self, relative: &std::path::Path) -> std::path::PathBuf {
        crate::tables::watched_path::folder_below(
            &self.root,
            &crate::RelativeFolder::of(relative).unwrap(),
        )
    }

    /// The release the scan reads at the folder `relative` below the watched
    /// folder: one audio file, named for the folder so that each folder's
    /// files hash apart.
    pub(crate) fn candidate_at(&self, relative: &std::path::Path) -> FolderCandidate {
        self.candidate_in(&self.root, relative)
    }

    /// The release the scan reads at the folder `relative` below the watched
    /// folder `root`.
    fn candidate_in(&self, root: &std::path::Path, relative: &std::path::Path) -> FolderCandidate {
        let path = crate::tables::watched_path::folder_below(
            root,
            &crate::RelativeFolder::of(relative).unwrap(),
        );
        let file_name = format!("{}.flac", hex::encode(coven::path_bytes(relative)));
        FolderCandidate {
            path: path.clone(),
            file_root: path.clone(),
            name: coven::path_text(std::path::Path::new(path.file_name().unwrap())),
            files: crate::folder_scanner::CategorizedFiles {
                files: vec![crate::folder_scanner::CandidateFile {
                    file: crate::folder_scanner::ScannedFile::new(
                        path.join(&file_name),
                        file_name,
                        1_000,
                        1,
                    )
                    .with_test_flac_audio(),
                    role: crate::folder_scanner::FileRole::Audio,
                }],
                parts: Vec::new(),
            },
            watched_folder_path: root.to_path_buf(),
            scope: crate::folder_scanner::ReleaseFileScope::Direct,
            file_edit_revision: 0,
            relative_folder: crate::RelativeFolder::of(relative).unwrap(),
            grouping: None,
        }
    }

    /// The release the scan reads at `display_path`.
    pub(crate) fn candidate(&self, display_path: &str) -> FolderCandidate {
        self.candidate_at(std::path::Path::new(display_path))
    }

    /// The release the scan reads at `display_path`, as a pass that read its
    /// file decisions at `revision` stores it.
    pub(crate) fn scan_revised(&self, display_path: &str, revision: u64) -> Write {
        let candidate =
            FolderCandidate { file_edit_revision: revision, ..self.candidate(display_path) };
        self.store(crate::folder_scanner::ScanItem::Valid(candidate), None)
    }

    /// The hash of the files of the release at `display_path`.
    pub(crate) fn hash(&self, display_path: &str) -> String {
        self.candidate(display_path).files.content_hash()
    }

    /// The folder at `relative` as the scan reads it when it fails
    /// validation: one empty track, and what its tags call it, if anything.
    pub(crate) fn invalid_at(
        &self,
        relative: &std::path::Path,
        tags: Option<crate::folder_scanner::SkippedTags>,
    ) -> crate::folder_scanner::InvalidCandidate {
        let candidate = self.candidate_at(relative);
        crate::folder_scanner::InvalidCandidate {
            files: vec![crate::folder_scanner::SkippedFile::empty_track(
                &candidate.path,
                "01.flac",
            )],
            path: candidate.path,
            name: candidate.name,
            watched_folder_path: candidate.watched_folder_path,
            relative_folder: candidate.relative_folder,
            grouping: None,
            tags,
        }
    }

    /// Run `write` alone, and return the SQLite steps its app writes ran,
    /// their triggers' included: what a test of those triggers' cost
    /// measures.
    pub(crate) async fn write_steps(&self, write: Write) -> usize {
        let counter = self.db.track_work_for_test().await.unwrap();
        for step in write.0 {
            step.await;
        }
        counter.take().app_write_steps
    }

    /// Run `write`, and check the counts and the search index kept beside the
    /// placement still say what it holds: every write here moves placement
    /// rows, and each must move the counts and the searched text with them.
    pub(crate) async fn run(&self, write: Write) {
        for step in write.0 {
            step.await;
        }
        self.searchable().await;
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
            assert_eq!(kept, counted, "the counts kept beside the placement");
        }
    }

    /// Check the search index holds each release's folded shown text under
    /// its placement row's number, and nothing else; and that every header
    /// says whether its folder is offered to be read as one release.
    pub(crate) async fn searchable(&self) {
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
        assert_eq!(indexed, placed, "the search index");
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
        assert!(unjudged.is_empty(), "headers saying nothing of combining: {unjudged:?}");
        exec_batch(
            &self.db,
            "INSERT INTO import_list_search (import_list_search, rank) VALUES ('integrity-check', 1);",
        )
        .await;
    }

    /// Open a list showing `view`.
    pub(crate) async fn open(&self, view: ImportListView) -> ImportListId {
        self.tables().open_import_list(view).await.unwrap()
    }

    /// One read of `list` through `window`. A searching or folded list's
    /// items read from its tab's rows and by its other read must agree, since
    /// its windows are read by either.
    pub(crate) async fn read_through(
        &self,
        list: ImportListId,
        window: LibraryPageWindow,
    ) -> ImportListProjection {
        let tables = self.tables();
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
        match self.tables().change_import_list_selection(list, change).await.unwrap() {
            crate::tables::SelectionWrite::Changed { revision } => revision,
            crate::tables::SelectionWrite::RunNotShown => {
                panic!("the run's ends are shown")
            }
        }
    }

    /// The group at `name` under the watched folder.
    pub(crate) fn group_key(&self, name: &str) -> FolderReleaseDecisionKey {
        FolderReleaseDecisionKey {
            watched_folder_path: self.root.clone(),
            relative_folder: crate::RelativeFolder::test(name),
        }
    }
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
        .flat_map(|window| &window.rows)
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
        .flat_map(|window| &window.rows)
        .find_map(|item| match item {
            ImportListItem::Candidate { row, live, .. } if row.display_path == display_path => {
                Some((row, live))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{display_path} is a row of the read"))
}

/// The candidate releases of `display_paths`, each scanned, which stores its
/// state with a blank draft.
pub(crate) fn scanned_with_states(lib: &Library, display_paths: &[&str]) -> Write {
    display_paths.iter().map(|display_path| lib.scan(display_path)).sum()
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
    lib.run(scanned_with_states(&lib, &["A", "B", "C", "D", "E"])).await;
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
    assert!(crate::tables::ImportTables::new(db.clone())
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
        .flat_map(|window| &window.rows)
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
    ) -> Result<crate::ImportListProjection, ImportListError> {
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
                let row = list_row(&sql, list)?.expect("the test's list is open");
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
