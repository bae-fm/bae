//! Recording a play: refused once this device is no longer the person the
//! play began as.

use crate::history::{HistoryAudience, HistoryEntry, HistoryPlay, PlayRecordError};
use crate::People;
use std::time::Duration;

/// A play that began as a person this device no longer is, such as one
/// removed from the library during it, is refused as that, and nothing of
/// it is written.
#[tokio::test]
async fn a_play_begun_as_another_person_is_refused() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let people = People::new(db);
    let person = coven::MemberId(uuid::Uuid::from_u128(1));
    let play = HistoryPlay {
        entry: HistoryEntry {
            id: uuid::Uuid::from_u128(2),
            track_id: "track".to_string(),
            started_at: "2026-10-18T16:30:00Z".parse().unwrap(),
            played: Duration::from_secs(30),
            audience: HistoryAudience::Person { person, audience: coven::Audience::Store },
        },
        duration: Some(Duration::from_secs(40)),
    };

    let refused = people.record_play(&play).await;

    assert!(
        matches!(refused, Err(PlayRecordError::PersonChanged(refused)) if refused == person),
        "{refused:?}"
    );
    assert!(people.history().await.unwrap().is_empty());
}
