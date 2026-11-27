//! A folder whose audio bae can't import is listed with why, and the pass
//! goes on with the rest of its watched folder.

use super::*;
use crate::scan_passes::tests::*;
use crate::service::tests::*;
use crate::tables::import_list::tests::request;

/// Four seconds of 64-bit float silence in a WAV file.
fn wav_of_64_bits() -> Vec<u8> {
    let silence = vec![0u8; 4 * 44_100 * 2 * 8];
    bae_audio::test_samples::wav_with_fmt(3, 64, 2, 44_100, &silence)
}

fn too_deep() -> crate::InvalidReason {
    crate::InvalidReason::DeepAudio { name: "album.wav".to_string(), bits: 64 }
}

/// Audio deeper than 32 bits makes its folder one bae can't import, named by
/// the file and its depth, whether the file is a track itself or a CUE sheet
/// splits it into tracks.
#[tokio::test]
async fn audio_deeper_than_32_bits_lists_its_folder_as_one_bae_cannot_import() {
    let test = setup_import_service().await;
    let tables = test.tables().clone();
    let root = test.temp.path().join("music");
    write_disc(&test.temp, &root.join("Importable"), 1);
    for folder in ["Loose", "Split"] {
        test.temp.create_folder(root.join(folder));
        test.temp.write(root.join(folder).join("album.wav"), wav_of_64_bits());
    }
    test.temp.write(
        root.join("Split").join("album.cue"),
        "FILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
         TRACK 02 AUDIO\n    INDEX 01 00:02:00\n",
    );
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    tables.add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan();

    scan.rescan(&root).await.unwrap();

    let mut listed: Vec<(String, Option<String>)> = tables
        .load_folder_scan_items(&root)
        .await
        .unwrap()
        .into_iter()
        .map(|item| match item {
            ScanItem::Valid(candidate) => (candidate.name, None),
            ScanItem::Invalid(candidate) => {
                let reason = candidate.reason().to_string();
                (candidate.name, Some(reason))
            }
            ScanItem::Discovered(candidate) => panic!("{} is still being read", candidate.name),
        })
        .collect();
    listed.sort();
    let refused = Some("album.wav is 64-bit audio".to_string());
    assert_eq!(
        listed,
        vec![
            ("Importable".to_string(), None),
            ("Loose".to_string(), refused.clone()),
            ("Split".to_string(), refused),
        ]
    );

    // The list's rows and the folder's pane give the same reason, and the pane
    // lists the folder's files.
    let mut rows: Vec<_> = tables
        .load_import_list_read(request(crate::TriageTab::Skipped).await)
        .await
        .unwrap()
        .windows
        .iter()
        .flat_map(|window| window.rows.iter().map(|(item, _key)| item))
        .filter_map(|item| match item {
            crate::list::ImportListItem::Invalid { row } => Some(row.clone()),
            _ => None,
        })
        .collect();
    rows.sort_by(|a, b| a.folder_name.cmp(&b.folder_name));
    assert_eq!(
        rows.iter().map(|row| (row.folder_name.as_str(), &row.reason)).collect::<Vec<_>>(),
        [("Loose", &too_deep()), ("Split", &too_deep())]
    );
    let mut detail = tables.subscribe_invalid_candidate(Some(rows[1].candidate_key.clone()));
    let pane = detail
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .expect("the key names a folder bae can't import");
    assert_eq!(pane.candidate, rows[1]);
    let mut files: Vec<&str> = pane.files.iter().map(|file| file.name.as_str()).collect();
    files.sort();
    assert_eq!(files, ["album.cue", "album.wav"]);
}
