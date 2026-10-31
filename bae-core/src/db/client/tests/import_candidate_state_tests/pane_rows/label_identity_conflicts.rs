use super::*;

/// A failure stopping on a label conflict reads back with both library
/// labels as they stand.
#[tokio::test]
async fn a_label_identity_conflict_round_trips_with_both_library_labels() {
    let (db, _tmp) = empty_db().await;
    let (_, hash) = stored_pane_candidate(&db).await;
    db.call(|sql| {
        sql.execute_batch(
            "INSERT INTO labels (id, name, name_key, discogs_label_id, created_at) \
                 VALUES ('by-discogs', 'Label Name', 'labelname', '7190', \
                         '2026-01-01T00:00:00Z'); \
             INSERT INTO labels (id, name, name_key, musicbrainz_label_id, created_at) \
                 VALUES ('by-musicbrainz', 'Label Name Records', 'labelname', 'mb-label', \
                         '2026-01-01T00:00:00Z');",
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let existing = |id: &str, name: &str, musicbrainz: Option<&str>, discogs: Option<&str>| {
        crate::import::ExistingLabel {
            label_id: id.to_string(),
            name: name.to_string(),
            musicbrainz_label_id: musicbrainz.map(str::to_string),
            discogs_label_id: discogs.map(str::to_string),
        }
    };
    let expected = ImportFailure {
        reason: crate::import::ImportFailureReason::error("the label identities disagree"),
        failed_at: fixed_now(),
        identity_conflict: Some(crate::import::IdentityConflict::Label(
            crate::import::LabelIdentityConflict {
                incoming_label_name: "Label Name".to_string(),
                discogs_label_id: "7190".to_string(),
                musicbrainz_label_id: "mb-label".to_string(),
                discogs_label: existing("by-discogs", "Label Name", None, Some("7190")),
                musicbrainz_label: existing(
                    "by-musicbrainz",
                    "Label Name Records",
                    Some("mb-label"),
                    None,
                ),
            },
        )),
    };

    db.save_import_candidate_failure(&hash, 0, &expected)
        .await
        .unwrap();

    let stored = db
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .unwrap();
    assert_eq!(stored, expected);
}
