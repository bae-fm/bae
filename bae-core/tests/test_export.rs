#![cfg(feature = "test-utils")]
//! Export of cloud-only (unpinned remote) releases.
//!
//! Export must not require a local copy: when this device holds none, the
//! bytes are downloaded from the cloud home and decrypted with the release's
//! item key — the same verified read pin and unmanage use.

use bae_test_support as support;

use bae_core::config::{
    SaveBitDepth, SaveCodec, SaveFilenameToken, SavePregapPlacement, SavePreset,
};
use bae_core::db::Database;
use bae_core::import::{IdentityChoice, ImportCommand, StorageMode};
use bae_core::library::{LibraryManager, OutputKind};
use coven::EncryptionService;
use coven::InMemoryCloudHome;
use coven::StoreDir;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

struct ExportFixture {
    mgr: LibraryManager,
    handle: bae_core::import::ImportServiceHandle,
    cloud: Arc<InMemoryCloudHome>,
    _temp: TempDir,
}

/// The cloud object key coven addresses a release file's blob by. The export
/// fixture's cloud home is the default opaque home, so the blob is sharded by id
/// under the `release_files` namespace (Hashed scheme, no readable cloud_path).
/// Removing the mock cloud at this key matches what coven reads.
async fn cloud_key(mgr: &LibraryManager, file: &bae_core::db::DbFile) -> String {
    mgr.resolve_track_cloud_key_for_test(&file.id).await
}

impl ExportFixture {
    async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();

        let db = Database::new_test(
            db_dir.join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let library_dir = StoreDir::new(db_dir.clone());
        let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
        let mgr = LibraryManager::new(
            db.clone(),
            config_handle,
            key_service,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
        );
        let cloud = Arc::new(InMemoryCloudHome::new());
        mgr.connect_test_cloud_home(
            cloud.clone(),
            bae_core::sync::CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
        )
        .await
        .unwrap();

        let handle = bae_core::import::ImportService::start(
            tokio::runtime::Handle::current(),
            mgr.clone(),
            bae_core::import::cover_art::CoverArtArchiveClient::hermetic(),
        );

        Self {
            mgr,
            handle,
            cloud,
            _temp: temp,
        }
    }

    fn temp_path(&self) -> &Path {
        self._temp.path()
    }
}

/// Import a one-track album from `album_dir` as Local with Unknown
/// identity (file tags only — no network), then flip it to cloud-only:
/// remote with no local copy, encrypted blobs seeded in the mock cloud,
/// originals deleted. This is the state export must handle: no local bytes,
/// audio only in the cloud.
async fn import_then_strand_in_cloud(f: &ExportFixture, album_dir: &Path) -> (String, Vec<u8>) {
    let import_id = "import-then-strand-in-cloud".to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.to_path_buf(),
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let files = f.mgr.get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 1);
    let original_bytes = fs::read(album_dir.join(&files[0].original_filename)).unwrap();

    f.mgr.coven_make_remote(&release_id, false).await.unwrap();
    let uploaded = f.mgr.drain_uploads_for_test().await.unwrap();
    assert_eq!(uploaded, files.len(), "each release blob uploaded");

    (release_id, original_bytes)
}

async fn import_unknown_local(f: &ExportFixture, album_dir: &Path) -> String {
    let import_id = "import-unknown-local".to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.to_path_buf(),
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;
    release_id
}

/// Exporting a single track of a cloud-only release downloads + decrypts the
/// audio and re-encodes it — no "pin the release before exporting" dead-end.
#[tokio::test]
async fn export_track_from_cloud_only_release() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let original_bytes = support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, _) = import_then_strand_in_cloud(&f, &album_dir).await;

    let tracks = f.mgr.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);

    let out = f.temp_path().join("exported.flac");
    f.mgr
        .save_track(&tracks[0].id, &out, "flac")
        .await
        .expect("cloud-only track must save");

    // FLAC is lossless: the exported file decodes to the same PCM as the
    // cloud copy.
    let exported = fs::read(&out).unwrap();
    let exported_pcm = bae_core::audio_codec::decode_audio(&exported, None, None).unwrap();
    let original_pcm = bae_core::audio_codec::decode_audio(&original_bytes, None, None).unwrap();
    assert_eq!(exported_pcm.samples, original_pcm.samples);
}

/// The track-save filename suggestion renders the *selected* preset's token
/// pattern: two presets with different patterns give different stems for the
/// same track. DB-only — no audio or cloud read.
#[tokio::test]
async fn save_track_suggested_name_uses_the_preset_tokens() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.mgr.get_tracks_for_release(&release_id).await.unwrap();

    let mut presets = f.mgr.save_presets();
    presets.push(SavePreset {
        id: "title-only".to_string(),
        name: "Title only".to_string(),
        codec: SaveCodec::Flac {
            bit_depth: SaveBitDepth::Source,
        },
        filename_tokens: vec![SaveFilenameToken::Title],
        pregap_placement: SavePregapPlacement::AppendToPreviousExceptHtoa,
        applies_to_track: true,
        applies_to_release: false,
        embed_cover: true,
    });
    presets.push(SavePreset {
        id: "num-title".to_string(),
        name: "Number and title".to_string(),
        codec: SaveCodec::Flac {
            bit_depth: SaveBitDepth::Source,
        },
        filename_tokens: vec![SaveFilenameToken::TrackNumber, SaveFilenameToken::Title],
        pregap_placement: SavePregapPlacement::AppendToPreviousExceptHtoa,
        applies_to_track: true,
        applies_to_release: false,
        embed_cover: true,
    });
    f.mgr.set_save_presets(presets).unwrap();

    let title_only = f
        .mgr
        .save_track_suggested_name(&tracks[0].id, "title-only")
        .await
        .expect("title-only preset renders a stem");
    let num_title = f
        .mgr
        .save_track_suggested_name(&tracks[0].id, "num-title")
        .await
        .expect("num-title preset renders a stem");

    assert_eq!(title_only, "Track One");
    assert_ne!(
        title_only, num_title,
        "a different token pattern must yield a different stem"
    );
    assert!(
        num_title.contains("Track One"),
        "the number-and-title stem still contains the title: {num_title}"
    );

    // An unknown preset id can't back a track suggestion.
    let err = f
        .mgr
        .save_track_suggested_name(&tracks[0].id, "no-such-preset")
        .await;
    assert!(err.is_err(), "an unknown preset id is rejected");
}

/// Plan assembly reads the cover only when the preset embeds: with art on the
/// release, `embed_cover: true` carries the bytes and `false` carries none — the
/// blob read is skipped entirely.
#[tokio::test]
async fn get_save_track_plan_skips_cover_read_when_not_embedding() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    // A folder cover so the imported release actually has art to embed.
    support::write_cover_png(&album_dir.join("cover.png"));
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.mgr.get_tracks_for_release(&release_id).await.unwrap();

    let with_cover = f
        .mgr
        .get_save_track_plan(&tracks[0].id, true)
        .await
        .expect("plan with embedding");
    assert!(
        with_cover.cover_image_bytes.is_some(),
        "embedding reads the release's cover"
    );

    let without_cover = f
        .mgr
        .get_save_track_plan(&tracks[0].id, false)
        .await
        .expect("plan without embedding");
    assert!(
        without_cover.cover_image_bytes.is_none(),
        "not embedding skips the cover read"
    );
}

/// Exporting a whole cloud-only release downloads each file and writes the
/// raw bytes — byte-identical to what was uploaded.
#[tokio::test]
async fn export_release_from_cloud_only_release() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, original_bytes) = import_then_strand_in_cloud(&f, &album_dir).await;

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    f.mgr
        .export_release(&release_id, &target, OutputKind::Export)
        .await
        .expect("cloud-only release must export");

    // One subfolder, containing the file byte-identical to the cloud copy.
    let subdir = fs::read_dir(&target)
        .unwrap()
        .next()
        .expect("export wrote a release folder")
        .unwrap()
        .path();
    assert!(subdir.join(".bae-output").exists());
    let written = fs::read(subdir.join("01.flac")).unwrap();
    assert_eq!(written, original_bytes);
}

#[tokio::test]
async fn export_release_single_file_with_cue_writes_image_and_cue() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let first_bytes = support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let second_bytes = support::write_tagged_flac(&album_dir, "02.flac", "Track Two");
    let release_id = import_unknown_local(&f, &album_dir).await;

    let image_preset = SavePreset {
        id: "flac-image".to_string(),
        name: "FLAC image".to_string(),
        codec: SaveCodec::Flac {
            bit_depth: SaveBitDepth::Source,
        },
        filename_tokens: vec![SaveFilenameToken::TrackNumber, SaveFilenameToken::Title],
        pregap_placement: SavePregapPlacement::SingleFileWithCue,
        applies_to_track: false,
        applies_to_release: true,
        embed_cover: true,
    };
    let mut presets = f.mgr.save_presets();
    presets.push(image_preset.clone());
    f.mgr.set_save_presets(presets).unwrap();

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    f.mgr
        .export_release(
            &release_id,
            &target,
            OutputKind::Save {
                preset: image_preset,
            },
        )
        .await
        .expect("single-file CUE release save must succeed");

    let subdir = fs::read_dir(&target)
        .unwrap()
        .next()
        .expect("export wrote a release folder")
        .unwrap()
        .path();
    assert!(subdir.join(".bae-output").exists());
    let mut exported_files: Vec<_> = fs::read_dir(&subdir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| name != ".bae-output")
        .collect();
    exported_files.sort();
    assert_eq!(exported_files, vec!["album.cue", "album.flac"]);

    let cue = fs::read_to_string(subdir.join("album.cue")).unwrap();
    assert!(cue.contains("FILE \"album.flac\" WAVE"));
    assert!(cue.contains("  TRACK 01 AUDIO"));
    assert!(cue.contains("  TRACK 02 AUDIO"));

    let image_pcm = bae_core::audio_codec::decode_audio(
        &fs::read(subdir.join("album.flac")).unwrap(),
        None,
        None,
    )
    .unwrap();
    let first_pcm = bae_core::audio_codec::decode_audio(&first_bytes, None, None).unwrap();
    let second_pcm = bae_core::audio_codec::decode_audio(&second_bytes, None, None).unwrap();
    assert_eq!(
        image_pcm.samples.len(),
        first_pcm.samples.len() + second_pcm.samples.len()
    );
}

/// A cloud-only release whose blob is missing exports nothing and errors —
/// no partial silent success.
#[tokio::test]
async fn export_release_missing_blob_is_hard_error() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, _) = import_then_strand_in_cloud(&f, &album_dir).await;

    // Blow away the seeded blob.
    let files = f.mgr.get_files_for_release(&release_id).await.unwrap();
    f.cloud.remove(&cloud_key(&f.mgr, &files[0]).await);

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    let result = f
        .mgr
        .export_release(&release_id, &target, OutputKind::Export)
        .await;
    assert!(result.is_err(), "missing blob must fail the export");
    assert!(
        fs::read_dir(&target).unwrap().next().is_none(),
        "failed export must leave no release folder or marker"
    );
}
