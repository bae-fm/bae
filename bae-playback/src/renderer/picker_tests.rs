//! The picker's list: `cast_receiver` as each write lists it whole, read by
//! name apart from the receiver playback is cast to, and emptied as the
//! library opens.

use super::*;
use crate::tables::PlaybackNowRow;
use crate::{PlaybackChange, RemotePlayback};
use bae_store::test_fixtures::temp_db;
use bae_util::test_channels::HANG_GUARD;

use crate::renderer::RendererKind;

fn receiver(id: &str, name: &str, kind: RendererKind) -> CastReceiver {
    CastReceiver { id: id.to_string(), name: name.to_string(), kind }
}

/// Playback moving to `receiver`, as playback writes it.
fn cast_to(receiver: &CastReceiver) -> PlaybackChange {
    PlaybackChange::Remote(RemotePlayback::On(receiver.clone()))
}

/// A list with nothing cast to.
fn found(found: Vec<CastReceiver>) -> CastReceiverList {
    CastReceiverList { casting: None, found }
}

/// A write replaces the whole list; the list reads by name, then id.
#[tokio::test]
async fn each_write_lists_the_whole_picker_by_name() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db);
    picker
        .show(vec![
            receiver("uuid:tv", "Television", RendererKind::Dlna),
            receiver("cast-1", "Kitchen", RendererKind::Cast),
            receiver("airplay-1", "Kitchen", RendererKind::AirPlay),
        ])
        .await
        .unwrap();
    assert_eq!(
        picker.receivers().await.unwrap(),
        found(vec![
            receiver("airplay-1", "Kitchen", RendererKind::AirPlay),
            receiver("cast-1", "Kitchen", RendererKind::Cast),
            receiver("uuid:tv", "Television", RendererKind::Dlna),
        ])
    );

    picker.show(vec![receiver("cast-1", "Kitchen", RendererKind::Cast)]).await.unwrap();
    assert_eq!(
        picker.receivers().await.unwrap(),
        found(vec![receiver("cast-1", "Kitchen", RendererKind::Cast)])
    );
}

/// The receiver playback is cast to is listed apart, with the kind playback
/// names it by, whether or not the last search found it; a search
/// that finds it lists it once. Once playback leaves it, only what discovery
/// found is listed.
#[tokio::test]
async fn the_receiver_cast_to_comes_first_even_before_discovery_finds_it() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db.clone());
    let playback = PlaybackNowRow::new(db);
    let kitchen = receiver("airplay-1", "Kitchen", RendererKind::AirPlay);
    let den = receiver("uuid:den", "Den", RendererKind::Dlna);
    let living_room = receiver("cast-1", "Living Room", RendererKind::Cast);
    picker.show(vec![kitchen.clone(), den.clone()]).await.unwrap();

    playback.apply(&cast_to(&living_room)).await.unwrap();
    let casting = CastReceiverList {
        casting: Some(living_room.clone()),
        found: vec![den.clone(), kitchen.clone()],
    };
    assert_eq!(picker.receivers().await.unwrap(), casting);

    picker.show(vec![kitchen.clone(), living_room, den.clone()]).await.unwrap();
    assert_eq!(picker.receivers().await.unwrap(), casting);

    picker.show(vec![kitchen.clone(), den.clone()]).await.unwrap();
    assert_eq!(picker.receivers().await.unwrap(), casting);

    playback.apply(&PlaybackChange::Remote(RemotePlayback::Local)).await.unwrap();
    assert_eq!(picker.receivers().await.unwrap(), found(vec![den, kitchen]));
}

/// The followed list's next value.
async fn next(listed: &mut coven::LiveQuery<CastReceiverList, DbError>) -> CastReceiverList {
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
    let living_room = receiver("cast-1", "Living Room", RendererKind::Cast);
    picker.show(vec![living_room.clone()]).await.unwrap();
    let mut listed = picker.subscribe();
    assert_eq!(next(&mut listed).await, found(vec![living_room.clone()]));

    PlaybackNowRow::new(db).apply(&cast_to(&living_room)).await.unwrap();

    assert_eq!(
        next(&mut listed).await,
        CastReceiverList { casting: Some(living_room), found: Vec::new() }
    );
}

/// Opening the library empties the list: no discovery outlives the process
/// that ran it.
#[tokio::test]
async fn opening_empties_the_picker() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db);
    picker.show(vec![receiver("cast-1", "Kitchen", RendererKind::Cast)]).await.unwrap();

    picker.clear().await.unwrap();

    assert_eq!(picker.receivers().await.unwrap(), found(Vec::new()));
}
