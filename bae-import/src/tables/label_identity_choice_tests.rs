//! An import's answers to a label's conflicting catalog identities: the
//! person's answer recorded, the merge "same label" makes, and the import
//! naming the label the answer names.

use super::*;
use bae_db::client::label_resolution::{import_label_write, write_release_labels_on};
use bae_db::test_fixtures::labels::{
    add_label, ids_of, label, library, placed, stored_label, with_ids, RELEASE,
};
use bae_types::pressing::ReleaseLabel;

const CANDIDATE: &str = "candidate-hash";

/// Write `RELEASE`'s labels as an import of `CANDIDATE` would, following its
/// answers to label conflicts.
async fn write_as_import(
    db: &Database,
    labels: Vec<ReleaseLabel>,
) -> Result<(), IdentityWriteError> {
    let parts = db.label_parts(&labels);
    db.write_resolving_identities(move |sql| {
        let answers =
            crate::tables::label_identity_choice::label_answers_on(&sql, CANDIDATE, &labels)?;
        write_release_labels_on(&sql, RELEASE, &labels, &import_label_write(&parts, &answers))
    })
    .await
}

/// Record `CANDIDATE`'s failed import stopping on the conflict over
/// `by-discogs` and `by-musicbrainz`; the first time, the candidate too.
async fn record_conflict(db: &Database, first: bool) {
    let candidate = if first {
        "INSERT INTO import_candidate_state (content_hash, candidate_key) \
             VALUES ('candidate-hash', CAST('/music/Album' AS BLOB));"
    } else {
        ""
    };
    let batch = format!(
        "{candidate} \
         INSERT INTO import_candidate_failure (content_hash, error, failed_at) \
             VALUES ('candidate-hash', 'conflict', '2026-01-01T00:00:00Z'); \
         INSERT INTO import_candidate_label_identity_conflict \
             (content_hash, incoming_label_name, discogs_label_id, musicbrainz_label_id, \
              discogs_library_label_id, musicbrainz_library_label_id) \
             VALUES ('candidate-hash', 'Label Name', '7190', 'mb-label', \
                     'by-discogs', 'by-musicbrainz');"
    );
    db.call(move |sql| {
        sql.execute_batch(&batch)?;
        Ok(())
    })
    .await
    .unwrap();
}

async fn has_failure(db: &Database) -> bool {
    db.read(|sql| {
        Ok(sql
            .query_row(
                "SELECT 1 FROM import_candidate_failure WHERE content_hash = ?",
                [CANDIDATE],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    })
    .await
    .unwrap()
}

fn conflicting() -> ReleaseLabel {
    with_ids(label(Some("Label Name"), Some("LN 1")), Some("mb-label"), Some("7190"))
}

/// "Same label" merges the MusicBrainz library label into the Discogs one
/// (bae-db's merge, tested beside it), dismisses the failure, and the import
/// then names the survivor.
#[tokio::test]
async fn the_same_label_answer_merges_the_two_and_the_import_names_the_survivor() {
    let (db, _tmp) = library(crate::TEST_SCHEMA).await;
    add_label(&db, "by-discogs", "Label Name", None, Some("7190")).await;
    add_label(&db, "by-musicbrainz", "Label Name Records", Some("mb-label"), None).await;
    record_conflict(&db, true).await;

    ImportTables::new(db.clone())
        .choose_import_label_identity(
            CANDIDATE,
            0,
            crate::LabelIdentityConflictChoice::SameLabel,
            &coven::UuidIds,
        )
        .await
        .unwrap();

    assert!(!has_failure(&db).await);
    write_as_import(&db, vec![conflicting()]).await.unwrap();
    assert_eq!(ids_of(&placed(&db).await), [Some("by-discogs")]);
}

/// "Neither" makes a label carrying neither id, and the import names it;
/// "It's the Discogs label" names that label and attaches no MusicBrainz id.
#[tokio::test]
async fn the_neither_and_one_label_answers_decide_the_import() {
    let (db, _tmp) = library(crate::TEST_SCHEMA).await;
    add_label(&db, "by-discogs", "Label Name", None, Some("7190")).await;
    add_label(&db, "by-musicbrainz", "Label Name", Some("mb-label"), None).await;
    record_conflict(&db, true).await;

    ImportTables::new(db.clone())
        .choose_import_label_identity(
            CANDIDATE,
            0,
            crate::LabelIdentityConflictChoice::Neither,
            &coven::UuidIds,
        )
        .await
        .unwrap();
    write_as_import(&db, vec![conflicting()]).await.unwrap();
    let made = placed(&db).await[0].0.clone().unwrap();
    assert!(!["by-discogs", "by-musicbrainz"].contains(&made.as_str()));
    let made = stored_label(&db, &made).await;
    assert_eq!((made.musicbrainz_label_id, made.discogs_label_id), (None, None));

    record_conflict(&db, false).await;
    ImportTables::new(db.clone())
        .choose_import_label_identity(
            CANDIDATE,
            1,
            crate::LabelIdentityConflictChoice::DiscogsLabel,
            &coven::UuidIds,
        )
        .await
        .unwrap();
    write_as_import(&db, vec![conflicting()]).await.unwrap();
    assert_eq!(ids_of(&placed(&db).await), [Some("by-discogs")]);
    assert_eq!(stored_label(&db, "by-discogs").await.musicbrainz_label_id, None);
}

/// A failed import of `content_hash` that stopped on the conflict over
/// `by-discogs` and `by-musicbrainz`.
fn label_identity_failure() -> crate::ImportFailure {
    let existing =
        |label_id: &str, musicbrainz: Option<&str>, discogs: Option<&str>| bae_db::ExistingLabel {
            label_id: label_id.to_string(),
            name: "Label Name".to_string(),
            musicbrainz_label_id: musicbrainz.map(str::to_string),
            discogs_label_id: discogs.map(str::to_string),
        };
    crate::ImportFailure {
        reason: crate::ImportFailureReason::error("the label identities disagree"),
        failed_at: bae_db::test_fixtures::fixed_now(),
        identity_conflict: Some(crate::IdentityConflict::Label(bae_db::LabelIdentityConflict {
            incoming_label_name: "Label Name".to_string(),
            discogs_label_id: "7190".to_string(),
            musicbrainz_label_id: "mb-label".to_string(),
            discogs_label: existing("by-discogs", None, Some("7190")),
            musicbrainz_label: existing("by-musicbrainz", Some("mb-label"), None),
        })),
    }
}

/// Save `content_hash`'s draft as read from its files' tags, a source other
/// than the one it was read from.
async fn change_source(tables: &ImportTables, content_hash: &str) {
    let mut prep =
        tables.load_candidate_preparation(content_hash).await.unwrap().unwrap().stored().unwrap();
    let expected = crate::tables::CandidateSaveExpectation {
        edit_revision: prep.file_edits.revision,
        metadata_revision: prep.metadata_revision,
        scanned: None,
    };
    prep.author = crate::MetadataAuthor::Person;
    prep.metadata.provenance = Some(crate::MetadataProvenance::FileMetadata);
    prep.metadata_revision += 1;
    tables.save_candidate_preparation(prep, expected, Default::default()).await.unwrap();
}

/// An answer belongs to the source credits it was given for, as an artist's
/// does: a save that changes where the draft is read from discards the
/// candidate's label answers, and the failure a label conflict left with them.
#[tokio::test]
async fn changing_the_source_discards_a_label_answer_and_its_failure() {
    let (db, _tmp) = library(crate::TEST_SCHEMA).await;
    add_label(&db, "by-discogs", "Label Name", None, Some("7190")).await;
    add_label(&db, "by-musicbrainz", "Label Name", Some("mb-label"), None).await;
    let tables = ImportTables::new(db.clone());
    let (_, hash) = crate::tables::import_state::tests::stored_pane_candidate(&db).await;
    let answers = |db: Database, hash: String| async move {
        db.read(move |sql| {
            crate::tables::label_identity_choice::label_answers_on(&sql, &hash, &[conflicting()])
        })
        .await
        .unwrap()
        .len()
    };
    tables.save_import_candidate_failure(&hash, 0, &label_identity_failure()).await.unwrap();
    tables
        .choose_import_label_identity(
            &hash,
            0,
            crate::LabelIdentityConflictChoice::DiscogsLabel,
            &coven::UuidIds,
        )
        .await
        .unwrap();
    assert_eq!(answers(db.clone(), hash.clone()).await, 1);

    change_source(&tables, &hash).await;
    assert_eq!(answers(db.clone(), hash.clone()).await, 0, "the answer went with its source");

    tables.save_import_candidate_failure(&hash, 0, &label_identity_failure()).await.unwrap();
    change_source(&tables, &hash).await;
    assert!(
        tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.is_none(),
        "the failure went with its source"
    );
}
