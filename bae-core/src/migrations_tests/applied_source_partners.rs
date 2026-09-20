use super::*;
use crate::import::{Catalog, MetadataRef};
use coven::rusqlite::params;
use serde_json::json;

fn original_snapshot() -> String {
    json!({
        "payloads": {
            "release": MetadataRef::new(Catalog::MusicBrainz, "selected-primary"),
            "anchor": r#"{ "id":"selected-primary", "title":"Frozen Album", "media":[], "cover-art-archive":{"front":false,"darkened":false} }"#,
            "supporting": []
        },
        "audio_durations_ms": [1234, 9876]
    }).to_string()
}

fn version_forty_two() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(42);
    migrations
}

async fn seed_candidates(
    handle: &coven::CovenHandle,
    candidates: Vec<(&'static str, Option<&'static str>)>,
) {
    handle.write(move |sql| {
        for (hash, partner) in candidates {
            sql.execute("INSERT INTO import_candidate_state(content_hash, folder_path) VALUES (?, ?)", params![hash, format!("/music/{hash}")])?;
            sql.execute("INSERT INTO import_candidate_edit(content_hash, album_title, album_year, year, format, label, catalog_number, country, barcode) VALUES (?, 'Typed Album', '', '', '', '', '', '', '')", [hash])?;
            sql.execute("INSERT INTO import_candidate_draft_provenance(content_hash, kind, source, release_id, author) VALUES (?, 'external_release', 'musicbrainz', 'selected-primary', 'user')", [hash])?;
            sql.execute("INSERT INTO import_candidate_applied_source(content_hash, snapshot) VALUES (?, ?)", params![hash, original_snapshot()])?;
            if let Some(partner) = partner {
                sql.execute("INSERT INTO import_candidate_provenance_partner(content_hash, source, release_id) VALUES (?, 'discogs', ?)", params![hash, partner])?;
            }
        }
        Ok(())
    }).await.unwrap();
}

#[tokio::test]
#[serial]
async fn applied_source_partners_preserve_primary_and_freeze_exact_selected_partner() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let handle = open(store.clone(), "freeze-partners", version_forty_two()).unwrap();
    seed_candidates(&handle, vec![("paired", Some("31")), ("single", None)]).await;
    handle.write(|sql| {
        for (source, key, document) in [
            ("musicbrainz", "selected-primary", json!({"id":"selected-primary","title":"Replaced Archive","media":[],"cover-art-archive":{"front":false,"darkened":false}})),
            ("discogs", "31", json!({"id":31,"title":"Selected Partner","master_id":41})),
            ("discogs_master", "41", json!({"id":41,"title":"Selected Album","year":1979})),
            ("discogs", "32", json!({"id":32,"title":"Unselected Partner","master_id":42})),
            ("discogs_master", "42", json!({"id":42,"title":"Unselected Album","year":1988})),
        ] {
            sql.execute("INSERT INTO source_release_payloads(source, source_release_id, json, fetched_at) VALUES (?, ?, ?, '2026-01-01T00:00:00Z')", params![source, key, document.to_string()])?;
        }
        Ok(())
    }).await.unwrap();
    drop(handle);

    let handle = open(store, "freeze-partners", all()).unwrap();
    // Later cache mutations must leave both the primary and the newly frozen
    // partner's documents unchanged in the upgraded application.
    handle
        .write(|sql| {
            sql.execute("UPDATE source_release_payloads SET json = '{}'", [])?;
            Ok(())
        })
        .await
        .unwrap();
    handle.read(|sql| {
        let version: i64 = sql.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        assert_eq!(version, i64::try_from(all().len()).expect("ladder fits"));
        let snapshots = sql.query("SELECT content_hash, snapshot FROM import_candidate_applied_source ORDER BY content_hash", [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        let original: serde_json::Value = serde_json::from_str(&original_snapshot()).unwrap();
        for (hash, snapshot) in snapshots {
            let json: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
            assert_eq!(json["payloads"], original["payloads"]);
            assert_eq!(json["audio_durations_ms"], original["audio_durations_ms"]);
            let applied: crate::import::payloads::AppliedSource = serde_json::from_str(&snapshot).unwrap();
            assert_eq!(applied.audio_durations_ms, [1234, 9876]);
            if hash == "paired" {
                assert_eq!(applied.partners.len(), 1);
                assert_eq!(applied.partners[0].release(), &MetadataRef::new(Catalog::Discogs, "31"));
                let record = applied.partners[0].records().unwrap().remove(0);
                assert_eq!(record.key(), "31");
                assert_eq!(record.album_ref(), Some(MetadataRef::new(Catalog::Discogs, "41")));
                assert!(serde_json::to_string(&applied.partners[0]).unwrap().contains("Selected Partner"));
            } else {
                assert_eq!(hash, "single");
                assert_eq!(json["partners"], json!([]));
                assert!(applied.partners.is_empty());
            }
        }
        Ok(())
    }).await.unwrap();
}

#[tokio::test]
#[serial]
async fn missing_applied_source_partner_rolls_back_the_entire_upgrade() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let handle = open(store.clone(), "missing-partner", version_forty_two()).unwrap();
    seed_candidates(&handle, vec![("a-single", None), ("z-missing", Some("31"))]).await;
    drop(handle);
    let error = match open(store.clone(), "missing-partner", all()) {
        Ok(_) => panic!("missing selected partner must fail the upgrade"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        CovenError::Migration(MigrationError::Failed { version: 43, .. })
    ));
    let connection = coven::rusqlite::Connection::open(store.db_path()).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 42);
    let snapshots: Vec<String> = connection
        .prepare("SELECT snapshot FROM import_candidate_applied_source ORDER BY content_hash")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        snapshots,
        [original_snapshot(), original_snapshot()],
        "earlier rows in the same upgrade must roll back too"
    );
}
