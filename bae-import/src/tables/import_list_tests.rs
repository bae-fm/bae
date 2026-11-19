//! The import list's shared test fixtures: a library a test writes through
//! the import's own writes, as the placement's triggers and the list's reads
//! take them, and the list read the way a surface reads it; and candidates
//! scanned and looked up through the import tables' own writes.

use super::*;
use crate::folder_scanner::{FolderCandidate, ScanItem};
use crate::list::ImportListRead;
use crate::tables::folder_scans::{ScanItemSeed, ScanItemToWrite};
use crate::tables::tests::*;
use crate::FolderReleaseDecisionKey;
use crate::PendingFilterEntry;
use crate::{
    ImportListItem, ImportListOrder, ImportListRequest, ImportListView, PendingFilter, TriageTab,
};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SourceTracks};
use bae_identify::TerminalVerdict;
use bae_store::test_fixtures::{empty_db, exec_batch};

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

    pub(crate) fn tables(&self) -> ImportTables {
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

    /// Run `write`, and check the search index and the counts kept beside
    /// the placement still say what the placement holds: every write here
    /// moves placement rows, and each must move the searched text and the
    /// counts with them.
    pub(crate) async fn run(&self, write: Write) {
        for step in write.0 {
            step.await;
        }
        self.searchable().await;
        self.counted().await;
    }

    /// Check the counts kept beside the placement (`import_list_count`) hold,
    /// for each tab, item and filter entry, what a COUNT over every placement
    /// row says, and hold nothing else; and that the group counts
    /// (`import_list_group_count`) hold, for each group and filter entry, what
    /// a COUNT over the group's members says, and hold nothing else.
    pub(crate) async fn counted(&self) {
        assert_eq!(
            self.stored_counts().await,
            self.count_rows(
                "SELECT tab, item, pending_entry, COUNT(*) FROM import_list_placement \
                 GROUP BY tab, item, pending_entry ORDER BY tab, item, pending_entry",
            )
            .await,
            "the counts kept beside the placement"
        );
        assert_eq!(
            self.stored_group_counts().await,
            self.count_rows(
                "SELECT coven_path_text(watched_folder_path), coven_path_text(group_folder), \
                        pending_entry, COUNT(*) \
                 FROM import_list_placement WHERE item <> 'group' AND group_folder IS NOT NULL \
                 GROUP BY watched_folder_path, group_folder, pending_entry ORDER BY 1, 2, 3",
            )
            .await,
            "the group counts kept beside the placement"
        );
    }

    /// The group counts kept beside the placement as they stand, in order.
    pub(crate) async fn stored_group_counts(&self) -> Vec<GroupCountRow> {
        self.count_rows(
            "SELECT coven_path_text(watched_folder_path), coven_path_text(group_folder), \
                    pending_entry, count \
             FROM import_list_group_count ORDER BY 1, 2, 3",
        )
        .await
    }

    /// The counts kept beside the placement as they stand, in order.
    pub(crate) async fn stored_counts(&self) -> Vec<CountRow> {
        self.count_rows(
            "SELECT tab, item, pending_entry, count FROM import_list_count \
             ORDER BY tab, item, pending_entry",
        )
        .await
    }

    /// The rows `statement` selects, each three columns that say what it
    /// counts (a tab, an item and a filter entry; or a watched folder, a
    /// group and a filter entry) and a count.
    async fn count_rows<A, B, C>(&self, statement: &'static str) -> Vec<(A, B, C, i64)>
    where
        A: rusqlite::types::FromSql + Send + 'static,
        B: rusqlite::types::FromSql + Send + 'static,
        C: rusqlite::types::FromSql + Send + 'static,
    {
        self.db
            .read(move |sql| {
                Ok(sql.query(statement, [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?)
            })
            .await
            .unwrap()
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

    /// One read of `list` through one window over every item, with nothing
    /// selected. Found's items read in runs and read sorted must agree.
    pub(crate) async fn read(&self, list: &ImportListRequest) -> ImportListRead {
        self.read_selecting(list, Vec::new()).await
    }

    /// One read of `list` through one window over every item, with what the
    /// rows at `selected` hold and offer. Found's items read in runs and read
    /// sorted must agree.
    pub(crate) async fn read_selecting(
        &self,
        list: &ImportListRequest,
        selected: Vec<crate::CandidateKey>,
    ) -> ImportListRead {
        let tables = self.tables();
        let read = tables.read_import_list_for_test(list.clone(), selected).await.unwrap();
        if let Some((walked, other)) =
            tables.import_list_read_both_ways(list.clone()).await.unwrap()
        {
            assert_eq!(
                walked, other,
                "{:?}: the tab's rows in order against its other read",
                list.view
            );
        }
        read
    }

    /// The window of `limit` items from the item at `offset`, read as a view
    /// reads it after a jump: the key at the offset, then the window from
    /// it.
    pub(crate) async fn window_at(
        &self,
        list: &ImportListRequest,
        offset: u64,
        limit: u64,
    ) -> crate::ImportWindow {
        let tables = self.tables();
        let key =
            tables.import_key_at(list.clone(), offset).await.unwrap().expect("the list has items");
        tables
            .read_import_window(list.clone(), bae_store::ListWindow { from: Some(key), limit })
            .await
            .unwrap()
    }

    /// One read of a list opened on `view`.
    pub(crate) async fn shown(&self, view: ImportListView) -> ImportListRead {
        let list = ImportListRequest::of(view);
        self.read(&list).await
    }

    /// The items a list opened on `view` shows.
    pub(crate) async fn items(&self, view: ImportListView) -> Vec<String> {
        sequence(&self.shown(view).await)
    }

    /// The group at `name` under the watched folder.
    pub(crate) fn group_key(&self, name: &str) -> FolderReleaseDecisionKey {
        FolderReleaseDecisionKey {
            watched_folder_path: self.root.clone(),
            relative_folder: crate::RelativeFolder::test(name),
        }
    }
}

/// One count of the placement's rows: their tab, their item, their entry of
/// Found's filter when they have one, and how many there are.
pub(crate) type CountRow = (String, String, Option<String>, i64);

/// One count of a group's members: the group's watched folder and folder,
/// their entry of Found's filter, and how many there are.
pub(crate) type GroupCountRow = (String, String, String, i64);

/// `tab` in the default order, unfiltered.
pub(crate) fn view(tab: TriageTab) -> ImportListView {
    ImportListView { tab, ..ImportListView::default() }
}

impl ImportListRequest {
    /// This request with `group` folded shut.
    pub(crate) fn folding(mut self, group: FolderReleaseDecisionKey) -> Self {
        match &mut self.folds {
            ImportListFolds::Only(folded) => {
                folded.insert(group);
            }
            ImportListFolds::AllBut(open) => {
                open.remove(&group);
            }
        }
        self
    }

    /// This request with `group` open.
    pub(crate) fn unfolding(mut self, group: &FolderReleaseDecisionKey) -> Self {
        match &mut self.folds {
            ImportListFolds::Only(folded) => {
                folded.remove(group);
            }
            ImportListFolds::AllBut(open) => {
                open.insert(group.clone());
            }
        }
        self
    }

    /// This request with every group folded shut, those Found gains later
    /// included.
    pub(crate) fn folding_all(self) -> Self {
        Self { folds: ImportListFolds::AllBut(Default::default()), ..self }
    }
}

/// The items one read holds, one readable line each: a header by its
/// group's name, a release or a folder that failed validation by its folder
/// below its watched folder.
pub(crate) fn sequence(projection: &ImportListRead) -> Vec<String> {
    projection.windows.iter().flat_map(|window| items_sequence(&unkeyed(&window.rows))).collect()
}

/// A window's items without their keys.
pub(crate) fn unkeyed(
    rows: &[bae_store::list_windows::Keyed<ImportListItem>],
) -> Vec<ImportListItem> {
    rows.iter().map(|(item, _key)| item.clone()).collect()
}

/// `items`, one readable line each, as [`sequence`] gives them.
pub(crate) fn items_sequence(items: &[ImportListItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            ImportListItem::GroupHeader(crate::ImportGroupHeader { group, .. }) => {
                format!("group {}", group.name)
            }
            ImportListItem::Candidate { row, .. } => format!("candidate {}", display_path(row)),
            ImportListItem::Invalid { row, .. } => {
                format!("invalid {}", shown_folder(&row.folder, &row.watched_folder_path))
            }
        })
        .collect()
}

/// The folder of a release that bae's scan read from one folder, below its
/// watched folder, as the list's statements show it. A grouping's row names
/// no one folder, so these tests read only releases that bae's scan read from
/// one folder.
pub(crate) fn display_path(row: &crate::TriageRow) -> String {
    let folder = coven::path_from_bytes(row.candidate_key.as_bytes().to_vec())
        .unwrap_or_else(|_| panic!("{} is a folder's key", row.candidate_key));
    shown_folder(&folder, &row.watched_folder_path)
}

/// `folder` below `watched_folder_path`, as the list's statements show it.
pub(crate) fn shown_folder(
    folder: &std::path::Path,
    watched_folder_path: &std::path::Path,
) -> String {
    coven::path_text(folder.strip_prefix(watched_folder_path).unwrap_or_else(|_| {
        panic!("{} is below {}", folder.display(), watched_folder_path.display())
    }))
}

/// The release row one read holds at `display_path`.
pub(crate) fn row_at<'a>(
    projection: &'a ImportListRead,
    display_path: &str,
) -> (&'a crate::TriageRow, &'a crate::CandidateLiveState) {
    projection
        .windows
        .iter()
        .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
        .find_map(|item| match item {
            ImportListItem::Candidate { row, live, .. }
                if self::display_path(row) == display_path =>
            {
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
                artist: Some(bae_types::credits::CreditLine::of_name("Verdict Artist")),
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

pub(crate) fn rows(projection: &ImportListRead) -> Vec<crate::TriageRow> {
    projection
        .windows
        .iter()
        .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
        .filter_map(|item| match item {
            ImportListItem::Candidate { row, .. } => Some(row.clone()),
            _ => None,
        })
        .collect()
}

/// The release rows one window holds.
pub(crate) fn window_rows(window: &crate::ImportWindow) -> Vec<crate::TriageRow> {
    window
        .rows
        .iter()
        .filter_map(|(item, _key)| match item {
            ImportListItem::Candidate { row, .. } => Some(row.clone()),
            _ => None,
        })
        .collect()
}

/// One read of a list: its view, and the rows its surface keeps selected.
pub(crate) struct ListRead {
    pub(crate) view: ImportListView,
    pub(crate) selected: Vec<crate::CandidateKey>,
}

/// `tab`, nothing selected.
pub(crate) async fn request(tab: TriageTab) -> ListRead {
    ListRead { view: ImportListView { tab, ..ImportListView::default() }, selected: Vec::new() }
}

impl ImportTables {
    /// `read` as a list opened for it shows it.
    pub(crate) async fn load_import_list_read(
        &self,
        read: ListRead,
    ) -> Result<ImportListRead, DbError> {
        self.read_import_list_for_test(ImportListRequest::of(read.view), read.selected).await
    }

    /// Every item a list shows, read from its tab's rows in order — Found's
    /// in runs — and another way: on Found, its items sorted with each member
    /// in its header's place. The function returns both reads, and a test
    /// checks that they agree. The function returns `None` for a list on Done
    /// or Skipped, because the list reads those rows only in order.
    pub(crate) async fn import_list_read_both_ways(
        &self,
        list: ImportListRequest,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, DbError> {
        self.store
            .read(move |sql| {
                let items = order::Items::new(list.clone());
                let everything = bae_store::ListWindow::top(u64::MAX);
                let keys = || -> Result<Vec<String>, DbError> {
                    Ok(items
                        .range(&sql, &everything)?
                        .into_iter()
                        .map(|item| format!("{:?} {}", item.item, hex::encode(&item.key)))
                        .collect())
                };
                let other = match list.view.tab {
                    crate::TriageTab::Pending => items
                        .query(
                            &sql,
                            &format!(
                                "SELECT i.item, i.key FROM {source} AS i {join} \
                             WHERE i.tab = 'pending' ORDER BY {terms}",
                                source = list.items(),
                                join = order::unit_join("i", "unit"),
                                terms = order::sorted_terms(list.view.order, "i", "unit"),
                            ),
                            &[],
                            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
                        )?
                        .into_iter()
                        .map(|(item, key)| {
                            Ok(format!("{:?} {}", order::Item::of(&item)?, hex::encode(key)))
                        })
                        .collect::<Result<_, DbError>>()?,
                    crate::TriageTab::Done | crate::TriageTab::Skipped => return Ok(None),
                };
                Ok(Some((keys()?, other)))
            })
            .await
    }
}

/// The request of a list on `tab` in path order.
pub(crate) fn in_path_order(tab: crate::TriageTab) -> ImportListRequest {
    ImportListRequest::of(crate::ImportListView {
        tab,
        order: crate::ImportListOrder::PathAscending,
        ..crate::ImportListView::default()
    })
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
