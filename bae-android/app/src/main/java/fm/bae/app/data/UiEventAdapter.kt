package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.ErrorLines
import fm.bae.app.playback.PlaybackEventSink
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeUiEvent

private const val TAG = "bae.UiEventAdapter"
private val logger = BaeLogger(TAG)

/**
 * Routes transient bridge UI events into Android's app state. Persistent values
 * arrive through their typed subscriptions owned by the open library session.
 */
object UiEventAdapter {
    suspend fun handle(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ) {
        try {
            route(event, stores, player, errors)
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            // No special case for Cancelled: core says whether an error has a line,
            // and showError drops the ones that do not.
            logger.error("event handling failed", e)
            stores.config.showError(errors.line(e))
        } catch (e: Exception) {
            logger.error("event handling failed", e)
            stores.config.showError(e.message ?: e::class.java.simpleName)
        }
    }

    private suspend fun route(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ) {
        handleDirectEvent(event, stores, player, errors)
    }

    private fun handleDirectEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ) {
        if (handlePlaybackEvent(event, stores, player, errors)) {
            return
        }
        if (handleAppErrorEvent(event, stores, errors)) {
            return
        }
        ignoreObsoleteEvent(event)
    }

    private fun handlePlaybackEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorLines,
    ): Boolean {
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

            is BridgeUiEvent.PlaybackError -> {
                stores.config.showError(errors.line(event.reason))
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

            is BridgeUiEvent.VolumeChanged -> {
                player.onVolumeChanged(event.volume)
            }

            is BridgeUiEvent.MuteChanged -> {
                player.onMuteChanged(event.isMuted)
            }

            is BridgeUiEvent.QueueItemsAdded -> {
                player.onQueueItemsAdded(event.count.toInt())
            }

            else -> {
                return false
            }
        }
        return true
    }

    private fun handleAppErrorEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        errors: ErrorLines,
    ): Boolean {
        when (event) {
            is BridgeUiEvent.Error -> {
                stores.config.showError(errors.line(event.error))
            }

            // Which device playback is on, including a receiver-side end core
            // noticed on its own.
            is BridgeUiEvent.CastStatusChanged -> {
                stores.cast.applyStatus(event.deviceName)
            }

            else -> {
                return false
            }
        }
        return true
    }

    private fun ignoreObsoleteEvent(event: BridgeUiEvent) {
        when (event) {
            BridgeUiEvent.PreviewIdle,
            is BridgeUiEvent.PreviewPlaying,
            is BridgeUiEvent.PreviewPaused,
            is BridgeUiEvent.PreviewProgress,
            is BridgeUiEvent.CandidateImportLoudnessProgress,
            // Importing is a desktop feature; Android has no import queue, so
            // how much of it has been identified drives nothing here.
            is BridgeUiEvent.ImportQueueIdentifyProgress,
            -> {
                logger.debug("ignoring ${event::class.simpleName}")
            }

            is BridgeUiEvent.PlaybackLoading,
            is BridgeUiEvent.PlaybackPlaying,
            is BridgeUiEvent.PlaybackPaused,
            BridgeUiEvent.PlaybackStopped,
            is BridgeUiEvent.PlaybackError,
            is BridgeUiEvent.PlaybackProgress,
            is BridgeUiEvent.PlaybackSeeked,
            is BridgeUiEvent.RepeatModeChanged,
            is BridgeUiEvent.VolumeChanged,
            is BridgeUiEvent.MuteChanged,
            is BridgeUiEvent.QueueItemsAdded,
            is BridgeUiEvent.Error,
            is BridgeUiEvent.CastStatusChanged,
            -> {
                error("handled event reached obsolete-event path: ${event::class.simpleName}")
            }
        }
    }

}
