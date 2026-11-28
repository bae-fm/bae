//! What `list_candidates` and `get_candidate` answer of a folder that failed
//! validation, and of the library albums that hold a release a verdict offers.

use super::*;
use bae_import::folder_scanner::{
    AudioDamage, FileVersion, InvalidCandidate, ProposedRole, SkippedFile,
};

/// The scan item of the folder `name` below `root`, which failed validation
/// because one of its two audio files can't be played.
fn invalid(root: &std::path::Path, name: &str) -> InvalidCandidate {
    let folder = root.join(name);
    let file = |relative_path: &str, role| SkippedFile {
        relative_path: relative_path.to_string(),
        file: FileVersion { path: folder.join(relative_path), size: 1_000, modified_at_ns: 1 },
        role,
    };
    InvalidCandidate {
        path: folder.clone(),
        name: name.to_string(),
        watched_folder_path: root.to_path_buf(),
        relative_folder: bae_import::RelativeFolder::test(name),
        grouping: None,
        files: vec![
            file("01.flac", ProposedRole::Audio),
            file("02.flac", ProposedRole::DamagedAudio { damage: AudioDamage::Unreadable }),
        ],
        tags: None,
    }
}

/// A folder that failed validation is listed with the reason, and its key
/// answers it.
#[tokio::test]
async fn an_invalid_folder_is_listed_with_its_reason_and_answered_by_its_key() {
    let fixture = automation_over().await;
    let root = fixture.root();
    let broken = invalid(&root, "Broken");
    let reason = broken.reason().to_string();
    write_scan(&fixture, &root, |items| items.push(ScanItem::Invalid(broken))).await;
    let key = key_of(&root, "Broken");

    let listed = fixture.list_candidates().await;
    assert_eq!(keys(&listed), vec![key.as_str()]);
    let found = fixture.get_candidate(&key).await.expect("an invalid folder is a candidate");
    for candidate in [&listed[0], &found] {
        assert!(
            matches!(
                candidate,
                AutomationCandidate::Invalid { invalid_reason, .. } if *invalid_reason == reason
            ),
            "{candidate:?}"
        );
    }
}

/// A library album holding a copy of a release.
async fn library_album(store: &bae_store::Store, title: &str, release: &str) -> String {
    let records = bae_records::test_fixtures::test_records(store);
    let album_id = bae_util::test_ids::test_uuid(title);
    let release_id = bae_util::test_ids::test_uuid(&format!("{title} release"));
    bae_records::test_fixtures::album_with_releases(&records, &album_id, title, &[&release_id])
        .await;
    let record = bae_catalogs::records::ReleaseRecord::new(
        &bae_catalogs::records::MetadataRef::new(
            bae_catalogs::catalog::Catalog::MusicBrainz,
            release,
        ),
        Some("mb-group".to_string()),
    );
    records.insert_release_records(&release_id, &[record]).await.unwrap();
    album_id
}

/// The library albums each release of `candidate`'s identify state names.
fn library_statuses(candidate: &AutomationCandidate) -> serde_json::Value {
    serde_json::to_value(candidate).unwrap()["runtime"]["identify_state"]["library_statuses"]
        .clone()
}

/// A candidate whose stored verdict offers a release that a library album
/// holds names that album, in the list and by its key.
#[tokio::test]
async fn a_verdict_s_offered_release_names_the_library_album_holding_it() {
    let fixture = automation_over().await;
    let root = fixture.root();
    let item = candidate(&fixture.tmp, &root, "A");
    let ScanItem::Valid(found) = &item else { unreachable!("the walk finds a release") };
    let found = found.clone();
    write_scan(&fixture, &root, |items| items.push(item)).await;
    bae_import::test_verdicts::store_found_verdict(
        &fixture.tables,
        &found,
        "mb-offered",
        "mb-group",
    )
    .await;
    let album = library_album(&fixture.store, "Held Album", "mb-offered").await;

    let expected = serde_json::json!([{
        "release_id": "mb-offered",
        "albums": [{ "album_id": album, "album_title": "Held Album", "holds_release": true }],
    }]);
    let listed = fixture.list_candidates().await;
    assert_eq!(library_statuses(&listed[0]), expected);
    let by_key = fixture.get_candidate(&key_of(&root, "A")).await.expect("a scanned candidate");
    assert_eq!(library_statuses(&by_key), expected);
}
