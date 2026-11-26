//! bae's members and each person's own data: who is in the library and on
//! which devices, the admin changes this device asked for, profiles, plays,
//! ratings and playlists, behind the `Membership` and `People` owners.

pub mod admin_change;
pub mod collections;
pub mod fragments;
pub mod history;
pub mod membership;
pub mod people;
pub mod synced;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;

pub use membership::Membership;
pub use people::People;

include!(concat!(env!("OUT_DIR"), "/schema.rs"));
