use super::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::payloads::ReleasePayloads;
use bae_catalogs::records::LookupRef;
use bae_types::mediums::ReleaseMedium;
use bae_types::pressing::Medium;
use std::num::NonZeroU16;

/// A file or a Subsonic song states its track's medium as its disc, for a
/// release of several mediums only.
#[test]
fn the_disc_a_track_states_is_its_medium_of_several() {
    let row = |position: u16| {
        DbReleaseMedium::new(
            &coven::UuidIds,
            "release-id",
            NonZeroU16::new(position).expect("mediums count from one"),
            ReleaseMedium::one_sided(Some(Medium::Cd)),
            chrono::Utc::now(),
        )
    };
    let mut track = DbTrack::new_test("release-id", "track-id", "Track Title", 3);
    let one = DbReleaseMediums(vec![row(1)]);
    assert_eq!(one.disc_number(&track), None);
    let two = DbReleaseMediums(vec![row(1), row(2)]);
    track.medium_id.clone_from(&two.0[1].id);
    assert_eq!(two.disc_number(&track), Some(2));
}

/// A library check carries the catalog, release and group keys of the
/// release's detail, with a group and without one.
#[cfg(desktop)]
#[test]
fn library_check_matches_detail_keys_with_and_without_source_groups() {
    for group in [None, Some(789)] {
        for source in [LookupCatalog::Discogs, LookupCatalog::MusicBrainz] {
            let anchor = match source {
                LookupCatalog::Discogs => serde_json::json!({
                    "id": 123, "title": "Album Title", "master_id": group
                }),
                LookupCatalog::MusicBrainz => serde_json::json!({
                    "id": "123", "title": "Album Title", "artist-credit": [],
                    "label-info": [], "media": [], "relations": [],
                    "cover-art-archive": { "front": false, "darkened": false },
                    "release-group": group.map(|id| serde_json::json!({ "id": id.to_string() }))
                }),
            };
            let payloads = ReleasePayloads::for_test(
                LookupRef::new(source, "123"),
                anchor.to_string(),
                Vec::new(),
            );
            let check = LibraryCheck::of_release(&payloads.extract().unwrap());
            let detail = payloads.extract().unwrap().detail(&[]);
            assert_eq!(check.source, detail.source.catalog());
            assert_eq!(check.release_id, detail.release_id);
            assert_eq!(check.source_group_id, detail.source_group_id);
            assert_eq!(check.source_group_id, group.map(|id| id.to_string()));
        }
    }
}
