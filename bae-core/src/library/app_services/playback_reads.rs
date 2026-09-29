//! The playback values on [`AppServices`], with the playing track's display
//! read live from the library.

use super::*;

impl AppServices {
    /// The playback values, each carrying the playing track's current display.
    /// A value follows each change to the service's values, and each change the
    /// library makes to what the playing track shows — its names, its album,
    /// its cover — while the same track plays.
    pub fn subscribe_playback_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::playback::NowPlayingValues, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let services = self.clone();
        let query_runtime = runtime_handle.clone();
        runtime_handle.spawn(async move {
            let mut values = services.inner.playback.subscribe_values();
            let mut current = values.borrow_and_update().clone();
            let mut request = current.state.track_id().map(str::to_string);
            let mut display_query = reconfigurable_live_query_events(
                &query_runtime,
                services
                    .inner
                    .manager
                    .subscribe_track_display(request.clone()),
            );
            // The display read for `request`; `None` until that read arrives.
            let mut display: Option<Option<crate::playback::TrackDisplay>> = None;
            loop {
                tokio::select! {
                    event = display_query.recv() => {
                        let Some(result) = event else { return };
                        let read = match result {
                            Ok(read) => read,
                            Err(error) => {
                                if tx.send(Err(error)).is_err() { return; }
                                continue;
                            }
                        };
                        let resolved = now_playing_values(&current, read.as_ref());
                        display = Some(read);
                        if let Some(value) = resolved {
                            if tx.send(Ok(value)).is_err() { return; }
                        }
                    }
                    changed = values.changed() => {
                        if changed.is_err() { return; }
                        current = values.borrow_and_update().clone();
                        let next = current.state.track_id().map(str::to_string);
                        if next != request {
                            request = next;
                            display = None;
                            display_query.set(request.clone());
                        } else if let Some(read) = &display {
                            if let Some(value) = now_playing_values(&current, read.as_ref()) {
                                if tx.send(Ok(value)).is_err() { return; }
                            }
                        }
                    }
                }
            }
        });
        rx
    }
}

/// `values` with its track's `display`, or `None` when the state names a
/// track the library has no display for: the track was deleted, and the
/// playback service has yet to move off it.
fn now_playing_values(
    values: &crate::playback::PlaybackValues,
    display: Option<&crate::playback::TrackDisplay>,
) -> Option<crate::playback::NowPlayingValues> {
    values
        .clone()
        .try_map_track(|track| match display {
            Some(display) => Ok(crate::playback::NowPlayingTrack {
                track,
                display: display.clone(),
            }),
            None => Err(track.track_id),
        })
        .inspect_err(|track_id| {
            tracing::warn!(
                "no library display for playing track {track_id}; holding the last value"
            );
        })
        .ok()
}
