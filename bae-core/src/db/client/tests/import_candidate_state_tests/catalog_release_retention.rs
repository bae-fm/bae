//! A catalog release is stored only while a row names it: stored in the write
//! that writes the first such row, and deleted in the write that drops the
//! last.

use crate::catalog_model::records::LookupRef;
use super::super::{exec, exec_batch};
use super::*;
use crate::identify::{IdentifyRunId, RunOutcome};
use crate::catalog_model::source_release::{SourceRelease, UnfetchedDocument, UnfetchedReason};

/// Candidate `a` with a draft, and MusicBrainz releases `rel-1` and `rel-2`
/// stored with a barcode each, which nothing names yet.
const SETUP: &str = "
    INSERT INTO import_candidate_state (content_hash, candidate_key)
    VALUES ('a', CAST('/music/a' AS BLOB));
    INSERT INTO import_candidate_edit
        (content_hash, album_title, album_year, year, author, draft_blank, draft_valid)
    VALUES ('a', 'Album Title', '', '', 'person', 0, 1);
    INSERT INTO source_release (catalog, release_id, album_title, album_links)
    VALUES ('musicbrainz', 'rel-1', 'Album Title', 'read'),
           ('musicbrainz', 'rel-2', 'Album Title', 'read');
    INSERT INTO source_release_barcode (catalog, release_id, position, barcode)
    VALUES ('musicbrainz', 'rel-1', 0, '0123456789012'),
           ('musicbrainz', 'rel-2', 0, '0123456789029');";

/// A verdict of candidate `owner` whose one match names `release`.
fn match_naming(owner: &str, release: &str) -> String {
    format!(
        "INSERT INTO import_candidate_verdict (content_hash, kind, kept_own_draft)
         VALUES ('{owner}', 'not_found', 0);
         INSERT INTO import_candidate_match
             (content_hash, position, listing, source, release_id, title, media_kind,
              album_links)
         VALUES ('{owner}', 0, 'unlisted', 'musicbrainz', '{release}', 'Album Title',
                 'undescribed', 'not_asked');"
    )
}

/// Whether MusicBrainz release `release` is stored, and how many rows hang
/// off it.
async fn stored(db: &Database, release: &str) -> (bool, i64) {
    let release = release.to_string();
    db.call(move |sql| {
        Ok(sql.query_row(
            "SELECT EXISTS (SELECT 1 FROM source_release \
                            WHERE catalog = 'musicbrainz' AND release_id = ?1), \
                    (SELECT COUNT(*) FROM source_release_barcode \
                     WHERE catalog = 'musicbrainz' AND release_id = ?1)",
            [&release],
            |row| Ok((row.get::<_, bool>(0)?, row.get::<_, i64>(1)?)),
        )?)
    })
    .await
    .unwrap()
}

/// Each kind of row that names a release keeps it, and deleting that row, or
/// changing it to name another release or none, deletes the release and the
/// rows under it in the same write. A candidate's whole state going — as an
/// import or a removed folder takes it — takes every release only it named.
#[tokio::test]
async fn the_last_row_naming_a_catalog_release_takes_it_with_it() {
    let cases: [(&str, String, &str); 9] = [
        (
            "a match whose verdict goes",
            match_naming("a", "rel-1"),
            "DELETE FROM import_candidate_verdict WHERE content_hash = 'a'",
        ),
        (
            "a match that names another release",
            match_naming("a", "rel-1"),
            "UPDATE import_candidate_match SET release_id = 'rel-2'",
        ),
        (
            "a draft's provenance that goes",
            "INSERT INTO import_candidate_draft_provenance (content_hash, kind, source, release_id)
             VALUES ('a', 'external_release', 'musicbrainz', 'rel-1');"
                .to_string(),
            "DELETE FROM import_candidate_draft_provenance",
        ),
        (
            "a draft's provenance that turns to the files' tags",
            "INSERT INTO import_candidate_draft_provenance (content_hash, kind, source, release_id)
             VALUES ('a', 'external_release', 'musicbrainz', 'rel-1');"
                .to_string(),
            "UPDATE import_candidate_draft_provenance
             SET kind = 'file_tags', source = NULL, release_id = NULL",
        ),
        (
            "a link that goes",
            "INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id)
             VALUES ('a', 'pressing', 'musicbrainz', 'rel-1');"
                .to_string(),
            "DELETE FROM import_candidate_release_link",
        ),
        (
            "a link that names another release",
            "INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id)
             VALUES ('a', 'pressing', 'musicbrainz', 'rel-1');"
                .to_string(),
            "UPDATE import_candidate_release_link SET release_id = 'rel-2'",
        ),
        (
            "a partner that goes",
            "INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id)
             VALUES ('a', 'pressing', 'musicbrainz', 'rel-2');
             INSERT INTO import_candidate_release_link_partner
                 (content_hash, kind, source, release_id)
             VALUES ('a', 'pressing', 'musicbrainz', 'rel-1');"
                .to_string(),
            "DELETE FROM import_candidate_release_link_partner",
        ),
        (
            "a partner whose link goes",
            "INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id)
             VALUES ('a', 'pressing', 'musicbrainz', 'rel-2');
             INSERT INTO import_candidate_release_link_partner
                 (content_hash, kind, source, release_id)
             VALUES ('a', 'pressing', 'musicbrainz', 'rel-1');"
                .to_string(),
            "DELETE FROM import_candidate_release_link",
        ),
        (
            "every row of a candidate whose state goes",
            format!(
                "{}
                 INSERT INTO import_candidate_draft_provenance
                     (content_hash, kind, source, release_id)
                 VALUES ('a', 'external_release', 'musicbrainz', 'rel-1');
                 INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id)
                 VALUES ('a', 'pressing', 'musicbrainz', 'rel-1');",
                match_naming("a", "rel-1")
            ),
            "DELETE FROM import_candidate_state WHERE content_hash = 'a'",
        ),
    ];
    for (case, naming, dropping) in cases {
        let (db, _tmp) = empty_db().await;
        exec_batch(&db, SETUP).await;
        exec_batch(&db, &naming).await;
        assert_eq!(stored(&db, "rel-1").await, (true, 1), "{case}: named, so kept");
        exec(&db, dropping, &[]).await;
        assert_eq!(stored(&db, "rel-1").await, (false, 0), "{case}: no row names it, so it goes");
    }
}

/// A release two candidates name stays while either names it, and goes with
/// the second.
#[tokio::test]
async fn a_release_two_candidates_name_stays_until_both_drop_it() {
    let (db, _tmp) = empty_db().await;
    exec_batch(&db, SETUP).await;
    exec_batch(
        &db,
        &format!(
            "INSERT INTO import_candidate_state (content_hash, candidate_key)
             VALUES ('b', CAST('/music/b' AS BLOB));
             INSERT INTO import_candidate_release_link (content_hash, kind, source, release_id)
             VALUES ('a', 'pressing', 'musicbrainz', 'rel-1');
             {}",
            match_naming("b", "rel-1")
        ),
    )
    .await;
    exec(&db, "DELETE FROM import_candidate_release_link WHERE content_hash = 'a'", &[]).await;
    assert_eq!(stored(&db, "rel-1").await, (true, 1), "b's match still names it");
    exec(&db, "DELETE FROM import_candidate_verdict WHERE content_hash = 'b'", &[]).await;
    assert_eq!(stored(&db, "rel-1").await, (false, 0), "nothing names it");
}

/// `release` as a fetch hands it over, unstored.
fn release(key: &str) -> SourceRelease {
    SourceRelease::for_test(LookupRef::new(LookupCatalog::MusicBrainz, key))
}

/// A verdict's write stores the releases its rows name from the ones it
/// carries, and none of the others; the verdict that replaces it, naming
/// none, takes them.
#[tokio::test]
async fn a_verdict_stores_only_the_releases_its_rows_name() {
    let (db, _tmp) = empty_db().await;
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    let mut row = new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict());
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    let preparations = crate::import::CandidatePreparations::new(db.clone());

    row.releases = vec![release("rel-1"), release("rel-unnamed")];
    assert!(preparations.store_verdict(&row).await.unwrap());
    let named = LookupRef::new(LookupCatalog::MusicBrainz, "rel-1");
    let unnamed = LookupRef::new(LookupCatalog::MusicBrainz, "rel-unnamed");
    assert_eq!(
        db.load_source_release(&named).await.unwrap(),
        Some(release("rel-1")),
        "the verdict's match names it"
    );
    assert_eq!(db.load_source_release(&unnamed).await.unwrap(), None);

    row.verdict = TerminalVerdict::NotFoundAnywhere { ledger: None };
    row.releases = Vec::new();
    assert!(preparations.store_verdict(&row).await.unwrap());
    assert_eq!(db.load_source_release(&named).await.unwrap(), None);
}

/// What a run in flight publishes keeps the releases it read that its rows
/// name, and none it read that they don't; the run's end takes them.
#[tokio::test]
async fn a_run_s_rows_keep_the_releases_it_read_until_the_run_ends() {
    let (db, _tmp) = empty_db().await;
    let key = crate::import::CandidateKey::test("release-reidentify");
    let run = IdentifyRunId::for_test(1);
    db.publish_release_run(
        &key,
        run,
        &RunOutcome::Answered(sample_verdict()),
        vec![release("rel-1"), release("rel-unnamed")],
        None,
    )
    .await
    .unwrap();
    let named = LookupRef::new(LookupCatalog::MusicBrainz, "rel-1");
    assert!(db.load_source_release(&named).await.unwrap().is_some());
    assert_eq!(
        db.load_source_release(&LookupRef::new(LookupCatalog::MusicBrainz, "rel-unnamed"))
            .await
            .unwrap(),
        None,
        "no row of the run names it"
    );

    db.end_release_run(&key, run).await.unwrap();
    assert_eq!(db.load_source_release(&named).await.unwrap(), None);
}

/// A write stores its copy of a release over the stored one only where its
/// copy read a document the stored one did not: a copy read earlier never
/// overwrites a fuller one stored since.
#[tokio::test]
async fn a_stored_release_is_replaced_only_by_a_copy_that_read_more() {
    let (db, _tmp) = empty_db().await;
    let key = crate::import::CandidateKey::test("release-reidentify");
    let run = IdentifyRunId::for_test(1);
    let mut partial = release("rel-1");
    partial.unfetched = vec![UnfetchedDocument {
        document: crate::catalog_model::records::PayloadSource::MusicBrainz,
        key: "rel-1".to_string(),
        reason: UnfetchedReason::StoppedShort,
    }];
    let complete = release("rel-1");
    let publish = |copy: SourceRelease| {
        let db = db.clone();
        let key = key.clone();
        async move {
            db.publish_release_run(
                &key,
                run,
                &RunOutcome::Answered(sample_verdict()),
                vec![copy],
                None,
            )
            .await
            .unwrap();
            db.load_source_release(&LookupRef::new(LookupCatalog::MusicBrainz, "rel-1"))
                .await
                .unwrap()
                .expect("the run's match names it")
        }
    };
    assert_eq!(publish(partial.clone()).await, partial);
    assert_eq!(publish(complete.clone()).await, complete, "it read more");
    assert_eq!(
        publish(partial).await,
        complete,
        "the copy read earlier stops short of the stored one"
    );
}
