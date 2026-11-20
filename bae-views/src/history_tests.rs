//! History's reads follow `history`'s indexes: no statement sorts after
//! reading, with or without a person.

use super::*;

/// The plan SQLite chooses for History's window statement under `filter`,
/// one detail line per step.
async fn window_plan(store: &Store, filter: HistoryFilter) -> Vec<String> {
    let order = history_order("h");
    let statement =
        format!("EXPLAIN QUERY PLAN {}", history_rows_query(&filter, &order, &order.everything()));
    store
        .read(move |sql| {
            let person = person_param(&filter);
            let mut params: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":limit", &50_i64)];
            if let Some(person) = &person {
                params.push((":person", person));
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
    let person = coven::MemberId(uuid::Uuid::from_u128(1));
    for (filter, index) in [(None, "history_order"), (Some(person), "history_person_order")] {
        let plan = window_plan(&db, filter).await;
        assert!(
            plan.iter()
                .any(|step| step.starts_with("SEARCH h USING") || step.starts_with("SCAN h USING"))
                && plan.iter().any(|step| step.contains(index)),
            "History's window under {filter:?} reads its plays along {index}: {plan:?}"
        );
        assert!(
            !plan.iter().any(|step| step == "USE TEMP B-TREE FOR ORDER BY"),
            "History's window under {filter:?} sorts after reading: {plan:?}"
        );
    }
}
