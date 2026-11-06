//! The scans a test writes: passes over the watched folders and what they
//! read, the folders' readings and groupings, and the person's skips.

use super::*;

impl Library {
    /// Store `item` as the scan open on the watched folder reads it, its
    /// folder dated `date` by the filesystem, or by when the scans first saw
    /// it with `None`.
    pub(crate) fn store(
        &self,
        item: crate::folder_scanner::ScanItem,
        date: Option<crate::folder_scanner::FolderDate>,
    ) -> Write {
        self.store_all(vec![ScanItemToWrite {
            item,
            seed: ScanItemSeed::Tags(None),
            folder_date: date,
        }])
    }

    /// Store `items` in one batch of the scan open on the watched folder.
    fn store_all(&self, items: Vec<ScanItemToWrite>) -> Write {
        let tables = self.tables();
        let root = self.root.clone();
        Write::step(async move {
            let generation = open_generation(&tables, &root).await;
            save_entries(&tables, &root, generation, items).await;
        })
    }

    /// The scan reading the releases at `display_paths` in one batch, each
    /// folder dated by the filesystem at its `Some` date, or by when the
    /// scans first saw it.
    pub(crate) fn scan_batch(
        &self,
        display_paths: impl IntoIterator<Item = (String, Option<i64>)>,
    ) -> Write {
        self.store_all(
            display_paths
                .into_iter()
                .map(|(display_path, at)| ScanItemToWrite {
                    item: crate::folder_scanner::ScanItem::Valid(self.candidate(&display_path)),
                    seed: ScanItemSeed::Tags(None),
                    folder_date: at.map(crate::folder_scanner::FolderDate::Created),
                })
                .collect(),
        )
    }

    /// The scan first reading the release at `display_path`, its track's
    /// tags naming the album `album_title` by `artist` and the track
    /// `track_title`: the draft the scan seeds from them.
    pub(crate) fn scan_tagged(
        &self,
        display_path: &str,
        album_title: &str,
        artist: &str,
        track_title: Option<&str>,
    ) -> Write {
        use crate::file_tag_snapshot::{FileObservation, FileTagFact, FileTagSnapshot, FileTags};
        let candidate = self.candidate(display_path);
        let files = candidate
            .files
            .audio()
            .map(|file| FileTagFact {
                observation: FileObservation {
                    relative_path: file.relative_path.clone(),
                    size: file.version.size,
                    modified_at_ns: file.version.modified_at_ns,
                },
                tags: crate::file_tag_snapshot::TagReading::Stated(FileTags {
                    title: track_title.map(str::to_string),
                    album_title: Some(album_title.to_string()),
                    album_artists: vec![artist.to_string()],
                    ..FileTags::untagged()
                }),
            })
            .collect();
        let tables = self.tables();
        let root = self.root.clone();
        Write::step(async move {
            let generation = open_generation(&tables, &root).await;
            let snapshot = FileTagSnapshot {
                scan_generation: generation,
                file_edit_revision: 0,
                files,
                embedded_cover: None,
            };
            let seed =
                crate::file_metadata_seed::FileMetadataSeed::project(&candidate, snapshot, None)
                    .unwrap();
            let item = ScanItemToWrite {
                item: crate::folder_scanner::ScanItem::Valid(candidate),
                seed: ScanItemSeed::Tags(Some(seed)),
                folder_date: None,
            };
            save_entries(&tables, &root, generation, vec![item]).await;
        })
    }

    /// A new pass over the watched folder beginning, its scan open.
    pub(crate) fn begin_pass(&self) -> Write {
        let tables = self.tables();
        let root = self.root.clone();
        Write::step(async move {
            tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
        })
    }

    /// The open pass over the watched folder finishing: every release it
    /// did not read leaves the scan.
    pub(crate) fn finish_pass(&self) -> Write {
        let tables = self.tables();
        let root = self.root.clone();
        Write::step(async move {
            let generation = open_generation(&tables, &root).await;
            tables
                .finish_folder_scan(&root, generation, None)
                .await
                .unwrap()
                .expect("the open pass is the watched folder's scan");
        })
    }

    /// The scan reading the release at `display_path` as `kind`: `valid`, a
    /// release found before the folders around it were read (`tentative`),
    /// or a folder that failed validation (`invalid`).
    pub(crate) fn scan_as(&self, display_path: &str, kind: &str) -> Write {
        self.scan_folder(std::path::Path::new(display_path), kind)
    }

    /// The scan reading the folder `relative` below the watched folder as
    /// `kind`.
    pub(crate) fn scan_folder(&self, relative: &std::path::Path, kind: &str) -> Write {
        use crate::folder_scanner::ScanItem;
        let item = match kind {
            "valid" => ScanItem::Valid(self.candidate_at(relative)),
            "tentative" => ScanItem::Discovered(self.candidate_at(relative)),
            "invalid" => ScanItem::Invalid(self.invalid_at(relative, None)),
            other => panic!("the scan reads no release as {other}"),
        };
        self.store(item, None)
    }

    /// The scan reading the folder at `display_path` as failing validation,
    /// its tags calling the release `tags` (album title and artist), or
    /// nothing with `None`.
    pub(crate) fn scan_invalid(
        &self,
        display_path: &str,
        tags: Option<(&str, Option<&str>)>,
    ) -> Write {
        let tags = tags.map(|(album_title, artist)| crate::folder_scanner::SkippedTags {
            album_title: album_title.to_string(),
            artist: artist.map(str::to_string),
        });
        self.store(
            crate::folder_scanner::ScanItem::Invalid(
                self.invalid_at(std::path::Path::new(display_path), tags),
            ),
            None,
        )
    }

    /// The scan reading the release at `display_path`.
    pub(crate) fn scan(&self, display_path: &str) -> Write {
        self.scan_as(display_path, "valid")
    }

    /// The scan reading the release at `display_path`, its folder dated `at`
    /// by the filesystem.
    pub(crate) fn scan_dated(&self, display_path: &str, at: i64) -> Write {
        self.store(
            crate::folder_scanner::ScanItem::Valid(self.candidate(display_path)),
            Some(crate::folder_scanner::FolderDate::Created(at)),
        )
    }

    /// A new pass over the watched folder that reads every release the last
    /// one stored but the one at `display_path`, which leaves the scan.
    pub(crate) fn unscan(&self, display_path: &str) -> Write {
        self.pass_without(Some(self.key(display_path)))
    }

    /// A new pass over the watched folder that reads every release the last
    /// one stored, as it stored them.
    pub(crate) fn rescan(&self) -> Write {
        self.pass_without(None)
    }

    /// A new pass over the watched folder that reads every release the last
    /// one stored but the one keyed `gone`.
    fn pass_without(&self, gone: Option<crate::CandidateKey>) -> Write {
        let tables = self.tables();
        let root = self.root.clone();
        Write::step(async move {
            let items = tables.load_folder_scan_items(&root).await.unwrap();
            let generation =
                tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
            let kept = items
                .into_iter()
                .filter(|item| Some(item.key()) != gone)
                .map(|item| {
                    crate::tables::folder_scans::FolderEntryToWrite::Release(
                        crate::tables::folder_scans::ScanItemToWrite {
                            item,
                            seed: crate::tables::folder_scans::ScanItemSeed::Tags(None),
                            folder_date: None,
                        },
                    )
                })
                .collect();
            tables
                .save_folder_scan_items(&root, generation, kept)
                .await
                .unwrap()
                .expect("the new pass is the watched folder's scan");
            tables
                .finish_folder_scan(&root, generation, None)
                .await
                .unwrap()
                .expect("the new pass is the watched folder's scan");
        })
    }

    /// The person watching `root` too, after the folders they watch, and a
    /// scan of it reading the release `name` directly in it, its folder dated
    /// `at` by the filesystem.
    pub(crate) fn scan_other_root(
        &self,
        root: &std::path::Path,
        name: &str,
        at: Option<i64>,
    ) -> Write {
        let tables = self.tables();
        let root = root.to_path_buf();
        let item = ScanItemToWrite {
            item: ScanItem::Valid(self.candidate_in(&root, std::path::Path::new(name))),
            seed: ScanItemSeed::Tags(None),
            folder_date: at.map(crate::folder_scanner::FolderDate::Created),
        };
        Write::step(async move {
            tables.add_watched_import_folder(&root).await.unwrap();
            let generation =
                tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
            save_entries(&tables, &root, generation, vec![item]).await;
        })
    }

    /// The scan settling how the folder at `display_path` reads: its
    /// releases kept apart, or combined as one.
    pub(crate) fn reading(
        &self,
        display_path: &str,
        decision: crate::folder_scanner::FolderReleaseDecision,
    ) -> Write {
        let tables = self.tables();
        let key = self.group_key(display_path);
        let grouping = crate::CandidateKey::grouping(&format!("reading {display_path}"));
        Write::step(async move {
            tables.record_scanned_folder_release_decision(&key, decision, &grouping).await.unwrap();
        })
    }

    /// The person no longer watching the watched folder.
    pub(crate) fn unwatch(&self) -> Write {
        let tables = self.tables();
        let root = self.root.clone();
        Write::step(async move {
            tables.remove_watched_import_folders(vec![root], None).await.unwrap();
        })
    }

    /// The person combining the releases at `members`, as the scan read
    /// them, into one release under the grouping `grouping:{id}`.
    pub(crate) fn combine(&self, members: &[&str], id: &str) -> Write {
        let tables = self.tables();
        let members: Vec<FolderCandidate> =
            members.iter().map(|member| self.candidate(member)).collect();
        let key = crate::CandidateKey::grouping(id);
        Write::step(async move {
            tables.combine_releases(key, members).await.unwrap();
        })
    }

    /// The person undoing the grouping `grouping:{id}`.
    pub(crate) fn separate(&self, id: &str) -> Write {
        let tables = self.tables();
        let key = crate::CandidateKey::grouping(id);
        Write::step(async move {
            tables.separate_picked_grouping(&key).await.unwrap();
        })
    }

    /// The person setting the release the grouping `grouping:{id}` reads
    /// aside, or back with `false`.
    pub(crate) fn grouping_skipped(&self, id: &str, skipped: bool) -> Write {
        let tables = self.tables();
        let key = crate::CandidateKey::grouping(id);
        Write::step(async move {
            assert!(tables.set_grouping_skipped(&key, skipped).await.unwrap());
        })
    }

    /// The person setting the release at `display_path` aside, or back with
    /// `false`.
    pub(crate) fn set_skipped(&self, display_path: &str, skipped: bool) -> Write {
        let tables = self.tables();
        let path = self.path(display_path);
        Write::step(async move {
            assert!(tables.set_import_candidate_skipped(&path, skipped).await.unwrap());
        })
    }

    /// The person's skip of the release at `display_path`.
    pub(crate) fn skip(&self, display_path: &str) -> Write {
        self.set_skipped(display_path, true)
    }
}

/// The generation of the scan open on `root`.
async fn open_generation(tables: &ImportTables, root: &std::path::Path) -> u64 {
    tables
        .current_folder_scan_generation(root)
        .await
        .unwrap()
        .expect("the watched folder's scan is open")
}

/// Store `items` in one batch of the scan of `root` at `generation`.
async fn save_entries(
    tables: &ImportTables,
    root: &std::path::Path,
    generation: u64,
    items: Vec<ScanItemToWrite>,
) {
    let entries =
        items.into_iter().map(crate::tables::folder_scans::FolderEntryToWrite::Release).collect();
    tables
        .save_folder_scan_items(root, generation, entries)
        .await
        .unwrap()
        .expect("the open scan takes the entries");
}
