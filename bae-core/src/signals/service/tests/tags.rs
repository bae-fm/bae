//! What the audio's own tags add: their album, artists and label as text the
//! rows are judged against, and where their recordings were registered.

use super::*;

/// A FLAC carrying these Vorbis comments.
fn tagged_flac(path: &Path, comments: &[(&str, &str)]) {
    use lofty::config::WriteOptions;
    use lofty::tag::TagExt;
    fs::write(path, fixture_flac()).unwrap();
    let mut tag = lofty::ogg::tag::VorbisComments::default();
    for (key, value) in comments {
        tag.push((*key).to_string(), (*value).to_string());
    }
    tag.save_to_path(path, WriteOptions::default()).unwrap();
}

#[test]
fn the_audio_s_tags_feed_the_text_and_name_where_it_was_registered() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    for (name, isrc) in [("01.flac", "IT0000000001"), ("02.flac", "IT0000000002")] {
        tagged_flac(
            &folder.join(name),
            &[
                ("ALBUM", "Album Title"),
                ("ARTIST", "Artist Name"),
                ("LABEL", "Imprint Name"),
                ("ISRC", isrc),
            ],
        );
    }
    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    let pass = gather_non_ocr_sources(&[folder], &files).expect("the fixture audio times");

    let tagged: Vec<&str> = pass
        .lines
        .iter()
        .filter(|line| TextOrigin::of_source(&line.source) == TextOrigin::FileTag)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(tagged, vec!["Album Title", "Artist Name", "Imprint Name"]);
    assert_eq!(
        pass.registered_in,
        crate::pressing::Country::from_code("IT").map(crate::pressing::ReleaseArea::Country)
    );
}

/// Read a folder of two FLACs carrying these comments, with `extra` files
/// beside them.
fn origin_of(comments: &[(&str, &str)], extra: &[(&str, &[u8])]) -> crate::signals::AudioOrigin {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    for name in ["01.flac", "02.flac"] {
        tagged_flac(&folder.join(name), comments);
    }
    for (name, bytes) in extra {
        fs::write(folder.join(name), bytes).unwrap();
    }
    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    gather_non_ocr_sources(&[folder], &files)
        .expect("the fixture audio times")
        .origin
}

const DELIVERED: &[(&str, &str)] = &[
    ("ISRC", "IT0000000001"),
    ("LABEL", "Imprint Name"),
    ("COPYRIGHT", "(C) 1999 Imprint Name (P) 1999 Imprint Name"),
];

/// A label's delivery set on every track proves a download; a ℗ line counts
/// as a "(P)" one.
#[test]
fn a_label_s_delivery_set_on_every_track_proves_a_download() {
    use crate::signals::{AudioSource, DownloadProof};
    assert_eq!(
        origin_of(DELIVERED, &[]).source,
        Some(AudioSource::Download(DownloadProof::DeliverySet))
    );
    assert_eq!(
        origin_of(
            &[
                ("ISRC", "IT0000000001"),
                ("LABEL", "Imprint Name"),
                ("COPYRIGHT", "℗ 1999 Imprint Name"),
            ],
            &[]
        )
        .source,
        Some(AudioSource::Download(DownloadProof::DeliverySet))
    );
}

/// The set proves nothing with a part missing, nor beside a rip log or a
/// track sheet, which say a disc was read — a ripper puts the disc's ISRCs
/// in the tags too.
#[test]
fn a_delivery_set_with_a_part_missing_or_beside_a_rip_file_proves_nothing() {
    assert_eq!(origin_of(&DELIVERED[..2], &[]).source, None);
    assert_eq!(
        origin_of(
            &[
                ("ISRC", "IT0000000001"),
                ("LABEL", "Imprint Name"),
                ("COPYRIGHT", "(C) 1999 Imprint Name"),
            ],
            &[]
        )
        .source,
        None
    );
    assert_eq!(origin_of(DELIVERED, &[("rip.log", b"not a log")]).source, None);
    assert_eq!(
        origin_of(DELIVERED, &[("Album.cue", b"FILE \"01.flac\" WAVE\n")]).source,
        None
    );
}

/// A store's own marker proves a download, and names the track it is on.
#[test]
fn a_store_s_marker_proves_a_download() {
    use crate::signals::{AudioSource, DownloadProof, StoreMarker};
    assert_eq!(
        origin_of(&[("COMMENT", "Visit https://artistname.bandcamp.com")], &[]).source,
        Some(AudioSource::Download(DownloadProof::Store {
            marker: StoreMarker::Bandcamp,
            file: "01.flac".to_string(),
        }))
    );
}

/// A ripper's log and a store's marker in one folder: the log was made by
/// reading a disc, and a tag can be copied, so the rip is what it proves.
#[test]
fn a_rip_log_outweighs_a_store_s_marker() {
    let log = fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/test_album.log"
    ))
    .unwrap();
    assert!(matches!(
        origin_of(
            &[("COMMENT", "Visit https://artistname.bandcamp.com")],
            &[("Album.log", &log)]
        )
        .source,
        Some(crate::signals::AudioSource::CdRip {
            proof: crate::signals::CdProof::RipLog,
            ..
        })
    ));
}
