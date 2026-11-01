package fm.bae.app.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgeQueueAddition
import uniffi.bae_bridge.PlaybackValuesSubscription
import uniffi.bae_bridge.QueueAdditionsSubscription

/**
 * A bridge object a screen pulls its values from: each [next] answers with
 * the next value core delivers, and [cancel] ends it.
 */
interface PulledSubscription<Value> {
    suspend fun next(): Value

    suspend fun cancel()
}

/**
 * The one way a screen reads a subscription: pull each value and hand it to
 * the screen on the main thread, until [stop] or until the subscription ends.
 * A failed read is handed to `onError` and the pulls go on: the next value
 * answers the next read that succeeds.
 */
class ScreenSubscription<Value>(
    private val source: PulledSubscription<Value>,
    private val scope: CoroutineScope,
    onValue: (Value) -> Unit,
    onError: (BridgeException) -> Unit,
) {
    private val pulls: Job =
        scope.launch(Dispatchers.Main.immediate) {
            while (isActive) {
                try {
                    onValue(source.next())
                } catch (_: BridgeException.Cancelled) {
                    return@launch
                } catch (error: BridgeException) {
                    onError(error)
                }
            }
        }

    /** Stop pulling and end the subscription. */
    fun stop() {
        pulls.cancel()
        // Ends even when the session's scope is closing.
        scope.launch(NonCancellable) { source.cancel() }
    }
}

// Every bridge object a screen pulls from, read only through ScreenSubscription.

fun PlaybackValuesSubscription.pulled(): PulledSubscription<BridgePlaybackValues> =
    object : PulledSubscription<BridgePlaybackValues> {
        override suspend fun next(): BridgePlaybackValues = this@pulled.next()

        override suspend fun cancel() = this@pulled.cancel()
    }

fun QueueAdditionsSubscription.pulled(): PulledSubscription<BridgeQueueAddition> =
    object : PulledSubscription<BridgeQueueAddition> {
        override suspend fun next(): BridgeQueueAddition = this@pulled.next()

        override suspend fun cancel() = this@pulled.cancel()
    }
