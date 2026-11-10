//! The picker's list: `cast_device` as each write lists it whole, read by
//! name after the receiver playback is cast to, and emptied as the library
//! opens.

use super::*;
use crate::tables::PlaybackNowRow;
use crate::{PlaybackChange, RemoteDevice, RemotePlayback};
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

fn device(id: &str, name: &str, kind: RendererKind) -> CastDevice {
    CastDevice { id: id.to_string(), name: name.to_string(), kind }
}

fn receiver(id: &str, name: &str, kind: RendererKind, casting: bool) -> CastReceiver {
    CastReceiver { id: id.to_string(), name: name.to_string(), kind, casting }
}

/// Playback moving to `device`, as playback writes it.
fn cast_to(device: &CastDevice) -> PlaybackChange {
    PlaybackChange::Remote(RemotePlayback::On(RemoteDevice {
        id: device.id.clone(),
        name: device.name.clone(),
        kind: device.kind,
    }))
}

/// A write replaces the whole list; the list reads by name, then id.
#[tokio::test]
async fn each_write_lists_the_whole_picker_by_name() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db);
    picker
        .show(vec![
            device("uuid:tv", "Television", RendererKind::Dlna),
            device("cast-1", "Kitchen", RendererKind::Cast),
            device("airplay-1", "Kitchen", RendererKind::AirPlay),
        ])
        .await
        .unwrap();
    assert_eq!(
        picker.receivers().await.unwrap(),
        vec![
            receiver("airplay-1", "Kitchen", RendererKind::AirPlay, false),
            receiver("cast-1", "Kitchen", RendererKind::Cast, false),
            receiver("uuid:tv", "Television", RendererKind::Dlna, false),
        ]
    );

    picker.show(vec![device("cast-1", "Kitchen", RendererKind::Cast)]).await.unwrap();
    assert_eq!(
        picker.receivers().await.unwrap(),
        vec![receiver("cast-1", "Kitchen", RendererKind::Cast, false)]
    );
}

/// The receiver playback is cast to comes first, marked, with the kind
/// playback names it by, whether or not the last search found it; a search
/// that finds it lists it once. Once playback leaves it, only what discovery
/// found is listed.
#[tokio::test]
async fn the_receiver_cast_to_comes_first_even_before_discovery_finds_it() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db.clone());
    let playback = PlaybackNowRow::new(db);
    let kitchen = device("airplay-1", "Kitchen", RendererKind::AirPlay);
    let den = device("uuid:den", "Den", RendererKind::Dlna);
    let living_room = device("cast-1", "Living Room", RendererKind::Cast);
    picker.show(vec![kitchen.clone(), den.clone()]).await.unwrap();

    playback.apply(&cast_to(&living_room)).await.unwrap();
    let casting_first = vec![
        receiver("cast-1", "Living Room", RendererKind::Cast, true),
        receiver("uuid:den", "Den", RendererKind::Dlna, false),
        receiver("airplay-1", "Kitchen", RendererKind::AirPlay, false),
    ];
    assert_eq!(picker.receivers().await.unwrap(), casting_first);

    picker.show(vec![kitchen.clone(), living_room, den.clone()]).await.unwrap();
    assert_eq!(picker.receivers().await.unwrap(), casting_first);

    picker.show(vec![kitchen, den]).await.unwrap();
    assert_eq!(picker.receivers().await.unwrap(), casting_first);

    playback.apply(&PlaybackChange::Remote(RemotePlayback::Local)).await.unwrap();
    assert_eq!(
        picker.receivers().await.unwrap(),
        vec![
            receiver("uuid:den", "Den", RendererKind::Dlna, false),
            receiver("airplay-1", "Kitchen", RendererKind::AirPlay, false),
        ]
    );
}

/// The followed list's next value.
async fn next(listed: &mut coven::LiveQuery<Vec<CastReceiver>, DbError>) -> Vec<CastReceiver> {
    tokio::time::timeout(HANG_GUARD, listed.next())
        .await
        .expect("the list answers")
        .expect("the list stays open")
        .unwrap()
}

/// The followed list changes as playback moves to a receiver, not only as
/// discovery writes.
#[tokio::test]
async fn the_followed_list_changes_as_playback_moves_to_a_receiver() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db.clone());
    let living_room = device("cast-1", "Living Room", RendererKind::Cast);
    picker.show(vec![living_room.clone()]).await.unwrap();
    let mut listed = picker.subscribe();
    assert_eq!(
        next(&mut listed).await,
        vec![receiver("cast-1", "Living Room", RendererKind::Cast, false)]
    );

    PlaybackNowRow::new(db).apply(&cast_to(&living_room)).await.unwrap();

    assert_eq!(
        next(&mut listed).await,
        vec![receiver("cast-1", "Living Room", RendererKind::Cast, true)]
    );
}

/// Opening the library empties the list: no discovery outlives the process
/// that ran it.
#[tokio::test]
async fn opening_empties_the_picker() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db);
    picker.show(vec![device("cast-1", "Kitchen", RendererKind::Cast)]).await.unwrap();

    picker.clear().await.unwrap();

    assert_eq!(picker.receivers().await.unwrap(), Vec::new());
}
