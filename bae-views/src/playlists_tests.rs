//! The playlist reads follow their indexes: no statement sorts after
//! reading.

use super::*;

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(store: &Store, statement: String) -> Vec<String> {
    store
        .read(move |sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {statement}"),
                rusqlite::params_from_iter(std::iter::repeat_n(
                    "id",
                    statement.matches('?').count(),
                )),
                |row| row.get::<_, String>(3),
            )
            .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn the_entry_reads_follow_their_index_without_a_sort() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    for statement in [entry_stats_sql(), playlist_entries_sql()] {
        let plan = plan(&db, statement.clone()).await;
        assert!(
            !plan
                .iter()
                .any(|step| step.starts_with("USE TEMP B-TREE FOR") && step.ends_with("ORDER BY")),
            "{statement} sorts after reading: {plan:?}"
        );
    }
}

fn cover(release: &str) -> Option<ImageRef> {
    cover_ref(release, Some("v".to_string()))
}

/// A playlist on four or more different releases shows the first four's
/// covers; on fewer, its first entry's; with no entries, none.
#[test]
fn a_playlists_picture_is_the_first_four_different_releases_or_its_first() {
    let stats = Stats::default();
    assert_eq!(stats.cover(), PlaylistCover::Single(None));

    let mut stats = Stats::default();
    for release in ["a", "a", "b", "a", "c"] {
        stats.add(release.to_string(), 1000, Some("v".to_string()));
    }
    assert_eq!(stats.tracks, 5);
    assert_eq!(stats.duration_ms, 5000);
    assert_eq!(stats.cover(), PlaylistCover::Single(cover("a")));

    let mut stats = Stats::default();
    for release in ["a", "a", "b", "a", "c", "b", "d", "e"] {
        stats.add(release.to_string(), 1, Some("v".to_string()));
    }
    assert_eq!(
        stats.cover(),
        PlaylistCover::Grid([cover("a"), cover("b"), cover("c"), cover("d")])
    );

    let mut stats = Stats::default();
    stats.add("a".to_string(), 1, None);
    stats.add("b".to_string(), 1, Some("v".to_string()));
    assert_eq!(stats.cover(), PlaylistCover::Single(None));
}
