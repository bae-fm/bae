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
    assert!(first(people.subscribe_profiles(root)).await.is_empty());
}

/// Profiles list by name in the language asked: Swedish puts ö after z, the
/// root order puts it with o, and the byte order puts it after every ASCII
/// letter.
#[tokio::test]
async fn profiles_list_by_name_in_the_language_asked() {
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
    let people = People::new(db);
    let names = |profiles: Vec<PersonProfile>| -> Vec<String> {
        profiles.into_iter().map(|profile| profile.name).collect()
    };

    assert_eq!(names(first(people.subscribe_profiles(swedish)).await), ["Olle", "Zoe", "Östen"]);
    assert_eq!(names(first(people.subscribe_profiles(root)).await), ["Olle", "Östen", "Zoe"]);
}

fn root(_: &SqlReadContext<'_>) -> Result<SortLanguage, DbError> {
    Ok(SortLanguage::root())
}

fn swedish(_: &SqlReadContext<'_>) -> Result<SortLanguage, DbError> {
    Ok(SortLanguage::parse("sv").unwrap())
}

/// The profiles a new subscription lists first: what is stored now.
async fn first(mut profiles: coven::LiveQuery<Vec<PersonProfile>, DbError>) -> Vec<PersonProfile> {
    tokio::time::timeout(std::time::Duration::from_secs(30), profiles.next())
        .await
        .expect("the list answers")
        .expect("the list stays open")
        .unwrap()
}
