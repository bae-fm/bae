//! Export of cloud-only (unpinned remote) releases.
//!
//! Export must not require a local copy: when this device holds none, the
//! bytes are downloaded from storage and decrypted with the release's
//! item key — the same verified read pin and unmanage use.

use bae_preferences::{
    NamePattern, NamePiece, SaveBitDepth, SaveCodec, SaveExtras, SaveFilenamePattern,
    SaveFilenameValue, SaveFolderPattern, SavePreset,
};
use bae_transfers::OutputKind;
use coven::MemoryStorage;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

pub(super) struct ExportFixture {
    pub(super) library: bae_library::test_libraries::TestLibrary,
    handle: bae_import::ImportServiceHandle,
    cloud: Arc<MemoryStorage>,
    _temp: TempDir,
}

impl ExportFixture {
    pub(super) async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();

        let (library, cloud) = bae_library::test_libraries::storage_library(
            &db_dir,
            bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
            bae_util::http::Http::for_test(),
        )
        .await;
        bae_library::test_libraries::connect_storage(library.manager(), library.store(), &cloud)
            .await;
        let handle = library.start_import().await.expect("test import service starts");

        Self { library, handle, cloud, _temp: temp }
    }

    pub(super) fn temp_path(&self) -> &Path {
        self._temp.path()
    }
}

/// Import a one-track album from `album_dir` as Local with Unknown
/// identity (file metadata only — no network), then flip it to cloud-only:
/// remote with no local copy, encrypted files seeded in the mock cloud,
/// originals deleted. This is the state export must handle: no local bytes,
/// audio only in the cloud.
async fn import_then_strand_in_cloud(f: &ExportFixture, album_dir: &Path) -> (String, Vec<u8>) {
    let import_id = "import-then-strand-in-cloud".to_string();
    f.handle
        .send_command(bae_import::test_imports::folder_import(
            &import_id,
            album_dir.to_path_buf(),
            bae_import::test_imports::DraftSource::FileTags,
        ))
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;

    let files = f.library.manager().get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 1);
    let original_bytes = fs::read(album_dir.join(&files[0].original_filename)).unwrap();

    f.library.manager().upload_release(&release_id).await.unwrap();
    let uploaded = f.library.manager().drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, files.len(), "each release file uploaded");

    for file in files {
        f.library.manager().evict_file_for_test(file.file).await.unwrap();
    }
    fs::remove_dir_all(album_dir).unwrap();
    (release_id, original_bytes)
}

pub(super) async fn import_unknown_local(f: &ExportFixture, album_dir: &Path) -> String {
    let import_id = "import-unknown-local".to_string();
    f.handle
        .send_command(bae_import::test_imports::folder_import(
            &import_id,
            album_dir.to_path_buf(),
            bae_import::test_imports::DraftSource::FileTags,
        ))
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) =
        bae_import::test_imports::wait_for_import_complete(&mut progress_rx).await;
    release_id
}

/// Exporting a single track of a cloud-only release downloads + decrypts the
/// audio and re-encodes it — no "pin the release before exporting" dead-end.
#[tokio::test]
async fn export_track_from_cloud_only_release() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let original_bytes =
        bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, _) = import_then_strand_in_cloud(&f, &album_dir).await;

    let tracks = f.library.records().get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);

    let out = f.temp_path().join("exported.flac");
    f.library
        .manager()
        .save_track(&tracks[0].id, &out, "flac")
        .await
        .expect("cloud-only track must save");

    // FLAC is lossless: the exported file decodes to the same PCM as the
    // cloud copy.
    let exported = fs::read(&out).unwrap();
    let exported_pcm = bae_audio::decode_audio(
        bae_audio::test_samples::filled_sparse_buffer(&exported),
        None,
        None,
    )
    .unwrap();
    let original_pcm = bae_audio::decode_audio(
        bae_audio::test_samples::filled_sparse_buffer(&original_bytes),
        None,
        None,
    )
    .unwrap();
    assert_eq!(exported_pcm.samples, original_pcm.samples);
}

/// The bit depth a saved file's header states.
fn saved_bit_depth(path: &Path) -> u8 {
    use lofty::file::AudioFile;
    lofty::read_from_path(path)
        .expect("the saved file reads")
        .properties()
        .bit_depth()
        .expect("a lossless file states its depth")
}

/// A lossless format set to the source's depth writes each track at the depth
/// it was stored at, a 16-bit CD rip at 16 and a 24-bit file at 24; set to
/// 16-bit it writes 16 for both.
#[tokio::test]
async fn a_lossless_format_writes_the_depth_of_the_track_it_saves() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let silence_24_bit = vec![0u8; 44_100 * 2 * 3];
    fs::write(
        album_dir.join("02.wav"),
        bae_audio::test_samples::wav_with_fmt(1, 24, 2, 44_100, &silence_24_bit),
    )
    .unwrap();
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.library.records().get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2);
    let manager = f.library.manager();

    let mut presets = manager.save_presets().await.unwrap();
    let same_as_source = presets.iter().find(|preset| preset.id == "flac").unwrap().clone();
    assert_eq!(same_as_source.codec, SaveCodec::Flac { bit_depth: SaveBitDepth::SameAsSource });
    let sixteen = SavePreset {
        id: "sixteen".to_string(),
        name: "16-bit".to_string(),
        codec: SaveCodec::Flac { bit_depth: SaveBitDepth::Bits16 },
    };
    presets.push(sixteen.clone());
    manager.set_save_presets(presets).await.unwrap();

    let depths = |preset: &SavePreset| {
        let (f, tracks, preset) = (&f, &tracks, preset.id.clone());
        async move {
            let mut depths = Vec::new();
            for (index, track) in tracks.iter().enumerate() {
                let out = f.temp_path().join(format!("{preset}-{index}.flac"));
                f.library
                    .manager()
                    .save_track(&track.id, &out, &preset)
                    .await
                    .expect("the track saves");
                depths.push(saved_bit_depth(&out));
            }
            depths
        }
    };
    assert_eq!(depths(&same_as_source).await, vec![16, 24]);
    assert_eq!(depths(&sixteen).await, vec![16, 16]);
}

/// A release save writes the release's cover as `cover.jpg` in its folder when
/// the extras ask for it, and not otherwise.
#[tokio::test]
async fn a_release_save_writes_the_cover_file_only_when_asked() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    bae_import::test_fixtures::write_cover_png(&album_dir.join("cover.png"));
    let release_id = import_unknown_local(&f, &album_dir).await;
    let flac = f.library.manager().save_presets().await.unwrap().remove(0);

    let save = |cover_file: bool, directory: &str| {
        let (f, release_id, flac) = (&f, &release_id, &flac);
        let target = f.temp_path().join(directory);
        async move {
            fs::create_dir_all(&target).unwrap();
            f.library
                .manager()
                .export_release(
                    release_id,
                    &target,
                    OutputKind::Save {
                        preset: flac.clone(),
                        filename_pattern: SaveFilenamePattern::default(),
                        folder_pattern: SaveFolderPattern::default(),
                        extras: SaveExtras { embed_cover: true, cover_file, loudness_tags: true },
                    },
                )
                .await
                .expect("the release save succeeds");
            let folder = fs::read_dir(&target).unwrap().next().unwrap().unwrap().path();
            folder.join("cover.jpg")
        }
    };

    let cover = save(true, "with-cover-file").await;
    let bytes = fs::read(&cover).expect("the folder holds cover.jpg");
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "the cover file is a JPEG");
    assert!(!save(false, "without-cover-file").await.exists(), "no cover file when not asked");
}

/// Saved files carry the release's measured loudness as ReplayGain tags when
/// the extras ask for it, and none otherwise.
#[tokio::test]
async fn a_saved_track_carries_replaygain_tags_only_when_asked() {
    use lofty::prelude::*;
    use lofty::tag::ItemKey;

    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.library.records().get_tracks_for_release(&release_id).await.unwrap();
    let manager = f.library.manager();
    let gain_of = |path: &Path| {
        let file = lofty::read_from_path(path).unwrap();
        let tag = file.primary_tag().expect("the saved file is tagged");
        (
            tag.get_string(ItemKey::ReplayGainTrackGain).map(str::to_string),
            tag.get_string(ItemKey::ReplayGainTrackPeak).map(str::to_string),
            tag.get_string(ItemKey::ReplayGainAlbumGain).map(str::to_string),
        )
    };

    let with_tags = f.temp_path().join("with.flac");
    manager.save_track(&tracks[0].id, &with_tags, "flac").await.unwrap();
    let (track_gain, track_peak, album_gain) = gain_of(&with_tags);
    assert!(track_gain.expect("a track gain").ends_with(" dB"));
    assert!(track_peak.is_some() && album_gain.is_some());

    manager.set_save_loudness_tags(false).await.unwrap();
    let without_tags = f.temp_path().join("without.flac");
    manager.save_track(&tracks[0].id, &without_tags, "flac").await.unwrap();
    assert_eq!(gain_of(&without_tags), (None, None, None));
}

/// A single track's save records its format as the one the next save starts
/// with, the same as a release save does.
#[tokio::test]
async fn a_track_save_records_its_format_as_last_used() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.library.records().get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(f.library.manager().preferences().await.unwrap().last_save_preset, "flac");

    let out = f.temp_path().join("saved.mp3");
    f.library.manager().save_track(&tracks[0].id, &out, "mp3").await.expect("track saves");

    assert_eq!(f.library.manager().preferences().await.unwrap().last_save_preset, "mp3");
}

/// A save reads the cover for embedding only when asked: with art on the
/// release, `true` carries the bytes and `false` carries none — the file read
/// is skipped entirely.
#[tokio::test]
async fn save_cover_image_skips_the_read_when_not_embedding() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    // A folder cover so the imported release actually has art to embed.
    bae_import::test_fixtures::write_cover_png(&album_dir.join("cover.png"));
    let release_id = import_unknown_local(&f, &album_dir).await;

    let with_cover = f
        .library
        .manager()
        .save_cover_image(&release_id, true)
        .await
        .expect("cover when embedding");
    assert!(with_cover.is_some(), "embedding reads the release's cover");

    let without_cover = f
        .library
        .manager()
        .save_cover_image(&release_id, false)
        .await
        .expect("no cover when not embedding");
    assert!(without_cover.is_none(), "not embedding skips the cover read");
}

/// Exporting a whole cloud-only release downloads each file and writes the
/// raw bytes — byte-identical to what was uploaded.
#[tokio::test]
async fn export_release_from_cloud_only_release() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, original_bytes) = import_then_strand_in_cloud(&f, &album_dir).await;

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    f.library
        .manager()
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

/// `Track?.flac` and `Track？.flac` are two names, and coven writes both as
/// `Track？.flac`: the export fails rather than write one file over the
/// other, and leaves nothing behind. Windows refuses `?` in a name, so the
/// album can't be made there.
#[cfg(not(windows))]
#[tokio::test]
async fn two_names_written_as_one_fail_the_export() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "Track?.flac", "Track One");
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "Track？.flac", "Track Two");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let mut names: Vec<String> = f
        .library
        .manager()
        .get_files_for_release(&release_id)
        .await
        .unwrap()
        .into_iter()
        .map(|file| file.original_filename)
        .collect();
    names.sort();
    assert_eq!(names, ["Track?.flac", "Track？.flac"]);

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    let error = f
        .library
        .manager()
        .export_release(&release_id, &target, OutputKind::Export)
        .await
        .expect_err("two files written at one path fail the export");

    assert!(error.to_string().contains("Track？.flac"), "the error names the path: {error}");
    assert!(
        fs::read_dir(&target).unwrap().next().is_none(),
        "the failed export leaves no release folder or staging folder"
    );
}

/// Whether `dir`'s drive holds names that differ only in case as one, as the
/// default macOS and Windows volumes do.
fn folds_case(dir: &Path) -> bool {
    let probe = dir.join("case-probe");
    fs::write(&probe, b"").unwrap();
    let folds = dir.join("CASE-PROBE").exists();
    fs::remove_file(&probe).unwrap();
    folds
}

/// A FLAC file's audio frames: what follows its metadata blocks.
fn flac_frames(bytes: &[u8]) -> &[u8] {
    assert_eq!(&bytes[..4], b"fLaC", "a FLAC file starts with its marker");
    let mut at = 4;
    loop {
        let last = bytes[at] & 0x80 != 0;
        let length = u32::from_be_bytes([0, bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        at += 4 + length;
        if last {
            return &bytes[at..];
        }
    }
}

/// Write a tagged copy of a FLAC file made by libFLAC, whose frames FFmpeg's
/// encoder does not reproduce, into `dir`, carrying a comment that a save never
/// writes. A copy of its audio keeps the frames and none of the tags.
fn write_libflac_track(dir: &Path, filename: &str, title: &str) -> Vec<u8> {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let destination = dir.join(filename);
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/libflac-level-0.flac"),
        &destination,
    )
    .unwrap();
    let mut file = lofty::read_from_path(&destination).unwrap();
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist("Artist Name".to_string());
    tag.set_album("Album Title".to_string());
    tag.insert_text(ItemKey::AlbumArtist, "Artist Name".to_string());
    tag.set_track(1);
    tag.insert_text(ItemKey::Comment, "the source's own comment".to_string());
    file.insert_tag(tag);
    file.save_to_path(&destination, WriteOptions::default()).unwrap();
    fs::read(destination).unwrap()
}

/// A track already stored as the format's codec at the depth it writes is
/// copied: its audio frames are the source's own bytes, while its tags, cover
/// and name are written fresh (a comment only the source carries is gone, the
/// release's cover is embedded, the name comes from the pattern). A track in
/// another codec or at another depth in the same release is converted.
#[tokio::test]
async fn a_release_save_copies_a_track_already_in_the_format_and_converts_the_rest() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let source = write_libflac_track(&album_dir, "01.flac", "Track One");
    let silence_24_bit = vec![0u8; 44_100 * 2 * 3];
    fs::write(
        album_dir.join("02.wav"),
        bae_audio::test_samples::wav_with_fmt(1, 24, 2, 44_100, &silence_24_bit),
    )
    .unwrap();
    bae_import::test_fixtures::write_cover_png(&album_dir.join("cover.png"));
    let release_id = import_unknown_local(&f, &album_dir).await;
    let flac = f
        .library
        .manager()
        .save_presets()
        .await
        .unwrap()
        .into_iter()
        .find(|preset| preset.id == "flac")
        .unwrap();

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    f.library
        .manager()
        .export_release(
            &release_id,
            &target,
            OutputKind::Save {
                preset: flac,
                filename_pattern: SaveFilenamePattern::default(),
                folder_pattern: SaveFolderPattern::default(),
                extras: SaveExtras::default(),
            },
        )
        .await
        .expect("the release saves");
    let folder = fs::read_dir(&target).unwrap().next().unwrap().unwrap().path();

    let copied = fs::read(folder.join("01 Track One.flac")).unwrap();
    assert!(flac_frames(&copied) == flac_frames(&source), "the audio is the source's own bytes");

    use lofty::prelude::*;
    let tagged = lofty::read_from_path(folder.join("01 Track One.flac")).unwrap();
    let tag = tagged.primary_tag().expect("the copy is tagged");
    assert_eq!(tag.title().as_deref(), Some("Track One"));
    assert_eq!(tag.artist().as_deref(), Some("Artist Name"));
    assert_eq!(tag.get_string(ItemKey::Comment), None, "the source's tags are not carried");
    assert_eq!(tag.picture_count(), 1, "the release's cover is embedded");

    // The 24-bit WAV is not FLAC: it is converted, at its own depth.
    let converted = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.file_name().unwrap().to_string_lossy().starts_with("02 "))
        .expect("the second track is saved");
    assert_eq!(saved_bit_depth(&converted), 24);
}

/// A single track's save copies a track already in the format too, and
/// replaces nothing of the source.
#[tokio::test]
async fn a_track_save_copies_a_track_already_in_the_format() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let source = write_libflac_track(&album_dir, "01.flac", "Track One");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.library.records().get_tracks_for_release(&release_id).await.unwrap();

    let out = f.temp_path().join("one.flac");
    f.library.manager().save_track(&tracks[0].id, &out, "flac").await.expect("the track saves");

    assert!(flac_frames(&fs::read(&out).unwrap()) == flac_frames(&source), "the audio is copied");
}

/// A WAV track saved as WAV keeps its depth and its audio whole, and at
/// 16-bit it is converted down: a WAV is copied only at the depth it writes.
#[tokio::test]
async fn a_wav_track_saves_as_wav_at_its_own_depth_or_16_bits() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let tone: Vec<u8> = (0..44_100u32 * 2)
        .flat_map(|frame| {
            let sample = ((f64::from(frame) * 0.01).sin() * 4_000_000.0) as i32;
            sample.to_le_bytes()[..3].to_vec()
        })
        .collect();
    let source = bae_audio::test_samples::wav_with_fmt(1, 24, 2, 44_100, &tone);
    // The release's artist comes from a tagged file beside the untagged WAV.
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    fs::write(album_dir.join("02.wav"), &source).unwrap();
    let release_id = import_unknown_local(&f, &album_dir).await;
    let tracks = f.library.records().get_tracks_for_release(&release_id).await.unwrap();
    let manager = f.library.manager();
    let wav = |id: &str, bit_depth| SavePreset {
        id: id.to_string(),
        name: id.to_string(),
        codec: SaveCodec::Wav { bit_depth },
    };
    let mut presets = manager.save_presets().await.unwrap();
    presets.push(wav("same", SaveBitDepth::SameAsSource));
    presets.push(wav("sixteen", SaveBitDepth::Bits16));
    manager.set_save_presets(presets).await.unwrap();

    let same = f.temp_path().join("same.wav");
    manager.save_track(&tracks[1].id, &same, "same").await.expect("the track saves");
    let sixteen = f.temp_path().join("sixteen.wav");
    manager.save_track(&tracks[1].id, &sixteen, "sixteen").await.expect("the track saves");

    assert_eq!(saved_bit_depth(&same), 24);
    assert_eq!(saved_bit_depth(&sixteen), 16);
    let samples = |bytes: &[u8]| {
        bae_audio::decode_audio(bae_audio::test_samples::filled_sparse_buffer(bytes), None, None)
            .unwrap()
            .samples
    };
    assert!(samples(&fs::read(&same).unwrap()) == samples(&source), "the audio is whole");
}

/// A release stored as one image with a CUE sheet saves as one file per track,
/// cut at the sheet's marks by one rule: a track's gap goes at the end of the
/// track before it, and hidden audio before track 1 stays at the start of
/// track 1. The 30 second image has 2 seconds of hidden audio before track 1,
/// and a 2 second gap before track 2.
#[tokio::test]
async fn a_cue_image_saves_one_file_per_track_with_each_gap_before_its_track() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    fs::copy(
        bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac"),
        album_dir.join("Test Album.flac"),
    )
    .unwrap();
    fs::write(
        album_dir.join("Test Album.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\nFILE \"Test Album.flac\" WAVE\n\
         \x20 TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n\
         \x20 TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 00 00:08:00\n    INDEX 01 00:10:00\n\
         \x20 TRACK 03 AUDIO\n    TITLE \"Track Three\"\n    INDEX 01 00:20:00\n",
    )
    .unwrap();
    let release_id = import_unknown_local(&f, &album_dir).await;
    let flac = f
        .library
        .manager()
        .save_presets()
        .await
        .unwrap()
        .into_iter()
        .find(|preset| preset.id == "flac")
        .unwrap();

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    f.library
        .manager()
        .export_release(
            &release_id,
            &target,
            OutputKind::Save {
                preset: flac,
                filename_pattern: SaveFilenamePattern::default(),
                folder_pattern: SaveFolderPattern::default(),
                extras: SaveExtras::default(),
            },
        )
        .await
        .expect("the release saves");

    let folder = fs::read_dir(&target).unwrap().next().unwrap().unwrap().path();
    let mut names: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| name != ".bae-output")
        .collect();
    names.sort();
    assert_eq!(names, vec!["01 Track One.flac", "02 Track Two.flac", "03 Track Three.flac"]);

    let seconds = |name: &str| {
        let pcm = bae_audio::decode_audio(
            bae_audio::test_samples::filled_sparse_buffer(&fs::read(folder.join(name)).unwrap()),
            None,
            None,
        )
        .unwrap();
        pcm.samples.len() as f64 / f64::from(pcm.sample_rate * pcm.channels)
    };
    // Track 1 holds its 2 seconds of hidden audio, its own 6, and track 2's
    // 2 second gap at the end; track 2 starts at its mark.
    assert_eq!(seconds("01 Track One.flac"), 10.0);
    assert_eq!(seconds("02 Track Two.flac"), 10.0);
    assert_eq!(seconds("03 Track Three.flac"), 10.0);
}

/// Tracks titled `Track Title` and `track title` save as two names, which a
/// drive that folds case holds as one: the release save fails rather than
/// write one track over the other, and leaves nothing behind.
#[tokio::test]
async fn a_release_save_of_two_names_one_drive_holds_as_one_fails() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    // Only a drive that folds case can hold the two names as one; on a
    // case-sensitive one (Linux's usual drives) both files are written and
    // there is nothing to refuse.
    if !folds_case(f.temp_path()) {
        eprintln!("skipped: the temporary directory's drive tells case apart");
        return;
    }
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track Title");
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "02.flac", "track title");
    let release_id = import_unknown_local(&f, &album_dir).await;
    let preset = flac_save_preset("titles");
    let mut presets = f.library.manager().save_presets().await.unwrap();
    presets.push(preset.clone());
    f.library.manager().set_save_presets(presets).await.unwrap();

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    let error = f
        .library
        .manager()
        .export_release(
            &release_id,
            &target,
            OutputKind::Save {
                preset,
                filename_pattern: NamePattern::new(vec![NamePiece::Value(
                    SaveFilenameValue::Title,
                )]),
                folder_pattern: SaveFolderPattern::default(),
                extras: SaveExtras::default(),
            },
        )
        .await
        .expect_err("two tracks written at one path fail the save");

    assert!(
        error.to_string().contains("where another file of the release already is"),
        "the error names the clash: {error}"
    );
    assert!(
        fs::read_dir(&target).unwrap().next().is_none(),
        "the failed save leaves no release folder or staging folder"
    );
}

/// A cloud-only release whose file is missing exports nothing and errors —
/// no partial silent success.
#[tokio::test]
async fn export_release_missing_file_is_hard_error() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    bae_import::test_fixtures::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, _) = import_then_strand_in_cloud(&f, &album_dir).await;

    // Blow away the seeded file.
    let files = f.library.manager().get_files_for_release(&release_id).await.unwrap();
    bae_library::test_libraries::remove_uploaded_file(f.library.manager(), &f.cloud, &files[0].id)
        .await;

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    let result = f.library.manager().export_release(&release_id, &target, OutputKind::Export).await;
    assert!(result.is_err(), "missing file must fail the export");
    assert!(
        fs::read_dir(&target).unwrap().next().is_none(),
        "failed export must leave no release folder or marker"
    );
}

/// A release save preset of FLAC at 16 bits.
fn flac_save_preset(id: &str) -> SavePreset {
    SavePreset {
        id: id.to_string(),
        name: id.to_string(),
        codec: SaveCodec::Flac { bit_depth: SaveBitDepth::Bits16 },
    }
}

/// Save `release_id` under `preset` into a fresh target, returning the most
/// files under `source_dir` the save held open at once. Every source is closed
/// by the time the save returns.
async fn peak_sources_open_saving(
    f: &ExportFixture,
    release_id: &str,
    source_dir: &Path,
    preset: SavePreset,
) -> usize {
    let mut presets = f.library.manager().save_presets().await.unwrap();
    presets.push(preset.clone());
    f.library.manager().set_save_presets(presets).await.unwrap();
    let target = f.temp_path().join(format!("export-{}", preset.id));
    fs::create_dir_all(&target).unwrap();

    let sampler = bae_util::open_files_peak::OpenFilesPeak::start(source_dir);
    f.library
        .manager()
        .export_release(
            release_id,
            &target,
            OutputKind::Save {
                preset,
                filename_pattern: SaveFilenamePattern::default(),
                folder_pattern: SaveFolderPattern::default(),
                extras: SaveExtras::default(),
            },
        )
        .await
        .expect("the release save succeeds");
    let peak = sampler.finish();
    coven::assert_no_open_files_under(source_dir);
    peak
}

fn cores() -> usize {
    std::thread::available_parallelism().expect("available parallelism").get()
}

/// Saving a release of a file per track opens each track's file only while
/// that track is being saved: at most one per core.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_save_holds_a_bounded_number_of_track_files_open() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    for n in 1..=24 {
        bae_import::test_fixtures::write_tagged_flac(
            &album_dir,
            &format!("{n:02}.flac"),
            &format!("Track {n}"),
        );
    }
    let release_id = import_unknown_local(&f, &album_dir).await;
    assert_eq!(f.library.records().get_tracks_for_release(&release_id).await.unwrap().len(), 24);

    let per_track =
        peak_sources_open_saving(&f, &release_id, &album_dir, flac_save_preset("tracks")).await;
    assert!(
        per_track <= cores(),
        "{per_track} of 24 track files open at once over {} cores",
        cores()
    );
}

/// Saving a release whose tracks share one CUE image reads the image through
/// one open file.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_save_reads_a_cue_image_through_one_open_file() {
    bae_util::test_logs::install();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    fs::copy(
        bae_audio::test_fixtures::cue_flac_dir().join("Test Album.flac"),
        album_dir.join("Test Album.flac"),
    )
    .unwrap();
    let mut cue = String::from(
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\nFILE \"Test Album.flac\" WAVE\n",
    );
    for n in 0..12 {
        cue.push_str(&format!(
            "  TRACK {:02} AUDIO\n    TITLE \"Track {}\"\n    INDEX 01 00:{:02}:00\n",
            n + 1,
            n + 1,
            n * 2
        ));
    }
    fs::write(album_dir.join("Test Album.cue"), cue).unwrap();
    let release_id = import_unknown_local(&f, &album_dir).await;
    assert_eq!(f.library.records().get_tracks_for_release(&release_id).await.unwrap().len(), 12);

    let peak =
        peak_sources_open_saving(&f, &release_id, &album_dir, flac_save_preset("tracks")).await;
    assert!(peak <= 1, "the save reads the image as one open file, not {peak}");
}
