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
            let mut infos = Vec::with_capacity(track_ids.len());
            for track_id in track_ids {
                let track = sql
                    .query_row("SELECT * FROM tracks WHERE id = ?", params![track_id], row_to_track)
                    .optional()
                    .map_err(DbError::from)?;
                let Some(track) = track else { continue };
                let mediums = release_mediums_on(&sql, &track.release_id)?;
                infos.push(crate::PlaybackTrackInfo::of(&track, &mediums));
            }
            Ok(infos)
        })
        .process(Ok)
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

/// A track's display and its catalog duration, as the queue rows show it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrackQueueMeta {
    pub(crate) display: crate::TrackDisplay,
    pub(crate) duration_ms: u64,
}

/// The joins from `tracks t` that [`track_display_columns`] reads: the
/// release the track is on, its album, and that release's cover.
pub(crate) const TRACK_DISPLAY_JOINS: &str = "JOIN releases r ON r.id = t.release_id \
     JOIN albums a ON a.id = r.album_id \
     LEFT JOIN covers c ON c.id = r.id";

/// The columns of a track's display over `tracks t` and
/// [`TRACK_DISPLAY_JOINS`], which [`track_queue_meta_from_row`] reads: its
/// title, its credited artists (the album's when the track credits none), its
/// album, the release it is on, and that release's cover. The cover is the
/// track's own release's, not the album's primary release's, so a track from
/// a non-primary release shows that release's art; its versioned reference is
/// what the UI caches art under, and a release with no cover row has none.
pub(crate) fn track_display_columns() -> String {
    format!(
        "t.id AS track_id, t.title, t.duration_ms, a.id AS album_id, \
         a.title AS album_title, r.id AS release_id, \
         {cover_version} AS cover_version, \
         {artist_names} AS artist_names",
        cover_version = bae_store::file_version_sql("c.file"),
        artist_names = track_credits_json_sql("t"),
    )
}

/// What the joins and columns of `playback_now`'s read add to its row for
/// the track it names: the joins from `playback_now n` to that track, its
/// release, album and cover, all LEFT, so a row naming no track, or one the
/// library no longer holds, still reads.
pub(crate) const NOW_TRACK_DISPLAY_JOINS: &str = "LEFT JOIN tracks t ON t.id = n.track_id \
     LEFT JOIN releases r ON r.id = t.release_id \
     LEFT JOIN albums a ON a.id = r.album_id \
     LEFT JOIN covers c ON c.id = r.id";

/// The columns of the playing track's display over
/// [`NOW_TRACK_DISPLAY_JOINS`], named apart from `playback_now`'s own
/// (`display_*`).
pub(crate) fn now_track_display_columns() -> String {
    format!(
        "t.title AS display_title, a.id AS display_album_id, a.title AS display_album_title, \
         r.id AS display_release_id, {cover_version} AS display_cover_version, \
         {artist_names} AS display_artist_names",
        cover_version = bae_store::file_version_sql("c.file"),
        artist_names = track_credits_json_sql("t"),
    )
}

/// The playing track's display from a row of `playback_now` read with
/// [`now_track_display_columns`]; `None` when the row names no track the
/// library holds.
pub(crate) fn now_track_display_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<Option<crate::TrackDisplay>> {
    let Some(release_id) = row.get::<_, Option<String>>("display_release_id")? else {
        return Ok(None);
    };
    // A release's cover image is keyed by the release's id.
    let cover_image = bae_records::cover_ref(&release_id, row.get("display_cover_version")?);
    Ok(Some(crate::TrackDisplay {
        title: row.get("display_title")?,
        artist_line: credit_line_column(row, "display_artist_names")?,
        album_id: row.get("display_album_id")?,
        release_id,
        album_title: row.get("display_album_title")?,
        cover_image,
    }))
}

pub(crate) fn track_queue_meta_from_row(row: &Row<'_>) -> rusqlite::Result<TrackQueueMeta> {
    let release_id: String = row.get("release_id")?;
    // A release's cover image is keyed by the release's id.
    let cover_image = bae_records::cover_ref(&release_id, row.get("cover_version")?);
    Ok(TrackQueueMeta {
        display: crate::TrackDisplay {
            title: row.get("title")?,
            artist_line: credit_line_column(row, "artist_names")?,
            album_id: row.get("album_id")?,
            release_id,
            album_title: row.get("album_title")?,
            cover_image,
        },
        duration_ms: row.get("duration_ms")?,
    })
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
                columns = track_display_columns(),
                joins = TRACK_DISPLAY_JOINS,
            ),
            [track_id],
            track_queue_meta_from_row,
        )
        .optional()?
        .map(|meta| meta.display))
}

#[cfg(test)]
#[path = "track_display_tests.rs"]
mod tests;
