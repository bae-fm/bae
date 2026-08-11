//! List and search endpoints: album lists, one song, and combined search.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::response::Response;
use bae_core::db::{DbAlbum, DbRelease};
use bae_core::library::{AppServices, LibrarySearchQuery};
use tracing::debug;

use crate::endpoints::respond;
use crate::envelope::Element;
use crate::error::SubError;
use crate::id::SubId;
use crate::library_map::{
    artist_id3, artist_release_count, lib_err, release_album_id3, release_album_id3_with,
    search_track_child, track_child,
};
use crate::params::Params;
use crate::AppState;

/// `getAlbumList2` — a list of albums (releases) under a `type` ordering.
pub(crate) async fn get_album_list2(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    respond(&params.format(), album_list2(&state, &params).await)
}

async fn album_list2(state: &AppState, params: &Params) -> Result<Option<Element>, SubError> {
    let services = &state.services;
    let list_type = params.require("type")?;
    let size = params.int_or("size", 10)?.max(0) as usize;
    let offset = params.int_or("offset", 0)?.max(0) as usize;

    let ordered = ordered_release_ids(services, params, list_type).await?;
    let page = ordered.into_iter().skip(offset).take(size);

    let mut payload = Element::new("albumList2");
    for release_id in page {
        payload = payload.child(release_album_id3(services, &release_id).await?.to_element());
    }
    Ok(Some(payload))
}

/// The release ids for a `getAlbumList2` `type`, ordered (and, for `byYear`,
/// filtered) but not yet paged. Unsupported types (`frequent`/`recent` need a
/// play-count store, `byGenre` a genre store) return an empty list rather than
/// an error, so a client's tab renders empty instead of failing.
async fn ordered_release_ids(
    services: &AppServices,
    params: &Params,
    list_type: &str,
) -> Result<Vec<String>, SubError> {
    // `random` is the only type with no derivable ordering; everything else
    // orders the same pair list. Build the pairs once.
    let mut pairs = release_album_pairs(services).await?;

    match list_type {
        "alphabeticalByName" => {
            pairs.sort_by(|a, b| {
                a.1.title
                    .to_lowercase()
                    .cmp(&b.1.title.to_lowercase())
                    .then_with(|| a.0.created_at.cmp(&b.0.created_at))
            });
        }
        "newest" => {
            pairs.sort_by_key(|pair| std::cmp::Reverse(pair.0.created_at));
        }
        "byYear" => {
            let from = params
                .int("fromYear")?
                .ok_or_else(|| SubError::missing_param("fromYear"))?;
            let to = params
                .int("toYear")?
                .ok_or_else(|| SubError::missing_param("toYear"))?;
            let (lo, hi, descending) = if from <= to {
                (from, to, false)
            } else {
                (to, from, true)
            };
            pairs.retain(|pair| {
                pair_year(pair).is_some_and(|y| i64::from(y) >= lo && i64::from(y) <= hi)
            });
            pairs.sort_by(|a, b| {
                let ay = pair_year(a).unwrap_or(0);
                let by = pair_year(b).unwrap_or(0);
                if descending {
                    by.cmp(&ay)
                } else {
                    ay.cmp(&by)
                }
            });
        }
        "random" => shuffle(&mut pairs),
        other => {
            debug!("getAlbumList2 type '{other}' is unsupported; returning an empty list");
            return Ok(Vec::new());
        }
    }

    Ok(pairs.into_iter().map(|pair| pair.0.id).collect())
}

/// Every release paired with its album, across the whole library.
async fn release_album_pairs(
    services: &AppServices,
) -> Result<Vec<(DbRelease, DbAlbum)>, SubError> {
    let albums = services.get_albums(&[]).await.map_err(lib_err)?;
    let mut pairs = Vec::new();
    for album in albums {
        let releases = services
            .get_releases_for_album(&album.id)
            .await
            .map_err(lib_err)?;
        for release in releases {
            pairs.push((release, album.clone()));
        }
    }
    Ok(pairs)
}

fn pair_year(pair: &(DbRelease, DbAlbum)) -> Option<i32> {
    pair.0.pressing.year.or(pair.1.year)
}

/// Fisher–Yates shuffle seeded from the wall clock. `random` only needs an
/// unpredictable order per call, not a reproducible or cryptographic one.
fn shuffle<T>(items: &mut [T]) {
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15)
        | 1;
    for i in (1..items.len()).rev() {
        // xorshift64* — a small, dependency-free PRNG.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let rand = state.wrapping_mul(0x2545F4914F6CDD1D);
        let j = (rand % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// `getSong` — one song by id.
pub(crate) async fn get_song(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    respond(&params.format(), get_song_inner(&state, &params).await)
}

async fn get_song_inner(state: &AppState, params: &Params) -> Result<Option<Element>, SubError> {
    let services = &state.services;
    let track_id = SubId::parse(params.require("id")?)?
        .expect_track()?
        .to_string();

    // The song must exist. `filter_existing_track_ids` gives a clean absence
    // (→ not found) instead of a generic error from a downstream resolve.
    let existing = services
        .filter_existing_track_ids(std::slice::from_ref(&track_id))
        .await
        .map_err(lib_err)?;
    if existing.is_empty() {
        return Err(SubError::not_found());
    }

    let info = services
        .get_playback_track_info(&track_id)
        .await
        .map_err(lib_err)?;
    let release = services
        .get_release_by_id(&info.release_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    let tracks = services
        .get_tracks_for_release(&info.release_id)
        .await
        .map_err(lib_err)?;
    let track = tracks
        .iter()
        .find(|t| t.id == track_id)
        .ok_or_else(SubError::not_found)?;
    let files = services
        .get_files_for_release(&info.release_id)
        .await
        .map_err(lib_err)?;
    let has_cover_art = release_album_id3_with(services, &release)
        .await?
        .cover_art
        .is_some();

    let child = track_child(
        services,
        track,
        &release,
        &info.album_title,
        &files,
        has_cover_art,
    )
    .await?;
    Ok(Some(child.to_element()))
}

/// `search3` — combined artist/album/song search. An empty query returns the
/// whole library (clients page through it), each kind capped by its count/offset.
pub(crate) async fn search3(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    respond(&params.format(), search3_inner(&state, &params).await)
}

struct Window {
    offset: usize,
    count: usize,
}

impl Window {
    fn read(params: &Params, count_name: &str, offset_name: &str) -> Result<Self, SubError> {
        Ok(Self {
            offset: params.int_or(offset_name, 0)?.max(0) as usize,
            count: params.int_or(count_name, 20)?.max(0) as usize,
        })
    }
}

async fn search3_inner(state: &AppState, params: &Params) -> Result<Option<Element>, SubError> {
    let services = &state.services;
    let query = params.get("query").unwrap_or("");
    let artists = Window::read(params, "artistCount", "artistOffset")?;
    let albums = Window::read(params, "albumCount", "albumOffset")?;
    let songs = Window::read(params, "songCount", "songOffset")?;

    let mut payload = Element::new("searchResult3");

    match LibrarySearchQuery::parse(query) {
        Some(parsed) => {
            let results = services.search_library(&parsed).await.map_err(lib_err)?;

            // Artists.
            for summary in page(&results.artists, &artists) {
                let artist = &summary.raw.artist;
                let count = artist_release_count(services, &artist.id).await?;
                payload = payload.child(
                    artist_id3(
                        &artist.id,
                        &artist.name,
                        count,
                        artist.musicbrainz_artist_id.clone(),
                        summary.image.is_some(),
                    )
                    .to_element(),
                );
            }

            // Albums: each bae-album hit expands to its releases (a Subsonic
            // album is a release); the release list is what the window pages.
            let mut release_ids = Vec::new();
            for hit in &results.albums {
                for release in services
                    .get_releases_for_album(&hit.id)
                    .await
                    .map_err(lib_err)?
                {
                    release_ids.push(release.id);
                }
            }
            for release_id in page(&release_ids, &albums) {
                payload =
                    payload.child(release_album_id3(services, release_id).await?.to_element());
            }

            // Songs.
            for hit in page(&results.tracks, &songs) {
                payload = payload.child(
                    search_track_child(
                        services,
                        &hit.id,
                        &hit.title,
                        &hit.album_title,
                        &hit.artist_name,
                    )
                    .await?
                    .to_element(),
                );
            }
        }
        None => {
            payload = whole_library(services, payload, &artists, &albums, &songs).await?;
        }
    }

    Ok(Some(payload))
}

/// The `offset..offset+count` slice of `items`, clamped to the available range.
fn page<'a, T>(items: &'a [T], window: &Window) -> impl Iterator<Item = &'a T> {
    items.iter().skip(window.offset).take(window.count)
}

/// The empty-query result: the whole library, each kind paged by its window.
async fn whole_library(
    services: &AppServices,
    mut payload: Element,
    artists: &Window,
    albums: &Window,
    songs: &Window,
) -> Result<Element, SubError> {
    // Artists.
    let sort = [bae_core::db::ArtistSortCriterion {
        field: bae_core::db::ArtistSortField::Name,
        direction: bae_core::db::SortDirection::Ascending,
    }];
    let artist_page = services
        .get_artist_page(&sort, artists.offset as u64, artists.count as u64)
        .await
        .map_err(lib_err)?;
    for summary in artist_page {
        let artist = &summary.raw.artist;
        let count = artist_release_count(services, &artist.id).await?;
        payload = payload.child(
            artist_id3(
                &artist.id,
                &artist.name,
                count,
                artist.musicbrainz_artist_id.clone(),
                summary.image.is_some(),
            )
            .to_element(),
        );
    }

    // Albums (releases).
    let release_ids: Vec<String> = release_album_pairs(services)
        .await?
        .into_iter()
        .map(|pair| pair.0.id)
        .collect();
    for release_id in page(&release_ids, albums) {
        payload = payload.child(release_album_id3(services, release_id).await?.to_element());
    }

    // Songs.
    let track_ids = services.get_all_track_ids().await.map_err(lib_err)?;
    for track_id in page(&track_ids, songs) {
        let info = services
            .get_playback_track_info(track_id)
            .await
            .map_err(lib_err)?;
        payload = payload.child(
            search_track_child(
                services,
                track_id,
                &info.track_title,
                &info.album_title,
                &info.artist_names,
            )
            .await?
            .to_element(),
        );
    }
    Ok(payload)
}
