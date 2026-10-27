// The pinned count `release_listing` keeps per release, against the count
// probed from `coven_pins` file by file, through random library writes and
// pins: uploads that keep their copy or not, their completion, pins, unpins,
// evictions, a file moving between releases, and deletes.
//
// `include!`d into `manager::tests`.

/// One release the harness wrote, and where its files stand.
#[cfg(feature = "test-utils")]
struct HarnessRelease {
    id: String,
    stage: HarnessStage,
}

#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum HarnessStage {
    Local,
    Attached,
    Remote,
}

/// Each release whose listed pinned or file count differs from the one probed
/// from its files, or that has no listing row: none when the triggers kept up.
#[cfg(feature = "test-utils")]
async fn listing_disagreements(manager: &LibraryManager) -> Vec<String> {
    manager
        .database
        .query_texts_for_test(
            "SELECT r.id || ': listed ' || COALESCE(l.pinned_count || ' of ' || l.file_count, 'nothing') \
                 || ', probed ' || probe.pinned || ' of ' || probe.files \
             FROM releases r \
             LEFT JOIN release_listing l ON l.release_id = r.id \
             JOIN (SELECT r2.id AS release_id, \
                     (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r2.id) AS files, \
                     (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r2.id \
                        AND EXISTS (SELECT 1 FROM coven_pins p \
                                    WHERE p.table_name = 'release_files' AND p.key = coven_key(f.id))) AS pinned \
                   FROM releases r2) probe ON probe.release_id = r.id \
             WHERE l.release_id IS NULL OR l.pinned_count <> probe.pinned OR l.file_count <> probe.files",
        )
        .await
        .unwrap()
}

/// The releases whose every file `coven_pins` lists, probed file by file.
#[cfg(feature = "test-utils")]
async fn probed_pinned_releases(manager: &LibraryManager) -> std::collections::BTreeSet<String> {
    manager
        .database
        .query_texts_for_test(
            "SELECT r.id FROM releases r \
             WHERE EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id) \
               AND NOT EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id \
                   AND NOT EXISTS (SELECT 1 FROM coven_pins p \
                                   WHERE p.table_name = 'release_files' AND p.key = coven_key(f.id)))",
        )
        .await
        .unwrap()
        .into_iter()
        .collect()
}

/// The uploaded releases with a file `coven_pins` doesn't list, probed file by
/// file.
#[cfg(feature = "test-utils")]
async fn probed_cloud_only_count(manager: &LibraryManager) -> u64 {
    manager
        .database
        .query_texts_for_test(
            "SELECT CAST(COUNT(*) AS TEXT) FROM release_listing l WHERE l.stored = 1 \
               AND EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = l.release_id \
                   AND NOT EXISTS (SELECT 1 FROM coven_pins p \
                                   WHERE p.table_name = 'release_files' AND p.key = coven_key(f.id)))",
        )
        .await
        .unwrap()[0]
        .parse()
        .unwrap()
}

/// The pinned marker each release shows in the Storage Manager list.
#[cfg(feature = "test-utils")]
async fn listed_pinned_releases(manager: &LibraryManager) -> std::collections::BTreeSet<String> {
    manager
        .database
        .get_storage_page(
            &crate::db::StorageSortCriterion {
                field: crate::db::StorageSortField::AlbumTitle,
                direction: crate::db::SortDirection::Ascending,
            },
            crate::db::StorageFilter::All,
            0,
            100,
        )
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.pinned)
        .map(|row| row.release.id)
        .collect()
}

#[cfg(feature = "test-utils")]
async fn harness_file_refs(manager: &LibraryManager, release_id: &str) -> Vec<coven::FileRef> {
    let mut refs = Vec::new();
    for file in release_files(manager, release_id).await {
        refs.push(manager.release_file_ref(&file.id).await.unwrap());
    }
    refs
}

/// Random writes against the probe: after each, every release's listed pinned
/// and file counts are the probed ones, and so are the cloud-only count and
/// the list's pinned markers read from them.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_listed_pinned_count_follows_every_pin_and_file_write() {
    use rand::{Rng, SeedableRng};
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x5eed);
    let mut releases: Vec<HarnessRelease> = Vec::new();
    let mut ran: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
    let mut most_pinned = 0usize;
    for step in 0..400 {
        let pick = |rng: &mut rand::rngs::StdRng, releases: &[HarnessRelease], stage| {
            let matching: Vec<usize> = (0..releases.len())
                .filter(|&index| releases[index].stage == stage)
                .collect();
            (!matching.is_empty()).then(|| matching[rng.random_range(0..matching.len())])
        };
        let op = match rng.random_range(0..10) {
            0 | 1 if releases.len() < 6 => {
                let files: Vec<(String, Vec<u8>)> = (0..rng.random_range(1..=3))
                    .map(|index| (format!("{index}.flac"), format!("{step}-{index}").into_bytes()))
                    .collect();
                let files: Vec<(&str, &[u8])> = files
                    .iter()
                    .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                    .collect();
                let release = insert_local_release_with_files(
                    &manager,
                    &temp_dir.path().join(format!("release-{step}")),
                    &format!("Album Title {step}"),
                    &files,
                )
                .await;
                releases.push(HarnessRelease {
                    id: release.id,
                    stage: HarnessStage::Local,
                });
                "add release"
            }
            2 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Local) else {
                    continue;
                };
                let pin = rng.random_bool(0.5);
                manager.upload_release(&releases[index].id, pin).await.unwrap();
                if pin {
                    wait_for_offline_copy_request(&manager, &releases[index].id).await;
                }
                releases[index].stage = HarnessStage::Attached;
                if pin { "attach pinned" } else { "attach" }
            }
            3 => {
                if !releases.iter().any(|release| release.stage == HarnessStage::Attached) {
                    continue;
                }
                manager.drain_uploads_expecting_work().await.unwrap();
                for release in &mut releases {
                    if release.stage == HarnessStage::Attached {
                        release.stage = HarnessStage::Remote;
                    }
                }
                "complete uploads"
            }
            4 | 5 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let refs = harness_file_refs(&manager, &releases[index].id).await;
                let count = rng.random_range(1..=refs.len());
                manager.database.pin(&refs[..count]).await.unwrap();
                "pin"
            }
            6 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let refs = harness_file_refs(&manager, &releases[index].id).await;
                let one = rng.random_range(0..refs.len());
                if rng.random_bool(0.5) {
                    manager.database.unpin(&refs[one..=one]).await.unwrap();
                    "unpin"
                } else {
                    manager.evict_file_for_test(&refs[one]).await.unwrap();
                    "evict"
                }
            }
            7 => {
                let Some(from) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let Some(to) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let files = release_files(&manager, &releases[from].id).await;
                if from == to || files.len() < 2 {
                    continue;
                }
                let file = &files[rng.random_range(0..files.len())];
                manager
                    .database
                    .execute_for_test(
                        "UPDATE release_files SET release_id = ?1 WHERE id = ?2",
                        vec![releases[to].id.clone(), file.id.clone()],
                    )
                    .await
                    .unwrap();
                "move file"
            }
            8 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let files = release_files(&manager, &releases[index].id).await;
                if files.len() < 2 {
                    continue;
                }
                let file = &files[rng.random_range(0..files.len())];
                manager
                    .database
                    .execute_for_test("DELETE FROM release_files WHERE id = ?1", vec![file.id.clone()])
                    .await
                    .unwrap();
                "delete file"
            }
            9 if !releases.is_empty() => {
                let index = rng.random_range(0..releases.len());
                if releases[index].stage == HarnessStage::Attached {
                    continue;
                }
                let release = releases.remove(index);
                manager.delete_release(&release.id).await.unwrap();
                "delete release"
            }
            _ => continue,
        };
        *ran.entry(op).or_default() += 1;

        assert_eq!(
            listing_disagreements(&manager).await,
            Vec::<String>::new(),
            "after step {step}, {op}"
        );
        let probed_pinned = probed_pinned_releases(&manager).await;
        most_pinned = most_pinned.max(probed_pinned.len());
        assert_eq!(
            listed_pinned_releases(&manager).await,
            probed_pinned,
            "pinned markers after step {step}, {op}"
        );
        assert_eq!(
            manager.cloud_only_release_count().await.unwrap(),
            probed_cloud_only_count(&manager).await,
            "cloud-only count after step {step}, {op}"
        );
    }
    for op in [
        "add release",
        "attach",
        "attach pinned",
        "complete uploads",
        "pin",
        "unpin",
        "evict",
        "move file",
        "delete file",
        "delete release",
    ] {
        assert!(ran.contains_key(op), "the walk never ran {op}: {ran:?}");
    }
    assert!(most_pinned > 0, "no release was ever wholly pinned");
}
