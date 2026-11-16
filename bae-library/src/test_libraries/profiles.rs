//! Profiles made as the app makes them: asked for, then written once sync
//! has made the person's private circle (§5.4).

use crate::followed::query_matching;
use crate::LibraryManager;
use bae_people::people::{PeopleView, PersonProfile};

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
    let mut people = manager.subscribe_people();
    manager.attach_and_start_sync().await.unwrap();
    let view = query_matching(&mut people, |view| own(view, member).is_some()).await;
    own(&view, member).expect("the matched view holds the person's profile")
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
    let mut view = people.subscribe_people();
    handle.start_sync().await.unwrap();
    let written = query_matching(&mut view, |view| own(view, member).is_some());
    let view = tokio::select! {
        ended = people.finish_profile_requests() => panic!("following the request ended: {ended:?}"),
        view = written => view,
    };
    own(&view, member).expect("the matched view holds the person's profile")
}

/// The profile of this device's person in `view`, when that person is
/// `member` and has made one.
fn own(view: &PeopleView, member: coven::MemberId) -> Option<PersonProfile> {
    view.own.as_ref().filter(|own| own.membership.id == member).and_then(|own| own.profile.clone())
}
