//! A draft's roles and works as stored values.

use crate::tables::import_state::tests::*;
use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;

/// A draft's roles and works are values it stores: they read back as they
/// were written.
#[tokio::test]
async fn preparation_round_trips_a_drafts_roles_and_works() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    fetched(&db, "release-with-role").await;
    let (_, hash) = stored_pane_candidate(&db).await;
    let credit = |name: &str, discogs: Option<&str>| bae_catalogs::credits::ArtistCredit {
        credited_as: Some(format!("{name} Printed")),
        name: name.to_string(),
        sort_name: None,
        musicbrainz_artist_id: None,
        discogs_artist_id: discogs.map(str::to_string),
    };
    let mut draft = candidate_draft("Release Title", "Artist Name");
    draft.tracks[0].stated_length_ms = Some(181_000);
    draft.tracks[0].roles = vec![bae_catalogs::credits::CreditedRole {
        artist: credit("Role Artist", Some("role-artist")),
        source: bae_catalogs::catalog::Catalog::Discogs,
        role: Some("Written-By".to_string()),
    }];
    draft.tracks[0].works = vec!["mb-work-child".to_string()];
    draft.release_roles = vec![bae_catalogs::credits::CreditedRole {
        artist: credit("Release Composer", None),
        source: bae_catalogs::catalog::Catalog::Discogs,
        role: None,
    }];
    draft.works = bae_catalogs::works::WorkGraph {
        works: vec![
            bae_catalogs::works::GraphWork {
                musicbrainz_work_id: "mb-work-child".to_string(),
                title: "Movement".to_string(),
                disambiguation: None,
                work_type: Some("Song".to_string()),
                composers: vec![credit("Composer Name", None)],
            },
            bae_catalogs::works::GraphWork {
                musicbrainz_work_id: "mb-work-parent".to_string(),
                title: "Suite".to_string(),
                disambiguation: Some("whole".to_string()),
                work_type: None,
                composers: Vec::new(),
            },
        ],
        parts: vec![bae_catalogs::works::WorkPart {
            parent: "mb-work-parent".to_string(),
            child: "mb-work-child".to_string(),
        }],
    };

    crate::tables::ImportTables::new(db.clone())
        .apply_source(
            &host_root("/music"),
            &as_read(&hash, 0),
            &pane_candidate_key(),
            &crate::CandidateMetadataDraft {
                draft: draft.clone(),
                provenance: Some(release_pick("release-with-role")),
                cover: None,
            },
        )
        .await
        .unwrap();

    let preparation = tables
        .load_candidate_preparation(&hash)
        .await
        .unwrap()
        .and_then(crate::tables::CandidatePreparationRead::stored)
        .expect("the candidate is prepared");
    assert_eq!(preparation.metadata.draft, draft);
}
