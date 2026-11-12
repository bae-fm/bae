use super::*;
use crate::manager::{ResolvedSaveTags, SaveTags};

fn resolved(
    title: &str,
    track_number: u32,
    total_tracks: usize,
    disc: Option<u32>,
    year: Option<i32>,
) -> ResolvedSaveTags {
    ResolvedSaveTags {
        tags: SaveTags {
            title: title.to_string(),
            artist: "Artist Name".to_string(),
            album_artist: "Artist Name".to_string(),
            album: "Album Title".to_string(),
            album_year: Some(1999),
            year,
            disc,
        },
        track_number,
        total_tracks,
    }
}

use bae_preferences::{
    NamePattern, NamePiece, SaveFilenamePattern, SaveFilenameValue, SaveFolderPattern,
    SaveFolderValue,
};

fn file_pattern(pieces: &[NamePiece<SaveFilenameValue>]) -> SaveFilenamePattern {
    NamePattern::new(pieces.to_vec())
}

fn folder_pattern(pieces: &[NamePiece<SaveFolderValue>]) -> SaveFolderPattern {
    NamePattern::new(pieces.to_vec())
}

fn value<V>(value: V) -> NamePiece<V> {
    NamePiece::Value(value)
}

fn text<V>(text: &str) -> NamePiece<V> {
    NamePiece::Text(text.to_string())
}

use SaveFilenameValue::{Album, AlbumYear, Artist, DiscNumber, ReleaseYear, Title, TrackNumber};

#[test]
fn default_pattern_all_present_pads_track_number() {
    let r = resolved("Track Title", 3, 10, None, Some(2001));
    assert_eq!(
        render_save_filename(&SaveFilenamePattern::default(), &r).unwrap(),
        "03 Track Title"
    );
}

#[test]
fn typed_text_is_kept_as_typed_with_no_spaces_added_or_collapsed() {
    let r = resolved("Track Title", 3, 10, None, None);
    let pattern = file_pattern(&[
        value(Artist),
        text("  -   "),
        value(Title),
        text("(live)"),
        value(TrackNumber),
    ]);
    assert_eq!(render_save_filename(&pattern, &r).unwrap(), "Artist Name  -   Track Title(live)03");
}

#[test]
fn an_empty_value_leaves_only_its_neighbours_text() {
    let r = resolved("Track Title", 3, 10, None, None);
    let pattern = file_pattern(&[
        value(DiscNumber),
        text(" - "),
        value(TrackNumber),
        text(" "),
        value(Title),
    ]);
    assert_eq!(render_save_filename(&pattern, &r).unwrap(), " - 03 Track Title");
}

#[test]
fn the_album_year_and_the_release_year_are_two_values() {
    let r = resolved("Track Title", 3, 10, None, Some(2001));
    let pattern = file_pattern(&[value(AlbumYear), text(" "), value(ReleaseYear)]);
    assert_eq!(render_save_filename(&pattern, &r).unwrap(), "1999 2001");
    let unstated = resolved("Track Title", 3, 10, None, None);
    assert_eq!(render_save_filename(&pattern, &unstated).unwrap(), "1999 ");
}

#[test]
fn full_pattern_substitutes_every_value() {
    let r = resolved("Track Title", 3, 10, Some(2), Some(2001));
    let pattern = file_pattern(&[
        value(Artist),
        text(" - "),
        value(Album),
        text(" "),
        value(DiscNumber),
        value(TrackNumber),
        value(Title),
    ]);
    assert_eq!(
        render_save_filename(&pattern, &r).unwrap(),
        "Artist Name - Album Title 203Track Title"
    );
}

/// The stem is the title's text; its file takes the name coven writes, where
/// `/` and `:` are their full-width lookalikes on every platform.
#[test]
fn slash_and_colon_are_written_as_coven_writes_them() {
    let r = resolved("Some/Weird:Title", 1, 1, None, None);
    let stem = render_save_filename(&file_pattern(&[value(Title)]), &r).unwrap();
    assert_eq!(stem, "Some/Weird:Title");
    assert_eq!(save_file_name(&stem, "flac"), Path::new("Some／Weird：Title.flac"));
}

#[test]
fn path_escape_leaves_one_name() {
    let r = resolved("../secret", 1, 1, None, None);
    let name =
        save_file_name(&render_save_filename(&file_pattern(&[value(Title)]), &r).unwrap(), "flac");
    assert_eq!(name, Path::new("／secret.flac"));
    assert_eq!(name.components().count(), 1, "one name: {name:?}");
}

#[test]
fn a_name_is_never_hidden_and_keeps_its_spacing() {
    let r = resolved("..Track   Title  ", 1, 1, None, None);
    assert_eq!(
        render_save_filename(&file_pattern(&[value(Title)]), &r).unwrap(),
        "Track   Title  "
    );
}

#[test]
fn empty_render_falls_back_to_title() {
    let r = resolved("Fallback Title", 1, 1, None, None);
    assert_eq!(
        render_save_filename(&file_pattern(&[value(ReleaseYear)]), &r).unwrap(),
        "Fallback Title"
    );
    assert_eq!(
        render_save_filename(&file_pattern(&[text("  "), value(ReleaseYear)]), &r).unwrap(),
        "Fallback Title",
        "a name of nothing but spaces is no name"
    );
}

/// A pattern that can't make a name is refused with its error, never replaced
/// by the default pattern.
#[test]
fn an_invalid_pattern_is_refused_with_its_error() {
    let r = resolved("Track Title", 3, 10, None, None);
    for invalid in [
        file_pattern(&[]),
        file_pattern(&[text(".hidden "), value(Title)]),
        file_pattern(&[value(Title), text(" a/b")]),
        file_pattern(&[value(Title), text("?")]),
    ] {
        assert_eq!(
            render_save_filename(&invalid, &r),
            Err(invalid.validate().expect_err("the pattern is invalid")),
            "{invalid:?}"
        );
    }
}

/// Encode a short FLAC and run the real `write_tags` over it with `cover`.
/// Returns the temp dir — which has to outlive the read-back — and the
/// tagged file's path.
fn write_tags_to_encoded_flac(cover: Option<&[u8]>) -> (tempfile::TempDir, std::path::PathBuf) {
    bae_audio::init();
    let samples: Vec<i32> =
        (0..4410).map(|i| ((i as f64 * 0.02).sin() * 0.5 * i32::MAX as f64) as i32).collect();
    let flac = bae_audio::encode_i32(
        bae_audio::EncodeFormat::Flac { bits_per_sample: 16 },
        &samples,
        44100,
        1,
    )
    .unwrap();

    let tags = SaveTags {
        title: "Track Title".to_string(),
        artist: "Artist Name".to_string(),
        album_artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
        album_year: Some(1999),
        year: Some(2001),
        disc: Some(1),
    };

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("tagged.flac");
    std::fs::write(&path, &flac).unwrap();
    write_tags(&path, lofty::tag::TagType::VorbisComments, &tags, Some(3), 10, cover, None)
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
        image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    };

    let (_dir, path) = write_tags_to_encoded_flac(Some(&cover_bytes));

    let tagged = lofty::read_from_path(&path).unwrap();
    let tag = tagged.tag(TagType::VorbisComments).expect("VorbisComments tag present");
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
    let tag = tagged.tag(TagType::VorbisComments).expect("VorbisComments tag present");
    assert_eq!(tag.title().as_deref(), Some("Track Title"));
    assert_eq!(tag.track(), Some(3));
    assert!(tag.pictures().is_empty(), "no cover bytes means no embedded picture");
}

/// AAC exports write MP4 `ilst` atoms. Encode a real .m4a, tag it with the
/// container `codec_tag_type` picks for AAC, and read every field back —
/// proving the tag type is wired to a container lofty writes natively.
#[test]
fn write_tags_round_trips_through_mp4_ilst() {
    use lofty::prelude::*;
    use lofty::tag::TagType;

    bae_audio::init();
    let samples: Vec<i32> =
        (0..44_100 * 2).map(|i| ((i as f64 * 0.02).sin() * 0.5 * i32::MAX as f64) as i32).collect();
    let m4a = bae_audio::encode_i32(
        bae_audio::EncodeFormat::Aac { bitrate_kbps: 256 },
        &samples,
        44_100,
        2,
    )
    .unwrap();

    let cover_bytes = {
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([120, 40, 200]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    };

    let tags = SaveTags {
        title: "Track Title".to_string(),
        artist: "Artist Name".to_string(),
        album_artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
        album_year: Some(1999),
        year: Some(2001),
        disc: Some(1),
    };

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("tagged.m4a");
    std::fs::write(&path, &m4a).unwrap();
    let tag_type = codec_tag_type(&bae_preferences::SaveCodec::Aac { bitrate_kbps: 256 });
    assert_eq!(tag_type, TagType::Mp4Ilst);
    write_tags(&path, tag_type, &tags, Some(3), 10, Some(&cover_bytes), None).unwrap();

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

/// A read of a track's source that panics fails its save as unfinished work,
/// which the library reports as a bug, never as a save that could not be
/// written. Sabotage: map `ReadPanicked` to `SaveError::Failed`, and the
/// track's save fails as an ordinary save error nobody reports.
#[test]
fn a_read_that_panics_fails_the_save_as_unfinished() {
    bae_audio::init();
    let bytes = std::fs::read(
        bae_audio::test_fixtures::cue_flac_dir()
            .join("03 Test Artist - Track Three (Brown Noise).flac"),
    )
    .expect("fixture");
    let buffer = bae_audio::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, &bytes);
    buffer.panic_reads();
    let decode = bae_track_audio::StreamDecodeParams::new(
        vec![bae_track_audio::SegmentDecodeParams::new(
            buffer,
            bae_records::SegmentSpan::whole_file(),
            0,
        )],
        true,
        0,
        0,
    );
    let dir = tempfile::TempDir::new().unwrap();
    let file = std::fs::File::create(dir.path().join("track.flac")).unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut encoder = StreamingEncoder::seekable(
        EncodeFormat::Flac { bits_per_sample: 16 },
        Box::new(file),
        cancel.clone(),
    );

    // A track's save runs its decode into the encoder with `?`, which is this
    // conversion.
    let result: Result<(), SaveError> =
        decode.run_to_sink(44_100, 2, &mut encoder, cancel).map_err(SaveError::from);

    match result {
        Err(SaveError::Unfinished(failed)) => assert!(failed.is_panic(), "{failed}"),
        Err(error) => panic!("a read that panicked leaves the save unfinished: {error}"),
        Ok(()) => panic!("a read that panicked leaves the save unfinished, not encoded"),
    }
}

/// A lossless codec set to the source's depth encodes at the depth of the
/// track it saves; set to 16-bit it encodes 16 whatever the track holds.
#[test]
fn a_lossless_codec_encodes_at_the_depth_it_resolves_to() {
    use bae_audio::EncodeFormat;
    use bae_preferences::{SaveBitDepth, SaveCodec};
    let source = NonZeroU32::new(24);
    assert!(matches!(
        encode_format(&SaveCodec::Flac { bit_depth: SaveBitDepth::SameAsSource }, source),
        EncodeFormat::Flac { bits_per_sample: 24 }
    ));
    assert!(matches!(
        encode_format(&SaveCodec::Wav { bit_depth: SaveBitDepth::Bits16 }, source),
        EncodeFormat::PcmWav { bits_per_sample: 16 }
    ));
    assert!(matches!(
        encode_format(&SaveCodec::Aiff { bit_depth: SaveBitDepth::SameAsSource }, None),
        EncodeFormat::PcmAiff { bits_per_sample: 16 }
    ));
}

fn folder_tags() -> SaveFolderTags {
    SaveFolderTags {
        artist: "Mara Quill".to_string(),
        album: "Low Tide Hours".to_string(),
        album_year: Some(1992),
        release_year: Some(2004),
        label: Some("Harbor Records".to_string()),
        catalog_number: Some("HR-1042".to_string()),
    }
}

/// A folder name is its pattern's text as typed with each value filled in; a
/// value a release does not state leaves only the text around it.
#[test]
fn a_folder_name_fills_the_values_its_pattern_names() {
    use SaveFolderValue::*;
    let tags = folder_tags();
    assert_eq!(
        render_save_folder_name(&SaveFolderPattern::default(), &tags).unwrap(),
        "Mara Quill - Low Tide Hours"
    );
    assert_eq!(
        render_save_folder_name(
            &folder_pattern(&[
                value(AlbumYear),
                text("  "),
                value(ReleaseYear),
                text(" ["),
                value(Label),
                text("] "),
                value(CatalogNumber),
                text(" "),
                value(Album),
            ]),
            &tags
        )
        .unwrap(),
        "1992  2004 [Harbor Records] HR-1042 Low Tide Hours"
    );
    let unstated = SaveFolderTags { release_year: None, label: None, catalog_number: None, ..tags };
    assert_eq!(
        render_save_folder_name(
            &folder_pattern(&[value(Artist), text(" - "), value(Label), text(" - "), value(Album)]),
            &unstated
        )
        .unwrap(),
        "Mara Quill -  - Low Tide Hours"
    );
}

/// A pattern that renders nothing falls back to the album, then to a fixed
/// name; one that can't make a name is refused.
#[test]
fn an_empty_folder_name_falls_back_to_the_album() {
    use SaveFolderValue::*;
    let tags = folder_tags();
    assert_eq!(
        render_save_folder_name(
            &folder_pattern(&[value(Label)]),
            &SaveFolderTags { label: None, ..tags }
        )
        .unwrap(),
        "Low Tide Hours"
    );
    let nothing = SaveFolderTags {
        artist: String::new(),
        album: String::new(),
        album_year: None,
        release_year: None,
        label: None,
        catalog_number: None,
    };
    assert_eq!(
        render_save_folder_name(&folder_pattern(&[value(Artist)]), &nothing).unwrap(),
        "release"
    );
}

#[test]
fn an_invalid_folder_pattern_is_refused_with_its_error() {
    use SaveFolderValue::*;
    let tags = folder_tags();
    for invalid in [
        folder_pattern(&[]),
        folder_pattern(&[text(".x"), value(Album)]),
        folder_pattern(&[value(Album), text("\\")]),
        folder_pattern(&[value(Album), text("<>")]),
    ] {
        assert_eq!(
            render_save_folder_name(&invalid, &tags),
            Err(invalid.validate().expect_err("the pattern is invalid")),
            "{invalid:?}"
        );
    }
}

/// ReplayGain tags are the gain in dB to two places and the peak as a linear
/// ratio to six, for each level that was measured.
#[test]
fn replaygain_tags_state_gain_and_peak_for_each_measured_level() {
    use lofty::tag::ItemKey;
    let tags = ReplayGainTags {
        track: Some(Level { gain_db: -3.2, peak_linear: 0.987654321 }),
        album: None,
    };
    assert_eq!(
        tags.items(),
        vec![
            (ItemKey::ReplayGainTrackGain, "-3.20 dB".to_string()),
            (ItemKey::ReplayGainTrackPeak, "0.987654".to_string()),
        ]
    );
    assert!(ReplayGainTags { track: None, album: None }.items().is_empty());
}
