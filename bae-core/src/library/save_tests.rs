use super::*;
use crate::library::manager::{ResolvedSaveTags, SaveTags};

fn resolved(
    title: &str,
    track_number: Option<i32>,
    total_tracks: usize,
    disc: Option<i32>,
    year: Option<i32>,
) -> ResolvedSaveTags {
    ResolvedSaveTags {
        tags: SaveTags {
            title: title.to_string(),
            artist: "Artist Name".to_string(),
            album: "Album Title".to_string(),
            year,
            disc,
        },
        track_number,
        total_tracks,
        is_digital: true,
    }
}

use crate::config::SaveFilenameToken::{Album, Artist, Title, TrackNumber, Year};

#[test]
fn default_pattern_all_present_pads_track_number() {
    let r = resolved("Track Title", Some(3), 10, None, Some(2001));
    assert_eq!(
        render_save_filename(&[TrackNumber, Title], &r),
        "03 Track Title"
    );
}

#[test]
fn absent_values_drop_out_of_the_join() {
    let r = resolved("Track Title", None, 10, None, None);
    assert_eq!(
        render_save_filename(&[TrackNumber, Title, Year], &r),
        "Track Title"
    );
}

#[test]
fn full_pattern_substitutes_every_token() {
    let r = resolved("Track Title", Some(3), 10, Some(2), Some(2001));
    assert_eq!(
        render_save_filename(&[Artist, Album, TrackNumber, Title], &r),
        "Artist Name Album Title 03 Track Title"
    );
}

#[test]
fn slash_and_colon_become_dashes() {
    let r = resolved("Some/Weird:Title", None, 1, None, None);
    assert_eq!(render_save_filename(&[Title], &r), "Some-Weird-Title");
}

#[test]
fn path_escape_leaves_no_separator() {
    let r = resolved("../secret", None, 1, None, None);
    let name = render_save_filename(&[Title], &r);
    assert!(!name.contains('/'), "no forward slash in {name}");
    assert!(!name.contains('\\'), "no backslash in {name}");
}

#[test]
fn empty_render_falls_back_to_title() {
    let r = resolved("Fallback Title", None, 1, None, None);
    assert_eq!(render_save_filename(&[TrackNumber], &r), "Fallback Title");
}

#[test]
fn release_image_cue_places_indexes_from_track_windows() {
    let cue = render_cue_sheet(
        "Album Title",
        "Artist Name",
        None,
        None,
        "Album.flac",
        "WAVE",
        44_100,
        &[
            CueTrack {
                number: 1,
                title: "Opening".to_string(),
                performer: "Artist Name".to_string(),
                index_00_sample_frame: Some(0),
                index_01_sample_frame: 44_100 * 2,
            },
            CueTrack {
                number: 2,
                title: "Second".to_string(),
                performer: "Artist Name".to_string(),
                index_00_sample_frame: Some(44_100 * 10),
                index_01_sample_frame: 44_100 * 12,
            },
        ],
    );

    assert!(cue.contains("FILE \"Album.flac\" WAVE"));
    assert!(cue.contains("  TRACK 01 AUDIO\n"));
    assert!(cue.contains("    INDEX 00 00:00:00\n"));
    assert!(cue.contains("    INDEX 01 00:02:00\n"));
    assert!(cue.contains("  TRACK 02 AUDIO\n"));
    assert!(cue.contains("    INDEX 00 00:10:00\n"));
    assert!(cue.contains("    INDEX 01 00:12:00\n"));
}

#[test]
fn release_image_cue_writes_catalog_and_date() {
    let cue = render_cue_sheet(
        "Album Title",
        "Artist Name",
        Some("0123456789012"),
        Some(2024),
        "Album.flac",
        "WAVE",
        44_100,
        &[CueTrack {
            number: 1,
            title: "Opening".to_string(),
            performer: "Artist Name".to_string(),
            index_00_sample_frame: None,
            index_01_sample_frame: 0,
        }],
    );

    assert!(cue.contains("CATALOG 0123456789012\n"));
    assert!(cue.contains("REM DATE 2024\n"));
}

/// Encode a short FLAC and run the real `write_tags` over it with `cover`.
/// Returns the temp dir — which has to outlive the read-back — and the
/// tagged file's path.
fn write_tags_to_encoded_flac(cover: Option<&[u8]>) -> (tempfile::TempDir, std::path::PathBuf) {
    crate::audio_codec::init();
    let samples: Vec<i32> = (0..4410)
        .map(|i| ((i as f64 * 0.02).sin() * 0.5 * i32::MAX as f64) as i32)
        .collect();
    let flac = crate::audio_codec::encode_i32(
        crate::audio_codec::EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &samples,
        44100,
        1,
    )
    .unwrap();

    let tags = SaveTags {
        title: "Track Title".to_string(),
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
        year: Some(2001),
        disc: Some(1),
    };

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("tagged.flac");
    std::fs::write(&path, &flac).unwrap();
    write_tags(
        &path,
        lofty::tag::TagType::VorbisComments,
        &tags,
        Some(3),
        10,
        true,
        cover,
    )
    .unwrap();
    (dir, path)
}

/// Exercises the real `write_tags` against an encoded FLAC: every known tag
/// and the cover image are embedded.
#[test]
fn write_tags_writes_every_known_field() {
    use lofty::prelude::*;
    use lofty::tag::TagType;

    let cover_bytes = {
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([120, 40, 200]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };

    let (_dir, path) = write_tags_to_encoded_flac(Some(&cover_bytes));

    let tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged
        .tag(TagType::VorbisComments)
        .expect("VorbisComments tag present");
    assert_eq!(tag.title().as_deref(), Some("Track Title"));
    assert_eq!(tag.artist().as_deref(), Some("Artist Name"));
    assert_eq!(tag.album().as_deref(), Some("Album Title"));
    assert_eq!(tag.track(), Some(3));
    assert_eq!(tag.track_total(), Some(10));
    assert_eq!(tag.disk(), Some(1));
    assert!(!tag.pictures().is_empty(), "cover embedded");
}

/// The counterpart: when the preset doesn't embed (so the plan carries no
/// cover bytes), `write_tags` embeds no picture — every other tag still lands.
#[test]
fn write_tags_without_cover_embeds_no_picture() {
    use lofty::prelude::*;
    use lofty::tag::TagType;

    let (_dir, path) = write_tags_to_encoded_flac(None);

    let tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged
        .tag(TagType::VorbisComments)
        .expect("VorbisComments tag present");
    assert_eq!(tag.title().as_deref(), Some("Track Title"));
    assert_eq!(tag.track(), Some(3));
    assert!(
        tag.pictures().is_empty(),
        "no cover bytes means no embedded picture"
    );
}

/// AAC exports write MP4 `ilst` atoms. Encode a real .m4a, tag it with the
/// container `codec_tag_type` picks for AAC, and read every field back —
/// proving the tag type is wired to a container lofty writes natively.
#[test]
fn write_tags_round_trips_through_mp4_ilst() {
    use lofty::prelude::*;
    use lofty::tag::TagType;

    crate::audio_codec::init();
    let samples: Vec<i32> = (0..44_100 * 2)
        .map(|i| ((i as f64 * 0.02).sin() * 0.5 * i32::MAX as f64) as i32)
        .collect();
    let m4a = crate::audio_codec::encode_i32(
        crate::audio_codec::EncodeFormat::Aac { bitrate_kbps: 256 },
        &samples,
        44_100,
        2,
    )
    .unwrap();

    let cover_bytes = {
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([120, 40, 200]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .unwrap();
        buf.into_inner()
    };

    let tags = SaveTags {
        title: "Track Title".to_string(),
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
        year: Some(2001),
        disc: Some(1),
    };

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("tagged.m4a");
    std::fs::write(&path, &m4a).unwrap();
    let tag_type = codec_tag_type(&crate::config::SaveCodec::Aac { bitrate_kbps: 256 });
    assert_eq!(tag_type, TagType::Mp4Ilst);
    write_tags(
        &path,
        tag_type,
        &tags,
        Some(3),
        10,
        true,
        Some(&cover_bytes),
    )
    .unwrap();

    let tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged.tag(TagType::Mp4Ilst).expect("MP4 ilst tag present");
    assert_eq!(tag.title().as_deref(), Some("Track Title"));
    assert_eq!(tag.artist().as_deref(), Some("Artist Name"));
    assert_eq!(tag.album().as_deref(), Some("Album Title"));
    assert_eq!(tag.track(), Some(3));
    assert_eq!(tag.track_total(), Some(10));
    assert_eq!(tag.disk(), Some(1));
    assert!(!tag.pictures().is_empty(), "cover embedded");
}
