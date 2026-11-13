package fm.bae.screensubscription

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeException

/**
 * A bridge object the app pulls its values from: each [next] answers with the next value core
 * delivers, and [cancel] ends it. Only this module calls [next]; the app reads a subscription
 * through [ScreenSubscription], [pullEach] or [firstValue].
 */
interface PulledSubscription<Value> {
    suspend fun next(): Value

    suspend fun cancel()
}

/**
 * A bridge object's pulls: every object the app pulls from has a `next` and a `cancel`, which
 * uniffi gives no interface in common, so each hands its own, as
 * `subscription.let { pulled(it::next, it::cancel) }`.
 */
fun <Value> pulled(
    pullNext: suspend () -> Value,
    cancelPulls: suspend () -> Unit,
): PulledSubscription<Value> =
    object : PulledSubscription<Value> {
        override suspend fun next(): Value = pullNext()

        override suspend fun cancel() = cancelPulls()
    }

/** These pulls, each value read through [transform]; cancelling the result cancels these. */
fun <Value, Mapped> PulledSubscription<Value>.map(transform: (Value) -> Mapped): PulledSubscription<Mapped> =
    pulled({ transform(next()) }, this::cancel)

/**
 * Pull each value and hand it to [onValue] in the calling coroutine, until that coroutine is
 * cancelled or the subscription ends (a pull answers [BridgeException.Cancelled], or
 * [BridgeException.ShuttingDown] once the runtime is ending with the process: neither clears, so
 * pulling on would spin on the answer). A failed read
 * is handed to [onError] and the pulls go on: the next value answers the next read that succeeds.
 */
suspend fun <Value> PulledSubscription<Value>.pullEach(
    onValue: suspend (Value) -> Unit,
    onError: suspend (BridgeException) -> Unit,
) {
    while (currentCoroutineContext().isActive) {
        try {
            onValue(next())
        } catch (_: BridgeException.Cancelled) {
            return
        } catch (_: BridgeException.ShuttingDown) {
            return
        } catch (error: BridgeException) {
            onError(error)
        }
    }
}

/** The first value the subscription answers; it is cancelled once that pull ends, however it ends. */
suspend fun <Value> PulledSubscription<Value>.firstValue(): Value =
    try {
        next()
    } finally {
        withContext(NonCancellable) { cancel() }
    }

/**
 * The one way a screen reads a subscription: [pullEach] on the main thread, handing each value to
 * the screen there, until [stop] or until the subscription ends.
 */
class ScreenSubscription<Value>(
    private val source: PulledSubscription<Value>,
    private val scope: CoroutineScope,
    onValue: (Value) -> Unit,
    onError: (BridgeException) -> Unit,
) {
    private val pulls: Job =
        scope.launch(Dispatchers.Main.immediate) {
            source.pullEach(onValue, onError)
        }

    /** Stop pulling and end the subscription. */
    fun stop() {
        pulls.cancel()
        // Ends even when the session's scope is closing.
        scope.launch(NonCancellable) { source.cancel() }
    }
}
