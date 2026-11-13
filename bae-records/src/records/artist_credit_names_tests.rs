//! An artist's other printed names: refused for an artist the library no
//! longer holds.

use super::*;

/// Adding a printed name to an artist the library no longer holds, as one
/// deleted or merged after the person opened it, is refused as that, and
/// nothing is written.
#[tokio::test]
async fn a_name_for_an_artist_no_longer_held_is_refused() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let name = bae_store::EnteredName::parse("Printed Name").unwrap();

    let refused = records.add_artist_credit_name("gone", &name).await;

    assert!(
        matches!(&refused, Err(CreditNameError::ArtistMissing(artist)) if artist == "gone"),
        "{refused:?}"
    );
    let held = records
        .store
        .read(|sql| credit_names_of_on(&sql, &["gone".to_string()], "credit_name.name"))
        .await
        .unwrap();
    assert!(held.is_empty());
}
