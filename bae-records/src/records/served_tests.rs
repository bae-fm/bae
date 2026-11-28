use super::*;
use crate::test_fixtures::audio::store_audio;
use crate::test_fixtures::{
    artist_album_release, test_records, ARTIST_PRIMARY, RELEASE_A, TRACK_A, TRACK_B,
};

/// `RELEASE_A` of the 2026 album `ALBUM_A`, credited to `ARTIST_PRIMARY`,
/// with `TRACK_A` then `TRACK_B` on its CD, each a whole FLAC file.
async fn served_release() -> (bae_store::Store, bae_util::test_files::TestFolder, Records) {
    let (db, temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = test_records(&db);
    artist_album_release()
        .tracks(RELEASE_A, &[(TRACK_A, "Track Title A"), (TRACK_B, "Track Title B")])
        .write(&records)
        .await;
    store_audio(&records, &temp, RELEASE_A, TRACK_A, false).await;
    store_audio(&records, &temp, RELEASE_A, TRACK_B, false).await;
    (db, temp, records)
}

/// An album outside bae is its release: the album's title, credits and year,
/// the release's track count and length, and each track numbered in playing
/// order with its credits and the file it plays from.
#[tokio::test]
async fn a_served_album_reads_its_release_and_numbered_tracks() {
    let (_db, _temp, records) = served_release().await;

    let album = records.served_album(RELEASE_A).await.unwrap().unwrap();

    let release = &album.release;
    assert_eq!(release.id, RELEASE_A);
    assert_eq!(release.album_title, "Album Title A");
    assert_eq!(release.year, Some(2026));
    assert_eq!(
        release.credits.first().map(|credit| credit.artist.id.as_str()),
        Some(ARTIST_PRIMARY)
    );
    assert_eq!(release.song_count, 2);
    let lengths: u64 = album.tracks.iter().map(|song| song.track.duration_ms).sum();
    assert_eq!(release.duration_ms, lengths);
    assert_eq!(release.musicbrainz_id, None);
    let songs: Vec<_> = album
        .tracks
        .iter()
        .map(|song| {
            let whole = song.track.whole_file.as_ref().unwrap();
            (song.track.id.as_str(), song.number, song.disc, whole.name.as_str(), whole.size)
        })
        .collect();
    assert_eq!(
        songs,
        [
            (TRACK_A, 1, None, format!("{TRACK_A}-0.flac").as_str(), 100),
            (TRACK_B, 2, None, format!("{TRACK_B}-0.flac").as_str(), 100),
        ]
    );
    assert!(album.tracks.iter().all(|song| !song.track.credits.is_empty()));
}

/// A release the library doesn't hold serves no album.
#[tokio::test]
async fn a_missing_release_serves_no_album() {
    let (_db, _temp, records) = served_release().await;

    assert!(records.served_album(crate::test_fixtures::RELEASE_B).await.unwrap().is_none());
}

/// A song outside bae is its track, numbered as its album numbers it, with
/// its release as its album.
#[tokio::test]
async fn a_served_song_reads_its_number_and_release() {
    let (_db, _temp, records) = served_release().await;

    let song = records.served_song(TRACK_B).await.unwrap().unwrap();

    assert_eq!(song.track.track.id, TRACK_B);
    assert_eq!(song.track.track.title, "Track Title B");
    assert_eq!((song.track.number, song.track.disc), (2, None));
    assert_eq!(song.release.id, RELEASE_A);
    assert_eq!(song.release.song_count, 2);
}

/// A track the library doesn't hold serves no song.
#[tokio::test]
async fn a_missing_track_serves_no_song() {
    let (_db, _temp, records) = served_release().await;

    assert!(records.served_song(crate::test_fixtures::TRACK_WORK_A).await.unwrap().is_none());
}

/// A song states its format, and a track that plays from more than one
/// segment, such as a pregap and its body, has no one file that plays it
/// whole.
#[tokio::test]
async fn a_song_of_several_segments_has_no_whole_file() {
    let (db, temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = test_records(&db);
    artist_album_release().tracks(RELEASE_A, &[(TRACK_A, "Track Title A")]).write(&records).await;
    store_audio(&records, &temp, RELEASE_A, TRACK_A, true).await;

    let song = records.served_song(TRACK_A).await.unwrap().unwrap().track.track;

    assert!(song.whole_file.is_none());
    assert_eq!(
        (song.sample_rate.get(), song.bits_per_sample.map(|bits| bits.get()), song.channels.get()),
        (44_100, Some(16), 2)
    );
}
