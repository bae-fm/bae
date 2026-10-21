package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
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
        errors: ErrorDisplays,
    ) {
        try {
            route(event, stores, player, errors)
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            // No special case for Cancelled: core says whether an error has a line,
            // and showError drops the ones that do not.
            logger.error("event handling failed", e)
            stores.config.showError(errors.display(e))
        } catch (e: Exception) {
            logger.error("event handling failed", e)
            stores.config.showError(DisplayError(e.toString()))
        }
    }

    private suspend fun route(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorDisplays,
    ) {
        handleDirectEvent(event, stores, player, errors)
    }

    private fun handleDirectEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorDisplays,
    ) {
        if (handlePlaybackEvent(event, stores, player, errors)) {
            return
        }
        error("unhandled UI event: ${event::class.simpleName}")
    }

    private fun handlePlaybackEvent(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink,
        errors: ErrorDisplays,
    ): Boolean {
        when (event) {
            is BridgeUiEvent.PlaybackError -> {
                stores.config.showError(errors.display(event.reason))
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
}
