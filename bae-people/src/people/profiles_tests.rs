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
    assert!(first(people.subscribe_profiles()).await.is_empty());
}

/// The profiles list sorts the profiles by name in the device's language, and
/// sorts them again when the language changes: the root order puts ö with o,
/// Swedish puts it after z, and the byte order puts it after every ASCII
/// letter.
#[tokio::test]
async fn profiles_list_by_name_in_the_devices_language() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    db.call(|sql| {
        for (id, name) in [
            ("00000000-0000-0000-0000-000000000001", "Östen"),
            ("00000000-0000-0000-0000-000000000002", "Zoe"),
            ("00000000-0000-0000-0000-000000000003", "Olle"),
        ] {
            sql.execute(
                "INSERT INTO people (id, name, private_circle, history_sharing) \
                 VALUES (?1, ?2, '0c1c0000-0000-4000-8000-000000000001', 'off')",
                [id, name],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    let people = People::new(db.clone());
    let names = |profiles: Vec<PersonProfile>| -> Vec<String> {
        profiles.into_iter().map(|profile| profile.name).collect()
    };
    let mut profiles = people.subscribe_profiles();
    assert_eq!(names(next(&mut profiles).await), ["Olle", "Östen", "Zoe"]);

    bae_records::test_fixtures::test_records(&db).set_sort_language("sv").await.unwrap();

    assert_eq!(names(next(&mut profiles).await), ["Olle", "Zoe", "Östen"]);
}

/// The profiles a new subscription lists first: what is stored now.
async fn first(mut profiles: coven::LiveQuery<Vec<PersonProfile>, DbError>) -> Vec<PersonProfile> {
    next(&mut profiles).await
}

/// The followed profiles' next list.
async fn next(profiles: &mut coven::LiveQuery<Vec<PersonProfile>, DbError>) -> Vec<PersonProfile> {
    tokio::time::timeout(std::time::Duration::from_secs(30), profiles.next())
        .await
        .expect("the list answers")
        .expect("the list stays open")
        .unwrap()
}
