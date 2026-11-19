use super::*;

/// Join `household` as a guest named `name` from a new device, and return
/// the guest's library, which syncs, with the profile the join wrote.
async fn guest_named(
    household: &Household,
    key: &str,
    name: &str,
) -> (
    bae_util::test_files::TestFolder,
    bae_util::test_files::TestFolder,
    bae_library::test_libraries::TestLibrary,
) {
    let invite = household.invite(key).await;
    let installation_dir = bae_util::test_files::TestFolder::new();
    let installation = installation_in(&installation_dir);
    let control = bae_library::JoinControl::new();
    let joining = bae_library::join_with_invite(
        household.device(&installation),
        &invite.code,
        bae_store::EnteredName::parse(name).unwrap(),
        "Guest Device",
        household.sources.clock().clone(),
        |_| {},
        &control,
        &NO_DIAGNOSTICS,
    );
    let (joined, ()) = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        tokio::join!(joining, household.approve())
    })
    .await
    .unwrap();
    let guest = loaded(joined.unwrap().unwrap());
    let library_dir = bae_util::test_files::TestFolder::new();
    let (library, profile) = joined_profile(guest, household, library_dir.path()).await;
    assert_eq!(profile.name, name);
    (installation_dir, library_dir, library)
}

/// The people list lists the other people by their profiles' names in the
/// device's language, and lists them again when the language changes: the
/// root order puts Östen before Zoe, and Swedish puts ö after z.
#[test]
fn the_people_list_others_by_name_in_the_devices_language() {
    on_app_runtime(async {
        let household = Household::new().await;
        let zoe = guest_named(&household, "zoe-key", "Zoe").await;
        let osten = guest_named(&household, "osten-key", "Östen").await;
        let owner_dir = bae_util::test_files::TestFolder::new();
        let owner = household.owners_library(owner_dir.path()).await;
        owner.manager().attach_and_start_sync().await.unwrap();
        let names = |people: &bae_people::people::PeopleView| -> Vec<String> {
            people
                .others
                .iter()
                .filter_map(|person| person.profile.as_ref().map(|profile| profile.name.clone()))
                .collect()
        };
        let mut people = owner.manager().subscribe_people();

        let listed = query_matching(&mut people, |people| names(people).len() == 2).await;
        assert_eq!(names(&listed), ["Östen", "Zoe"]);

        owner.records().set_sort_language("sv").await.unwrap();

        query_matching(&mut people, |people| names(people) == ["Zoe", "Östen"]).await;
        for (_, _, library) in [zoe, osten] {
            library.manager().close().await.unwrap();
        }
        owner.manager().close().await.unwrap();
    });
}
