//! What names a save's files and folder: the file and folder name patterns,
//! typed text and values in them, and the refusal of a save while one can't
//! make a name.

use super::test_export::{import_unknown_local, ExportFixture};
use bae_preferences::{
    NamePattern, NamePiece, SaveExtras, SaveFilenamePattern, SaveFilenameValue, SaveFolderPattern,
    SaveFolderValue, SavePreset,
};
use bae_transfers::OutputKind;
use std::fs;

/// A release save writes into a folder named by the folder name pattern, from
/// the release's own data, not the folder it was imported from.
#[tokio::test]
async fn a_release_save_names_its_folder_by_the_folder_pattern() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("source folder");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let manager = f.library.manager();
    let release = manager.get_release_by_id(&release_id).await.unwrap().unwrap();
    let album = f.library.records().get_album_for_release(&release).await.unwrap();
    let flac = manager.save_presets().await.unwrap().remove(0);

    let saved_folder = |pattern: SaveFolderPattern, directory: &str| {
        let (f, release_id, flac) = (&f, &release_id, &flac);
        let directory = directory.to_string();
        async move {
            let target = f.temp_path().join(directory);
            fs::create_dir_all(&target).unwrap();
            f.library
                .manager()
                .export_release(
                    release_id,
                    &target,
                    OutputKind::Save {
                        preset: flac.clone(),
                        filename_pattern: SaveFilenamePattern::default(),
                        folder_pattern: pattern,
                        extras: SaveExtras::default(),
                    },
                )
                .await
                .expect("the release save succeeds");
            let entries: Vec<_> = fs::read_dir(&target).unwrap().collect();
            assert_eq!(entries.len(), 1, "one folder per save");
            entries[0].as_ref().unwrap().file_name().to_string_lossy().to_string()
        }
    };

    let album_only = NamePattern::new(vec![NamePiece::Value(SaveFolderValue::Album)]);
    assert_eq!(saved_folder(album_only, "by-album").await, album.title);
    assert_ne!(album.title, "source folder", "the name is not the import's folder");
    let label_only = NamePattern::new(vec![NamePiece::Value(SaveFolderValue::Label)]);
    assert_eq!(
        saved_folder(label_only, "by-label").await,
        album.title,
        "a pattern that renders nothing names the folder by the album"
    );
    // A pattern that can't make a name refuses the save and writes nothing.
    let slash = NamePattern::new(vec![
        NamePiece::Value(SaveFolderValue::Album),
        NamePiece::Text(" / ".to_string()),
    ]);
    for (invalid, directory) in [(NamePattern::new(Vec::new()), "by-nothing"), (slash, "by-slash")]
    {
        let target = f.temp_path().join(directory);
        fs::create_dir_all(&target).unwrap();
        let refused = manager
            .export_release(
                &release_id,
                &target,
                OutputKind::Save {
                    preset: flac.clone(),
                    filename_pattern: SaveFilenamePattern::default(),
                    folder_pattern: invalid,
                    extras: SaveExtras::default(),
                },
            )
            .await;
        assert!(
            matches!(refused, Err(bae_library::LibraryError::FolderPattern(_))),
            "the save is refused for its folder pattern: {refused:?}"
        );
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0, "nothing was written");
    }
}

/// The one filename pattern names every file a save writes, whatever the
/// format: a single track's suggested name and each file of a release save in
/// FLAC and in MP3 come from it, and setting the pattern changes all of them.
/// The suggestion is DB-only — no audio or cloud read.
#[tokio::test]
async fn one_filename_pattern_names_every_format() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "02.flac", "Track Two");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.library.records().get_tracks_for_release(&release_id).await.unwrap();
    let manager = f.library.manager();

    let saves = std::sync::atomic::AtomicUsize::new(0);
    let saved_names = |preset: SavePreset, pattern: SaveFilenamePattern| {
        let (f, release_id) = (&f, &release_id);
        let save = saves.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        async move {
            let target = f.temp_path().join(format!("out-{}-{save}", preset.id));
            fs::create_dir_all(&target).unwrap();
            f.library
                .manager()
                .export_release(
                    release_id,
                    &target,
                    OutputKind::Save {
                        preset,
                        filename_pattern: pattern,
                        folder_pattern: SaveFolderPattern::default(),
                        extras: SaveExtras::default(),
                    },
                )
                .await
                .expect("the release save succeeds");
            let folder = fs::read_dir(&target).unwrap().next().unwrap().unwrap().path();
            let mut names: Vec<String> = fs::read_dir(folder)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
                .filter(|name| name != ".bae-output")
                .collect();
            names.sort();
            names
        }
    };
    let presets = manager.save_presets().await.unwrap();
    let flac = presets.iter().find(|preset| preset.id == "flac").unwrap().clone();
    let mp3 = presets.iter().find(|preset| preset.id == "mp3").unwrap().clone();

    // A new library names files by track number, a space, then title.
    let default_pattern = manager.preferences().await.unwrap().save_filename_pattern;
    assert_eq!(default_pattern, SaveFilenamePattern::default());
    assert_eq!(manager.save_track_suggested_name(&tracks[0].id).await.unwrap(), "01 Track One");
    assert_eq!(
        saved_names(flac.clone(), default_pattern.clone()).await,
        vec!["01 Track One.flac", "01 Track Two.flac"]
    );
    assert_eq!(
        saved_names(mp3.clone(), default_pattern).await,
        vec!["01 Track One.mp3", "01 Track Two.mp3"]
    );

    // Set once, the new pattern names a track's suggestion and both formats'
    // release saves, its typed text as typed.
    let pattern = NamePattern::new(vec![
        NamePiece::Value(SaveFilenameValue::TrackNumber),
        NamePiece::Text(" -  ".to_string()),
        NamePiece::Value(SaveFilenameValue::Title),
    ]);
    manager.set_save_filename_pattern(pattern.clone()).await.unwrap();
    assert_eq!(manager.preferences().await.unwrap().save_filename_pattern, pattern);
    assert_eq!(manager.save_track_suggested_name(&tracks[0].id).await.unwrap(), "01 -  Track One");
    assert_eq!(
        saved_names(flac.clone(), pattern.clone()).await,
        vec!["01 -  Track One.flac", "01 -  Track Two.flac"]
    );
    assert_eq!(
        saved_names(mp3.clone(), pattern).await,
        vec!["01 -  Track One.mp3", "01 -  Track Two.mp3"]
    );

    // A pattern that can't make a name is kept as typed, and saves are refused
    // until it is fixed.
    let invalid = NamePattern::new(vec![
        NamePiece::Value(SaveFilenameValue::Title),
        NamePiece::Text("/".to_string()),
    ]);
    manager.set_save_filename_pattern(invalid.clone()).await.unwrap();
    assert_eq!(manager.preferences().await.unwrap().save_filename_pattern, invalid);
    assert!(matches!(
        manager.save_track_suggested_name(&tracks[0].id).await,
        Err(bae_library::LibraryError::FilenamePattern(_))
    ));
    let target = f.temp_path().join("out-refused");
    fs::create_dir_all(&target).unwrap();
    let refused = manager
        .export_release(
            &release_id,
            &target,
            OutputKind::Save {
                preset: flac,
                filename_pattern: invalid.clone(),
                folder_pattern: SaveFolderPattern::default(),
                extras: SaveExtras::default(),
            },
        )
        .await;
    assert!(
        matches!(refused, Err(bae_library::LibraryError::FilenamePattern(_))),
        "the save is refused for its file name pattern: {refused:?}"
    );
    manager
        .set_save_filename_pattern(NamePattern::new(vec![NamePiece::Text(String::new())]))
        .await
        .expect_err("typed text of nothing is refused");
    assert_eq!(manager.preferences().await.unwrap().save_filename_pattern, invalid);
}
