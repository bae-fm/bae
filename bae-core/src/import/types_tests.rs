use super::*;

#[cfg(test)]
mod metadata_provenance_tests {
    use super::*;

    #[test]
    fn a_metadata_provenance_round_trips_without_an_identity_proxy() {
        let provenance = MetadataProvenance::ExternalRelease {
            source: MetadataSource::MusicBrainz,
            release_id: "release-a".to_string(),
        };
        let stored = serde_json::to_string(&provenance).expect("metadata provenance encodes");
        let read_back: MetadataProvenance =
            serde_json::from_str(&stored).expect("stored metadata provenance decodes");
        assert_eq!(read_back, provenance);
    }
}

#[cfg(test)]
mod edit_shaping_tests {
    use super::*;

    fn existing_artist() -> ExistingArtist {
        ExistingArtist {
            artist_id: "artist-1".to_string(),
            name: "Existing Artist".to_string(),
            sort_name: Some("Artist, Existing".to_string()),
            musicbrainz_artist_id: Some("mb-existing".to_string()),
            discogs_artist_id: None,
        }
    }

    /// A raw form that shapes cleanly: one album artist, a track with its
    /// own artists, all pressing fields filled. The individual tests mutate
    /// one aspect to exercise a single rule.
    fn valid_form() -> RawReleaseEdit {
        RawReleaseEdit {
            album_title: "Album Title".to_string(),
            album_artist_assignments: vec![ArtistAssignment::new("Artist One")],
            album_year: "1987".to_string(),
            pressing: RawPressingEdit {
                year: "1999".to_string(),
                format: "2×LP".to_string(),
                label: "Label Name".to_string(),
                catalog_number: "CAT-123".to_string(),
                country: "US".to_string(),
                barcode: "0123456789".to_string(),
            },
            tracks: vec![RawTrackEdit {
                id: "track-0".to_string(),
                title: "Track Title".to_string(),
                artist_assignments: TrackArtistAssignments::Explicit(vec![ArtistAssignment::new(
                    "Artist Two",
                )]),
                side: 1,
                track_number: Some(1),
                file: Some(AudioFile::Standalone {
                    file_id: "01.flac".to_string(),
                }),
            }],
        }
    }

    #[test]
    fn shapes_distinct_album_and_pressing_years() {
        let shaped = valid_form().shape().expect("valid form shapes");
        assert_eq!(shaped.album_title, "Album Title");
        assert_eq!(
            shaped.album_artist_assignments,
            vec![ArtistAssignment::new("Artist One")]
        );
        assert_eq!(shaped.album_year, Some(1987));
        assert_eq!(shaped.pressing.year, Some(1999));
        assert_eq!(shaped.pressing.format.as_deref(), Some("2×LP"));
        assert_eq!(shaped.tracks.len(), 1);
        assert_eq!(shaped.tracks[0].title, "Track Title");
        assert_eq!(
            shaped.tracks[0].artist_assignments,
            TrackArtistAssignments::Explicit(vec![ArtistAssignment::new("Artist Two")])
        );
    }

    #[test]
    fn trims_new_artist_metadata_without_losing_provider_ids() {
        let mut form = valid_form();
        form.album_artist_assignments = vec![ArtistAssignment::New {
            seed: NewArtistSeed {
                name: " Artist One ".to_string(),
                sort_name: Some("  One, Artist ".to_string()),
                musicbrainz_artist_id: Some(" mb-1 ".to_string()),
                discogs_artist_id: None,
            },
        }];
        let shaped = form.shape().expect("shapes");
        assert_eq!(
            shaped.album_artist_assignments,
            vec![ArtistAssignment::New {
                seed: NewArtistSeed {
                    name: "Artist One".to_string(),
                    sort_name: Some("One, Artist".to_string()),
                    musicbrainz_artist_id: Some("mb-1".to_string()),
                    discogs_artist_id: None,
                }
            }]
        );
    }

    #[test]
    fn album_artist_mode_survives_shaping() {
        let mut form = valid_form();
        form.tracks[0].artist_assignments = TrackArtistAssignments::AlbumArtists;
        let shaped = form.shape().expect("shapes");
        assert_eq!(
            shaped.tracks[0].artist_assignments,
            TrackArtistAssignments::AlbumArtists
        );
    }

    #[test]
    fn trims_album_title() {
        let mut form = valid_form();
        form.album_title = "  Album Title  ".to_string();
        assert_eq!(form.shape().expect("shapes").album_title, "Album Title");
    }

    #[test]
    fn empty_pressing_fields_map_to_none() {
        let mut form = valid_form();
        form.pressing = RawPressingEdit {
            year: "".to_string(),
            format: "  ".to_string(),
            label: "".to_string(),
            catalog_number: "".to_string(),
            country: "".to_string(),
            barcode: "".to_string(),
        };
        let pressing = form.shape().expect("shapes").pressing;
        assert_eq!(pressing, PressingEdit::blank());
    }

    #[test]
    fn parses_year_and_trims_pressing_fields() {
        let mut form = valid_form();
        form.pressing.year = "  2001  ".to_string();
        form.pressing.country = "  JP  ".to_string();
        let pressing = form.shape().expect("shapes").pressing;
        assert_eq!(pressing.year, Some(2001));
        assert_eq!(pressing.country.as_deref(), Some("JP"));
    }

    #[test]
    fn empty_album_title_is_a_validation_error() {
        let mut form = valid_form();
        form.album_title = "   ".to_string();
        assert_eq!(form.shape(), Err(EditValidationError::EmptyAlbumTitle));
    }

    #[test]
    fn no_album_artist_is_a_validation_error() {
        let mut form = valid_form();
        form.album_artist_assignments.clear();
        assert_eq!(form.shape(), Err(EditValidationError::NoAlbumArtist));
    }

    #[test]
    fn blank_new_artist_is_a_validation_error() {
        let mut form = valid_form();
        form.album_artist_assignments = vec![ArtistAssignment::new("   ")];
        assert_eq!(form.shape(), Err(EditValidationError::EmptyArtistName));
    }

    #[test]
    fn unparseable_year_is_a_validation_error() {
        let mut form = valid_form();
        form.pressing.year = "19x9".to_string();
        assert_eq!(form.shape(), Err(EditValidationError::InvalidYear));
    }

    /// Seeding a form from a wire edit then shaping it back recovers the
    /// original wire edit: `from_user_edit` and `shape` are inverses.
    #[test]
    fn from_user_edit_round_trips_through_shape() {
        let original = ReleaseUserEdit {
            album_title: "Album Title".to_string(),
            album_artist_assignments: vec![
                ArtistAssignment::existing(existing_artist()),
                ArtistAssignment::new("Artist Two"),
            ],
            album_year: Some(1987),
            pressing: PressingEdit {
                year: Some(1999),
                format: Some("2×LP".to_string()),
                label: None,
                catalog_number: Some("CAT-123".to_string()),
                country: None,
                barcode: None,
            },
            tracks: vec![
                TrackUserEdit {
                    title: "Track One".to_string(),
                    side: 1,
                    track_number: Some(1),
                    artist_assignments: TrackArtistAssignments::Explicit(vec![
                        ArtistAssignment::new("Track Artist"),
                    ]),
                    file: Some(AudioFile::Standalone {
                        file_id: "01.flac".to_string(),
                    }),
                },
                TrackUserEdit {
                    title: "Track Two".to_string(),
                    side: 2,
                    track_number: Some(1),
                    artist_assignments: TrackArtistAssignments::AlbumArtists,
                    file: None,
                },
            ],
        };

        let raw = RawReleaseEdit::from_user_edit(original.clone(), "reset-track");
        assert_eq!(
            raw.album_artist_assignments,
            original.album_artist_assignments
        );
        assert_eq!(raw.tracks[0].id, "reset-track-0");
        assert_eq!(raw.tracks[1].id, "reset-track-1");
        assert_eq!(
            raw.tracks[0].artist_assignments,
            original.tracks[0].artist_assignments
        );
        assert_eq!(
            raw.tracks[1].artist_assignments,
            TrackArtistAssignments::AlbumArtists
        );
        assert_eq!(raw.pressing.year, "1999");
        assert_eq!(raw.album_year, "1987");
        assert_eq!(raw.pressing.label, "");

        assert_eq!(raw.shape().expect("re-shapes"), original);
    }
}

#[cfg(test)]
mod metadata_source_tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn from_str_round_trips_known_sources() {
        assert_eq!(
            MetadataSource::from_str("musicbrainz"),
            Ok(MetadataSource::MusicBrainz)
        );
        assert_eq!(
            MetadataSource::from_str("discogs"),
            Ok(MetadataSource::Discogs)
        );
        // as_str is the inverse of from_str.
        assert_eq!(
            MetadataSource::from_str(MetadataSource::MusicBrainz.as_str()),
            Ok(MetadataSource::MusicBrainz)
        );
    }

    #[test]
    fn from_str_rejects_unknown_source() {
        let err = MetadataSource::from_str("bandcamp").expect_err("unknown source should error");
        assert!(
            err.contains("unknown metadata source") && err.contains("bandcamp"),
            "unexpected error: {err}"
        );
        // The match is exact — casing isn't accepted.
        assert!(MetadataSource::from_str("MusicBrainz").is_err());
    }

    #[test]
    fn group_urls_are_source_specific() {
        assert_eq!(
            MetadataSource::MusicBrainz.group_url("rg-1"),
            "https://musicbrainz.org/release-group/rg-1"
        );
        assert_eq!(
            MetadataSource::Discogs.group_url("master-7"),
            "https://www.discogs.com/master/master-7"
        );
    }
}
