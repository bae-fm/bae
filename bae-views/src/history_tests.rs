//! History's reads follow `history`'s indexes: no window sorts after reading,
//! from the top or from a key, with or without a person, and each finds a day's newest play by its day.

use super::*;
use bae_store::test_fixtures::query_plan::{plan_steps, unbound_plan, PlanOf};

#[tokio::test]
async fn the_window_reads_follow_their_indexes_without_a_sort() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let windows: &[(PlanOf, &str, &str)] = &[
        (plan_steps::<HistoryWindowParams>, "history_order", "history_day"),
        (plan_steps::<HistoryTopParams>, "history_order", "history_day"),
        (plan_steps::<PersonHistoryWindowParams>, "history_person_order", "history_person_day"),
        (plan_steps::<PersonHistoryTopParams>, "history_person_order", "history_person_day"),
    ];
    for &(plan_of, order_index, day_index) in windows {
        let plan = unbound_plan(&db, plan_of).await;
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
