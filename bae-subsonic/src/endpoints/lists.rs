//! List and search endpoints: album lists, one song, and combined search.

use bae_views::served::{ServedAlbumOrder, ServedPage, ServedSearchRequest};
use bae_views::LibrarySearchQuery;
use tracing::debug;

use crate::envelope::Element;
use crate::error::SubError;
use crate::id::SubId;
use crate::library_map::{album_id3, artist_id3, lib_err, search_track_child, track_child};
use crate::params::Params;
use crate::AppState;

/// `getAlbumList2` — a list of albums (releases) under a `type` ordering.
/// Unsupported types (`frequent`/`recent` need a play-count store, `byGenre`
/// a genre store) return an empty list rather than an error, so a client's
/// tab renders empty instead of failing.
pub(crate) async fn get_album_list2(
    state: AppState,
    params: Params,
) -> Result<Option<Element>, SubError> {
    let list_type = params.require("type")?;
    let page = ServedPage {
        offset: count_param(&params, "offset", 0)?,
        limit: count_param(&params, "size", 10)?,
    };
    let order = match list_type {
        "alphabeticalByName" => ServedAlbumOrder::Title,
        "newest" => ServedAlbumOrder::Newest,
        "byYear" => ServedAlbumOrder::Years {
            from: params.int("fromYear")?.ok_or_else(|| SubError::missing_param("fromYear"))?,
            to: params.int("toYear")?.ok_or_else(|| SubError::missing_param("toYear"))?,
        },
        "random" => ServedAlbumOrder::Random,
        other => {
            debug!("getAlbumList2 type '{other}' is unsupported; returning an empty list");
            return Ok(Some(Element::new("albumList2")));
        }
    };
    let releases = state.services.served_album_list(order, page).await.map_err(lib_err)?;
    Ok(Some(
        Element::new("albumList2")
            .children(releases.iter().map(|release| album_id3(release).to_element())),
    ))
}

/// The count or offset the parameter `name` gives, `default` when it is
/// absent and 0 when it is below 0.
fn count_param(params: &Params, name: &str, default: i64) -> Result<u64, SubError> {
    Ok(u64::try_from(params.int_or(name, default)?).unwrap_or(0))
}

/// `getSong` — one song by id.
pub(crate) async fn get_song(state: AppState, params: Params) -> Result<Option<Element>, SubError> {
    let track_id = SubId::parse(params.require("id")?)?.expect_track()?.to_string();
    let song = state
        .services
        .served_song(&track_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    let child = track_child(&song.track, &song.release, &album_id3(&song.release));
    Ok(Some(child.to_element()))
}

/// The page of one kind of `search3` result that the parameters
/// `count_name` and `offset_name` ask for, 20 from the first by default.
fn search_page(
    params: &Params,
    count_name: &str,
    offset_name: &str,
) -> Result<ServedPage, SubError> {
    Ok(ServedPage {
        offset: count_param(params, offset_name, 0)?,
        limit: count_param(params, count_name, 20)?,
    })
}

/// `search3` — combined artist/album/song search. An empty query returns the
/// whole library (clients page through it), each kind paged by its
/// count/offset.
pub(crate) async fn search3(state: AppState, params: Params) -> Result<Option<Element>, SubError> {
    let request = ServedSearchRequest {
        query: LibrarySearchQuery::parse(params.get("query").unwrap_or("")),
        artists: search_page(&params, "artistCount", "artistOffset")?,
        albums: search_page(&params, "albumCount", "albumOffset")?,
        songs: search_page(&params, "songCount", "songOffset")?,
    };
    let found = state.services.served_search(request).await.map_err(lib_err)?;
    Ok(Some(
        Element::new("searchResult3")
            .children(found.artists.iter().map(|artist| artist_id3(artist).to_element()))
            .children(found.albums.iter().map(|release| album_id3(release).to_element()))
            .children(
                found
                    .songs
                    .iter()
                    .map(|(track, release)| search_track_child(track, release).to_element()),
            ),
    ))
}
