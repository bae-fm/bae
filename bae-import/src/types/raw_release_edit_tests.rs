use super::*;
use bae_db::ExistingArtist;

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
        album_artist_assignments: vec![crate::Credited::plain(ArtistAssignment::named(
            "Artist One",
        ))],
        album_year: "1987".to_string(),
        pressing: RawPressingEdit {
            year: "1999".to_string(),
            labels: vec![RawLabelEdit {
                name: "Label Name".to_string(),
                catalog_number: "CAT-123".to_string(),
                ..Default::default()
            }],
            facts: bae_types::pressing::PressingFacts {
                area: Some(bae_types::pressing::area("US")),
                ..bae_types::pressing::made_of(bae_types::pressing::Medium::Vinyl, 2)
            },
            barcodes: vec!["0123456789".to_string()],
        },
        tracks: vec![RawTrackEdit {
            id: "track-0".to_string(),
            title: "Track Title".to_string(),
            artist_assignments: TrackArtistAssignments::Explicit(vec![crate::Credited::plain(
                ArtistAssignment::named("Artist Two"),
            )]),
            file: Some(AudioFile::Standalone { file_id: "01.flac".to_string() }),
        }],
    }
}

#[test]
fn shapes_distinct_album_and_pressing_years() {
    let shaped = valid_form().shape().expect("valid form shapes");
    assert_eq!(shaped.album_title, "Album Title");
    assert_eq!(
        shaped.album_artist_assignments,
        crate::Credited::plain_list(vec![ArtistAssignment::named("Artist One")])
    );
    assert_eq!(shaped.album_year, Some(1987));
    assert_eq!(shaped.pressing.year, Some(1999));
    assert_eq!(
        shaped.pressing.facts.media,
        bae_types::pressing::made_of(bae_types::pressing::Medium::Vinyl, 2).media
    );
    assert_eq!(shaped.tracks.len(), 1);
    assert_eq!(shaped.tracks[0].title, "Track Title");
    assert_eq!(
        shaped.tracks[0].artist_assignments,
        TrackArtistAssignments::Explicit(vec![crate::Credited::plain(ArtistAssignment::named(
            "Artist Two"
        ))])
    );
}

#[test]
fn trims_new_artist_metadata_without_losing_provider_ids() {
    let mut form = valid_form();
    form.album_artist_assignments = vec![crate::Credited::plain(ArtistAssignment::Credit {
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
        crate::Credited::plain_list(vec![ArtistAssignment::Credit {
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
    assert_eq!(shaped.tracks[0].artist_assignments, TrackArtistAssignments::AlbumArtists);
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
        barcodes: Vec::new(),
    };
    let pressing = form.shape().expect("shapes").pressing;
    assert_eq!(pressing, bae_types::pressing::Pressing::blank());
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

/// A label row an editor sends keeps the catalog ids of the stated row of
/// its name, at its own place or another, and shapes with them; a row
/// whose name was changed, or typed, carries none.
#[test]
fn a_label_row_keeps_its_ids_while_it_keeps_its_name() {
    let row = |name: &str, catalog_number: &str| RawLabelEdit {
        name: name.to_string(),
        catalog_number: catalog_number.to_string(),
        ..Default::default()
    };
    let stated = vec![
        RawLabelEdit { discogs_label_id: Some("7190".to_string()), ..row("Rhino", "R2 1") },
        RawLabelEdit { musicbrainz_label_id: Some("mb-harbor".to_string()), ..row("Harbor", "") },
    ];
    let sent = vec![row("Harbor", "H 2"), row("Rhino Records", "R2 1"), row("New", "")];
    let kept = RawLabelEdit::kept_ids(sent, &stated);
    assert_eq!(
        kept.iter()
            .map(|row| (row.musicbrainz_label_id.as_deref(), row.discogs_label_id.as_deref()))
            .collect::<Vec<_>>(),
        [(Some("mb-harbor"), None), (None, None), (None, None)]
    );
    let mut form = valid_form();
    form.pressing.labels = kept;
    assert_eq!(
        form.shape().expect("shapes").pressing.labels[0],
        bae_types::pressing::ReleaseLabel::of(Some("Harbor"), Some("H 2"))
            .with_ids(Some("mb-harbor".to_string()), None)
    );
}

/// Every label row shapes into a label, in order, with the number typed
/// beside it; a blank row and a repeated one are left out.
#[test]
fn every_label_row_shapes_with_its_own_number() {
    let mut form = valid_form();
    let row = |name: &str, catalog_number: &str| RawLabelEdit {
        name: name.to_string(),
        catalog_number: catalog_number.to_string(),
        ..Default::default()
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
            bae_types::pressing::ReleaseLabel::of(Some("Label A"), Some("AB 100")),
            bae_types::pressing::ReleaseLabel::of(Some("Label B"), Some("CL 719")),
            bae_types::pressing::ReleaseLabel::of(None, Some("XY 3")),
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
    form.album_artist_assignments = vec![crate::Credited::plain(ArtistAssignment::named("   "))];
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
            crate::Credited::plain(ArtistAssignment::picked(existing_artist())),
            crate::Credited::plain(ArtistAssignment::named("Artist Two")),
        ],
        album_year: Some(1987),
        pressing: bae_types::pressing::Pressing {
            year: Some(1999),
            labels: vec![bae_types::pressing::ReleaseLabel::of(None, Some("CAT-123"))],
            facts: Default::default(),
            barcodes: Vec::new(),
        },
        tracks: vec![
            TrackUserEdit {
                title: "Track One".to_string(),
                artist_assignments: TrackArtistAssignments::Explicit(vec![crate::Credited::plain(
                    ArtistAssignment::named("Track Artist"),
                )]),
                file: Some(AudioFile::Standalone { file_id: "01.flac".to_string() }),
            },
            TrackUserEdit {
                title: "Track Two".to_string(),
                artist_assignments: TrackArtistAssignments::AlbumArtists,
                file: None,
            },
        ],
    };

    let raw = RawReleaseEdit::from_user_edit(original.clone(), "reset-track");
    assert_eq!(raw.album_artist_assignments, original.album_artist_assignments);
    assert_eq!(raw.tracks[0].id, "reset-track-0");
    assert_eq!(raw.tracks[1].id, "reset-track-1");
    assert_eq!(raw.tracks[0].artist_assignments, original.tracks[0].artist_assignments);
    assert_eq!(raw.tracks[1].artist_assignments, TrackArtistAssignments::AlbumArtists);
    assert_eq!(raw.pressing.year, "1999");
    assert_eq!(raw.album_year, "1987");
    assert_eq!(
        raw.pressing.labels,
        vec![RawLabelEdit {
            name: String::new(),
            catalog_number: "CAT-123".to_string(),
            ..Default::default()
        }]
    );

    assert_eq!(raw.shape().expect("re-shapes"), original);
}

/// The session's form takes each edit as the import pane's draft does: a
/// field's text as typed, label rows keeping the ids of the row with their
/// name, and a track row's title and artists by its id, its audio kept.
#[test]
fn a_form_takes_each_edit_by_its_field_or_row() {
    let mut form = valid_form();
    form.pressing.labels[0].musicbrainz_label_id = Some("mb-label".to_string());
    form.tracks[0].file = Some(AudioFile::Standalone { file_id: "01.flac".to_string() });

    form.set_field(DraftFieldEdit::Text {
        field: CandidateEditField::PressingYear,
        value: " 2001 ".to_string(),
    });
    form.set_field(DraftFieldEdit::Labels(vec![
        RawLabelEdit { name: "Label Name".to_string(), ..Default::default() },
        RawLabelEdit { name: "Other Label".to_string(), ..Default::default() },
    ]));
    let renamed = RawTrackEdit {
        id: "track-0".to_string(),
        title: "Renamed".to_string(),
        artist_assignments: TrackArtistAssignments::AlbumArtists,
        file: None,
    };
    assert!(form.set_track(renamed.clone()));

    assert_eq!(form.pressing.year, " 2001 ");
    assert_eq!(
        form.pressing
            .labels
            .iter()
            .map(|label| label.musicbrainz_label_id.as_deref())
            .collect::<Vec<_>>(),
        [Some("mb-label"), None]
    );
    assert_eq!(form.tracks[0].title, "Renamed");
    assert_eq!(form.tracks[0].artist_assignments, TrackArtistAssignments::AlbumArtists);
    assert_eq!(
        form.tracks[0].file,
        Some(AudioFile::Standalone { file_id: "01.flac".to_string() }),
        "a row keeps its audio"
    );
    assert!(!form.set_track(RawTrackEdit { id: "no-such-row".to_string(), ..renamed }));
}
