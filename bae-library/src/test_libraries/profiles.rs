//! Profiles made as the app makes them: asked for, then written once sync
//! has made the person's private circle (§5.4).

use crate::followed::value_matching;
use crate::LibraryManager;
use bae_people::people::PersonProfile;

/// Ask for this device's person's profile named `name`, and sync until the
/// manager has written it.
pub async fn made_profile(
    manager: &LibraryManager,
    people: &bae_people::People,
    name: &str,
) -> PersonProfile {
    people.create_profile(name).await.unwrap();
    requested_profile(manager, people).await
}

/// The profile this device's person asked for, written by the manager once
/// sync has made their private circle. Sync starts if it was stopped, and
/// runs on once the profile is written.
pub async fn requested_profile(
    manager: &LibraryManager,
    people: &bae_people::People,
) -> PersonProfile {
    let member = people
        .own_person()
        .await
        .unwrap()
        .expect("asking for a profile needs a registered person")
        .membership
        .id;
    let mut profiles = manager.subscribe_profiles();
    manager.attach_and_start_sync().await.unwrap();
    let profile = own(
        value_matching(&mut profiles, |profiles| {
            profiles.iter().any(|profile| profile.id == member)
        })
        .await,
        member,
    );
    profile
}

/// [`made_profile`] for a store opened without a library manager, as
/// [`requested_profile_on`] writes it.
pub async fn made_profile_on(
    handle: &coven::CovenHandle,
    people: &bae_people::People,
    name: &str,
) -> PersonProfile {
    people.create_profile(name).await.unwrap();
    requested_profile_on(handle, people).await
}

/// The profile this device's person asked for on `people` over `handle`,
/// written once sync has made their private circle: sync starts, if it
/// isn't running, and keeps running, and `people` follows the request
/// meanwhile, as an open library's manager does.
pub async fn requested_profile_on(
    handle: &coven::CovenHandle,
    people: &bae_people::People,
) -> PersonProfile {
    let member = people
        .own_person()
        .await
        .unwrap()
        .expect("asking for a profile needs a registered person")
        .membership
        .id;
    let mut profiles = people.subscribe_profiles();
    handle.start_sync().await.unwrap();
    let written = value_matching(&mut profiles, |profiles| {
        profiles.iter().any(|profile| profile.id == member)
    });
    let profiles = tokio::select! {
        ended = people.finish_profile_requests() => panic!("following the request ended: {ended:?}"),
        profiles = written => profiles,
    };
    own(profiles, member)
}

fn own(profiles: Vec<PersonProfile>, member: coven::MemberId) -> PersonProfile {
    profiles
        .into_iter()
        .find(|profile| profile.id == member)
        .expect("the matched profiles hold the person's")
}
