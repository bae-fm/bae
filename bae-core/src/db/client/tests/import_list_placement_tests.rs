//! The import list's placement table against the tables it is kept from: each
//! write to one of them moves exactly the placement rows it should, in that
//! write.

use super::{empty_db, exec_batch, folder_below};
use std::collections::BTreeMap;

mod candidates;
mod groupings;
mod groups;
mod list_reads;
mod scans;
mod verdicts;

/// A placement row's columns, past its key.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
    root_position: i64,
    tab: String,
    pending_state: Option<String>,
    needs_you_reason: Option<String>,
    group_name: Option<String>,
    own_date: Option<i64>,
    unit_date: Option<i64>,
    display_path: Option<String>,
    shown_title: Option<String>,
    shown_artists: Option<String>,
}

/// A placement row's key: its watched folder, what it is, and the release's
/// key or the group's folder.
type Item = (String, &'static str, String);

/// What a write did to the placement: each row it changed, as it now stands,
/// or `None` for one it took out.
type Moved = BTreeMap<Item, Option<Placed>>;

/// A release on Found nobody has looked up, as its scan alone places it.
fn found(display_path: &str) -> Placed {
    Placed {
        root_position: 0,
        tab: "pending".to_string(),
        pending_state: Some("not_looked_up".to_string()),
        needs_you_reason: None,
        group_name: None,
        own_date: None,
        unit_date: None,
        display_path: Some(display_path.to_string()),
        shown_title: Some(last_component(display_path).to_string()),
        shown_artists: None,
    }
}

/// A folder that failed validation, which only Skipped lists.
fn failed_validation(display_path: &str) -> Placed {
    Placed {
        tab: "skipped".to_string(),
        pending_state: None,
        ..found(display_path)
    }
}

/// The header of the group at `name`, dated `date`.
fn header(name: &str, date: Option<i64>) -> Placed {
    Placed {
        root_position: 0,
        tab: "pending".to_string(),
        pending_state: None,
        needs_you_reason: None,
        group_name: Some(name.to_string()),
        own_date: None,
        unit_date: date,
        display_path: None,
        shown_title: None,
        shown_artists: None,
    }
}

fn last_component(display_path: &str) -> &str {
    display_path.rsplit('/').next().unwrap_or(display_path)
}

impl Placed {
    /// On `tab`, where only Found holds a state.
    fn on(self, tab: &str) -> Self {
        Self {
            tab: tab.to_string(),
            pending_state: (tab == "pending").then(|| "not_looked_up".to_string()),
            needs_you_reason: None,
            ..self
        }
    }

    fn state(self, state: &str) -> Self {
        Self {
            pending_state: Some(state.to_string()),
            needs_you_reason: None,
            ..self
        }
    }

    /// Needing the person, for `reason`.
    fn needs_you(self, reason: &str) -> Self {
        Self {
            pending_state: Some("needs_you".to_string()),
            needs_you_reason: Some(reason.to_string()),
            ..self
        }
    }

    /// Dated `date` and sorting by it.
    fn dated(self, date: i64) -> Self {
        Self {
            own_date: Some(date),
            unit_date: Some(date),
            ..self
        }
    }

    /// Listed under the group at `name`, sorting in its header's place.
    fn grouped(self, name: &str) -> Self {
        Self {
            group_name: Some(name.to_string()),
            unit_date: None,
            ..self
        }
    }

    fn shown(self, title: &str, artists: Option<&str>) -> Self {
        Self {
            shown_title: Some(title.to_string()),
            shown_artists: artists.map(str::to_string),
            ..self
        }
    }

    fn at_root(self, root_position: i64) -> Self {
        Self {
            root_position,
            ..self
        }
    }
}

/// A database with one watched folder, its scan begun, and helpers that spell
/// the rows each test writes.
struct Library {
    db: crate::db::Database,
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

impl Library {
    async fn new() -> Self {
        let (db, tmp) = empty_db().await;
        let root = crate::import::watched_folder::host_root("/music");
        let library = Self {
            db,
            _tmp: tmp,
            root,
        };
        library.run(&library.watch(&library.root, 0)).await;
        library
    }

    /// Watch `root` at `position`, its scan begun.
    fn watch(&self, root: &std::path::Path, position: i64) -> String {
        format!(
            "INSERT INTO watched_import_folders (path, position) VALUES ({root}, {position});
             INSERT INTO folder_scan_roots (watched_folder_path, generation, status, volume)
             VALUES ({root}, 1, 'scanning', 'local');",
            root = sql_blob(&coven::path_bytes(root)),
        )
    }

    /// The folder at `display_path` below the watched folder.
    fn path(&self, display_path: &str) -> std::path::PathBuf {
        folder_below(&self.root, display_path)
    }

    /// The key of the release scanned at `display_path`.
    fn key(&self, display_path: &str) -> crate::import::CandidateKey {
        crate::import::CandidateKey::folder(&self.path(display_path))
    }

    /// The folder at `display_path` as a SQL literal of its bytes.
    fn sql(&self, display_path: &str) -> String {
        sql_blob(&coven::path_bytes(&self.path(display_path)))
    }

    /// The watched folder as a SQL literal of its bytes.
    fn sql_root(&self) -> String {
        sql_blob(&coven::path_bytes(&self.root))
    }

    fn release(&self, display_path: &str) -> Item {
        (self.root_text(), "candidate", self.text(display_path))
    }

    /// The release at the folder `relative` below the watched folder.
    fn release_at(&self, relative: &std::path::Path) -> Item {
        (
            self.root_text(),
            "candidate",
            coven::path_text(&self.folder(relative)),
        )
    }

    fn invalid(&self, display_path: &str) -> Item {
        (self.root_text(), "invalid", self.text(display_path))
    }

    fn group(&self, name: &str) -> Item {
        (self.root_text(), "group", name.to_string())
    }

    fn root_text(&self) -> String {
        coven::path_text(&self.root)
    }

    /// The text of the folder at `display_path`, as a placement row's key
    /// reads back.
    fn text(&self, display_path: &str) -> String {
        coven::path_text(&self.path(display_path))
    }

    /// Store the scan's release at `display_path` as `kind`.
    fn scan_as(&self, display_path: &str, kind: &str) -> String {
        self.scan_folder(std::path::Path::new(display_path), kind)
    }

    /// Store the scan's release at the folder `relative` below the watched
    /// folder as `kind`.
    fn scan_folder(&self, relative: &std::path::Path, kind: &str) -> String {
        let folder = self.folder(relative);
        let relative = crate::import::RelativeFolder::of(relative).unwrap();
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
    fn folder(&self, relative: &std::path::Path) -> std::path::PathBuf {
        crate::import::relative_folder::folder_below(
            &self.root,
            &crate::import::RelativeFolder::of(relative).unwrap(),
        )
    }

    /// Store the scan's settled release at `display_path`.
    fn scan(&self, display_path: &str) -> String {
        self.scan_as(display_path, "valid")
    }

    /// Record when the scans first saw the folder at `display_path`.
    fn discovered(&self, display_path: &str, first_seen_at: i64) -> String {
        format!(
            "INSERT INTO folder_discovery (folder, first_seen_at, settled)
             VALUES ({}, {first_seen_at}, 1);",
            self.sql(display_path)
        )
    }

    /// The release's state at its files' current revision, with a blank
    /// draft a person owns.
    fn state(&self, display_path: &str) -> String {
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
    fn draft(&self, display_path: &str, title: &str) -> String {
        format!(
            "UPDATE import_candidate_edit SET album_title = '{title}', draft_blank = 0
             WHERE content_hash = '{}';",
            hash(display_path)
        )
    }

    /// A stored verdict of `kind` over a folder of `track_count` tracks.
    fn verdict(&self, display_path: &str, kind: &str, track_count: Option<i64>) -> String {
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
                 (content_hash, kind, track_count, failures_json, error_detail, kept_own_draft,
                  identified_at)
             VALUES ('{}', '{kind}', {}, {failures}, {error}, 0, '2026-01-01T00:00:00Z');",
            hash(display_path),
            track_count.map_or("NULL".to_string(), |count| count.to_string()),
        )
    }

    /// One release the verdict found, in pressing row `pressing`, listing
    /// `source_tracks` (its kind and count).
    fn found_release(
        &self,
        display_path: &str,
        position: i64,
        pressing: i64,
        source_tracks: (Option<&str>, Option<i64>),
    ) -> String {
        let (kind, count) = source_tracks;
        format!(
            "INSERT INTO import_candidate_match
                 (content_hash, position, pressing, source, release_id, title, media_kind,
                  album_links, source_tracks_kind, source_tracks_count, by_disc_id, by_barcode,
                  by_catalog, by_isrc, by_search, by_pressing)
             VALUES ('{}', {position}, {pressing}, 'musicbrainz', 'mb-{position}', 'Album',
                     'undescribed', 'not_asked', {}, {}, 1, 0, 0, 0, 0, 0);",
            hash(display_path),
            kind.map_or("NULL".to_string(), |kind| format!("'{kind}'")),
            count.map_or("NULL".to_string(), |count| count.to_string()),
        )
    }

    /// Run `sql`, and check the counts and the search index kept beside the
    /// placement still say what it holds: every write here moves placement
    /// rows, and each must move the counts and the searched text with them.
    async fn run(&self, sql: &str) {
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
            assert_eq!(
                kept, counted,
                "the counts kept beside the placement after {sql}"
            );
        }
    }

    /// Check the search index holds each release's folded shown text under
    /// its placement row's number, and nothing else; and that every header
    /// says whether its folder is offered to be read as one release.
    async fn searchable(&self, after: &str) {
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

    /// Every placement row as it stands.
    async fn placed(&self) -> BTreeMap<Item, Placed> {
        self.db
            .read(|sql| {
                let rows = sql.query(
                    "SELECT coven_path_text(watched_folder_path), item, coven_path_text(key),
                            root_position, tab, pending_state,
                            needs_you_reason, group_name, own_date, unit_date, display_path,
                            shown_title, shown_artists
                     FROM import_list_placement",
                    [],
                    |row| {
                        let item = match row.get::<_, String>(1)?.as_str() {
                            "candidate" => "candidate",
                            "invalid" => "invalid",
                            "group" => "group",
                            other => panic!("placement row of unknown item {other}"),
                        };
                        Ok((
                            (row.get::<_, String>(0)?, item, row.get::<_, String>(2)?),
                            Placed {
                                root_position: row.get(3)?,
                                tab: row.get(4)?,
                                pending_state: row.get(5)?,
                                needs_you_reason: row.get(6)?,
                                group_name: row.get(7)?,
                                own_date: row.get(8)?,
                                unit_date: row.get(9)?,
                                display_path: row.get(10)?,
                                shown_title: row.get(11)?,
                                shown_artists: row.get(12)?,
                            },
                        ))
                    },
                )?;
                Ok(rows.into_iter().collect())
            })
            .await
            .unwrap()
    }

    /// What running `sql` did to the placement.
    async fn moved(&self, sql: &str) -> Moved {
        let before = self.placed().await;
        self.run(sql).await;
        let after = self.placed().await;
        before
            .keys()
            .chain(after.keys())
            .filter(|item| before.get(*item) != after.get(*item))
            .map(|item| (item.clone(), after.get(item).cloned()))
            .collect()
    }
}

/// `bytes` as a SQL BLOB literal.
fn sql_blob(bytes: &[u8]) -> String {
    format!("X'{}'", hex::encode(bytes))
}

/// The folder at `display_path` below a watched folder as a SQL literal of its
/// `RelativeFolder` bytes.
fn sql_relative(display_path: &str) -> String {
    sql_blob(crate::import::RelativeFolder::test(display_path).as_bytes())
}

/// The content hash the fixtures give the release at `display_path`.
fn hash(display_path: &str) -> String {
    format!("hash-{display_path}")
}

/// The moves `rows` name, as a test spells them.
fn moves<const N: usize>(rows: [(Item, Option<Placed>); N]) -> Moved {
    rows.into_iter().collect()
}
