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
