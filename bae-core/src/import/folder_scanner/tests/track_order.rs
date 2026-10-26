// The order a folder's tracks play in, and the numbers its draft gives them:
// each file's disc and track tags first, the numbers its name starts with only
// where its tags state none.

/// Write `bytes` to `path` and tag it with `title` and whichever of
/// `disc` and `track` are given, in `tag_type`'s own form.
fn write_tagged_audio(
    path: &Path,
    bytes: &[u8],
    tag_type: lofty::tag::TagType,
    title: &str,
    disc: Option<u32>,
    track: Option<u32>,
) {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::{Accessor, Tag};
    std::fs::write(path, bytes).unwrap();
    let mut tagged = lofty::read_from_path(path).expect("read for tagging");
    let mut tag = Tag::new(tag_type);
    tag.set_title(title.to_string());
    if let Some(disc) = disc {
        tag.set_disk(disc);
    }
    if let Some(track) = track {
        tag.set_track(track);
    }
    tagged.insert_tag(tag);
    tagged
        .save_to_path(path, WriteOptions::default())
        .expect("write tags");
}

fn tagged_flac(album: &Path, name: &str, title: &str, disc: Option<u32>, track: Option<u32>) {
    write_tagged_audio(
        &album.join(name),
        &fake_flac(),
        lofty::tag::TagType::VorbisComments,
        title,
        disc,
        track,
    );
}

fn audio_order(files: &CategorizedFiles) -> Vec<String> {
    crate::import::audio_layout::audio_units(files)
        .iter()
        .map(|unit| unit.file_id().to_string())
        .collect()
}

/// The draft a scan of `root`'s one release starts from: its files' tags, read
/// the way discovery reads them. Each track as (file, medium, number, title).
fn tag_draft(root: &Path) -> Vec<(String, u16, u32, String)> {
    let candidates = scan_valid(root.to_path_buf());
    let [candidate] = candidates.as_slice() else {
        panic!("one release under {}", root.display());
    };
    let seed = crate::import::file_metadata_seed::FileMetadataSeed::read(
        candidate,
        0,
        &FileReads::new(Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader)),
        None,
    )
    .expect("tags read");
    seed.draft
        .tracks
        .iter()
        .map(|track| {
            (
                track.edit.file.file_id().to_string(),
                track.place.medium.get(),
                track.place.number.get(),
                track.edit.title.clone(),
            )
        })
        .collect()
}

#[test]
fn tagged_files_whose_names_carry_no_numbers_play_in_their_tags_order() {
    let (tmp, album) = album_dir();
    tagged_flac(&album, "Alpha.flac", "Track Title Three", Some(1), Some(3));
    tagged_flac(&album, "Beta.flac", "Track Title One", Some(1), Some(1));
    tagged_flac(&album, "Gamma.flac", "Track Title Two", Some(1), Some(2));

    assert_eq!(
        audio_order(&scan_files(&album)),
        ["Beta.flac", "Gamma.flac", "Alpha.flac"]
    );
    assert_eq!(
        tag_draft(tmp.path()),
        [
            ("Beta.flac".to_string(), 1, 1, "Track Title One".to_string()),
            ("Gamma.flac".to_string(), 1, 2, "Track Title Two".to_string()),
            ("Alpha.flac".to_string(), 1, 3, "Track Title Three".to_string()),
        ]
    );
}

#[test]
fn an_mp4_s_track_atom_and_an_id3_track_frame_order_their_files() {
    let (_tmp, album) = album_dir();
    let m4a = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/test-fixtures/audio-format/placeholder-aac.m4a"
    ))
    .unwrap();
    for (name, track) in [("Alpha.m4a", 2), ("Beta.m4a", 1)] {
        write_tagged_audio(
            &album.join(name),
            &m4a,
            lofty::tag::TagType::Mp4Ilst,
            "Track Title",
            None,
            Some(track),
        );
    }
    assert_eq!(audio_order(&scan_files(&album)), ["Beta.m4a", "Alpha.m4a"]);

    let (_tmp, album) = album_dir();
    for (name, track) in [("Alpha.mp3", 2), ("Beta.mp3", 1)] {
        write_tagged_audio(
            &album.join(name),
            &fake_mp3(),
            lofty::tag::TagType::Id3v2,
            "Track Title",
            None,
            Some(track),
        );
    }
    assert_eq!(audio_order(&scan_files(&album)), ["Beta.mp3", "Alpha.mp3"]);
}

#[test]
fn a_file_s_tags_win_over_the_number_its_name_starts_with() {
    let (tmp, album) = album_dir();
    tagged_flac(&album, "01 Gamma.flac", "Track Title Three", None, Some(3));
    tagged_flac(&album, "02 Alpha.flac", "Track Title One", None, Some(1));
    tagged_flac(&album, "03 Beta.flac", "Track Title Two", None, Some(2));

    assert_eq!(
        audio_order(&scan_files(&album)),
        ["02 Alpha.flac", "03 Beta.flac", "01 Gamma.flac"]
    );
    let numbers: Vec<_> = tag_draft(tmp.path())
        .into_iter()
        .map(|(file, _, number, _)| (file, number))
        .collect();
    assert_eq!(
        numbers,
        [
            ("02 Alpha.flac".to_string(), 1),
            ("03 Beta.flac".to_string(), 2),
            ("01 Gamma.flac".to_string(), 3),
        ]
    );
}

#[test]
fn untagged_files_take_their_disc_and_track_from_the_numbers_their_names_start_with() {
    let (tmp, album) = album_dir();
    for name in ["2-01 Gamma.flac", "1-02 Beta.flac", "1-01 Alpha.flac"] {
        std::fs::write(album.join(name), fake_flac()).unwrap();
    }

    assert_eq!(
        audio_order(&scan_files(&album)),
        ["1-01 Alpha.flac", "1-02 Beta.flac", "2-01 Gamma.flac"]
    );
    assert_eq!(
        tag_draft(tmp.path()),
        [
            ("1-01 Alpha.flac".to_string(), 1, 1, "1-01 Alpha".to_string()),
            ("1-02 Beta.flac".to_string(), 1, 2, "1-02 Beta".to_string()),
            ("2-01 Gamma.flac".to_string(), 2, 1, "2-01 Gamma".to_string()),
        ]
    );
}

#[test]
fn an_untagged_file_s_name_places_it_among_tagged_ones_and_an_unnumbered_one_comes_last() {
    let (_tmp, album) = album_dir();
    std::fs::write(album.join("03 Zeta.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("Bonus.flac"), fake_flac()).unwrap();
    tagged_flac(&album, "Alpha.flac", "Track Title Two", None, Some(2));
    tagged_flac(&album, "Beta.flac", "Track Title One", None, Some(1));

    assert_eq!(
        audio_order(&scan_files(&album)),
        ["Beta.flac", "Alpha.flac", "03 Zeta.flac", "Bonus.flac"]
    );
}

#[test]
fn disc_tags_put_each_disc_s_tracks_together_in_their_order() {
    let (tmp, album) = album_dir();
    tagged_flac(&album, "Alpha.flac", "Disc Two Track One", Some(2), Some(1));
    tagged_flac(&album, "Beta.flac", "Disc One Track Two", Some(1), Some(2));
    tagged_flac(&album, "Delta.flac", "Disc One Track One", Some(1), Some(1));
    tagged_flac(&album, "Gamma.flac", "Disc Two Track Two", Some(2), Some(2));

    assert_eq!(
        audio_order(&scan_files(&album)),
        ["Delta.flac", "Beta.flac", "Alpha.flac", "Gamma.flac"]
    );
    let numbers: Vec<_> = tag_draft(tmp.path())
        .into_iter()
        .map(|(file, disc, number, _)| (file, disc, number))
        .collect();
    assert_eq!(
        numbers,
        [
            ("Delta.flac".to_string(), 1, 1),
            ("Beta.flac".to_string(), 1, 2),
            ("Alpha.flac".to_string(), 2, 1),
            ("Gamma.flac".to_string(), 2, 2),
        ]
    );
}

#[test]
fn a_catalog_tracklist_lands_on_the_files_its_numbers_are_tagged_with() {
    let (tmp, album) = album_dir();
    tagged_flac(&album, "Alpha.flac", "Track Title Three", None, Some(3));
    tagged_flac(&album, "Beta.flac", "Track Title One", None, Some(1));
    tagged_flac(&album, "Gamma.flac", "Track Title Two", None, Some(2));
    let candidates = scan_valid(tmp.path().to_path_buf());
    let [candidate] = candidates.as_slice() else {
        panic!("one release");
    };
    let seed = crate::import::file_metadata_seed::FileMetadataSeed::read(
        candidate,
        0,
        &FileReads::new(Arc::new(crate::import::file_tag_snapshot::LoftyFileTagReader)),
        None,
    )
    .expect("tags read");
    let catalog = crate::import::RawReleaseEdit::from_user_edit(
        crate::import::ReleaseUserEdit {
            album_title: "Album Title".into(),
            album_artist_assignments: Vec::new(),
            album_year: None,
            pressing: bae_types::pressing::Pressing::blank(),
            tracks: ["Catalog One", "Catalog Two", "Catalog Three"]
                .into_iter()
                .map(|title| crate::import::TrackUserEdit {
                    title: title.into(),
                    artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                    file: None,
                })
                .collect(),
        },
        "catalog-track",
    );

    let laid = crate::import::pane::edit_over_audio(catalog, &seed.draft).expect("lays over");
    let pairs: Vec<_> = laid
        .tracks
        .iter()
        .map(|track| (track.edit.title.clone(), track.edit.file.file_id().to_string()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("Catalog One".to_string(), "Beta.flac".to_string()),
            ("Catalog Two".to_string(), "Gamma.flac".to_string()),
            ("Catalog Three".to_string(), "Alpha.flac".to_string()),
        ]
    );
}
