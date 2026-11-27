//! History's reads follow `history`'s indexes: no window sorts after reading,
//! with or without a person, and each finds a day's newest play by its day.

use super::*;

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(store: &Store, statement: &'static str, person: bool) -> Vec<String> {
    let statement = format!("EXPLAIN QUERY PLAN {statement}");
    store
        .read(move |sql| {
            let mut params: Vec<(&str, &dyn rusqlite::ToSql)> =
                vec![(":from_at", &AFTER_EVERY_PLAY), (":from_id", &""), (":limit", &50_i64)];
            let id = "00000000-0000-0000-0000-000000000001";
            if person {
                params.push((":person", &id));
            }
            sql.query(&statement, params.as_slice(), |row| row.get::<_, String>(3))
                .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn the_window_reads_follow_their_indexes_without_a_sort() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    for (statement, person, order_index, day_index) in [
        (HistoryWindow::SQL, false, "history_order", "history_day"),
        (PersonHistoryWindow::SQL, true, "history_person_order", "history_person_day"),
    ] {
        let plan = plan(&db, statement, person).await;
        for index in [order_index, day_index] {
            assert!(
                plan.iter().any(|step| step.split(' ').any(|word| word == index)),
                "the window reads along {index}: {plan:?}"
            );
        }
        assert!(
            !plan.iter().any(|step| step == "USE TEMP B-TREE FOR ORDER BY"),
            "the window sorts after reading: {plan:?}"
        );
    }
}
