package fm.bae.app.data

import android.util.Log
import fm.bae.app.playback.PlaybackEventSink
import uniffi.bae_bridge.BridgeUiEvent

private const val TAG = "bae.UiEventReducer"

/**
 * Reduces [BridgeUiEvent]s into the [LibraryStore], [ConfigStore], and the
 * playback [PlaybackEventSink] (the [fm.bae.app.playback.BaeCorePlayer]).
 *
 * bae-core owns playback on Android, so the playback / queue / repeat / volume /
 * mute variants project into the [PlaybackEventSink]. Library, config, sync, and
 * error events go to the stores. Desktop-only import, scan, candidate, and
 * preview events never fire (those services aren't started on mobile); the `when`
 * stays exhaustive via the trailing `else`.
 *
 * Callers dispatch this on the main thread; the stores' flows and the player's
 * state are read by Compose.
 */
object UiEventReducer {
    fun reduce(
        event: BridgeUiEvent,
        libraryStore: LibraryStore,
        configStore: ConfigStore,
        player: PlaybackEventSink,
    ) {
        when (event) {
            // ── Library ────────────────────────────────────────────────────
            is BridgeUiEvent.AlbumAdded -> libraryStore.handleAlbumAdded(event.album)
            is BridgeUiEvent.AlbumUpdated -> libraryStore.handleAlbumUpdated(event.album)
            is BridgeUiEvent.AlbumRemoved -> libraryStore.handleAlbumRemoved(event.albumId)
            is BridgeUiEvent.ReleaseAdded ->
                libraryStore.handleReleaseAdded(event.album, event.release)
            is BridgeUiEvent.ReleaseUpdated ->
                libraryStore.handleReleaseUpdated(event.albumId, event.release)
            is BridgeUiEvent.ReleaseRemoved ->
                libraryStore.handleReleaseRemoved(event.albumId, event.releaseId, event.album)

            // ── Playback (projected into the BaeCorePlayer) ─────────────────
            is BridgeUiEvent.PlaybackLoading -> player.onLoading(event.trackId, event.track)
            is BridgeUiEvent.PlaybackPlaying ->
                player.onPlaying(
                    trackId = event.trackId,
                    trackTitle = event.trackTitle,
                    artistNames = event.artistNames,
                    albumTitle = event.albumTitle,
                    coverImageId = event.coverImageId,
                    durationMs = event.durationMs.toLong(),
                )
            is BridgeUiEvent.PlaybackPaused ->
                player.onPaused(
                    trackId = event.trackId,
                    trackTitle = event.trackTitle,
                    artistNames = event.artistNames,
                    albumTitle = event.albumTitle,
                    coverImageId = event.coverImageId,
                    durationMs = event.durationMs.toLong(),
                )
            BridgeUiEvent.PlaybackStopped -> player.onStopped()
            // A track couldn't be played (cloud-only not downloaded, decode
            // failure); core has already fallen back to stopped. Surface why.
            is BridgeUiEvent.PlaybackError -> configStore.showError(event.message)
            is BridgeUiEvent.PlaybackProgress ->
                player.onProgress(
                    positionMs = event.positionMs.toLong(),
                    durationMs = event.durationMs.toLong(),
                    progress = event.progress,
                    elapsedLabel = event.elapsedLabel,
                    remainingLabel = event.remainingLabel,
                )
            is BridgeUiEvent.RepeatModeChanged -> player.onRepeatModeChanged(event.mode)
            is BridgeUiEvent.QueueUpdated ->
                player.onQueueUpdated(
                    items = event.items,
                    hasNext = event.hasNext,
                    hasPrevious = event.hasPrevious,
                )
            is BridgeUiEvent.VolumeChanged -> player.onVolumeChanged(event.volume)
            is BridgeUiEvent.MuteChanged -> player.onMuteChanged(event.isMuted)

            // ── Config / sync ──────────────────────────────────────────────
            is BridgeUiEvent.ConfigChanged -> {
                configStore.setConfig(event.config)
                configStore.setSyncReady(event.syncReady)
            }
            is BridgeUiEvent.SyncError -> configStore.setSyncError(event.message)

            // ── Errors ─────────────────────────────────────────────────────
            is BridgeUiEvent.Error -> configStore.showError(event.message)
            BridgeUiEvent.ErrorCleared -> configStore.clearError()

            // Desktop-only events (import/scan/candidate/preview) never fire
            // on mobile; log if one ever does so a future mobile-firing event is
            // visible.
            else -> Log.d(TAG, "ignoring ${event::class.simpleName}")
        }
    }
}
