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
        .subscribe_reconfigurable(initial, |track_ids| {
            Ok(bae_records::records::track_places_read(track_ids))
        })
        .process(|_, places| {
            places
                .into_iter()
                .map(|place| {
                    Ok(crate::PlaybackTrackInfo::of(bae_records::records::track_place_of(place)?))
                })
                .collect()
        })
}

// The display of one track; no row for a track the library does not hold.
coven::read!(
    writer OneTrackDisplay(track: String),
    r#"
    SELECT track.*
    FROM @bae_records::records::track_display::DisplayedTrack AS track
    WHERE track.display_track_id = :track
"#
);

/// The display of `track_id` as the library holds it now, or `None` when it
/// no longer holds the track.
pub async fn track_display(
    store: &Store,
    track_id: &str,
) -> Result<Option<crate::TrackDisplay>, DbError> {
    let statement = OneTrackDisplayParams { track: track_id.to_owned() };
    store
        .read(move |sql| {
            statement_optional(&sql, &statement)?.map(|row| row.track.into_display()).transpose()
        })
        .await
}

#[cfg(test)]
#[path = "track_display_tests.rs"]
mod tests;
