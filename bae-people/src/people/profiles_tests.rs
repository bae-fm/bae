//! Writing the profile a request asked for, once its circle landed.

use super::*;

/// The write that ends a request whose circle landed finds the request it
/// was handed no longer stored: it writes nothing and stops as a changed
/// request, which only a bug of bae's makes.
#[tokio::test]
async fn a_request_no_longer_stored_is_not_written_as_a_profile() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let people = People::new(db);
    let operation = coven::OperationId(7);
    let request = ProfileRequest {
        operation,
        circle: coven::CircleId(uuid::Uuid::from_u128(1)),
        name: "Robin".to_string(),
        outcome: None,
    };

    let refused = people.finish_profile(request).await;

    assert!(
        matches!(refused, Err(ProfileRequestError::Changed(refused)) if refused == operation),
        "{refused:?}"
    );
    assert!(people.profiles().await.unwrap().is_empty());
}
