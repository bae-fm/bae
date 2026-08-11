//! Browsing endpoints: the artist index, one artist's albums, and one album's
//! songs. All tag-based (`*ID3`) — bae has no folder tree.

use std::collections::BTreeMap;
use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::response::Response;
use bae_core::db::{ArtistSortCriterion, ArtistSortField, SortDirection};

use crate::endpoints::respond;
use crate::envelope::Element;
use crate::error::SubError;
use crate::id::SubId;
use crate::library_map::{
    artist_id3, lib_err, release_album_id3, release_album_id3_with, track_child,
};
use crate::model::ArtistId3;
use crate::params::Params;
use crate::AppState;

/// `getArtists` — every artist, grouped into `<index>` buckets by first letter.
pub(crate) async fn get_artists(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    respond(&params.format(), artist_index(&state, "artists").await)
}

/// `getIndexes` — the same artist index as `getArtists`, under the legacy
/// `<indexes>` payload name with a `lastModified`.
pub(crate) async fn get_indexes(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    respond(&params.format(), artist_index(&state, "indexes").await)
}

async fn artist_index(
    state: &AppState,
    payload_name: &'static str,
) -> Result<Option<Element>, SubError> {
    let services = &state.services;
    let count = services.get_artist_count().await.map_err(lib_err)?;
    let sort = [ArtistSortCriterion {
        field: ArtistSortField::Name,
        direction: SortDirection::Ascending,
    }];
    let summaries = services
        .get_artist_page(&sort, 0, count)
        .await
        .map_err(lib_err)?;

    // Bucket by first sort letter, keeping buckets ordered.
    let mut buckets: BTreeMap<String, Vec<ArtistId3>> = BTreeMap::new();
    for summary in summaries {
        let artist = &summary.raw.artist;
        // Album count is this artist's release total (a Subsonic album is a
        // release), summed over the artist's bae albums.
        let release_count = crate::library_map::artist_release_count(services, &artist.id).await?;
        let id3 = artist_id3(
            &artist.id,
            &artist.name,
            release_count,
            artist.musicbrainz_artist_id.clone(),
            summary.image.is_some(),
        );
        buckets
            .entry(index_letter(&artist.name))
            .or_default()
            .push(id3);
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
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    respond(&params.format(), get_artist_inner(&state, &params).await)
}

async fn get_artist_inner(state: &AppState, params: &Params) -> Result<Option<Element>, SubError> {
    let services = &state.services;
    let artist_id = SubId::parse(params.require("id")?)?
        .expect_artist()?
        .to_string();
    let detail = services
        .get_artist_detail(&artist_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;

    let artist = &detail.artist.raw.artist;
    let release_ids: Vec<String> = detail
        .albums
        .iter()
        .flat_map(|album| album.release_ids.iter().cloned())
        .collect();

    let mut element = artist_id3(
        &artist.id,
        &artist.name,
        release_ids.len() as i64,
        artist.musicbrainz_artist_id.clone(),
        detail.artist.image.is_some(),
    )
    .to_element();

    for release_id in release_ids {
        let album = release_album_id3(services, &release_id).await?;
        element = element.child(album.to_element());
    }
    Ok(Some(element))
}

/// `getAlbum` — one album (release) with its songs.
pub(crate) async fn get_album(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    respond(&params.format(), get_album_inner(&state, &params).await)
}

async fn get_album_inner(state: &AppState, params: &Params) -> Result<Option<Element>, SubError> {
    let services = &state.services;
    let release_id = SubId::parse(params.require("id")?)?
        .expect_album()?
        .to_string();
    let release = services
        .get_release_by_id(&release_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;

    let album = release_album_id3_with(services, &release).await?;
    let album_title = album.name.clone();
    let has_cover_art = album.cover_art.is_some();

    let tracks = services
        .get_tracks_for_release(&release_id)
        .await
        .map_err(lib_err)?;
    let files = services
        .get_files_for_release(&release_id)
        .await
        .map_err(lib_err)?;

    let mut songs = Vec::with_capacity(tracks.len());
    for track in &tracks {
        songs.push(
            track_child(
                services,
                track,
                &release,
                &album_title,
                &files,
                has_cover_art,
            )
            .await?,
        );
    }
    Ok(Some(album.with_songs(songs)))
}
