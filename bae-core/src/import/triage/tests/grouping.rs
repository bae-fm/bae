#[test]
fn nested_candidates_form_a_collapsible_group_with_a_combine_target() {
    let snapshot = snapshot_of(vec![
        candidate("Group/Release One", false, false),
        candidate("Group/Wrapper/Release Two", false, false),
    ]);
    let queue = project_idle(
        snapshot,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );

    assert_eq!(queue.sections.len(), 1);
    let group = queue.sections[0].group.as_ref().expect("grouped section");
    assert_eq!(group.name, "Group");
    assert_eq!(
        group.key,
        FolderReleaseDecisionKey {
            watched_folder_path: host_root("/music"),
            relative_folder_path: "Group".to_string(),
        }
    );
    assert_eq!(queue.sections[0].entries.len(), 2);
}

#[test]
fn direct_release_joins_its_top_level_descendant_group() {
    let snapshot = snapshot_of(vec![
        candidate("Artist", false, false),
        candidate("Artist/Album", false, false),
    ]);
    let queue = project_idle(
        snapshot,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );

    assert_eq!(queue.sections.len(), 1);
    let section = &queue.sections[0];
    assert_eq!(
        section.group.as_ref().map(|group| group.name.as_str()),
        Some("Artist")
    );
    assert_eq!(section.entries.len(), 2);
}

#[test]
fn candidate_and_boundary_entries_share_natural_path_order() {
    let mut snapshot = snapshot_of(vec![candidate("Group/Release 10", false, false)]);
    snapshot.boundaries.push(FolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: host_root("/music"),
            relative_folder_path: "Group/Release 2".to_string(),
        },
        name: "Release 2".to_string(),
        display_path: "Group/Release 2".to_string(),
        shared_file_count: 0,
        tree_rows: Vec::new(),
        candidate_keys: Vec::new(),
    });

    let queue = project_idle(
        snapshot,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );
    assert_eq!(queue.sections.len(), 1);
    assert!(matches!(
        &queue.sections[0].entries[..],
        [TriageEntry::Boundary(boundary), TriageEntry::Candidate(row)]
            if boundary.display_path == "Group/Release 2"
                && row.display_path == "Group/Release 10"
    ));
}

#[test]
fn projected_entry_keys_are_stable_and_variant_distinct() {
    let mut snapshot = snapshot_of(vec![candidate("Group/Release", false, false)]);
    snapshot.boundaries.push(FolderReleaseBoundary {
        key: FolderReleaseDecisionKey {
            watched_folder_path: host_root("/music"),
            relative_folder_path: "Group/Release".to_string(),
        },
        name: "Release".to_string(),
        display_path: "Group/Release".to_string(),
        shared_file_count: 0,
        tree_rows: Vec::new(),
        candidate_keys: Vec::new(),
    });

    let first = project_idle(
        snapshot.clone(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );
    let second = project_idle(
        snapshot,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );
    let first_keys: Vec<_> = first.sections[0]
        .entries
        .iter()
        .map(TriageEntry::stable_key)
        .collect();
    let second_keys: Vec<_> = second.sections[0]
        .entries
        .iter()
        .map(TriageEntry::stable_key)
        .collect();

    assert_eq!(first_keys, second_keys);
    assert_eq!(first_keys.len(), 2);
    assert_ne!(first_keys[0], first_keys[1]);
}

// ── `load`: the real read ───────────────────────────────────────────────────

mod load {
    use super::*;
    use crate::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack, NewImportCandidateVerdict};
    use crate::import::ReleaseIdentity;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    const ARTIST_ID: &str = "e36744a5-1a36-460f-891c-e7e558034edf";
    const FLAC_FIXTURES: [&str; 2] = ["01 Test Track 1.flac", "02 Test Track 2.flac"];

    /// A real database, library manager and import service over a tempdir. No
    /// provider is faked and nothing identifies: these tests seed the stored
    /// verdicts directly, because what is under test is the read that turns
    /// them into rows.
    struct Fixture {
        manager: LibraryManager,
        import: ImportServiceHandle,
        root: PathBuf,
        _temp: TempDir,
    }

    impl Fixture {
        async fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let clock: coven::ClockRef = Arc::new(coven::SystemClock);
            let ids: coven::IdRef = Arc::new(coven::UuidProvider);
            let database = Database::new_test(
                temp.path().join("test.db").to_str().unwrap(),
                clock.clone(),
                ids.clone(),
            )
            .await
            .unwrap();
            let library_dir = coven::StoreDir::new(temp.path());
            let library_id = format!("triage-{}", uuid::Uuid::new_v4());
            let config = crate::config::Config::with_defaults(
                library_id.clone(),
                "test-device".to_string(),
                library_dir,
                "Test Library".to_string(),
            );
            crate::config::install_test_keyring();
            let manager = LibraryManager::new(
                database,
                Arc::new(crate::config::ConfigHandle::new(config)),
                clock,
                ids,
                crate::diagnostics::Diagnostics::noop(),
                tokio::runtime::Handle::current(),
                crate::import::cover_art::RemoteImageCache::for_test(),
            );
            let import = manager
                .start_import_service(tokio::runtime::Handle::current())
                .await
                .unwrap();
            let root = temp.path().join("watched");
            std::fs::create_dir_all(&root).unwrap();
            Fixture {
                manager,
                import,
                root,
                _temp: temp,
            }
        }

        /// A candidate folder with two real FLACs, so the scan produces a
        /// folder candidate with a real content hash.
        ///
        /// The rip log is named after the folder because the content hash is
        /// over relative paths and sizes: two folders holding the same files
        /// under the same names *are* one candidate as far as the stored
        /// verdicts are concerned, which is correct and not what these tests
        /// are about.
        fn candidate_dir(&self, folder: &str) -> PathBuf {
            let dir = self.root.join(folder);
            std::fs::create_dir_all(&dir).unwrap();
            for name in FLAC_FIXTURES {
                std::fs::copy(Path::new("tests/fixtures/flac").join(name), dir.join(name)).unwrap();
            }
            std::fs::write(dir.join(format!("{folder}.txt")), folder).unwrap();
            dir
        }

        /// Watch the root and wait for the scan to surface every candidate.
        async fn scan(&self, expected: usize) {
            let mut events = self.import.subscribe_events();
            self.import
                .add_watched_folder(self.root.to_string_lossy().into_owned())
                .await
                .unwrap();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                let event = tokio::time::timeout(remaining, events.recv())
                    .await
                    .expect("the scan finishes")
                    .expect("the bus stays open");
                if matches!(
                    event,
                    crate::import::ImportEvent::Scan(crate::import::ScanEvent::Finished)
                ) {
                    break;
                }
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            tokio::time::timeout(
                remaining,
                self.import
                    .wait_for_candidates(|snapshot| snapshot.folder_candidates.len() == expected),
            )
            .await
            .expect("the candidate list reflects the finished scan");
        }

        fn content_hash(&self, dir: &Path) -> String {
            // No stored bindings: these fixtures never edit one, so every
            // sheet keeps the scan's own reading.
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                dir,
                crate::import::ReleaseFileScope::Recursive,
                &crate::import::folder_scanner::StoredCandidateEdits::none(),
            )
            .expect("the candidate folder is readable")
            .content_hash()
        }

        /// Seed the row a sweep would have written for this folder.
        async fn store(&self, dir: &Path, verdict: &str, probed_total_duration_ms: i64) {
            self.manager
                .save_import_candidate_verdict(&NewImportCandidateVerdict {
                    content_hash: self.content_hash(dir),
                    folder_path: dir.to_string_lossy().into_owned(),
                    verdict: verdict.to_string(),
                    probed_total_duration_ms,
                    expected_edit_revision: 0,
                    identity_pick: None,
                })
                .await
                .unwrap();
        }

        async fn store_verdict(&self, dir: &Path, verdict: &TerminalVerdict, probed: i64) {
            self.store(dir, &serde_json::to_string(verdict).unwrap(), probed)
                .await;
        }

        /// Put a release into the library under `mb_release_id`, so a live
        /// check answers "already in the library" for it.
        async fn own_release(&self, mb_group_id: &str, mb_release_id: &str) {
            let now = chrono::Utc::now();
            self.manager
                .insert_artist(&DbArtist {
                    id: ARTIST_ID.to_string(),
                    name: "Artist Name".to_string(),
                    sort_name: None,
                    discogs_artist_id: None,
                    musicbrainz_artist_id: None,
                    created_at: now,
                })
                .await
                .unwrap();
            let album = DbAlbum {
                id: uuid::Uuid::new_v4().to_string(),
                title: "Album Title".to_string(),
                artist_id: ARTIST_ID.to_string(),
                year: Some(1999),
                primary_release_id: None,
                is_compilation: false,
                created_at: now,
            };
            let release = DbRelease {
                id: uuid::Uuid::new_v4().to_string(),
                album_id: album.id.clone(),
                release_name: None,
                pressing: crate::db::Pressing {
                    year: Some(1999),
                    format: None,
                    label: None,
                    catalog_number: None,
                    country: None,
                    barcode: None,
                },
                disc_id: None,
                metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
                metadata_source_release_id: Some(mb_release_id.to_string()),
                remote: true,
                source_folder_name: None,
                content_hash: None,
                album_loudness_lufs: None,
                album_peak_linear: None,
                created_at: now,
            };
            let track = DbTrack {
                id: uuid::Uuid::new_v4().to_string(),
                release_id: release.id.clone(),
                title: "Track 1".to_string(),
                side: 1,
                track_number: Some(1),
                duration_ms: Some(180_000),
                discogs_position: None,
                created_at: now,
            };
            self.manager
                .insert_album_with_release_and_tracks(&album, &release, &[track], &[])
                .await
                .unwrap();
            self.manager
                .insert_release_identities(
                    &release.id,
                    &[ReleaseIdentity {
                        source: MetadataSource::MusicBrainz,
                        source_group_id: mb_group_id.to_string(),
                        source_release_id: Some(mb_release_id.to_string()),
                    }],
                )
                .await
                .unwrap();
        }

        async fn load(&self) -> Result<TriageQueue, LibraryError> {
            super::super::load(&self.import, &self.manager).await
        }
    }

    /// A verdict whose one match agrees with the folder on count and length —
    /// everything the Ready rule wants except the library check.
    fn agreeing_verdict(probed_ms: u64, release_id: &str, group_id: &str) -> TerminalVerdict {
        let mut only = result(release_id);
        only.source_group_id = Some(group_id.to_string());
        only.source_tracks = Some(SourceTracks::Listed {
            count: 2,
            total_duration_ms: Some(probed_ms),
        });
        let mut verdict = found(vec![only]);
        if let TerminalVerdict::Found {
            track_count, group, ..
        } = &mut verdict
        {
            *track_count = 2;
            group.source_group_id = group_id.to_string();
        }
        verdict
    }

    /// The probed total the fixture FLACs really have — the number a sweep
    /// would have stored, so the Ready rule's duration check passes on it.
    fn probed_total_ms(dir: &Path) -> u64 {
        FLAC_FIXTURES
            .iter()
            .map(|name| {
                crate::audio_codec::probe_audio_from_path(dir.join(name).to_str().unwrap())
                    .expect("the fixture FLAC probes")
                    .duration
                    .as_millis() as u64
            })
            .sum()
    }

    /// The control: a candidate whose stored verdict agrees with the folder and
    /// whose release is *not* in the library is Ready. Everything below is this
    /// case with one thing changed, so a failure there is that thing.
    #[tokio::test]
    async fn an_agreeing_verdict_not_in_the_library_is_ready() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        let probed = probed_total_ms(&dir);
        fixture
            .store_verdict(
                &dir,
                &agreeing_verdict(probed, "mb-rel-1", "group-1"),
                probed as i64,
            )
            .await;

        let queue = fixture.load().await.unwrap();
        assert_eq!(candidate_rows(&queue).len(), 1);
        assert_eq!(candidate_rows(&queue)[0].placement, TriagePlacement::Ready);
        assert_eq!(queue.counts.pending, 1);
        assert!(candidate_rows(&queue)[0].selectable);
    }

    /// The same candidate with the release now in the library must not be
    /// Ready. A missing status reads as "not in the library", so this is the
    /// case `load` refuses to guess at when a check comes back short.
    #[tokio::test]
    async fn a_candidate_already_in_the_library_is_not_ready() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        let probed = probed_total_ms(&dir);
        fixture
            .store_verdict(
                &dir,
                &agreeing_verdict(probed, "mb-rel-1", "group-1"),
                probed as i64,
            )
            .await;
        fixture.own_release("group-1", "mb-rel-1").await;

        let queue = fixture.load().await.unwrap();
        assert_eq!(
            candidate_rows(&queue)[0].placement,
            TriagePlacement::NeedsYou {
                group: NeedsYouGroup::AlreadyInLibrary,
                reason: NeedsYouReason::Disagreement(NeedsYou::AlreadyInLibrary),
            },
            "a release the library already holds must never be bulk-importable"
        );
        assert_eq!(queue.counts.pending, 1);
        assert!(!candidate_rows(&queue)[0].selectable);
    }

    /// A stored row this build can no longer parse is corruption, not an absent
    /// answer. The queue read must fail instead of inventing a usable state.
    #[tokio::test]
    async fn an_undecodable_row_fails_the_read() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        fixture
            .store(&dir, r#"{"Found":{"shape":"from a future build"}}"#, 0)
            .await;

        let error = fixture
            .load()
            .await
            .expect_err("an undecodable verdict cannot be treated as absent");
        assert!(error.to_string().contains("does not decode"));
    }

    /// A negative probed total cannot come from anything that writes the
    /// column. Clamping it to zero would classify the candidate
    /// `LocalDurationUnknown` — a believable answer standing in for a corrupt
    /// row — so the read fails instead.
    #[tokio::test]
    async fn a_negative_probed_total_is_rejected_by_the_write() {
        let fixture = Fixture::new().await;
        let dir = fixture.candidate_dir("album");
        fixture.scan(1).await;
        let verdict = agreeing_verdict(2_400_000, "mb-rel-1", "group-1");
        let error = fixture
            .manager
            .save_import_candidate_verdict(&NewImportCandidateVerdict {
                content_hash: fixture.content_hash(&dir),
                folder_path: dir.to_string_lossy().into_owned(),
                verdict: serde_json::to_string(&verdict).unwrap(),
                probed_total_duration_ms: -1,
                expected_edit_revision: 0,
                identity_pick: None,
            })
            .await
            .expect_err("a negative probed total cannot enter durable state");
        assert!(
            error.to_string().contains("CHECK constraint failed"),
            "unexpected error: {error}"
        );
    }

    /// Two candidates, one verdict each, one batched library check — and the
    /// statuses land on the right candidates rather than being transposed by
    /// the dedup that batches them.
    #[tokio::test]
    async fn one_batched_check_lands_on_the_right_candidates() {
        let fixture = Fixture::new().await;
        let owned = fixture.candidate_dir("owned");
        let fresh = fixture.candidate_dir("fresh");
        fixture.scan(2).await;

        let owned_probed = probed_total_ms(&owned);
        fixture
            .store_verdict(
                &owned,
                &agreeing_verdict(owned_probed, "mb-rel-owned", "group-owned"),
                owned_probed as i64,
            )
            .await;
        let fresh_probed = probed_total_ms(&fresh);
        fixture
            .store_verdict(
                &fresh,
                &agreeing_verdict(fresh_probed, "mb-rel-fresh", "group-fresh"),
                fresh_probed as i64,
            )
            .await;
        fixture.own_release("group-owned", "mb-rel-owned").await;

        let queue = fixture.load().await.unwrap();
        let placement_of = |name: &str| {
            candidate_rows(&queue)
                .into_iter()
                .find(|row| row.folder_name == name)
                .unwrap_or_else(|| panic!("no row for {name}"))
                .placement
                .clone()
        };
        assert_eq!(
            placement_of("owned"),
            TriagePlacement::NeedsYou {
                group: NeedsYouGroup::AlreadyInLibrary,
                reason: NeedsYouReason::Disagreement(NeedsYou::AlreadyInLibrary),
            }
        );
        assert_eq!(placement_of("fresh"), TriagePlacement::Ready);
        assert_eq!(queue.counts.pending, 2);
    }
}
