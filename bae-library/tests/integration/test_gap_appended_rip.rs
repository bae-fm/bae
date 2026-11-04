//! A gap-appended rip: a multi-FILE sheet whose files each hold one track's
//! body, from its INDEX 01 at the file's start, followed by the next track's
//! pregap, its INDEX 00 at the file's tail (spec §2.2, §7.4).

use bae_audio::EncodeFormat;

const SAMPLE_RATE: u32 = 44_100;
/// Every file's length.
const FILE_SECONDS: u32 = 3;
/// The files of the rip, one per track, in track order.
const FILES: [&str; 3] = ["01 Track One.wav", "02 Track Two.wav", "03 Track Three.wav"];

/// `FILE_SECONDS` of a quiet tone at `frequency`, as 16-bit-range stereo.
fn tone(frequency: f64) -> Vec<i32> {
    let frames = (SAMPLE_RATE * FILE_SECONDS) as usize;
    (0..frames * 2)
        .map(|i| {
            let t = (i / 2) as f64 / f64::from(SAMPLE_RATE);
            let v = 0.1 * (2.0 * std::f64::consts::PI * frequency * t).sin();
            ((v * f64::from(i16::MAX)) as i32) << 16
        })
        .collect()
}

/// Import the rip: three WAV files and a sheet whose tracks 2 and 3 start
/// their pregap two seconds into the file before their own. Returns the
/// library, the release and the directory that owns it all.
async fn import_rip() -> (bae_library::test_libraries::TestLibrary, String, tempfile::TempDir) {
    bae_util::test_logs::install();
    bae_audio::init();
    let release = bae_catalogs::discogs::models::DiscogsRelease {
        master_id: Some("gap-appended-master".to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
            "gap-appended",
            "Album Title",
            &[("Track One", "0:02"), ("Track Two", "0:02"), ("Track Three", "0:03")],
        )
    };
    let (library, imported) = bae_library::test_libraries::imported_release_setup(
        release,
        "test",
        uuid::Uuid::new_v4().to_string(),
        |dir| {
            for (name, frequency) in FILES.iter().zip([220.0, 440.0, 880.0]) {
                let audio = bae_audio::encode_i32(
                    EncodeFormat::PcmWav { bits_per_sample: 16 },
                    &tone(frequency),
                    SAMPLE_RATE,
                    2,
                )
                .expect("encode a file");
                std::fs::write(dir.join(name), audio).unwrap();
            }
            std::fs::write(
                dir.join("Album Title.cue"),
                format!(
                    "FILE \"{}\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 00:00:00\n  \
                     TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 00 00:02:00\n\
                     FILE \"{}\" WAVE\n    INDEX 01 00:00:00\n  \
                     TRACK 03 AUDIO\n    TITLE \"Track Three\"\n    INDEX 00 00:02:00\n\
                     FILE \"{}\" WAVE\n    INDEX 01 00:00:00\n",
                    FILES[0], FILES[1], FILES[2]
                ),
            )
            .unwrap();
        },
    )
    .await
    .expect("import the rip");
    assert_eq!(imported.track_ids.len(), 3, "the rip's three tracks");
    (library, imported.release_id, imported.temp_dir)
}

/// Each of the editor's rows names the file its track's body plays from,
/// never the file before, which holds only its pregap.
#[tokio::test]
async fn the_editor_s_rows_each_name_the_file_their_body_plays_from() {
    let (library, release_id, _dir) = import_rip().await;
    let session = library.open_release_edit(&release_id).await.expect("open the editor");

    let sources: Vec<&str> =
        session.opened().display.tracks().map(|track| track.source.name.as_str()).collect();

    assert_eq!(sources, FILES);
}
