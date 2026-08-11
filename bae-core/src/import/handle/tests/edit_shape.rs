#[test]
fn validation_folds_validate_token_outcomes() {
    use crate::config::DiscogsValidation;
    use crate::discogs::client::DiscogsError;

    assert_eq!(
        validation_from_validate_result(Ok(())),
        DiscogsValidation::Valid
    );
    // A 401 is the one outcome that rejects the stored key.
    assert_eq!(
        validation_from_validate_result(Err(DiscogsError::InvalidApiKey)),
        DiscogsValidation::Rejected
    );
    // Anything that merely fails to confirm the key leaves it unvalidated
    // to retry — never rejected.
    for couldnt_confirm in [
        DiscogsError::RateLimit,
        DiscogsError::NotFound,
        DiscogsError::Serialization(serde_json::from_str::<i32>("nope").unwrap_err()),
    ] {
        assert_eq!(
            validation_from_validate_result(Err(couldnt_confirm)),
            DiscogsValidation::Unvalidated
        );
    }
}

fn track_artist(artist_id: &str) -> crate::db::DbTrackArtist {
    crate::db::DbTrackArtist {
        id: Uuid::new_v4().to_string(),
        track_id: TRACK_1.to_string(),
        artist_id: artist_id.to_string(),
        position: 3,
        created_at: Utc::now(),
    }
}

#[test]
fn remap_links_rewrites_id_and_preserves_the_rest() {
    let ta = track_artist("parsed-1");
    let map = std::collections::HashMap::from([("parsed-1".to_string(), "db-1".to_string())]);

    let remapped = remap_links(
        std::slice::from_ref(&ta),
        &map,
        "track artist",
        |ta| &ta.artist_id,
        |ta, artist_id| ta.artist_id = artist_id,
    )
    .unwrap();
    assert_eq!(remapped.len(), 1);
    assert_eq!(remapped[0].artist_id, "db-1");
    // Everything other than the remapped artist id carries through.
    assert_eq!(remapped[0].id, ta.id);
    assert_eq!(remapped[0].track_id, TRACK_1);
    assert_eq!(remapped[0].position, 3);
}

#[test]
fn remap_links_errors_on_unmapped_id() {
    let ta = track_artist("orphan-track-artist");
    let err = remap_links(
        std::slice::from_ref(&ta),
        &std::collections::HashMap::new(),
        "track artist",
        |ta| &ta.artist_id,
        |ta, artist_id| ta.artist_id = artist_id,
    )
    .unwrap_err();
    assert!(
        matches!(&err, crate::import::ImportError::Internal { detail } if detail.contains("orphan-track-artist")),
        "error should name the unmapped id: {err}"
    );
}

// ── editor seed (the prefetch's `seed`, projected from the mapper) ───

fn mb_credit(id: &str, name: &str) -> crate::musicbrainz::MbArtistCredit {
    crate::musicbrainz::MbArtistCredit {
        name: name.to_string(),
        artist: Some(crate::musicbrainz::MbArtistRef {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            sort_name: Some(name.to_string()),
        }),
    }
}

fn mb_track(number: &str, title: &str) -> crate::musicbrainz::MbTrack {
    crate::musicbrainz::MbTrack {
        position: None,
        number: Some(number.to_string()),
        title: None,
        length: None,
        recording: Some(crate::musicbrainz::MbRecording {
            id: None,
            title: Some(title.to_string()),
            artist_credit: vec![],
            relations: vec![],
        }),
        artist_credit: vec![],
    }
}

/// A 2-side vinyl release credited to "Artist Name": A1, A2 on side 1; B1 on
/// side 2, pressed in 1969 on "Label" (CAT-1, US).
fn vinyl_response() -> crate::musicbrainz::MbReleaseResponse {
    crate::musicbrainz::MbReleaseResponse {
        id: REL_1.to_string(),
        title: "Album Title".to_string(),
        date: Some("1969".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![mb_credit("mb-artist-1", "Artist Name")],
        release_group: Some(crate::musicbrainz::MbReleaseGroupRef {
            id: "rg-1".to_string(),
            first_release_date: Some("1969".to_string()),
            relations: None,
        }),
        label_info: vec![crate::musicbrainz::MbLabelInfo {
            catalog_number: Some("CAT-1".to_string()),
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Label".to_string()),
            }),
        }],
        media: vec![crate::musicbrainz::MbMedium {
            format: Some("12\" Vinyl".to_string()),
            tracks: vec![
                mb_track("A1", "A1 title"),
                mb_track("A2", "A2 title"),
                mb_track("B1", "B1 title"),
            ],
        }],
        relations: vec![],
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    }
}

/// The editor seed for a release, exactly as `prefetch_release` builds it: the
/// commit worker's own `ParsedAlbum`, projected into the editor's shape.
fn seed_for(response: &crate::musicbrainz::MbReleaseResponse) -> crate::import::ReleaseUserEdit {
    let parsed = crate::import::musicbrainz_mapper::map_mb_response_to_db(
        response,
        None,
        None,
        &coven::SystemClock,
        &coven::UuidProvider,
    )
    .expect("synthetic MB response maps");
    parsed_album_to_user_edit(&parsed)
}

fn exact_choice() -> crate::import::IdentityChoice {
    crate::import::IdentityChoice::Exact {
        release_ref: crate::import::MetadataRef {
            id: REL_1.to_string(),
            source: MetadataSource::MusicBrainz,
        },
    }
}

/// The seed carries the mapper's per-side track numbering (A1,A2 -> 1,2 ; B1 ->
/// 1), not a release-global 1..N index. `apply_user_edit_to_seed` writes
/// `track_number` verbatim back onto the seed, so a flat index would overwrite
/// the correct per-side numbers of every multi-side vinyl / cassette /
/// multi-disc release.
#[test]
fn seed_numbers_tracks_per_side() {
    let edit = seed_for(&vinyl_response());
    let numbers: Vec<Option<i32>> = edit.tracks.iter().map(|t| t.track_number).collect();
    assert_eq!(numbers, vec![Some(1), Some(2), Some(1)]);
    let sides: Vec<i32> = edit.tracks.iter().map(|t| t.side).collect();
    assert_eq!(sides, vec![1, 1, 2]);
}

/// Exact keeps the picked release's pressing fields; Approximate and Unknown
/// blank them — the user didn't claim a specific pressing. Nothing else about
/// the seed depends on the claim.
#[test]
fn shape_user_edit_for_choice_masks_pressing_only() {
    let seed = seed_for(&vinyl_response());

    let exact = shape_user_edit_for_choice(&seed, &exact_choice());
    assert_eq!(exact.pressing.year, Some(1969));
    assert_eq!(exact.pressing.label.as_deref(), Some("Label"));
    assert_eq!(exact.pressing.catalog_number.as_deref(), Some("CAT-1"));
    assert_eq!(exact.pressing.country.as_deref(), Some("US"));
    assert_eq!(exact, seed);

    let blank = crate::import::PressingEdit::blank();
    for choice in [
        crate::import::IdentityChoice::Approximate {
            release_ref: crate::import::MetadataRef {
                id: REL_1.to_string(),
                source: MetadataSource::MusicBrainz,
            },
        },
        crate::import::IdentityChoice::Unknown,
    ] {
        let masked = shape_user_edit_for_choice(&seed, &choice);
        assert_eq!(masked.pressing, blank);
        assert_eq!(masked.album_title, seed.album_title);
        assert_eq!(masked.album_artist_names, seed.album_artist_names);
        assert_eq!(masked.tracks, seed.tracks);
    }
}

/// Every artist the release credits reaches the editor, in credit order. The
/// picker's display shape collapses them to one name; the seed must not, or the
/// commit's artist comparison reads an untouched list as an edit and drops the
/// junction rows.
#[test]
fn seed_carries_every_album_artist() {
    let mut response = vinyl_response();
    response.artist_credit = vec![
        mb_credit("mb-artist-a", "Artist A"),
        mb_credit("mb-artist-b", "Artist B"),
    ];

    let edit = seed_for(&response);

    assert_eq!(
        edit.album_artist_names,
        vec!["Artist A".to_string(), "Artist B".to_string()]
    );
}

/// A track crediting a guest seeds that name; a track with no credit of its own
/// seeds an empty list — the editor's "share the album artist" convention.
#[test]
fn seed_per_track_artist_override() {
    let mut response = vinyl_response();
    response.media[0].tracks[2].artist_credit = vec![mb_credit("mb-guest", "Guest Artist")];

    let edit = seed_for(&response);

    assert_eq!(edit.tracks[0].artist_names, Vec::<String>::new());
    assert_eq!(edit.tracks[1].artist_names, Vec::<String>::new());
    assert_eq!(
        edit.tracks[2].artist_names,
        vec!["Guest Artist".to_string()]
    );
}

// ── ImportCandidateState reducer ────────────────────────────────────

use crate::import::folder_scanner::{CategorizedFiles, InvalidReason};
use crate::import::types::{ImportPhase, ImportProgress};
use std::path::{Path, PathBuf};
