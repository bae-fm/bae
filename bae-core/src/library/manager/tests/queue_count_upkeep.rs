// The queued count and first queued place `release_listing` keeps per
// release, against those probed from coven's `coven_uploads` file by file,
// through random library writes and uploads: attaching, completing and
// cancelling uploads, abandoning one by deleting its file's row, a file moving
// between releases, covers queued beside them, and deletes.
//
// `include!`d into `manager::tests`, beside `pin_count_upkeep.rs`, whose
// `HarnessRelease` it shares.

/// Each release whose listed queued count or first place differs from the
/// ones probed from its files in coven's queue: none when the triggers kept up.
#[cfg(feature = "test-utils")]
async fn queue_listing_disagreements(manager: &LibraryManager) -> Vec<String> {
    manager
        .database
        .query_texts_for_test(
            "SELECT r.id || ': listed ' || COALESCE(l.queued_count || ' from ' \
                     || COALESCE(l.first_queued_position, 'nowhere'), 'nothing') \
                 || ', probed ' || probe.queued || ' from ' || COALESCE(probe.first, 'nowhere') \
             FROM releases r \
             LEFT JOIN release_listing l ON l.release_id = r.id \
             JOIN (SELECT r2.id AS release_id, \
                     (SELECT COUNT(*) FROM release_files f JOIN coven_uploads u \
                        ON u.table_name = 'release_files' AND u.key = coven_key(f.id) \
                      WHERE f.release_id = r2.id) AS queued, \
                     (SELECT MIN(u.position) FROM release_files f JOIN coven_uploads u \
                        ON u.table_name = 'release_files' AND u.key = coven_key(f.id) \
                      WHERE f.release_id = r2.id) AS first \
                   FROM releases r2) probe ON probe.release_id = r.id \
             WHERE l.release_id IS NULL OR l.queued_count <> probe.queued \
                OR l.first_queued_position IS NOT probe.first",
        )
        .await
        .unwrap()
}

/// The releases with files in coven's queue, in the order their first files
/// wait, probed file by file.
#[cfg(feature = "test-utils")]
async fn probed_uploading_releases(manager: &LibraryManager) -> Vec<String> {
    manager
        .database
        .query_texts_for_test(
            "SELECT f.release_id FROM coven_uploads u \
             JOIN release_files f ON u.table_name = 'release_files' AND u.key = coven_key(f.id) \
             GROUP BY f.release_id ORDER BY MIN(u.position)",
        )
        .await
        .unwrap()
}

/// Random writes against the probe: after each, every release's listed
/// queued count and first place are the probed ones, and so are the
/// releases uploading, in the order read from them.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn the_listed_queue_counts_follow_every_upload_and_file_write() {
    use rand::{Rng, SeedableRng};
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x9e7e);
    let mut releases: Vec<HarnessRelease> = Vec::new();
    let mut ran: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut most_uploading = 0usize;
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
            2 | 3 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Local) else {
                    continue;
                };
                manager.upload_release(&releases[index].id, false).await.unwrap();
                releases[index].stage = HarnessStage::Attached;
                "attach"
            }
            4 => {
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
            5 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Attached) else {
                    continue;
                };
                manager.cancel_release_upload(&releases[index].id).await.unwrap();
                releases[index].stage = HarnessStage::Local;
                "cancel upload"
            }
            6 => {
                if releases.is_empty() {
                    continue;
                }
                let index = rng.random_range(0..releases.len());
                store_test_cover(&manager, &releases[index].id, format!("cover {step}").as_bytes())
                    .await;
                "queue cover"
            }
            7 => {
                let Some(from) = pick(&mut rng, &releases, HarnessStage::Attached) else {
                    continue;
                };
                let Some(to) = pick(&mut rng, &releases, HarnessStage::Attached) else {
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
                "move queued file"
            }
            8 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Attached) else {
                    continue;
                };
                let files = release_files(&manager, &releases[index].id).await;
                if files.len() < 2 {
                    continue;
                }
                let file = &files[rng.random_range(0..files.len())];
                manager
                    .database
                    .execute_for_test(
                        "DELETE FROM release_files WHERE id = ?1",
                        vec![file.id.clone()],
                    )
                    .await
                    .unwrap();
                "delete queued file"
            }
            9 if !releases.is_empty() => {
                let index = rng.random_range(0..releases.len());
                let release = releases.remove(index);
                manager.delete_release(&release.id).await.unwrap();
                "delete release"
            }
            _ => continue,
        };
        *ran.entry(op).or_default() += 1;

        assert_eq!(
            queue_listing_disagreements(&manager).await,
            Vec::<String>::new(),
            "after step {step}, {op}"
        );
        let probed = probed_uploading_releases(&manager).await;
        most_uploading = most_uploading.max(probed.len());
        assert_eq!(
            manager.database.uploading_releases().await.unwrap(),
            probed,
            "the releases uploading after step {step}, {op}"
        );
    }
    for op in [
        "add release",
        "attach",
        "complete uploads",
        "cancel upload",
        "queue cover",
        "move queued file",
        "delete queued file",
        "delete release",
    ] {
        assert!(ran.contains_key(op), "the walk never ran {op}: {ran:?}");
    }
    assert!(most_uploading > 1, "never more than one release uploading");
}
