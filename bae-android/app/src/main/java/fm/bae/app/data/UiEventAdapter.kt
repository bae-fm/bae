package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.BugReports
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
        messages: SessionMessages,
        bugs: BugReports,
        player: PlaybackEventSink,
        errors: ErrorDisplays,
    ) {
        try {
            route(event, player)
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            // No special case for Cancelled: core says whether an error has a line,
            // and show drops the ones that do not.
            logger.error("event handling failed", e)
            messages.show(errors.display(e))
        } catch (e: Exception) {
            // A bug, reported here rather than by the scope's handler so the
            // loop goes on to the next event.
            bugs.report("handle ${event::class.simpleName}", e)
        }
    }

    private fun route(
        event: BridgeUiEvent,
        player: PlaybackEventSink,
    ) {
        when (event) {
            is BridgeUiEvent.QueueItemsAdded -> {
                player.onQueueItemsAdded(event.count.toInt())
            }
        }
    }
}
