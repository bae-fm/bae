use super::*;

#[test]
fn album_identity_is_exposed_without_a_pressing_or_draft_claim() {
    let record = bae_catalogs::records::ReleaseRecord::album(&MetadataRef::new(
        Catalog::MusicBrainz,
        "group-1",
    ));
    let value = serde_json::to_value(AutomationReleaseRecord::from_core(record)).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "kind": "album",
            "album": { "catalog": "music_brainz", "key": "group-1" },
            "url": "https://musicbrainz.org/release-group/group-1"
        })
    );
}

#[test]
fn pressing_identity_keeps_its_known_parent() {
    let record = bae_catalogs::records::ReleaseRecord::new(
        &MetadataRef::new(Catalog::Discogs, "42"),
        Some("73".into()),
    );
    let value = serde_json::to_value(AutomationReleaseRecord::from_core(record)).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "kind": "pressing",
            "release": { "catalog": "discogs", "key": "42" },
            "album_key": "73",
            "url": "https://www.discogs.com/release/42"
        })
    );
}

/// `release_metadata_update` enforces the same rule as the desktop editor's
/// Save, so it can't write what the editor would refuse.
mod release_metadata_update_input {
    use super::*;

    fn edit(album_title: &str, album_artist_credit_names: &[&str]) -> AutomationReleaseUserEdit {
        AutomationReleaseUserEdit {
            album_title: album_title.to_string(),
            album_artist_assignments: album_artist_credit_names
                .iter()
                .map(|name| AutomationCredited {
                    artist: AutomationArtistAssignment::Credit {
                        credit: AutomationArtistCredit {
                            name: (*name).to_string(),
                            sort_name: None,
                            musicbrainz_artist_id: None,
                            discogs_artist_id: None,
                            credited_as: None,
                        },
                    },
                    join: None,
                })
                .collect(),
            album_year: None,
            pressing: AutomationPressingEdit {
                year: None,
                labels: Vec::new(),
                barcodes: Vec::new(),
                facts: Default::default(),
            },
            tracks: Vec::new(),
        }
    }

    #[test]
    fn an_empty_album_title_fails_the_edit_rule() {
        let wire = edit("", &["Artist Alpha"]).into_core();
        assert_eq!(wire.validate(), Err(bae_import::EditValidationError::EmptyAlbumTitle),);
    }

    #[test]
    fn an_artist_less_edit_fails_the_edit_rule() {
        let wire = edit("Album Alpha", &[]).into_core();
        assert_eq!(wire.validate(), Err(bae_import::EditValidationError::NoAlbumArtist),);
    }

    /// An untrimmed title is trimmed, as the desktop editor does, not refused.
    #[test]
    fn an_untrimmed_album_title_normalizes() {
        let wire = edit("  Album Alpha  ", &["  Artist Alpha  "]).into_core().normalized();
        assert_eq!(wire.validate(), Ok(()));
        assert_eq!(wire.album_title, "Album Alpha");
        assert_eq!(
            wire.album_artist_assignments,
            bae_import::Credited::plain_list([bae_import::ArtistAssignment::named("Artist Alpha")])
        );
    }

    #[test]
    fn an_existing_artist_keeps_its_library_identity() {
        let artist = AutomationExistingArtist {
            artist_id: "00000000-0000-0000-0000-000000000001".to_string(),
            name: "Artist Alpha".to_string(),
            sort_name: Some("Alpha, Artist".to_string()),
            musicbrainz_artist_id: Some("musicbrainz-artist".to_string()),
            discogs_artist_id: Some("discogs-artist".to_string()),
        };
        let mut edit = edit("Album Alpha", &[]);
        edit.album_artist_assignments = vec![AutomationCredited {
            artist: AutomationArtistAssignment::Picked {
                credit_name: None,
                artist: artist.clone(),
            },
            join: None,
        }];

        let round_trip = AutomationReleaseUserEdit::from_core(edit.into_core());
        let AutomationArtistAssignment::Picked { credit_name: None, artist: round_trip_artist } =
            &round_trip.album_artist_assignments[0].artist
        else {
            panic!("the existing artist remains an existing artist");
        };
        assert_eq!(round_trip_artist.artist_id, artist.artist_id);
        assert_eq!(round_trip_artist.name, artist.name);
        assert_eq!(round_trip_artist.sort_name, artist.sort_name);
        assert_eq!(round_trip_artist.musicbrainz_artist_id, artist.musicbrainz_artist_id);
        assert_eq!(round_trip_artist.discogs_artist_id, artist.discogs_artist_id);
    }

    /// A refused edit reaches the client as `validation`, not `import`.
    #[test]
    fn a_refused_edit_crosses_as_a_validation_error() {
        let error = AutomationError::from(LibraryError::Edit(
            bae_import::EditValidationError::EmptyAlbumTitle,
        ));
        assert_eq!(error.kind(), "validation");
        assert_eq!(error.message(), "Album title is required");
    }
}

/// The storage state and actions serialize as snake_case strings.
#[test]
fn release_storage_state_and_actions_serialize_snake_case() {
    let summary = AutomationReleaseSummary::from_core(bae_views::album_detail::ReleaseSummary {
        id: "rel-1".to_string(),
        album_id: "alb-1".to_string(),
        media: vec![bae_types::pressing::MediaCount {
            medium: bae_types::pressing::Medium::Digital,
            count: 1,
        }],
        storage_state: ReleaseStorageState::InStorage { pinned: true },
        storage_actions: vec![
            ReleaseStorageAction::Unpin,
            ReleaseStorageAction::MoveToThisDevice,
            ReleaseStorageAction::Upload,
            ReleaseStorageAction::Pin,
        ],
        upload: None,
        transitioning: true,
        file_count: 2,
        total_size: 100,
        cover: None,
    });
    let json = serde_json::to_value(summary).unwrap();

    assert_eq!(json["storage_state"], serde_json::json!({"place": "in_storage", "pinned": true}));
    assert_eq!(
        json["storage_actions"],
        serde_json::json!(["unpin", "move_to_this_device", "upload", "pin"]),
    );
}

#[test]
fn a_local_release_serializes_its_state_and_absent_transfer() {
    let summary = AutomationReleaseSummary::from_core(bae_views::album_detail::ReleaseSummary {
        id: "rel-2".to_string(),
        album_id: "alb-1".to_string(),
        media: Vec::new(),
        storage_state: ReleaseStorageState::OnOneDevice,
        storage_actions: Vec::new(),
        upload: None,
        transitioning: false,
        file_count: 0,
        total_size: 0,
        cover: None,
    });
    let json = serde_json::to_value(summary).unwrap();

    assert_eq!(json["storage_state"], serde_json::json!({"place": "on_one_device"}));
}
