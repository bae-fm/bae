//! Where each of a draft's album-level fields came from, and what the catalogs
//! claiming the pick say about it.
//!
//! Every assertion reads the pane, because the per-field dot is drawn from
//! what `candidate_pane` hands back and from nothing else.

use super::*;
use crate::import::{CandidateEditField, FieldDot, FieldOrigin};
use serial_test::serial;

fn provenance(
    pane: &crate::import::ImportCandidateDetail,
    field: CandidateEditField,
) -> crate::import::FieldProvenance {
    pane.field_provenance
        .iter()
        .find(|entry| entry.field == field)
        .expect("every album-level field has one entry")
        .clone()
}

fn claim(
    provenance: &crate::import::FieldProvenance,
    catalog: crate::import::Catalog,
) -> Option<String> {
    provenance
        .claims
        .iter()
        .find(|claim| claim.catalog == catalog)
        .expect("the catalog claiming the pick states its reading")
        .value
        .clone()
}

/// A Discogs release stating a label and a catalog number, so two catalogs can
/// be made to disagree about one field and agree about another.
fn seed_discogs_pressing(release_id: &str, label: &str, catalog_number: &str) {
    let raw_release = serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": "Album Title",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "country": "US",
        "labels": [{ "name": label, "catno": catalog_number }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track One",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }],
    })
    .to_string();
    crate::discogs::client::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    crate::discogs::client::seed_release_cache(release_id, raw_release);
    crate::discogs::client::seed_artist_image_response("1", None);
    crate::musicbrainz::seed_discogs_url_lookup(release_id, None);
}

/// The folder read as its own tags: every field the tags state was read from
/// them, and nothing describes a field they leave blank.
#[tokio::test(flavor = "multi_thread")]
async fn a_tag_read_draft_says_the_files_own_tags() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    let title = provenance(&pane, CandidateEditField::AlbumTitle);
    assert!(
        !pane.metadata_draft.album_title.is_empty(),
        "the fixture's tags state an album title"
    );
    assert_eq!(title.origin, Some(FieldOrigin::Tags));
    assert_eq!(
        title.dot, None,
        "a value read from the tags is not worth pointing at"
    );

    let barcode = provenance(&pane, CandidateEditField::Barcode);
    assert!(pane.metadata_draft.pressing.barcode.is_empty());
    assert_eq!(
        barcode.origin, None,
        "an absent value came from nowhere"
    );
}

/// Typing in a field makes the value the person's, and emptying it leaves no
/// value for an origin to describe.
#[tokio::test(flavor = "multi_thread")]
async fn typing_makes_the_value_the_persons() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_edit_field(&key, CandidateEditField::Label, "Typed Label".to_string())
        .await
        .unwrap();
    let typed = provenance(&pane(&handle, &key).await, CandidateEditField::Label);
    assert_eq!(typed.origin, Some(FieldOrigin::Typed));
    assert_eq!(typed.dot, Some(FieldDot::Typed));

    handle
        .set_candidate_edit_field(&key, CandidateEditField::Label, String::new())
        .await
        .unwrap();
    let emptied = provenance(&pane(&handle, &key).await, CandidateEditField::Label);
    shut_down(handle).await;
    assert_eq!(emptied.origin, None);
    assert_eq!(emptied.dot, None);
}

/// Reading the folder as its own tags again is a reset: what a person typed
/// goes back to what the tags say, and the tags are where it was read.
#[tokio::test(flavor = "multi_thread")]
async fn resetting_to_the_tags_drops_what_was_typed() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let from_tags = pane(&handle, &key).await.metadata_draft;

    handle
        .set_candidate_edit_field(&key, CandidateEditField::AlbumTitle, "Typed Title".to_string())
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(&key, CandidateEditField::Label, "Typed Label".to_string())
        .await
        .unwrap();
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileTags,
        )
        .await
        .unwrap();

    let reset = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(
        reset.metadata_draft.album_title, from_tags.album_title,
        "the title the tags state replaces the one that was typed over it"
    );
    assert_eq!(
        provenance(&reset, CandidateEditField::AlbumTitle).origin,
        Some(FieldOrigin::Tags)
    );
    assert_eq!(
        reset.metadata_draft.pressing.label, from_tags.pressing.label,
        "a field the tags leave blank goes back to blank"
    );
    assert_eq!(
        provenance(&reset, CandidateEditField::Label).origin,
        (!from_tags.pressing.label.trim().is_empty()).then_some(FieldOrigin::Tags),
        "and nothing describes it unless the tags state one"
    );
}

/// Clearing the metadata blanks the draft, so no field is left with an origin.
#[tokio::test(flavor = "multi_thread")]
async fn clearing_the_metadata_leaves_no_origin() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .set_candidate_edit_field(&key, CandidateEditField::Label, "Typed Label".to_string())
        .await
        .unwrap();
    handle.clear_candidate_metadata(key.clone()).await.unwrap();
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    assert!(
        pane.field_provenance
            .iter()
            .all(|entry| entry.origin.is_none() && entry.dot.is_none()),
        "a cleared draft states nothing, so nothing describes it: {:?}",
        pane.field_provenance
    );
}

/// Picking a release reads every field the catalog states from that catalog,
/// and what a person typed stands through the rewrite.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_reads_its_fields_from_the_catalog_and_keeps_what_was_typed() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();
    handle
        .set_candidate_edit_field(&key, CandidateEditField::Barcode, "5099749".to_string())
        .await
        .unwrap();

    let release_id = "70000101";
    seed_discogs_pressing(release_id, "Label Name", "CAT-1");
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    let label = provenance(&pane, CandidateEditField::Label);
    assert_eq!(pane.metadata_draft.pressing.label, "Label Name");
    assert_eq!(
        label.origin,
        Some(FieldOrigin::Record(crate::import::Catalog::Discogs)),
        "the pick's own document is where the field was read"
    );
    assert_eq!(
        claim(&label, crate::import::Catalog::Discogs),
        Some("Label Name".to_string())
    );
    assert_eq!(label.dot, None, "one catalog disagrees with nobody");

    let barcode = provenance(&pane, CandidateEditField::Barcode);
    assert_eq!(
        pane.metadata_draft.pressing.barcode, "5099749",
        "the rewrite leaves what the person typed"
    );
    assert_eq!(barcode.origin, Some(FieldOrigin::Typed));
    assert_eq!(barcode.dot, Some(FieldDot::Typed));
}

/// Two catalogs claiming one pressing each state their own reading. The field
/// they disagree about is the one worth pointing at — including when a person
/// has typed over it, because the alternatives are what the dot invites a look
/// at.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn the_catalogs_readings_say_where_they_disagree() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();

    let discogs_release_id = "70000102";
    seed_discogs_pressing(discogs_release_id, "Label Name", "CAT-1");
    let mb_release_id = "provenance-mb-rel-1";
    seed_mb_pressing(mb_release_id, Some("provenance-mb-group-1"), "CAT-2", None);
    let partner = crate::import::MetadataRef::new(
        crate::import::Catalog::MusicBrainz,
        mb_release_id.to_string(),
    );
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    discogs_release_id.to_string(),
                ),
                partners: vec![partner],
            },
        )
        .await
        .unwrap();

    let picked = pane(&handle, &key).await;
    let catalog_number = provenance(&picked, CandidateEditField::CatalogNumber);
    assert_eq!(
        claim(&catalog_number, crate::import::Catalog::Discogs),
        Some("CAT-1".to_string())
    );
    assert_eq!(
        claim(&catalog_number, crate::import::Catalog::MusicBrainz),
        Some("CAT-2".to_string())
    );
    assert_eq!(catalog_number.dot, Some(FieldDot::Disagreement));

    let label = provenance(&picked, CandidateEditField::Label);
    assert_eq!(
        claim(&label, crate::import::Catalog::MusicBrainz),
        Some("Label Name".to_string())
    );
    assert_eq!(
        label.dot, None,
        "two catalogs stating the same thing disagree about nothing"
    );

    handle
        .set_candidate_edit_field(
            &key,
            CandidateEditField::CatalogNumber,
            "CAT-3".to_string(),
        )
        .await
        .unwrap();
    let typed_over = provenance(
        &pane(&handle, &key).await,
        CandidateEditField::CatalogNumber,
    );
    shut_down(handle).await;
    assert_eq!(typed_over.origin, Some(FieldOrigin::Typed));
    assert_eq!(
        typed_over.dot,
        Some(FieldDot::Disagreement),
        "a disagreement outranks a typed value: the readings are still there \
         to look at"
    );
}

/// The catalogs compared are the release's records, not the releases the pick
/// claims: a MusicBrainz pick whose document links a Discogs release has that
/// release's document archived beside its own, and the two are compared the
/// same way before the import — in the candidate pane — and after it, in the
/// library's editor.
#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_cross_linked_document_is_compared_before_import_and_after() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();

    let discogs_release_id = "70000103";
    seed_discogs_pressing(discogs_release_id, "Other Label", "CAT-1");
    let mb_release_id = "provenance-mb-rel-2";
    // No release group: the pick would otherwise fetch the group's front
    // cover from the archive, which no test serves.
    seed_mb_pressing(mb_release_id, None, "CAT-1", Some(discogs_release_id));
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::MusicBrainz,
                    mb_release_id.to_string(),
                ),
                partners: vec![],
            },
        )
        .await
        .unwrap();

    let picked = pane(&handle, &key).await;
    let crate::import::triage::TriageReading::Identified { records } = &picked.row.reading else {
        panic!("a pick reads as identified");
    };
    assert_eq!(
        records
            .iter()
            .map(|record| record.catalog)
            .collect::<Vec<_>>(),
        vec![
            crate::import::Catalog::MusicBrainz,
            crate::import::Catalog::Discogs
        ],
        "the cross-linked release is one of the pressing's records"
    );
    let label = provenance(&picked, CandidateEditField::Label);
    assert_eq!(
        claim(&label, crate::import::Catalog::MusicBrainz),
        Some("Label Name".to_string())
    );
    assert_eq!(
        claim(&label, crate::import::Catalog::Discogs),
        Some("Other Label".to_string()),
        "the cross-linked document states its own reading"
    );
    assert_eq!(label.dot, Some(FieldDot::Disagreement));
    let catalog_number = provenance(&picked, CandidateEditField::CatalogNumber);
    assert_eq!(
        catalog_number.dot, None,
        "the two documents agree about the catalog number"
    );

    let mut events = handle.subscribe_events();
    let import_id = handle
        .start_import(&key, crate::import::StorageMode::Local, false)
        .await
        .expect("the picked candidate enters the import queue");
    let (release_id, _) = await_import_outcome(&mut events, &import_id)
        .await
        .unwrap_or_else(|error| panic!("import failed: {error}"));
    let seed = handle
        .library_manager
        .release_edit_seed(&release_id)
        .await
        .unwrap();
    shut_down(handle).await;

    let stored_label = seed
        .field_provenance
        .iter()
        .find(|entry| entry.field == CandidateEditField::Label)
        .expect("every album-level field has one entry");
    assert_eq!(stored_label.claims, label.claims);
    assert_eq!(stored_label.dot, Some(FieldDot::Disagreement));
}

/// A one-track MusicBrainz release stating a label and a catalog number, in
/// the release group `release_group_id` names, and — where `discogs_release`
/// names one — an editor's link to the Discogs release of the same pressing.
fn seed_mb_pressing(
    release_id: &str,
    release_group_id: Option<&str>,
    catalog_number: &str,
    discogs_release: Option<&str>,
) {
    let relations = discogs_release
        .map(|id| crate::musicbrainz::MbRelation {
            url: Some(crate::musicbrainz::MbUrlResource {
                resource: Some(format!("https://www.discogs.com/release/{id}")),
            }),
            target_type: Some("url".to_string()),
            relation_type: Some("discogs".to_string()),
            direction: None,
            artist: None,
            work: None,
            target_credit: None,
        })
        .into_iter()
        .collect();
    let response = crate::musicbrainz::MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![crate::musicbrainz::MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(crate::musicbrainz::MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: release_group_id.map(|id| crate::musicbrainz::MbReleaseGroupRef {
            id: id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![crate::musicbrainz::MbLabelInfo {
            label: Some(crate::musicbrainz::MbLabel {
                name: Some("Label Name".to_string()),
            }),
            catalog_number: Some(catalog_number.to_string()),
        }],
        media: vec![crate::musicbrainz::MbMedium {
            discs: vec![],
            format: Some("CD".to_string()),
            tracks: vec![crate::musicbrainz::MbTrack {
                position: Some(1),
                number: Some("1".to_string()),
                title: None,
                length: None,
                recording: Some(crate::musicbrainz::MbRecording {
                    id: None,
                    title: Some("Track One".to_string()),
                    artist_credit: vec![],
                    relations: vec![],
                }),
                artist_credit: vec![],
            }],
        }],
        relations,
        cover_art_archive: crate::musicbrainz::MbCoverArtArchive {
            front: false,
            darkened: false,
        },
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    crate::musicbrainz::seed_release_cache(release_id, raw_json);
    if let Some(release_group_id) = release_group_id {
        crate::musicbrainz::seed_release_group_json_cache(
            release_group_id,
            serde_json::json!({ "id": release_group_id }).to_string(),
        );
    }
}
