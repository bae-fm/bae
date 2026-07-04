package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.ErrorLines
import fm.bae.app.playback.PlaybackEventSink
import uniffi.bae_bridge.BridgeUiEvent

private const val TAG = "bae.UiEventReducer"
private val logger = BaeLogger(TAG)

/**
 * Reduces [BridgeUiEvent]s into the open library stores and the playback
 * [PlaybackEventSink] (the [fm.bae.app.playback.BaeCorePlayer]).
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
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ) {
        when (event) {
            is BridgeUiEvent.AlbumAdded,
            is BridgeUiEvent.AlbumUpdated,
            is BridgeUiEvent.AlbumRemoved,
            is BridgeUiEvent.ReleaseAdded,
            is BridgeUiEvent.ReleaseUpdated,
            is BridgeUiEvent.ReleaseRemoved,
            -> {
                reduceLibrary(event, stores.library)
            }

            is BridgeUiEvent.PlaybackLoading,
            is BridgeUiEvent.PlaybackPlaying,
            is BridgeUiEvent.PlaybackPaused,
            BridgeUiEvent.PlaybackStopped,
            is BridgeUiEvent.PlaybackError,
            is BridgeUiEvent.PlaybackProgress,
            is BridgeUiEvent.PlaybackSeeked,
            is BridgeUiEvent.RepeatModeChanged,
            is BridgeUiEvent.QueueUpdated,
            is BridgeUiEvent.VolumeChanged,
            is BridgeUiEvent.MuteChanged,
            -> {
                reducePlayback(event, player, stores.config, errors)
            }

            is BridgeUiEvent.ConfigChanged,
            is BridgeUiEvent.SyncingChanged,
            is BridgeUiEvent.SyncError,
            is BridgeUiEvent.Error,
            BridgeUiEvent.ErrorCleared,
            -> {
                reduceConfig(event, stores.config, errors)
            }

            is BridgeUiEvent.DownloadQueueChanged -> {
                stores.downloads.setSnapshot(event.snapshot)
            }

            // Desktop-only events (import/scan/candidate/preview) never fire
            // on mobile; log if one ever does so a future mobile-firing event is visible.
            else -> {
                logger.debug("ignoring ${event::class.simpleName}")
            }
        }
    }

    private fun reduceLibrary(
        event: BridgeUiEvent,
        store: LibraryStore,
    ) {
        when (event) {
            is BridgeUiEvent.AlbumAdded -> {
                store.handleAlbumAdded(event.album)
            }

            is BridgeUiEvent.AlbumUpdated -> {
                store.handleAlbumUpdated(event.album)
            }

            is BridgeUiEvent.AlbumRemoved -> {
                store.handleAlbumRemoved(event.albumId)
            }

            is BridgeUiEvent.ReleaseAdded -> {
                store.handleReleaseAdded(event.album, event.release)
            }

            is BridgeUiEvent.ReleaseUpdated -> {
                store.handleReleaseUpdated(event.albumId, event.release)
            }

            is BridgeUiEvent.ReleaseRemoved -> {
                store.handleReleaseRemoved(event.albumId, event.releaseId, event.album)
            }

            else -> {}
        }
    }

    private fun reducePlayback(
        event: BridgeUiEvent,
        player: PlaybackEventSink,
        configStore: ConfigStore,
        errors: ErrorLines,
    ) {
        when (event) {
            is BridgeUiEvent.PlaybackLoading -> {
                player.onLoading(event.trackId, event.track)
            }

            is BridgeUiEvent.PlaybackPlaying -> {
                player.onPlaying(event)
            }

            is BridgeUiEvent.PlaybackPaused -> {
                player.onPaused(event)
            }

            BridgeUiEvent.PlaybackStopped -> {
                player.onStopped()
            }

            // A track couldn't be played (cloud-only not downloaded, decode failure);
            // core has already fallen back to stopped. Surface why.
            is BridgeUiEvent.PlaybackError -> {
                configStore.showError(errors.line(event.reason))
            }

            is BridgeUiEvent.PlaybackProgress -> {
                player.onProgress(
                    trackId = event.trackId,
                    positionMs = event.positionMs.toLong(),
                    durationMs = event.durationMs.toLong(),
                    progress = event.progress,
                )
            }

            is BridgeUiEvent.PlaybackSeeked -> {
                player.onSeeked(
                    trackId = event.trackId,
                    positionMs = event.positionMs.toLong(),
                    durationMs = event.durationMs.toLong(),
                    progress = event.progress,
                )
            }

            is BridgeUiEvent.RepeatModeChanged -> {
                player.onRepeatModeChanged(event.mode)
            }

            is BridgeUiEvent.QueueUpdated -> {
                player.onQueueUpdated(
                    manual = event.manual,
                    context = event.context,
                    hasNext = event.hasNext,
                    hasPrevious = event.hasPrevious,
                )
            }

            is BridgeUiEvent.VolumeChanged -> {
                player.onVolumeChanged(event.volume)
            }

            is BridgeUiEvent.MuteChanged -> {
                player.onMuteChanged(event.isMuted)
            }

            else -> {}
        }
    }

    private fun reduceConfig(
        event: BridgeUiEvent,
        configStore: ConfigStore,
        errors: ErrorLines,
    ) {
        when (event) {
            is BridgeUiEvent.ConfigChanged -> {
                configStore.setConfig(event.config)
                configStore.setSyncReady(event.syncReady)
            }

            is BridgeUiEvent.SyncingChanged -> {
                configStore.setSyncing(event.syncing)
            }

            // A null error means sync recovered — it clears the banner.
            is BridgeUiEvent.SyncError -> {
                configStore.setSyncError(event.error?.let { errors.line(it) })
            }

            is BridgeUiEvent.Error -> {
                configStore.showError(errors.line(event.error))
            }

            BridgeUiEvent.ErrorCleared -> {
                configStore.clearError()
            }

            else -> {}
        }
    }
}
