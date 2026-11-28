//! History's reads follow `history`'s indexes: no window sorts after reading,
//! from the top or from a key, with or without a person, and each finds a day's newest play by its day.

use super::*;

/// The plan SQLite chooses for `statement`, whose parameters stay unbound,
/// one detail line per step.
async fn plan(store: &Store, statement: &'static str) -> Vec<String> {
    let statement = format!("EXPLAIN QUERY PLAN {statement}");
    store
        .read(move |sql| {
            let unbound: &[(&str, &dyn rusqlite::ToSql)] = &[];
            sql.query(&statement, unbound, |row| row.get::<_, String>(3))
                .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn the_window_reads_follow_their_indexes_without_a_sort() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    for (statement, order_index, day_index) in [
        (HistoryWindow::SQL, "history_order", "history_day"),
        (HistoryTop::SQL, "history_order", "history_day"),
        (PersonHistoryWindow::SQL, "history_person_order", "history_person_day"),
        (PersonHistoryTop::SQL, "history_person_order", "history_person_day"),
    ] {
        let plan = plan(&db, statement).await;
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
