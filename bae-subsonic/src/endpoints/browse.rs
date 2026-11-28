//! Browsing endpoints: the artist index, one artist's albums, and one album's
//! songs. All tag-based (`*ID3`) — bae has no folder tree.

use std::collections::BTreeMap;

use crate::envelope::Element;
use crate::error::SubError;
use crate::id::SubId;
use crate::library_map::{album_id3, artist_id3, lib_err, track_child};
use crate::model::ArtistId3;
use crate::params::Params;
use crate::AppState;

/// `getArtists` — every artist, grouped into `<index>` buckets by first letter.
pub(crate) async fn get_artists(state: AppState, _: Params) -> Result<Option<Element>, SubError> {
    artist_index(&state, "artists").await
}

/// `getIndexes` — the same artist index as `getArtists`, under the legacy
/// `<indexes>` payload name with a `lastModified`.
pub(crate) async fn get_indexes(state: AppState, _: Params) -> Result<Option<Element>, SubError> {
    artist_index(&state, "indexes").await
}

async fn artist_index(
    state: &AppState,
    payload_name: &'static str,
) -> Result<Option<Element>, SubError> {
    let artists = state.services.served_artists().await.map_err(lib_err)?;

    // Bucket by first sort letter, keeping buckets ordered.
    let mut buckets: BTreeMap<String, Vec<ArtistId3>> = BTreeMap::new();
    for artist in &artists {
        buckets
            .entry(index_letter(&artist.artist.artist.name))
            .or_default()
            .push(artist_id3(artist));
    }

    let mut payload = Element::new(payload_name).attr("ignoredArticles", "");
    if payload_name == "indexes" {
        payload = payload.attr("lastModified", 0_i64);
    }
    for (letter, artists) in buckets {
        let index = Element::new("index")
            .attr("name", letter)
            .children(artists.iter().map(ArtistId3::to_element));
        payload = payload.child(index);
    }
    Ok(Some(payload))
}

/// The bucket letter for an artist name: the uppercased first character, or `#`
/// when it isn't a Latin letter.
fn index_letter(name: &str) -> String {
    match name.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase().to_string(),
        _ => "#".to_string(),
    }
}

/// `getArtist` — one artist plus their albums (each a release).
pub(crate) async fn get_artist(
    state: AppState,
    params: Params,
) -> Result<Option<Element>, SubError> {
    let artist_id = SubId::parse(params.require("id")?)?.expect_artist()?.to_string();
    let (artist, releases) = state
        .services
        .served_artist(&artist_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    let element = artist_id3(&artist)
        .to_element()
        .children(releases.iter().map(|release| album_id3(release).to_element()));
    Ok(Some(element))
}

/// `getAlbum` — one album (release) with its songs.
pub(crate) async fn get_album(
    state: AppState,
    params: Params,
) -> Result<Option<Element>, SubError> {
    let release_id = SubId::parse(params.require("id")?)?.expect_album()?.to_string();
    let served = state
        .services
        .served_album(&release_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    let album = album_id3(&served.release);
    let songs =
        served.tracks.iter().map(|track| track_child(track, &served.release, &album)).collect();
    Ok(Some(album.with_songs(songs)))
}
