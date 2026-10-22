use super::*;

#[cfg(test)]
mod metadata_provenance_tests {
    use super::*;

    #[test]
    fn a_metadata_provenance_round_trips_without_an_identity_proxy() {
        let provenance = MetadataProvenance::ExternalRelease {
            record: crate::import::MetadataRef::new(Catalog::MusicBrainz, "release-a".to_string()),
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
            album_artist_assignments: vec![crate::import::Credited::plain(
                ArtistAssignment::named("Artist One"),
            )],
            album_year: "1987".to_string(),
            pressing: RawPressingEdit {
                year: "1999".to_string(),
                labels: vec![RawLabelEdit {
                    name: "Label Name".to_string(),
                    catalog_number: "CAT-123".to_string(),
                }],
                facts: crate::pressing::PressingFacts {
                    area: Some(crate::pressing::area("US")),
                    ..crate::pressing::made_of(crate::pressing::Medium::Vinyl, 2)
                },
                barcode: "0123456789".to_string(),
            },
            tracks: vec![RawTrackEdit {
                id: "track-0".to_string(),
                title: "Track Title".to_string(),
                artist_assignments: TrackArtistAssignments::Explicit(vec![
                    crate::import::Credited::plain(ArtistAssignment::named("Artist Two")),
                ]),
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
            crate::import::Credited::plain_list(vec![ArtistAssignment::named("Artist One")])
        );
        assert_eq!(shaped.album_year, Some(1987));
        assert_eq!(shaped.pressing.year, Some(1999));
        assert_eq!(
            shaped.pressing.facts.media,
            crate::pressing::made_of(crate::pressing::Medium::Vinyl, 2).media
        );
        assert_eq!(shaped.tracks.len(), 1);
        assert_eq!(shaped.tracks[0].title, "Track Title");
        assert_eq!(
            shaped.tracks[0].artist_assignments,
            TrackArtistAssignments::Explicit(vec![crate::import::Credited::plain(
                ArtistAssignment::named("Artist Two")
            )])
        );
    }

    #[test]
    fn trims_new_artist_metadata_without_losing_provider_ids() {
        let mut form = valid_form();
        form.album_artist_assignments =
            vec![crate::import::Credited::plain(ArtistAssignment::Credit {
                credit: ArtistCredit {
                    credited_as: None,
                    name: " Artist One ".to_string(),
                    sort_name: Some("  One, Artist ".to_string()),
                    musicbrainz_artist_id: Some(" mb-1 ".to_string()),
                    discogs_artist_id: None,
                },
            })];
        let shaped = form.shape().expect("shapes");
        assert_eq!(
            shaped.album_artist_assignments,
            crate::import::Credited::plain_list(vec![ArtistAssignment::Credit {
                credit: ArtistCredit {
                    credited_as: None,
                    name: "Artist One".to_string(),
                    sort_name: Some("One, Artist".to_string()),
                    musicbrainz_artist_id: Some("mb-1".to_string()),
                    discogs_artist_id: None,
                }
            }])
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

    /// Every artist the draft credits that the import may create needs a
    /// picture answer: the album's and the tracks' credits, the roles, and the
    /// works' composers. A picked library artist is never created.
    #[test]
    fn a_draft_needs_an_answer_for_every_discogs_artist_it_credits() {
        let credit = |name: &str, discogs_artist_id: &str| ArtistCredit {
            credited_as: None,
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: None,
            discogs_artist_id: Some(discogs_artist_id.to_string()),
        };
        let role = |name: &str, discogs_artist_id: &str| crate::import::CreditedRole {
            artist: credit(name, discogs_artist_id),
            source: Catalog::Discogs,
            role: None,
        };
        let mut draft = crate::import::pane::draft_of_edit(RawReleaseEdit {
            tracks: valid_form()
                .tracks
                .into_iter()
                .map(|track| RawTrackEdit {
                    file: Some(AudioFile::Standalone {
                        file_id: "01.flac".to_string(),
                    }),
                    ..track
                })
                .collect(),
            ..valid_form()
        });
        draft.album_artist_assignments = vec![
            crate::import::Credited::plain(ArtistAssignment::Credit {
                credit: credit("Album Artist", "album-artist"),
            }),
            crate::import::Credited::plain(ArtistAssignment::picked(ExistingArtist {
                artist_id: "library-artist".to_string(),
                name: "Library Artist".to_string(),
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: Some("picked-artist".to_string()),
            })),
        ];
        draft.tracks[0].edit.artist_assignments =
            TrackArtistAssignments::Explicit(vec![crate::import::Credited::plain(
                ArtistAssignment::Credit {
                    credit: credit("Track Artist", "track-artist"),
                },
            )]);
        draft.tracks[0].roles = vec![role("Track Composer", "track-composer")];
        draft.release_roles = vec![role("Release Composer", "release-composer")];
        draft.works.works = vec![crate::import::GraphWork {
            musicbrainz_work_id: "mb-work".to_string(),
            title: "Work".to_string(),
            disambiguation: None,
            work_type: None,
            composers: vec![credit("Work Composer", "work-composer")],
        }];

        assert_eq!(
            draft.discogs_artist_ids(),
            std::collections::BTreeSet::from(
                [
                    "album-artist",
                    "release-composer",
                    "track-artist",
                    "track-composer",
                    "work-composer",
                ]
                .map(str::to_string)
            )
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
            labels: vec![RawLabelEdit::default()],
            facts: Default::default(),
            barcode: "".to_string(),
        };
        let pressing = form.shape().expect("shapes").pressing;
        assert_eq!(pressing, crate::pressing::Pressing::blank());
    }

    #[test]
    fn parses_year_and_trims_pressing_fields() {
        let mut form = valid_form();
        form.pressing.year = "  2001  ".to_string();
        form.pressing.labels[0].name = "  Label  ".to_string();
        let pressing = form.shape().expect("shapes").pressing;
        assert_eq!(pressing.year, Some(2001));
        assert_eq!(pressing.labels[0].name(), Some("Label"));
    }

    /// Every label row shapes into a label, in order, with the number typed
    /// beside it; a blank row and a repeated one are left out.
    #[test]
    fn every_label_row_shapes_with_its_own_number() {
        let mut form = valid_form();
        let row = |name: &str, catalog_number: &str| RawLabelEdit {
            name: name.to_string(),
            catalog_number: catalog_number.to_string(),
        };
        form.pressing.labels = vec![
            row("Label A", "AB 100"),
            row(" ", ""),
            row("Label B", " CL 719 "),
            row("", "XY 3"),
            row("Label A", "AB 100"),
        ];
        assert_eq!(
            form.shape().expect("shapes").pressing.labels,
            vec![
                crate::pressing::ReleaseLabel::of(Some("Label A"), Some("AB 100")),
                crate::pressing::ReleaseLabel::of(Some("Label B"), Some("CL 719")),
                crate::pressing::ReleaseLabel::of(None, Some("XY 3")),
            ]
        );
    }

    /// What the pressing is was chosen, not typed, and shapes as chosen.
    #[test]
    fn chosen_pressing_facts_shape_unchanged() {
        let form = valid_form();
        let pressing = form.shape().expect("shapes").pressing;
        assert_eq!(pressing.facts, form.pressing.facts);
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
    fn a_blank_artist_credit_is_a_validation_error() {
        let mut form = valid_form();
        form.album_artist_assignments = vec![crate::import::Credited::plain(
            ArtistAssignment::named("   "),
        )];
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
                crate::import::Credited::plain(ArtistAssignment::picked(existing_artist())),
                crate::import::Credited::plain(ArtistAssignment::named("Artist Two")),
            ],
            album_year: Some(1987),
            pressing: crate::pressing::Pressing {
                year: Some(1999),
                labels: vec![crate::pressing::ReleaseLabel::of(None, Some("CAT-123"))],
                facts: Default::default(),
                barcode: None,
            },
            tracks: vec![
                TrackUserEdit {
                    title: "Track One".to_string(),
                    artist_assignments: TrackArtistAssignments::Explicit(vec![
                        crate::import::Credited::plain(ArtistAssignment::named("Track Artist")),
                    ]),
                    file: Some(AudioFile::Standalone {
                        file_id: "01.flac".to_string(),
                    }),
                },
                TrackUserEdit {
                    title: "Track Two".to_string(),
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
        assert_eq!(
            raw.pressing.labels,
            vec![RawLabelEdit {
                name: String::new(),
                catalog_number: "CAT-123".to_string(),
            }]
        );

        assert_eq!(raw.shape().expect("re-shapes"), original);
    }
}
