/// Deterministic clock for the `apply_user_edit_to_seed` tests — the
/// exact instant is immaterial to what they assert (artist-row
/// preservation / rebuild), only that the same one feeds every row.
fn test_clock() -> FixedClock {
    FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
}

// ── apply_user_edit_to_seed ────────────────────────────────────────

fn make_seed_album_release_track() -> (
    crate::db::DbAlbum,
    crate::db::DbRelease,
    crate::db::DbTrack,
    crate::db::DbArtist,
) {
    let now = chrono::Utc::now();
    let artist = crate::db::DbArtist {
        id: "artist-orig".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "9fd7bfa8-3c7c-4026-8559-da66af02f636".to_string(),
        title: "Album Title".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-001".to_string()),
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "rel-mb".to_string(),
        }),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Original Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180000),
        discogs_position: None,
        created_at: now,
    };
    (album, release, track, artist)
}

#[test]
fn user_edit_overrides_album_year_and_pressing_fields() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_assignments: vec![crate::import::ArtistAssignment::new("Edited Artist")],
        album_year: Some(1981),
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            format: Some("Vinyl".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("EDIT-1".to_string()),
            country: Some("JP".to_string()),
            barcode: Some("4943674000000".to_string()),
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: "Edited Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
            file: None,
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &HashMap::new(),
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(album.title, "Edited Title");
    assert_eq!(album.year, Some(1981));
    assert_eq!(release.pressing.year, Some(1995));
    assert_eq!(release.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
    assert_eq!(tracks[0].title, "Edited Track");

    // The new album artist gets a placeholder DbArtist row so the
    // import pipeline can canonicalize it at DB-write time.
    assert!(artists.iter().any(|a| a.name == "Edited Artist"));
    assert_eq!(
        album.artist_id,
        artists
            .iter()
            .find(|a| a.name == "Edited Artist")
            .unwrap()
            .id
    );
}

#[test]
fn user_edit_can_fill_country_for_approximate_seed() {
    // Approximate seed clears pressing fields; the user can supply
    // them via the editor and the overlay applies the value.
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    // Simulate the Approximate-cleared release row.
    release.pressing = crate::db::Pressing::blank();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_assignments: vec![crate::import::ArtistAssignment::new(
            artists[0].name.clone(),
        )],
        album_year: album.year,
        pressing: crate::import::PressingEdit {
            country: Some("JP".to_string()),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
            file: None,
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &HashMap::new(),
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert!(release.pressing.year.is_none());
    assert!(release.pressing.format.is_none());
}

#[test]
fn user_edit_track_count_mismatch_is_an_error() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "T".to_string(),
        album_artist_assignments: vec![crate::import::ArtistAssignment::new("A")],
        album_year: None,
        pressing: crate::import::PressingEdit::blank(),
        // Two edits but seed has one track.
        tracks: vec![
            crate::import::TrackUserEdit {
                title: "X".to_string(),
                side: 1,
                track_number: Some(1),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: None,
            },
            crate::import::TrackUserEdit {
                title: "Y".to_string(),
                side: 1,
                track_number: Some(2),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: None,
            },
        ],
    };

    let err = apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &HashMap::new(),
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::Internal { detail } if detail.contains("Track count mismatch")),
        "got: {err}"
    );
}

/// Source-id linkage on artist rows (e.g. `musicbrainz_artist_id`)
/// must survive a user edit that doesn't touch artist names. The
/// editor round-trips an unchanged artist field as the same string
/// it was seeded with, so the apply step must compare and short-
/// circuit rather than rebuild rows from name-only placeholders.
#[test]
fn user_edit_preserves_source_id_artist_rows_when_names_unchanged() {
    let now = chrono::Utc::now();
    // Seeded artist row carrying the MB id the mapper attached.
    let seed_artist = crate::db::DbArtist {
        id: "artist-mb".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-artist-1".to_string()),
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "9fd7bfa8-3c7c-4026-8559-da66af02f636".to_string(),
        title: "Album Title".to_string(),
        artist_id: seed_artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_provenance: Some(crate::import::MetadataProvenance::ExternalRelease {
            source: crate::import::MetadataSource::MusicBrainz,
            release_id: "rel-mb".to_string(),
        }),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Track Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: None,
        discogs_position: None,
        created_at: now,
    };
    // Seeded track credit pointing at the MB-id-bearing artist.
    let seed_track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &seed_artist.id,
        0,
        "track-artist-1".to_string(),
        now,
    );

    let mut album = album;
    let mut release = release;
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::<crate::db::DbAlbumArtist>::new();
    let mut track_artists = vec![seed_track_artist.clone()];

    // The user changes pressing fields but leaves artist names
    // alone. The track's edit ships `artist_names = []` because
    // the editor's "no override" form maps to empty when the
    // track's credit equals the album's.
    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_assignments: vec![crate::import::ArtistAssignment::existing(
            seed_artist.clone().into(),
        )],
        album_year: album.year,
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_assignments: crate::import::TrackArtistAssignments::Explicit(vec![
                crate::import::ArtistAssignment::existing(seed_artist.clone().into()),
            ]),
            file: None,
        }],
    };

    let existing_artists = HashMap::from([(seed_artist.id.clone(), seed_artist.clone())]);
    let explicit_existing = apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &existing_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();
    assert_eq!(explicit_existing, HashSet::from([seed_artist.id.clone()]));

    // The MB-id-bearing artist row must still exist with its
    // source binding intact — no fresh placeholder created.
    assert_eq!(artists.len(), 1, "no extra placeholder rows expected");
    assert_eq!(
        artists[0].musicbrainz_artist_id.as_deref(),
        Some("mb-artist-1"),
        "MB artist id must survive the edit",
    );
    assert_eq!(
        album.artist_id, seed_artist.id,
        "album.artist_id should still reference the seeded row",
    );

    // Track credit must still reference the seeded artist row.
    assert_eq!(track_artists.len(), 1);
    assert_eq!(track_artists[0].artist_id, seed_artist.id);
}

/// User-renaming an artist must rebuild the credit rows. The new
/// name has no source binding, so the inserted `DbArtist` row
/// carries `None` for both source ids.
#[test]
fn user_edit_renaming_album_artist_rebuilds_credits() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_assignments: vec![crate::import::ArtistAssignment::new("Different Artist")],
        album_year: album.year,
        pressing: crate::import::PressingEdit::blank(),
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
            file: None,
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &HashMap::new(),
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    let new_artist = artists
        .iter()
        .find(|a| a.name == "Different Artist")
        .expect("new placeholder should be inserted");
    assert!(new_artist.musicbrainz_artist_id.is_none());
    assert!(new_artist.discogs_artist_id.is_none());
    assert_eq!(album.artist_id, new_artist.id);
}

#[test]
fn dropping_a_track_removes_its_disconnected_work_graph() {
    let now = test_clock().0;
    let (album, release, first_track, artist) = make_seed_album_release_track();
    let second_track = crate::db::DbTrack {
        id: "track-2".to_string(),
        release_id: release.id.clone(),
        title: "Second Track".to_string(),
        side: 1,
        track_number: Some(2),
        duration_ms: None,
        discogs_position: None,
        created_at: now,
    };
    let work = |id: &str| crate::db::DbWork {
        id: id.to_string(),
        title: format!("Work {id}"),
        disambiguation: None,
        work_type: None,
        musicbrainz_work_id: format!("mb-{id}"),
        created_at: now,
    };
    let mut parsed = crate::import::ParsedAlbum {
        album,
        release,
        tracks: vec![first_track, second_track],
        artists: vec![
            artist,
            crate::db::DbArtist {
                id: "artist-kept-work".into(),
                name: "Kept Work Artist".into(),
                sort_name: None,
                discogs_artist_id: Some("discogs-kept-work".into()),
                musicbrainz_artist_id: None,
                created_at: now,
            },
            crate::db::DbArtist {
                id: "artist-dropped-work".into(),
                name: "Dropped Work Artist".into(),
                sort_name: None,
                discogs_artist_id: Some("discogs-dropped-work".into()),
                musicbrainz_artist_id: None,
                created_at: now,
            },
        ],
        album_artists: Vec::new(),
        track_artists: Vec::new(),
        work_graph: crate::import::ParsedWorkGraph {
            works: vec![work("kept"), work("kept-child"), work("dropped"), work("dropped-child")],
            work_artists: vec![
                crate::db::DbWorkArtist::new(
                    "kept-child",
                    "artist-kept-work",
                    0,
                    crate::import::MetadataSource::MusicBrainz,
                    "kept-work-artist".into(),
                    now,
                ),
                crate::db::DbWorkArtist::new(
                    "dropped-child",
                    "artist-dropped-work",
                    0,
                    crate::import::MetadataSource::MusicBrainz,
                    "dropped-work-artist".into(),
                    now,
                ),
            ],
            work_parts: vec![
                crate::db::DbWorkPart::new(
                    "kept",
                    "kept-child",
                    0,
                    crate::import::MetadataSource::MusicBrainz,
                    "kept-part".into(),
                    now,
                ),
                crate::db::DbWorkPart::new(
                    "dropped",
                    "dropped-child",
                    0,
                    crate::import::MetadataSource::MusicBrainz,
                    "dropped-part".into(),
                    now,
                ),
            ],
            track_works: vec![
                crate::db::DbTrackWork::new(
                    "track-1",
                    "kept",
                    0,
                    crate::import::MetadataSource::MusicBrainz,
                    "kept-track-work".into(),
                    now,
                ),
                crate::db::DbTrackWork::new(
                    "track-2",
                    "dropped",
                    0,
                    crate::import::MetadataSource::MusicBrainz,
                    "dropped-track-work".into(),
                    now,
                ),
            ],
        },
        release_artist_roles: Vec::new(),
        track_artist_roles: Vec::new(),
        identities: Vec::new(),
    };
    let mut edit = Some(crate::import::ReleaseUserEdit {
        album_title: "Album Title".into(),
        album_artist_assignments: vec![crate::import::ArtistAssignment::new("Artist Name")],
        album_year: None,
        pressing: crate::import::PressingEdit::blank(),
        tracks: vec![
            crate::import::TrackUserEdit {
                title: "First Track".into(),
                side: 1,
                track_number: Some(1),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: Some(crate::import::AudioFile::Standalone {
                    file_id: "audio-1".into(),
                }),
            },
            crate::import::TrackUserEdit {
                title: "Second Track".into(),
                side: 1,
                track_number: Some(2),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: None,
            },
        ],
    });

    let mut prepared_projection = parsed.clone();
    let mapped = crate::import::RawReleaseEdit::from_user_edit(
        edit.clone().expect("the mapped edit is present"),
        crate::import::pane::CANDIDATE_TRACK_ID_PREFIX,
    );
    crate::import::pane::retain_mapped_source_track_metadata(
        &mut prepared_projection,
        &mapped.tracks,
        crate::import::pane::CANDIDATE_TRACK_ID_PREFIX,
    );
    assert_eq!(
        crate::import::pane::source_discogs_artist_ids(&prepared_projection),
        std::collections::BTreeSet::from(["discogs-kept-work".to_string()]),
        "a source track without audio cannot require an artist asset"
    );

    settle_track_rows(
        &mut parsed,
        &mut edit,
        &crate::import::folder_scanner::CategorizedFiles { files: Vec::new() },
        &SequentialIdProvider::new("track"),
        now,
    );

    assert_eq!(
        parsed
            .work_graph
            .works
            .iter()
            .map(|work| work.id.as_str())
            .collect::<Vec<_>>(),
        vec!["kept", "kept-child"]
    );
    assert_eq!(parsed.work_graph.work_artists.len(), 1);
    assert_eq!(parsed.work_graph.work_parts.len(), 1);
    assert_eq!(parsed.work_graph.track_works.len(), 1);
    assert_eq!(
        crate::import::pane::source_discogs_artist_ids(&parsed),
        std::collections::BTreeSet::from(["discogs-kept-work".to_string()])
    );
}

// ── build_audio_formats: CUE track byte windows ────────────────────

/// Build the `TrackFile::CueBacked` list for a single-file CUE album, reusing
/// the same codec probe the scanner stores on the candidate.
fn cue_backed_tracks(dir: &str) -> Vec<TrackFile> {
    let audio_path = PathBuf::from(format!("{dir}/Test Album.ape"));
    let cue_path = PathBuf::from(format!("{dir}/Test Album.cue"));
    let cue_sheet = crate::cue_flac::parse_cue_sheet(&cue_path).expect("parse cue");
    let probe = crate::audio_codec::probe_audio_from_path(
        audio_path.to_str().expect("fixture path is UTF-8"),
    )
    .expect("analyze ape");
    let cue_pair = Arc::new(crate::import::types::CueFlacAnalysis {
        cue_sheet,
        audio_files: vec![crate::import::types::CueAnalyzedAudioFile {
            file_reference: "Test Album.ape".to_string(),
            path: audio_path.clone(),
            probe,
        }],
    });
    (0..cue_pair.cue_sheet.tracks.len())
        .map(|index| TrackFile::CueBacked {
            db_track: DbTrack {
                id: format!("track-{index}"),
                release_id: "rel".to_string(),
                title: format!("Track {index}"),
                side: 1,
                track_number: Some(index as i32 + 1),
                duration_ms: None,
                discogs_position: None,
                created_at: test_clock().0,
            },
            file_path: audio_path.clone(),
            cue_pair: Arc::clone(&cue_pair),
            cue_index: index,
        })
        .collect()
}

/// A CUE track's read-ahead ceiling is its `end_byte`; playback fills up to it
/// and stops, so every non-last track must carry a real end byte or the fill
/// streams the whole rest of the shared file. Ends derive from the next track's
/// start byte (`start[N+1]`), computed via `seek_landing_bytes` -- the AVIO
/// landing, defined for every format, including APE whose packets carry no byte
/// position. This drives `build_audio_formats` over the APE CUE fixture and
/// asserts the user-visible outcome: the two non-last tracks get `Some`,
/// ascending, in-file end bytes and the last track runs to EOF (`None`).
#[test]
fn build_audio_formats_gives_ape_cue_tracks_real_end_bytes() {
    crate::audio_codec::init();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/cue_ape");
    let tracks = cue_backed_tracks(dir);
    assert_eq!(
        tracks.len(),
        3,
        "fixture is a 3-track single-file CUE album"
    );

    let file_size = std::fs::metadata(format!("{dir}/Test Album.ape"))
        .unwrap()
        .len() as i64;
    let mut file_ids = HashMap::new();
    file_ids.insert(
        PathBuf::from(format!("{dir}/Test Album.ape")),
        "file-1".to_string(),
    );

    let built = ImportService::build_audio_formats(
        &tracks,
        &file_ids,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .expect("build_audio_formats");

    let main_segments: Vec<_> = built
        .audio_segments
        .iter()
        .filter(|segment| segment.role == crate::db::DbAudioSegmentRole::Main)
        .collect();
    let ends: Vec<Option<i64>> = main_segments
        .iter()
        .map(|segment| segment.end_byte)
        .collect();
    // Non-last tracks carry a real, ascending, in-file end byte.
    let e0 = ends[0].expect("track 1 (non-last) must have an end byte");
    let e1 = ends[1].expect("track 2 (non-last) must have an end byte");
    assert!(
        e0 > 0 && e0 < file_size,
        "track 1 end within file: {e0} of {file_size}"
    );
    assert!(
        e1 > 0 && e1 < file_size,
        "track 2 end within file: {e1} of {file_size}"
    );
    assert!(e1 > e0, "end bytes ascend track to track: {ends:?}");
    // The last track runs to EOF.
    assert_eq!(ends[2], None, "the last track runs to EOF");

    // Each track's end is the next track's start byte -- one boundary, not two.
    assert_eq!(
        main_segments[1].start_byte,
        Some(e0),
        "track 2 starts where track 1 ends"
    );
    assert_eq!(
        main_segments[2].start_byte,
        Some(e1),
        "track 3 starts where track 2 ends"
    );
}
