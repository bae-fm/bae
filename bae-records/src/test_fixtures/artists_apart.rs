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
            count(db, "SELECT COUNT(*) FROM artists WHERE discogs_artist_id = ?1", &[discogs]).await
                == 2
        })
        .await;
    test_records(devices.a()).find_or_create_artists(&[crediting(first)]).await.unwrap();
    devices
        .settle("the merge", async |_, db| {
            count(db, "SELECT COUNT(*) FROM artist_merges", &[]).await == 1
        })
        .await;
}

/// What `statement`, counting rows, answers with `parameters` bound.
async fn count(db: &bae_store::Store, statement: &'static str, parameters: &[&str]) -> i64 {
    let parameters: Vec<String> = parameters.iter().map(|value| value.to_string()).collect();
    db.read(move |sql| {
        Ok(sql.query_row(statement, rusqlite::params_from_iter(&parameters), |row| {
            row.get::<_, i64>(0)
        })?)
    })
    .await
    .unwrap()
}
