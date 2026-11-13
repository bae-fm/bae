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

/// A role or work credit printing one of the artist's other names links it as
/// an album credit does: the name is among the artist's other names, and
/// removing it unlinks the credit in the same write.
#[tokio::test]
async fn role_and_work_credits_link_the_names_they_print() {
    use crate::test_fixtures::{
        artist_album_release, ARTIST_COMPOSER, RELEASE_A, TRACK_A, TRACK_ROLE_A, WORK_A,
    };
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let printed = CreditName::of("Printed Name");
    artist_album_release()
        .artist(ARTIST_COMPOSER, "Composer Name")
        .tracks(RELEASE_A, &[(TRACK_A, "Track Title")])
        .work(WORK_A, "Work Title")
        .composer(WORK_A, ARTIST_COMPOSER)
        .release_role(
            "9b72bbbf-621e-41ca-8930-1623b643a20d",
            RELEASE_A,
            ARTIST_COMPOSER,
            (bae_catalogs::catalog::Catalog::Discogs, "Composed By"),
        )
        .track_role(
            TRACK_ROLE_A,
            TRACK_A,
            ARTIST_COMPOSER,
            (bae_catalogs::catalog::Catalog::Discogs, "Written-By"),
        )
        .edit(|rows| {
            rows.work_artists[0].credit = Some(printed.clone());
            rows.release_artist_roles[0].credit = Some(printed.clone());
            rows.track_artist_roles[0].credit = Some(printed.clone());
        })
        .write(&records)
        .await;
    let linked = || async {
        records
            .store
            .read(|sql| {
                ["work_artists", "release_artist_roles", "track_artist_roles"]
                    .into_iter()
                    .map(|table| {
                        sql.query_row(&format!("SELECT credit_key FROM {table}"), [], |row| {
                            row.get::<_, Option<String>>(0)
                        })
                        .map_err(DbError::from)
                    })
                    .collect::<Result<Vec<_>, DbError>>()
            })
            .await
            .unwrap()
    };

    let held = records
        .store
        .read(|sql| credit_names_of_on(&sql, &[ARTIST_COMPOSER.to_string()], "credit_name.name"))
        .await
        .unwrap();
    assert_eq!(
        held[ARTIST_COMPOSER].iter().map(|name| name.credit.name.as_str()).collect::<Vec<_>>(),
        ["Printed Name"]
    );
    assert_eq!(linked().await, vec![Some(printed.key.clone()); 3]);

    records.remove_artist_credit_name(ARTIST_COMPOSER, &printed.key).await.unwrap();

    assert_eq!(linked().await, vec![None, None, None]);
}
