//! A folder that failed validation, as the list and the pane looking into it
//! read it back.

use super::*;

/// A folder that failed validation is listed under Skipped as what the scan
/// read of it — its files, what its tags call it, the image it leads with —
/// selected like any row, and read whole by the pane that looks into it. A
/// scan that finds it again finds it unchanged.
#[tokio::test]
async fn a_skipped_folder_is_listed_and_read_as_what_its_scan_read() {
    use crate::import::folder_scanner::{
        AudioDamage, InvalidCandidate, ProposedRole, SkippedFile, SkippedTags,
    };

    let (db, _tmp, root) = watched_root().await;
    db.add_watched_import_folder(&root).await.unwrap();
    let folder = PathBuf::from(&root).join("Half Copied");
    let version = |name: &str, size| FileVersion {
        path: folder.join(name),
        size,
        modified_at_ns: 1,
    };
    let invalid = InvalidCandidate {
        path: folder.clone(),
        name: "Half Copied".to_string(),
        watched_folder_path: root.clone(),
        display_path: "Half Copied".to_string(),
        grouping: None,
        files: vec![
            SkippedFile {
                relative_path: "01.flac".to_string(),
                file: version("01.flac", 1_000),
                role: ProposedRole::Audio,
            },
            SkippedFile {
                relative_path: "02.flac".to_string(),
                file: version("02.flac", 4_096),
                role: ProposedRole::DamagedAudio {
                    damage: AudioDamage::Unreadable,
                },
            },
            SkippedFile {
                relative_path: "front.jpg".to_string(),
                file: version("front.jpg", 4),
                role: ProposedRole::Image,
            },
        ],
        tags: Some(SkippedTags {
            album_title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
        }),
    };
    let key = invalid.key();
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    db.save_folder_scan_item(&root, generation, &ScanItem::Invalid(invalid.clone()))
        .await
        .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();
    db.change_candidate_selection(
        request(TriageTab::Skipped).await,
        crate::import::selection::SelectionChange::Replace {
            keys: vec![key.clone()],
        },
    )
    .await
    .unwrap();

    let list = db
        .load_import_list(request(TriageTab::Skipped).await)
        .await
        .unwrap();
    let listed: Vec<_> = list
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            ImportListItem::Invalid {
                candidate,
                selected,
                ..
            } => Some((candidate.clone(), *selected)),
            _ => None,
        })
        .collect();
    assert_eq!(listed, [(invalid.clone(), true)]);
    assert_eq!(
        invalid.cover_image(),
        Some(crate::import::CoverImageSource::Local {
            file: version("front.jpg", 4)
        })
    );

    let mut detail = db.subscribe_invalid_candidate(Some(key.clone()));
    let read = detail
        .next()
        .await
        .result
        .unwrap()
        .expect("the key names a folder that failed validation");
    assert_eq!(read.candidate, invalid);
    assert_eq!(
        read.images
            .iter()
            .map(|image| image.file_id.as_str())
            .collect::<Vec<_>>(),
        ["front.jpg"]
    );
    assert_eq!(
        read.files
            .iter()
            .map(|file| (file.file_id.as_str(), file.role))
            .collect::<Vec<_>>(),
        [
            ("01.flac", crate::import::MappingRole::Audio),
            (
                "02.flac",
                crate::import::MappingRole::DamagedAudio {
                    damage: AudioDamage::Unreadable
                }
            ),
        ]
    );

    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    let again = db
        .save_folder_scan_item(&root, generation, &ScanItem::Invalid(invalid))
        .await
        .unwrap()
        .expect("the scan entry is written");
    assert!(
        !again.changed(),
        "the folder read the same is stored unchanged"
    );
}
