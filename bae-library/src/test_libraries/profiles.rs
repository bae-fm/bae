//! The profile a device's person has from their device's registration: the
//! one the library's creation gave its creator, or a join a joiner (§5.4).

use bae_people::people::PersonProfile;

/// The profile of this device's person, which the joining write gave them as
/// this device registered.
pub async fn own_profile(people: &bae_people::People) -> PersonProfile {
    people
        .own_person()
        .await
        .unwrap()
        .expect("this device's registration has applied")
        .profile
        .expect("the joining write gave the person their profile as this device registered")
}
