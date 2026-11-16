//! What playback and the queue rows read of a track: the side facts playback
//! pauses by, and the display the queue rows and the now-playing surfaces
//! show.

use super::*;

/// Follow the side facts of the tracks `initial` names, in its order,
/// leaving out a track the library no longer holds. The playback service
/// points it at the tracks whose crossing it has staged.
pub(crate) fn subscribe_playback_track_infos(
    store: &Store,
    initial: Vec<String>,
) -> coven::ReconfigurableLiveQuery<Vec<String>, Vec<crate::PlaybackTrackInfo>, bae_store::DbError>
{
    store
        .subscribe_reconfigurable(initial, |track_ids, sql| {
            bae_records::records::track_places_on(&sql, track_ids)
        })
        .process(|places| Ok(places.into_iter().map(crate::PlaybackTrackInfo::of).collect()))
}

/// The display of `track_id` as the library holds it now, or `None` when it
/// no longer holds the track.
pub async fn track_display(
    store: &Store,
    track_id: &str,
) -> Result<Option<crate::TrackDisplay>, DbError> {
    let track_id = track_id.to_string();
    store.read(move |sql| track_display_on(&sql, &track_id)).await
}

/// The joins from `tracks t` that [`track_display_columns`] reads: the
/// release the track is on, its album, and that release's cover.
pub(crate) const TRACK_DISPLAY_JOINS: &str = "JOIN releases r ON r.id = t.release_id \
     JOIN albums a ON a.id = r.album_id \
     LEFT JOIN covers c ON c.id = r.id";

/// The joins from `playback_now n` to its track, release, album and cover,
/// all LEFT, so the query still returns a row that names no track or a track
/// the library no longer holds.
pub(crate) const NOW_TRACK_DISPLAY_JOINS: &str = "LEFT JOIN tracks t ON t.id = n.track_id \
     LEFT JOIN releases r ON r.id = t.release_id \
     LEFT JOIN albums a ON a.id = r.album_id \
     LEFT JOIN covers c ON c.id = r.id";

/// The columns of a track's display over `tracks t` and the joins of
/// [`TRACK_DISPLAY_JOINS`] or [`NOW_TRACK_DISPLAY_JOINS`], each named with
/// `prefix` so it does not clash with the other columns of the statement,
/// which [`track_display_from_row`] reads: its title, its credited artists
/// (the album's when the track credits none), its album, the release it is
/// on, and that release's cover. The cover is the track's own release's, not
/// the album's primary release's, so a track from a non-primary release shows
/// that release's art; its versioned reference is what the UI caches art
/// under, and a release with no cover row has none.
pub(crate) fn track_display_columns(prefix: &str) -> String {
    format!(
        "t.title AS {prefix}title, a.id AS {prefix}album_id, \
         a.title AS {prefix}album_title, r.id AS {prefix}release_id, \
         {cover_version} AS {prefix}cover_version, \
         {artist_names} AS {prefix}artist_names",
        cover_version = bae_store::file_version_sql("c.file"),
        artist_names = track_credits_json_sql("t"),
    )
}

/// The display of the track a row read with [`track_display_columns`] under
/// `prefix` names.
pub(crate) fn track_display_from_row(
    row: &Row<'_>,
    prefix: &str,
) -> rusqlite::Result<crate::TrackDisplay> {
    let column = |name: &str| format!("{prefix}{name}");
    let release_id: String = row.get(column("release_id").as_str())?;
    // A release's cover image is keyed by the release's id.
    let cover_image =
        bae_records::cover_ref(&release_id, row.get(column("cover_version").as_str())?);
    Ok(crate::TrackDisplay {
        title: row.get(column("title").as_str())?,
        artist_line: credit_line_column(row, &column("artist_names"))?,
        album_id: row.get(column("album_id").as_str())?,
        release_id,
        album_title: row.get(column("album_title").as_str())?,
        cover_image,
    })
}

/// The playing track's display from a row of `playback_now` read with
/// [`track_display_columns`] under the prefix `display_`; `None` when the row
/// names no track the library holds, and its release column is NULL.
pub(crate) fn now_track_display_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<Option<crate::TrackDisplay>> {
    if row.get::<_, Option<String>>("display_release_id")?.is_none() {
        return Ok(None);
    }
    Ok(Some(track_display_from_row(row, "display_")?))
}

/// The display of `track_id`, or `None` when the library no longer holds it.
pub(crate) fn track_display_on(
    sql: &SqlReadContext<'_>,
    track_id: &str,
) -> Result<Option<crate::TrackDisplay>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                "SELECT {columns} FROM tracks t {joins} WHERE t.id = ?",
                columns = track_display_columns(""),
                joins = TRACK_DISPLAY_JOINS,
            ),
            [track_id],
            |row| track_display_from_row(row, ""),
        )
        .optional()?)
}

#[cfg(test)]
#[path = "track_display_tests.rs"]
mod tests;
