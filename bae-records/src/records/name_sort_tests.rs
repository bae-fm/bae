//! The device's sort language: what a write stores, and what it refuses.

use super::*;
use crate::test_fixtures::{test_records, TEST_SCHEMA};

/// The language and collation data version `name_sort` holds.
async fn stored(db: &bae_store::Store) -> (String, u32) {
    db.read(|sql| {
        Ok(sql.query_row("SELECT language, data_version FROM name_sort", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn a_device_orders_in_the_root_order_until_a_language_is_set() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(TEST_SCHEMA).await;
    test_records(&db).settle_name_keys().await.unwrap();
    assert_eq!(stored(&db).await, ("und".to_string(), DATA_VERSION));

    test_records(&db).set_sort_language("sv").await.unwrap();
    assert_eq!(stored(&db).await, ("sv".to_string(), DATA_VERSION));
}

#[tokio::test]
async fn a_language_that_is_not_one_is_refused_and_changes_nothing() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(TEST_SCHEMA).await;
    assert!(matches!(
        test_records(&db).set_sort_language("not a language").await,
        Err(SortLanguageError::Invalid(_))
    ));
    assert_eq!(stored(&db).await, ("und".to_string(), 0));
}

#[tokio::test]
async fn keys_of_other_collation_data_are_recorded_as_this_builds_once_settled() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(TEST_SCHEMA).await;
    let records = test_records(&db);
    records.set_sort_language("ru").await.unwrap();
    records.age_name_keys_for_test().await.unwrap();

    records.settle_name_keys().await.unwrap();

    assert_eq!(stored(&db).await, ("ru".to_string(), DATA_VERSION), "the language stays");
}
