//! What two devices apart leave of an artist: one artist row a catalog id
//! names, written under different names, and artists a write on each gave
//! one catalog id, which the next write naming it merges.

use super::{credit, test_records};
use crate::{DbArtist, Records};
use bae_store::test_devices::TwoStores;

/// The library artist a credit naming MusicBrainz artist `musicbrainz` is,
/// on every device.
pub fn musicbrainz_artist(musicbrainz: &str) -> String {
    crate::identity::artist_id(Some(musicbrainz), None).expect("a catalog artist's id")
}

/// Meet MusicBrainz artist `musicbrainz` on a credit printing `name`, sorted
/// as `sort_name`, as an import does; on a device that doesn't hold the
/// artist yet, that writes its row under the id every device gives it.
pub async fn meet_musicbrainz_artist(
    records: &Records,
    musicbrainz: &str,
    name: &str,
    sort_name: Option<&str>,
) {
    let met = records
        .find_or_create_artists(&[DbArtist {
            sort_name: sort_name.map(str::to_string),
            musicbrainz_artist_id: Some(musicbrainz.to_string()),
            ..credit(name)
        }])
        .await
        .unwrap();
    assert_eq!(met, [musicbrainz_artist(musicbrainz)]);
}

/// Give the artist named `first` on device A and the one named `second` on
/// device B the Discogs id `discogs`, each by a credit printing its name
/// with that id, while the devices are apart; once both hold both, the next
/// write naming the id on A merges them into the smaller id. Returns once
/// both devices hold the merge.
pub async fn merge_apart(devices: &TwoStores, (first, second): (&str, &str), discogs: &str) {
    let crediting =
        |name: &str| DbArtist { discogs_artist_id: Some(discogs.to_string()), ..credit(name) };
    devices.pause().await;
    test_records(devices.a()).find_or_create_artists(&[crediting(first)]).await.unwrap();
    test_records(devices.b()).find_or_create_artists(&[crediting(second)]).await.unwrap();
    devices.resume().await;
    devices
        .settle("both artists holding the id", async |_, db| {
            let statement = crate::records::identity_merges::ArtistsHoldingDiscogsIdParams {
                discogs: discogs.to_string(),
            };
            db.read_row(statement).await.unwrap().holding == 2
        })
        .await;
    test_records(devices.a()).find_or_create_artists(&[crediting(first)]).await.unwrap();
    devices
        .settle("the merge", async |_, db| {
            db.read_row(crate::records::identity_merges::ArtistMergeCountParams {})
                .await
                .unwrap()
                .merges
                == 1
        })
        .await;
}
