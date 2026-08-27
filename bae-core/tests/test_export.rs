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
use bae_core::import::{ImportCommand, MetadataSeed, StorageMode};
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

/// Remove one release file's cloud object from the mock home.
///
/// coven keys an exact object by its locator hash — `release_files/opaque/<hash>`
/// — so a test cannot name the object from the blob id. Read the blob through
/// coven once, which records the slot that read touched, remove exactly that
/// slot, then drop the cache copy the read just populated so the next read has
/// to go back to the (now empty) cloud.
async fn remove_cloud_blob(mgr: &LibraryManager, cloud: &InMemoryCloudHome, file_id: &str) {
    let blob = mgr
        .release_blob_ref_for_test(file_id)
        .await
        .expect("the release_files row");
    cloud.clear_exact_reads();
    mgr.materialize_release_blob_for_test(file_id)
        .await
        .expect("the blob is readable before it is removed");
    let slots = cloud.exact_reads();
    assert_eq!(slots.len(), 1, "one exact read for one blob");
    cloud.remove_exact_object(&slots[0]);
    mgr.evict_blob_for_test(&blob)
        .await
        .expect("drop the cache copy the probe read populated");
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
        let config_handle = support::test_config(&library_dir);
        let mgr = LibraryManager::new(
            db.clone(),
            config_handle,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );
        let cloud = Arc::new(InMemoryCloudHome::new());
        mgr.connect_test_cloud_home_caller_driven(
            cloud.clone(),
            bae_core::sync::CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
        )
        .await
        .unwrap();

        let handle = mgr
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .expect("import service starts");

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
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_seed: MetadataSeed::FileTags,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let files = f.mgr.get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 1);
    let original_bytes = fs::read(album_dir.join(&files[0].original_filename)).unwrap();

    f.mgr.coven_make_remote(&release_id, false).await.unwrap();
    let uploaded = f.mgr.drain_uploads_expecting_work().await.unwrap();
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
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_seed: MetadataSeed::FileTags,
            user_edit: None,
        })
        .await
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
    let exported_pcm =
        bae_core::audio_codec::decode_audio(buffer_from(&exported), None, None).unwrap();
    let original_pcm =
        bae_core::audio_codec::decode_audio(buffer_from(&original_bytes), None, None).unwrap();
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
        with_cover.has_cover_image_for_test(),
        "embedding reads the release's cover"
    );

    let without_cover = f
        .mgr
        .get_save_track_plan(&tracks[0].id, false)
        .await
        .expect("plan without embedding");
    assert!(
        !without_cover.has_cover_image_for_test(),
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
        buffer_from(&fs::read(subdir.join("album.flac")).unwrap()),
        None,
        None,
    )
    .unwrap();
    let first_pcm =
        bae_core::audio_codec::decode_audio(buffer_from(&first_bytes), None, None).unwrap();
    let second_pcm =
        bae_core::audio_codec::decode_audio(buffer_from(&second_bytes), None, None).unwrap();
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
    remove_cloud_blob(&f.mgr, &f.cloud, &files[0].id).await;

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

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> bae_core::playback::SharedSparseBuffer {
    let buffer = bae_core::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}
